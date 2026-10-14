// tenun.cloud: the landing page (static), the publish API, account pages, and hosted dashboards.
import { Hono } from 'hono';
import type { Context } from 'hono';
import { ROW_COLUMNS, api, canView, deleteDashboard } from './api';
import { auth, loadAccount, safeReturn, sameOrigin } from './auth';
import type { HonoEnv } from './env';
import { LIMITS, type Kind } from './env';
import { accountPage, connectPage, hostedShell, notice, page, passwordPage, type DashRow, type TokenRow } from './pages';
import { OPEN_SECONDS, TRIES, checkPassword, cookieName, cookieValue, triesOf, whoOf, wrongTry } from './password';
import { emailOn, sendScheduled } from './mail';
import { now, rid, sha256 } from './util';

const app = new Hono<HonoEnv>();

// No page here runs inline script, and nothing may frame it, except an item its owner lets other sites embed.
const csp = (frame: boolean) =>
  [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "font-src 'self' data:",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'self' https://api.workos.com",
  frame ? 'frame-ancestors *' : "frame-ancestors 'none'",
].join('; ');

app.use('*', async (c, next) => {
  await next();
  c.res.headers.set('content-security-policy', csp(!!c.get('frame')));
  c.res.headers.set('x-content-type-options', 'nosniff');
  c.res.headers.set('referrer-policy', 'same-origin');
});

app.use('*', loadAccount);
app.route('/auth', auth);
app.route('/api/v1', api);

app.get('/api/v1/health', (c) => c.json({ ok: true, service: 'tenun.cloud', limits: LIMITS, emailUpdates: emailOn(c.env) }));

// ---- one-click unsubscribe (v5 build 13) -------------------------------------------

/** Stops the email updates of one recipient of one item. The token in the link is the only key. */
async function unsubscribe(c: Context<HonoEnv>) {
  const token = c.req.param('token') ?? '';
  const r = await c.env.DB.prepare('SELECT r.dashboard_id, d.title FROM recipients r JOIN dashboards d ON d.id = r.dashboard_id WHERE r.token = ?1')
    .bind(token)
    .first<{ dashboard_id: string; title: string }>();
  if (r) await c.env.DB.prepare('UPDATE recipients SET active = 0 WHERE token = ?1').bind(token).run();
  return r;
}

app.get('/u/:token', async (c) => {
  const r = await unsubscribe(c);
  if (!r) return c.html(notice('Not found', 'This link does not stop any email updates any more.', 'warn'), 404);
  return c.html(notice('No more emails', `You will get no more email updates about “${r.title.replace(/[<>&"]/g, '')}”.`, 'ok'));
});
// The one-click unsubscribe of mail apps (RFC 8058) posts to the same link.
app.post('/u/:token', async (c) => ((await unsubscribe(c)) ? c.text('Unsubscribed') : c.text('Not found', 404)));

// ---- device approval --------------------------------------------------------

app.get('/connect', async (c) => {
  const me = c.get('account');
  const code = (c.req.query('code') ?? '').toUpperCase().trim();
  if (!me) return c.redirect(`/auth/login?return=${encodeURIComponent(safeReturn(code ? `/connect?code=${code}` : '/connect'))}`);
  if (!code) return c.html(connectPage(me, '', null));
  const row = await c.env.DB.prepare('SELECT label, expires_at, account_id FROM devices WHERE user_code = ?1')
    .bind(code)
    .first<{ label: string; expires_at: number; account_id: string | null }>();
  if (!row || row.expires_at < now()) return c.html(connectPage(me, code, null, 'That code is not valid any more. Start again in Tenun.'), 404);
  if (row.account_id) return c.html(notice('Already connected', 'You can go back to Tenun.', 'ok', me));
  return c.html(connectPage(me, code, row.label));
});

