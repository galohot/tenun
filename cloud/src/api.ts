// The publish API (Bearer tokens from device sign-in) and the hosted dashboards.
import { Hono } from 'hono';
import type { Context } from 'hono';
import type { HonoEnv } from './env';
import { LIMITS, type Kind } from './env';
import type { DashRow } from './pages';
import { now, rid, sha256, userCode, validEmail } from './util';

export const api = new Hono<HonoEnv>();

const DEVICE_TTL = 10 * 60 * 1000;
const json = (c: Context, status: number, body: unknown) => c.json(body as never, status as never);
const fail = (c: Context, status: number, error: string, extra: Record<string, unknown> = {}) => json(c, status, { error, ...extra });

const snapKey = (dash: string, version: number) => `snap/${dash}/${version}.json.gz`;

/** The account behind a Bearer token, or null. Marks the token as used. */
async function bearer(c: Context<HonoEnv>): Promise<{ account: string; token: string } | null> {
  const h = c.req.header('authorization') ?? '';
  const m = /^Bearer (tnn_[A-Za-z0-9]{40})$/.exec(h);
  if (!m) return null;
  const row = await c.env.DB.prepare('SELECT id, account_id FROM tokens WHERE hash = ?1').bind(await sha256(m[1])).first<{ id: string; account_id: string }>();
  if (!row) return null;
  c.executionCtx.waitUntil(c.env.DB.prepare('UPDATE tokens SET last_used_at = ?1 WHERE id = ?2').bind(now(), row.id).run());
  return { account: row.account_id, token: row.id };
}

// ---- device sign-in ---------------------------------------------------------

api.post('/devices', async (c) => {
  const body = await c.req.json<{ label?: string }>().catch(() => ({}) as { label?: string });
  const label = String(body.label ?? 'Tenun').slice(0, 60);
  const device = rid(40);
  const code = userCode();
  const t = now();
  await c.env.DB.prepare('DELETE FROM devices WHERE expires_at < ?1').bind(t).run();
  // Sign-in codes are open to anyone, so their number is capped (each lives 10 minutes).
  const open = await c.env.DB.prepare('SELECT count(*) AS n FROM devices').first<{ n: number }>();
  if ((open?.n ?? 0) >= LIMITS.openDevices) return fail(c, 503, 'busy', { message: 'Too many sign-ins are waiting. Try again in a few minutes.' });
  await c.env.DB.prepare('INSERT INTO devices (code_hash, user_code, label, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)')
    .bind(await sha256(device), code, label, t, t + DEVICE_TTL)
    .run();
  const origin = new URL(c.req.url).origin;
  return json(c, 200, {
    device_code: device,
    user_code: code,
    verification_uri: `${origin}/connect`,
    verification_uri_complete: `${origin}/connect?code=${code}`,
    interval: 3,
    expires_in: DEVICE_TTL / 1000,
  });
});

api.post('/devices/token', async (c) => {
  const body = await c.req.json<{ device_code?: string }>().catch(() => ({}) as { device_code?: string });
  if (!body.device_code) return fail(c, 400, 'invalid_request');
  const hash = await sha256(body.device_code);
  const row = await c.env.DB.prepare('SELECT account_id, token, expires_at FROM devices WHERE code_hash = ?1')
    .bind(hash)
    .first<{ account_id: string | null; token: string | null; expires_at: number }>();
  if (!row || row.expires_at < now()) return fail(c, 400, 'expired_token');
  if (!row.token || !row.account_id) return fail(c, 400, 'authorization_pending');
  // Handed over exactly once.
  await c.env.DB.prepare('DELETE FROM devices WHERE code_hash = ?1').bind(hash).run();
  const acc = await c.env.DB.prepare('SELECT email FROM accounts WHERE id = ?1').bind(row.account_id).first<{ email: string }>();
  return json(c, 200, { token: row.token, email: acc?.email ?? '' });
});

// ---- account and dashboards -------------------------------------------------

