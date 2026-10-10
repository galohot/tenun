// Sign-in through the shared Akun WorkOS project (V2-D5; pattern from galohot/akun and Dheevite):
// hosted AuthKit, a signed state bound to the browser by a nonce cookie, and our own signed session
// cookie. Signing out also ends the AuthKit session (akun D6).
import { Hono } from 'hono';
import type { Context, Next } from 'hono';
import { cookie, sign, verify } from './akun/token';
import { authenticate, authorizeUrl, revokeSession } from './akun/workos';
import type { Account, Env, HonoEnv } from './env';
import { page } from './pages';
import { esc, now, rid, validEmail } from './util';

const SESSION_DAYS = 30;
export const SESSION_COOKIE = 'tn_s';
const STATE_COOKIE = 'tn_state';

const isHttps = (c: Context) => new URL(c.req.url).protocol === 'https:';

function setCookie(c: Context, name: string, value: string, maxAge: number, path = '/') {
  // Secure everywhere except plain-http local development.
  const secure = isHttps(c) || c.env.DEV_LOGIN !== '1';
  c.header('set-cookie', `${name}=${value}; Path=${path}; Max-Age=${maxAge}; HttpOnly;${secure ? ' Secure;' : ''} SameSite=Lax`, {
    append: true,
  });
}

/** Only our own pages may be returned to after sign-in. */
export function safeReturn(r: string | null | undefined): string {
  return r && /^\/[a-z0-9/_-]*(\?code=[A-Z0-9-]{9})?$/i.test(r) && !r.startsWith('//') ? r : '/account';
}

export const signinMode = (env: Env): 'open' | 'dev' | 'closed' =>
  env.SIGNIN === 'open' && env.WORKOS_CLIENT_ID && env.WORKOS_API_KEY ? 'open' : env.SIGNIN === 'dev' && env.DEV_LOGIN === '1' ? 'dev' : 'closed';

export const auth = new Hono<HonoEnv>();

auth.get('/login', async (c) => {
  const ret = safeReturn(c.req.query('return'));
  const mode = signinMode(c.env);
  if (mode === 'dev') return c.redirect(`/auth/dev?return=${encodeURIComponent(ret)}`);
  if (mode === 'closed') {
    return c.html(
      page('Sign-in opens soon', `<h1>Sign-in opens soon</h1><p class="lede">Publishing to tenun.cloud is almost ready. Dashboards you were sent still open as usual.</p>`),
      503,
    );
  }
  const nonce = rid(20);
  const state = await sign({ k: 'state', nonce, ret, exp: now() / 1000 + 600 }, c.env.SESSION_SECRET);
  setCookie(c, STATE_COOKIE, nonce, 600, '/auth');
  return c.redirect(authorizeUrl({ clientId: c.env.WORKOS_CLIENT_ID!, redirectUri: `${new URL(c.req.url).origin}/auth/callback`, state }));
});

auth.get('/callback', async (c) => {
  const env = c.env;
  const st = await verify(c.req.query('state'), 'state', env.SESSION_SECRET);
  const code = c.req.query('code');
  const failed = () => c.html(page('Sign-in failed', `<h1>Sign-in did not work</h1><p class="lede"><a href="/auth/login">Try again</a></p>`), 400);
  if (!st || !code || st.nonce !== cookie(c.req.raw, STATE_COOKIE) || signinMode(env) !== 'open') return failed();
  const got = await authenticate(code, env.WORKOS_CLIENT_ID!, env.WORKOS_API_KEY!);
  if (!got) return failed();
  return startSession(c, got.user, safeReturn(String(st.ret ?? '')), got.sid);
});

/** Local development only: sign in as any email without WorkOS. Refuses unless DEV_LOGIN=1. */
auth.get('/dev', async (c) => {
  if (signinMode(c.env) !== 'dev') return c.notFound();
  const email = c.req.query('email');
  const ret = safeReturn(c.req.query('return'));
  if (!email || !validEmail(email)) {
    return c.html(
      page(
        'Test sign-in',
        `<h1>Test sign-in</h1><p class="lede">Local development only.</p>
         <form method="get" action="/auth/dev" class="stack"><input type="hidden" name="return" value="${esc(ret)}">
         <input name="email" type="email" required placeholder="you@example.com" class="field">
         <button class="button primary">Sign in</button></form>`,
      ),
    );
  }
  return startSession(c, { id: `dev_${email.toLowerCase().replace(/[^a-z0-9]/g, '')}`, email: email.toLowerCase(), name: email.split('@')[0] }, ret);
});

auth.get('/logout', async (c) => {
  const s = await verify(cookie(c.req.raw, SESSION_COOKIE), 'session', c.env.SESSION_SECRET);
  if (typeof s?.sid === 'string' && c.env.WORKOS_API_KEY) await revokeSession(s.sid, c.env.WORKOS_API_KEY);
  setCookie(c, SESSION_COOKIE, '', 0);
  return c.redirect('/');
});

async function startSession(c: Context<HonoEnv>, u: Account, ret: string, sid: string | null = null) {
  const t = now();
  // One account per email: a WorkOS id that changes for the same email keeps the account.
  await c.env.DB.prepare(
    `INSERT INTO accounts (id, email, name, created_at) VALUES (?1, ?2, ?3, ?4)
     ON CONFLICT(email) DO UPDATE SET name = ?3`,
  )
    .bind(u.id, u.email.toLowerCase(), u.name, t)
    .run();
  const row = await c.env.DB.prepare('SELECT id FROM accounts WHERE email = ?1').bind(u.email.toLowerCase()).first<{ id: string }>();
  const token = await sign({ k: 'session', u: row!.id, e: u.email.toLowerCase(), n: u.name, sid, exp: t / 1000 + SESSION_DAYS * 86400 }, c.env.SESSION_SECRET);
  setCookie(c, SESSION_COOKIE, token, SESSION_DAYS * 86400);
  setCookie(c, STATE_COOKIE, '', 0, '/auth');
  return c.redirect(ret);
}

/** Middleware: puts the signed-in account (or null) on the context. */
export async function loadAccount(c: Context<HonoEnv>, next: Next) {
  const s = await verify(cookie(c.req.raw, SESSION_COOKIE), 'session', c.env.SESSION_SECRET);
  c.set('account', s ? { id: String(s.u), email: String(s.e), name: String(s.n ?? '') } : null);
  await next();
}

/** Page forms post only from our own pages (the session cookie is SameSite=Lax as a second wall). */
export function sameOrigin(c: Context): boolean {
  const origin = c.req.header('origin');
  return !!origin && origin === new URL(c.req.url).origin;
}