app.post('/connect', async (c) => {
  const me = c.get('account');
  if (!me || !sameOrigin(c)) return c.text('Forbidden', 403);
  const form = await c.req.parseBody();
  const code = String(form.code ?? '').toUpperCase();
  const row = await c.env.DB.prepare('SELECT label, expires_at, account_id FROM devices WHERE user_code = ?1')
    .bind(code)
    .first<{ label: string; expires_at: number; account_id: string | null }>();
  if (!row || row.expires_at < now() || row.account_id) return c.html(connectPage(me, code, null, 'That code is not valid any more.'), 400);
  const token = `tnn_${rid(40)}`;
  const t = now();
  await c.env.DB.batch([
    c.env.DB.prepare('INSERT INTO tokens (id, account_id, hash, label, created_at) VALUES (?1, ?2, ?3, ?4, ?5)').bind(rid(16), me.id, await sha256(token), row.label, t),
    c.env.DB.prepare('UPDATE devices SET account_id = ?1, token = ?2 WHERE user_code = ?3').bind(me.id, token, code),
  ]);
  return c.html(notice('Connected', `“${row.label.replace(/[<>&"]/g, '')}” can now publish to your account. You can go back to Tenun.`, 'ok', me));
});

// ---- account ----------------------------------------------------------------

app.get('/account', async (c) => {
  const me = c.get('account');
  if (!me) return c.redirect('/auth/login?return=/account');
  const [d, t] = await c.env.DB.batch([
    c.env.DB.prepare(`SELECT ${ROW_COLUMNS} FROM dashboards WHERE account_id = ?1 ORDER BY created_at`).bind(me.id),
    c.env.DB.prepare('SELECT id, label, created_at, last_used_at FROM tokens WHERE account_id = ?1 ORDER BY created_at').bind(me.id),
  ]);
  return c.html(accountPage(me, d.results as DashRow[], t.results as TokenRow[], LIMITS.dashboards));
});

app.post('/account/dashboards/:id/delete', async (c) => {
  const me = c.get('account');
  if (!me || !sameOrigin(c)) return c.text('Forbidden', 403);
  const id = c.req.param('id');
  const own = await c.env.DB.prepare('SELECT 1 AS ok FROM dashboards WHERE id = ?1 AND account_id = ?2').bind(id, me.id).first();
  if (own) await deleteDashboard(c.env, id);
  return c.redirect('/account');
});

app.post('/account/tokens/:id/revoke', async (c) => {
  const me = c.get('account');
  if (!me || !sameOrigin(c)) return c.text('Forbidden', 403);
  await c.env.DB.prepare('DELETE FROM tokens WHERE id = ?1 AND account_id = ?2').bind(c.req.param('id'), me.id).run();
  return c.redirect('/account');
});

app.post('/account/delete', async (c) => {
  const me = c.get('account');
  if (!me || !sameOrigin(c)) return c.text('Forbidden', 403);
  const form = await c.req.parseBody();
  if (form.sure !== 'yes') return c.redirect('/account');
  const { results } = await c.env.DB.prepare('SELECT id FROM dashboards WHERE account_id = ?1').bind(me.id).all<{ id: string }>();
  for (const d of results) await deleteDashboard(c.env, d.id);
  await c.env.DB.prepare('DELETE FROM accounts WHERE id = ?1').bind(me.id).run();
  return c.redirect('/auth/logout');
});

// ---- hosted dashboards ------------------------------------------------------

/** A hosted dashboard (/d/) or site (/s/, build 14): the same access rules for both. */
async function hosted(c: Context<HonoEnv>, kind: Kind) {
  const what = kind === 'site' ? 'site' : 'dashboard';
  const { row, allowed, needsSignIn, needsPassword } = await canView(c, c.req.param('id') ?? '', kind);
  if (!row) return c.html(notice('Not found', `This ${what} does not exist, or its owner deleted it.`, 'warn', c.get('account')), 404);
  // An item for invited people is never framed: a Google sign-in does not work inside another site (V5-D14).
  c.set('frame', !!row.embed && row.access !== 'invited');
  if (needsPassword) {
    c.header('cache-control', 'private, no-store');
    return c.html(passwordPage(kind, row.id), 401);
  }
  if (needsSignIn) return c.redirect(`/auth/login?return=${encodeURIComponent(`/${kind === 'site' ? 's' : 'd'}/${row.id}`)}`);
  if (!allowed) {
    return c.html(
      notice('Invited people only', `This ${what} is shared with a list of people. You are signed in as ${c.get('account')?.email.replace(/[<>&"]/g, '')}. Ask its owner to invite this address.`, 'warn', c.get('account')),
      403,
    );
  }
  if (!row.version) return c.html(notice('Not published yet', `The owner has not published this ${what} yet.`, 'warn'), 404);
  c.header('cache-control', 'private, no-cache');
  // Invited readers may ask for email updates on the page, when the owner offers them and tenun.cloud sends them.
  const updates = emailOn(c.env) && row.access === 'invited' && !!row.updates && row.updates !== 'off' && !!c.get('account');
  return c.html(hostedShell(row, { updates }));
}