api.get('/me', async (c) => {
  const who = await bearer(c);
  if (!who) return fail(c, 401, 'unauthorized');
  const acc = await c.env.DB.prepare('SELECT email FROM accounts WHERE id = ?1').bind(who.account).first<{ email: string }>();
  const { results } = await c.env.DB.prepare(
    'SELECT id, title, access, source, every_minutes, version, generated_at, bytes, kind, pages FROM dashboards WHERE account_id = ?1 ORDER BY created_at',
  )
    .bind(who.account)
    .all<DashRow>();
  const origin = new URL(c.req.url).origin;
  return json(c, 200, {
    email: acc?.email,
    dashboards: results.map((d) => ({ ...d, url: urlOf(origin, d.kind, d.id) })),
    limits: LIMITS,
  });
});

/** Where a published item is seen: a dashboard at /d/, a site at /s/ (build 14). */
export const urlOf = (origin: string, kind: Kind | undefined, id: string) => `${origin}/${kind === 'site' ? 's' : 'd'}/${id}`;
/** The path segment of a kind in the API: /dashboards or /sites. */
const KINDS: [string, Kind][] = [
  ['dashboards', 'dashboard'],
  ['sites', 'site'],
];

for (const [path, kind] of KINDS) {
  api.post(`/${path}`, async (c) => {
    const who = await bearer(c);
    if (!who) return fail(c, 401, 'unauthorized');
    const body = await c.req.json<{ title?: string }>().catch(() => ({}) as { title?: string });
    // A site counts as one item, as a dashboard does (V4-D17).
    const n = await c.env.DB.prepare('SELECT count(*) AS n FROM dashboards WHERE account_id = ?1').bind(who.account).first<{ n: number }>();
    if ((n?.n ?? 0) >= LIMITS.dashboards) {
      return fail(c, 403, 'limit', {
        message: `The free plan publishes up to ${LIMITS.dashboards} dashboards and sites. Delete one on your account page first.`,
      });
    }
    const id = rid(22);
    const t = now();
    await c.env.DB.prepare('INSERT INTO dashboards (id, account_id, title, kind, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)')
      .bind(id, who.account, String(body.title ?? (kind === 'site' ? 'Site' : 'Dashboard')).slice(0, 120), kind, t)
      .run();
    return json(c, 201, { id, url: urlOf(new URL(c.req.url).origin, kind, id) });
  });
  api.patch(`/${path}/:id`, (c) => patchItem(c, kind));
  api.delete(`/${path}/:id`, (c) => deleteItem(c, kind));
  api.put(`/${path}/:id/snapshot`, (c) => putSnapshot(c, kind));
}

async function owned(c: Context<HonoEnv>, id: string, account: string, kind: Kind) {
  return c.env.DB.prepare('SELECT id, version FROM dashboards WHERE id = ?1 AND account_id = ?2 AND kind = ?3')
    .bind(id, account, kind)
    .first<{ id: string; version: number }>();
}

async function patchItem(c: Context<HonoEnv>, kind: Kind) {
  const who = await bearer(c);
  if (!who) return fail(c, 401, 'unauthorized');
  const id = c.req.param('id') ?? '';
  if (!(await owned(c, id, who.account, kind))) return fail(c, 404, 'not_found');
  const b = await c.req.json<{ title?: string; access?: string; invites?: string[] }>().catch(() => null);
  if (!b) return fail(c, 400, 'invalid_json');
  const stmts: D1PreparedStatement[] = [];
  if (b.title !== undefined) stmts.push(c.env.DB.prepare('UPDATE dashboards SET title = ?1 WHERE id = ?2').bind(String(b.title).slice(0, 120), id));
  if (b.access !== undefined) {
    if (b.access !== 'link' && b.access !== 'invited') return fail(c, 400, 'access must be link or invited');
    stmts.push(c.env.DB.prepare('UPDATE dashboards SET access = ?1 WHERE id = ?2').bind(b.access, id));
  }
  if (b.invites !== undefined) {
    const emails = [...new Set(b.invites.map((e) => String(e).trim().toLowerCase()))];
    if (emails.length > 50 || emails.some((e) => !validEmail(e))) return fail(c, 400, 'invites must be up to 50 valid emails');
    stmts.push(c.env.DB.prepare('DELETE FROM invites WHERE dashboard_id = ?1').bind(id));
    for (const e of emails) stmts.push(c.env.DB.prepare('INSERT INTO invites (dashboard_id, email) VALUES (?1, ?2)').bind(id, e));
  }
  if (stmts.length) await c.env.DB.batch(stmts);
  return json(c, 200, { ok: true });
}

