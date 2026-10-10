// tenun.cloud: the landing page (static), the publish API, account pages, and hosted dashboards.
import { Hono } from 'hono';
import { api, canView, deleteDashboard } from './api';
import { auth, loadAccount, safeReturn, sameOrigin } from './auth';
import type { HonoEnv } from './env';
import { LIMITS } from './env';
import { accountPage, connectPage, hostedShell, notice, page, type DashRow, type TokenRow } from './pages';
import { now, rid, sha256 } from './util';

const app = new Hono<HonoEnv>();

// No page here runs inline script, and nothing may frame it.
const CSP = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "font-src 'self' data:",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'self' https://api.workos.com",
  "frame-ancestors 'none'",
].join('; ');

app.use('*', async (c, next) => {
  await next();
  c.res.headers.set('content-security-policy', CSP);
  c.res.headers.set('x-content-type-options', 'nosniff');
  c.res.headers.set('referrer-policy', 'same-origin');
});

app.use('*', loadAccount);
app.route('/auth', auth);
app.route('/api/v1', api);

app.get('/api/v1/health', (c) => c.json({ ok: true, service: 'tenun.cloud', limits: LIMITS }));

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
    c.env.DB.prepare('SELECT id, title, access, source, every_minutes, version, generated_at, bytes FROM dashboards WHERE account_id = ?1 ORDER BY created_at').bind(me.id),
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

app.get('/d/:id', async (c) => {
  const { row, allowed, needsSignIn } = await canView(c, c.req.param('id'));
  if (!row) return c.html(notice('Not found', 'This dashboard does not exist, or its owner deleted it.', 'warn', c.get('account')), 404);
  if (needsSignIn) return c.redirect(`/auth/login?return=${encodeURIComponent(`/d/${row.id}`)}`);
  if (!allowed) {
    return c.html(
      notice('Invited people only', `This dashboard is shared with a list of people. You are signed in as ${c.get('account')?.email.replace(/[<>&"]/g, '')}. Ask its owner to invite this address.`, 'warn', c.get('account')),
      403,
    );
  }
  if (!row.version) return c.html(notice('Not published yet', 'The owner has not published this dashboard yet.', 'warn'), 404);
  c.header('cache-control', 'private, no-cache');
  return c.html(hostedShell(row));
});

app.notFound(async (c) => {
  // Everything else is a static file (the landing page and the viewer bundle).
  const res = await c.env.ASSETS.fetch(c.req.raw);
  if (res.status !== 404) return res;
  return c.html(page('Not found', '<h1>This page does not exist.</h1><p class="lede"><a href="/">Go to the Tenun home page</a></p>'), 404);
});

export default {
  fetch: app.fetch,
  /** Daily: delete publish records older than a day and devices that expired. */
  async scheduled(_e: ScheduledController, env: HonoEnv['Bindings']) {
    const t = now();
    await env.DB.batch([
      env.DB.prepare('DELETE FROM publishes WHERE at < ?1').bind(t - 86400_000),
      env.DB.prepare('DELETE FROM devices WHERE expires_at < ?1').bind(t),
    ]);
  },
} satisfies ExportedHandler<HonoEnv['Bindings']>;