app.get('/d/:id', (c) => hosted(c, 'dashboard'));
app.get('/s/:id', (c) => hosted(c, 'site'));

/**
 * A password, typed on the password page. Right: a cookie for this item only, for 7 days, then back to the item.
 * Wrong: the page again, and one more try counted against the address; at five in a minute, even the right one waits.
 */
async function unlock(c: Context<HonoEnv>, kind: Kind) {
  const id = c.req.param('id') ?? '';
  const back = `/${kind === 'site' ? 's' : 'd'}/${encodeURIComponent(id)}`;
  const row = await c.env.DB.prepare(
    "SELECT id, embed, pw_hash, pw_salt, pw_iter, pw_version FROM dashboards WHERE id = ?1 AND kind = ?2 AND access = 'link' AND pw_hash IS NOT NULL",
  )
    .bind(id, kind)
    .first<{ id: string; embed: number; pw_hash: string; pw_salt: string; pw_iter: number; pw_version: number }>();
  if (!row) return c.redirect(back, 303);
  if (!sameOrigin(c)) return c.text('Forbidden', 403);
  c.set('frame', !!row.embed);
  c.header('cache-control', 'private, no-store');
  const who = await whoOf(c.req.header('cf-connecting-ip'));
  if ((await triesOf(c.env, who)) >= TRIES) {
    return c.html(passwordPage(kind, row.id, 'Too many wrong passwords. Wait a minute, then try again.'), 429);
  }
  const form = await c.req.parseBody();
  const pw = typeof form.password === 'string' ? form.password : '';
  if (!pw || !(await checkPassword(pw, row))) {
    await wrongTry(c.env, who);
    return c.html(passwordPage(kind, row.id, 'That password is not right.'), 401);
  }
  // Inside another site, a cookie is sent only when it is SameSite=None; Partitioned keeps it to that site.
  const secure = new URL(c.req.url).protocol === 'https:' || c.env.DEV_LOGIN !== '1';
  const site = row.embed && secure ? 'SameSite=None; Partitioned' : 'SameSite=Lax';
  c.header(
    'set-cookie',
    `${cookieName(row.id)}=${await cookieValue(c.env, row.id, row.pw_version)}; Path=/; Max-Age=${OPEN_SECONDS}; HttpOnly;${secure ? ' Secure;' : ''} ${site}`,
  );
  return c.redirect(back, 303);
}

app.post('/d/:id/unlock', (c) => unlock(c, 'dashboard'));
app.post('/s/:id/unlock', (c) => unlock(c, 'site'));
// A page of a site: the site routes by the address's #, which a server never sees.
app.get('/s/:id/:page', (c) => c.redirect(`/s/${encodeURIComponent(c.req.param('id'))}#/p/${encodeURIComponent(c.req.param('page'))}`));

app.notFound(async (c) => {
  // Everything else is a static file (the landing page and the viewer bundle).
  const res = await c.env.ASSETS.fetch(c.req.raw);
  if (res.status !== 404) return res;
  return c.html(page('Not found', '<h1>This page does not exist.</h1><p class="lede"><a href="/">Go to the Tenun home page</a></p>'), 404);
});

export default {
  fetch: app.fetch,
  /** Daily: delete publish records older than a day, devices that expired, and wrong passwords older than a minute; send the
   *  daily and weekly email updates. */
  async scheduled(e: ScheduledController, env: HonoEnv['Bindings']) {
    const t = e.scheduledTime || now();
    await env.DB.batch([
      env.DB.prepare('DELETE FROM publishes WHERE at < ?1').bind(t - 86400_000),
      env.DB.prepare('DELETE FROM devices WHERE expires_at < ?1').bind(t),
      env.DB.prepare('DELETE FROM unlock_tries WHERE at < ?1').bind(t - 60_000),
    ]);
    // Daily and weekly email updates (v5 build 13); nothing while they are off.
    await sendScheduled(env, t).catch(() => 0);
  },
} satisfies ExportedHandler<HonoEnv['Bindings']>;