/** Deletes a dashboard: every stored version first, then the rows. */
export async function deleteDashboard(env: HonoEnv['Bindings'], id: string) {
  const { results } = await env.DB.prepare('SELECT version FROM versions WHERE dashboard_id = ?1').bind(id).all<{ version: number }>();
  if (results.length) await env.SNAPSHOTS.delete(results.map((r) => snapKey(id, r.version)));
  await env.DB.prepare('DELETE FROM dashboards WHERE id = ?1').bind(id).run();
}

async function deleteItem(c: Context<HonoEnv>, kind: Kind) {
  const who = await bearer(c);
  if (!who) return fail(c, 401, 'unauthorized');
  const id = c.req.param('id') ?? '';
  if (!(await owned(c, id, who.account, kind))) return fail(c, 404, 'not_found');
  await deleteDashboard(c.env, id);
  return json(c, 200, { ok: true });
}

// ---- publishing -------------------------------------------------------------

async function putSnapshot(c: Context<HonoEnv>, kind: Kind) {
  const who = await bearer(c);
  if (!who) return fail(c, 401, 'unauthorized');
  const id = c.req.param('id') ?? '';
  const dash = await owned(c, id, who.account, kind);
  if (!dash) return fail(c, 404, 'not_found');

  // A site may be bigger, and holds up to 10 pages; the app says how many (the Worker never parses a snapshot).
  const maxBytes = kind === 'site' ? LIMITS.siteMaxBytes : LIMITS.maxBytes;
  const pages = kind === 'site' ? Number(c.req.header('x-tenun-pages') ?? 0) : 1;
  if (kind === 'site' && !(pages >= 1 && pages <= LIMITS.sitePages)) {
    return fail(c, 413, 'too_many_pages', { max_pages: LIMITS.sitePages, message: `A site on tenun.cloud holds up to ${LIMITS.sitePages} pages.` });
  }
  const declared = Number(c.req.header('content-length') ?? 0);
  if (declared > maxBytes) return fail(c, 413, 'too_large', { max_bytes: maxBytes });
  const t = now();
  // Rate limits count publishes (versions are pruned to the last few, so they cannot be counted).
  const pub = await c.env.DB.prepare('SELECT count(*) AS n FROM publishes WHERE dashboard_id = ?1 AND at > ?2')
    .bind(id, t - 3600_000)
    .first<{ n: number }>();
  const pubDay = await c.env.DB.prepare('SELECT count(*) AS n FROM publishes WHERE dashboard_id = ?1 AND at > ?2')
    .bind(id, t - 86400_000)
    .first<{ n: number }>();
  if ((pub?.n ?? 0) >= LIMITS.perHour || (pubDay?.n ?? 0) >= LIMITS.perDay) {
    c.header('retry-after', String(pub && pub.n >= LIMITS.perHour ? 3600 : 86400));
    return fail(c, 429, 'rate_limited', { per_hour: LIMITS.perHour, per_day: LIMITS.perDay });
  }

  // The Worker never parses a snapshot (the free plan allows ~10 ms of CPU): it checks the size
  // and the gzip signature, stores the bytes, and the viewer parses them in the browser.
  const bytes = new Uint8Array(await c.req.arrayBuffer());
  if (bytes.byteLength > maxBytes) return fail(c, 413, 'too_large', { max_bytes: maxBytes });
  if (bytes.byteLength < 18 || bytes[0] !== 0x1f || bytes[1] !== 0x8b) return fail(c, 400, 'body must be a gzip-compressed snapshot');

  const generated = (c.req.header('x-tenun-generated') ?? '').slice(0, 40) || new Date(t).toISOString();
  const source = (c.req.header('x-tenun-source') ?? '').slice(0, 60);
  const everyRaw = c.req.header('x-tenun-every');
  const every = everyRaw ? Math.max(LIMITS.minEveryMinutes, Math.min(10080, Number(everyRaw) || 0)) : null;
  const title = c.req.header('x-tenun-title');
  const version = dash.version + 1;

  await c.env.SNAPSHOTS.put(snapKey(id, version), bytes, {
    httpMetadata: { contentType: 'application/json', contentEncoding: 'gzip' },
  });
  const stmts = [
    c.env.DB.prepare('INSERT INTO versions (dashboard_id, version, bytes, generated_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5)').bind(
      id,
      version,
      bytes.byteLength,
      generated,
      t,
    ),
    c.env.DB.prepare(
      'UPDATE dashboards SET version = ?1, generated_at = ?2, source = ?3, every_minutes = ?4, bytes = ?5, updated_at = ?6, title = coalesce(?7, title), pages = ?8 WHERE id = ?9',
    ).bind(version, generated, source, every, bytes.byteLength, t, title ? decodeURIComponent(title).slice(0, 120) : null, pages, id),
    c.env.DB.prepare('INSERT INTO publishes (dashboard_id, at) VALUES (?1, ?2)').bind(id, t),
    c.env.DB.prepare('DELETE FROM publishes WHERE at < ?1').bind(t - 86400_000),
  ];
  await c.env.DB.batch(stmts);

  // Keep the last few versions.
  const { results: old } = await c.env.DB.prepare('SELECT version FROM versions WHERE dashboard_id = ?1 ORDER BY version DESC LIMIT -1 OFFSET ?2')
    .bind(id, LIMITS.keepVersions)
    .all<{ version: number }>();
  if (old.length) {
    await c.env.SNAPSHOTS.delete(old.map((r) => snapKey(id, r.version)));
    await c.env.DB.prepare(`DELETE FROM versions WHERE dashboard_id = ?1 AND version <= ?2`).bind(id, old[0].version).run();
  }
  return json(c, 200, { version, url: urlOf(new URL(c.req.url).origin, kind, id) });
}

// ---- the hosted dashboard's data -------------------------------------------

/** Can the current visitor see this dashboard? */
export async function canView(c: Context<HonoEnv>, id: string, kind: Kind = 'dashboard'): Promise<{ row: DashRow | null; allowed: boolean; needsSignIn: boolean }> {
  const row = await c.env.DB.prepare(
    'SELECT id, account_id, title, access, source, every_minutes, version, generated_at, bytes, kind, pages FROM dashboards WHERE id = ?1 AND kind = ?2',
  )
    .bind(id, kind)
    .first<DashRow & { account_id: string }>();
  if (!row) return { row: null, allowed: false, needsSignIn: false };
  if (row.access === 'link') return { row, allowed: true, needsSignIn: false };
  const me = c.get('account');
  if (!me) return { row, allowed: false, needsSignIn: true };
  if (me.id === row.account_id) return { row, allowed: true, needsSignIn: false };
  const inv = await c.env.DB.prepare('SELECT 1 AS ok FROM invites WHERE dashboard_id = ?1 AND email = ?2').bind(id, me.email.toLowerCase()).first();
  return { row, allowed: !!inv, needsSignIn: false };
}

for (const [seg, kind] of [['d', 'dashboard'], ['s', 'site']] as const) api.get(`/${seg}/:id/snapshot`, (c) => snapshotOf(c, kind));

async function snapshotOf(c: Context<HonoEnv>, kind: Kind) {
  const { row, allowed } = await canView(c, c.req.param('id') ?? '', kind);
  if (!row || !allowed || !row.version) return fail(c, 404, 'not_found');
  const obj = await c.env.SNAPSHOTS.get(snapKey(row.id, row.version));
  if (!obj) return fail(c, 404, 'not_found');
  return new Response(obj.body, {
    headers: {
      'content-type': 'application/json',
      'content-encoding': 'gzip',
      'cache-control': row.access === 'link' ? 'public, max-age=300' : 'private, max-age=60',
      'x-content-type-options': 'nosniff',
    },
    encodeBody: 'manual',
  });
}
