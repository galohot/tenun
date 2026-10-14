// The publish API (Bearer tokens from device sign-in) and the hosted dashboards.
import { Hono } from 'hono';
import type { Context } from 'hono';
import type { HonoEnv } from './env';
import { LIMITS, type Kind } from './env';
import type { DashRow } from './pages';
import { now, rid, sha256, userCode, validEmail } from './util';
import { cookie } from './akun/token';
import { MAX_PASSWORD, MIN_PASSWORD, cookieName, cookieOpens, hashPassword } from './password';
import { UPDATES, addOwner, digestKey, emailOn, readDigest, sendUpdates } from './mail';
import { sameOrigin } from './auth';

/** The columns of an item as its owner and the pages read them. A password is access 'password' (v5 build 11). */
export const ROW_COLUMNS =
  "id, title, CASE WHEN pw_hash IS NOT NULL AND access = 'link' THEN 'password' ELSE access END AS access, source, every_minutes, version, generated_at, bytes, kind, pages, embed, updates";

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
  const { results } = await c.env.DB.prepare(`SELECT ${ROW_COLUMNS} FROM dashboards WHERE account_id = ?1 ORDER BY created_at`)
    .bind(who.account)
    .all<DashRow>();
  const origin = new URL(c.req.url).origin;
  return json(c, 200, {
    email: acc?.email,
    dashboards: results.map((d) => ({ ...d, url: urlOf(origin, d.kind, d.id) })),
    limits: LIMITS,
    // Email updates are kept as a setting either way; they are sent only when tenun.cloud turns them on (V5-D16).
    emailUpdates: emailOn(c.env),
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
  const b = await c.req
    .json<{ title?: string; access?: string; invites?: string[]; password?: string; embed?: boolean; updates?: string }>()
    .catch(() => null);
  if (!b) return fail(c, 400, 'invalid_json');
  const cur = await c.env.DB.prepare('SELECT access, pw_hash, embed FROM dashboards WHERE id = ?1')
    .bind(id)
    .first<{ access: string; pw_hash: string | null; embed: number }>();
  const stmts: D1PreparedStatement[] = [];
  if (b.title !== undefined) stmts.push(c.env.DB.prepare('UPDATE dashboards SET title = ?1 WHERE id = ?2').bind(String(b.title).slice(0, 120), id));
  if (b.access !== undefined && b.access !== 'link' && b.access !== 'invited' && b.access !== 'password') {
    return fail(c, 400, 'access must be link, password or invited');
  }
  // Embedding works for a link or a password; inside another site, a Google sign-in fails (V5-D14).
  const access = b.access ?? (cur?.pw_hash && cur.access === 'link' ? 'password' : (cur?.access ?? 'link'));
  const embed = b.embed ?? !!cur?.embed;
  if (embed && access === 'invited') {
    return fail(c, 400, 'embed_invited', { message: 'A dashboard for invited people cannot be embedded: their Google sign-in does not work inside another site.' });
  }
  if (b.embed !== undefined) stmts.push(c.env.DB.prepare('UPDATE dashboards SET embed = ?1 WHERE id = ?2').bind(b.embed ? 1 : 0, id));
  if (b.updates !== undefined) {
    if (!(UPDATES as readonly string[]).includes(b.updates)) return fail(c, 400, 'updates must be off, each, daily or weekly');
    stmts.push(c.env.DB.prepare('UPDATE dashboards SET updates = ?1 WHERE id = ?2').bind(b.updates, id));
  }
  if (b.access === 'password') {
    // A new password replaces the old one, and the cookies of the old one stop opening it. Without one, the
    // password set before stays; there must be one.
    if (b.password !== undefined) {
      const pw = String(b.password);
      // Counted in characters, as the app counts them (an emoji is one).
      const n = [...pw].length;
      if (n < MIN_PASSWORD || n > MAX_PASSWORD) {
        return fail(c, 400, 'password', { message: `A password has ${MIN_PASSWORD} to ${MAX_PASSWORD} characters.` });
      }
      const h = await hashPassword(pw);
      stmts.push(
        c.env.DB.prepare("UPDATE dashboards SET access = 'link', pw_hash = ?1, pw_salt = ?2, pw_iter = ?3, pw_version = pw_version + 1 WHERE id = ?4").bind(
          h.hash,
          h.salt,
          h.iter,
          id,
        ),
      );
    } else if (!cur?.pw_hash) {
      return fail(c, 400, 'password', { message: 'Set a password first.' });
    } else {
      stmts.push(c.env.DB.prepare("UPDATE dashboards SET access = 'link' WHERE id = ?1").bind(id));
    }
  } else if (b.access !== undefined) {
    stmts.push(c.env.DB.prepare('UPDATE dashboards SET access = ?1, pw_hash = NULL, pw_salt = NULL, pw_iter = NULL WHERE id = ?2').bind(b.access, id));
  }
  if (b.invites !== undefined) {
    const emails = [...new Set(b.invites.map((e) => String(e).trim().toLowerCase()))];
    if (emails.length > 50 || emails.some((e) => !validEmail(e))) return fail(c, 400, 'invites must be up to 50 valid emails');
    stmts.push(c.env.DB.prepare('DELETE FROM invites WHERE dashboard_id = ?1').bind(id));
    for (const e of emails) stmts.push(c.env.DB.prepare('INSERT INTO invites (dashboard_id, email) VALUES (?1, ?2)').bind(id, e));
  }
  if (stmts.length) await c.env.DB.batch(stmts);
  // Its owner gets the updates first (v5 build 13).
  if (b.updates !== undefined && b.updates !== 'off') {
    const acc = await c.env.DB.prepare('SELECT email FROM accounts WHERE id = ?1').bind(who.account).first<{ email: string }>();
    if (acc) await addOwner(c.env, id, acc.email);
  }
  return json(c, 200, { ok: true });
}

/** Deletes a dashboard: every stored version first, then the rows. */
export async function deleteDashboard(env: HonoEnv['Bindings'], id: string) {
  const { results } = await env.DB.prepare('SELECT version FROM versions WHERE dashboard_id = ?1').bind(id).all<{ version: number }>();
  if (results.length) await env.SNAPSHOTS.delete(results.map((r) => snapKey(id, r.version)));
  await env.SNAPSHOTS.delete(digestKey(id));
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
  // The numbers an email update holds, as the app wrote them (v5 build 13): the Worker never parses a snapshot.
  const digest = readDigest(c.req.header('x-tenun-digest'));
  if (digest) await c.env.SNAPSHOTS.put(digestKey(id), JSON.stringify(digest), { httpMetadata: { contentType: 'application/json' } });
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
  // "After each update": its recipients get the new numbers now, at most one email a day each. A failed send never
  // fails the publish.
  if (emailOn(c.env)) {
    const row = await c.env.DB.prepare('SELECT updates FROM dashboards WHERE id = ?1').bind(id).first<{ updates: string }>();
    if (row?.updates === 'each') await sendUpdates(c.env, id, 'each', new URL(c.req.url).origin).catch(() => 0);
  }
  return json(c, 200, { version, url: urlOf(new URL(c.req.url).origin, kind, id) });
}

// ---- email updates: invited people ask for them on the page (v5 build 13) --------

for (const [seg, kind] of [['d', 'dashboard'], ['s', 'site']] as const) {
  api.get(`/${seg}/:id/updates`, (c) => updatesFor(c, kind, null));
  api.post(`/${seg}/:id/updates`, async (c) => {
    if (!sameOrigin(c)) return fail(c, 403, 'forbidden');
    const b = await c.req.json<{ on?: boolean }>().catch(() => null);
    return updatesFor(c, kind, !!b?.on);
  });
}

/** Whether the signed-in reader gets email updates of an item; with `on`, switches them. Invited items only. */
async function updatesFor(c: Context<HonoEnv>, kind: Kind, on: boolean | null) {
  if (!emailOn(c.env)) return fail(c, 404, 'email_updates_off');
  const me = c.get('account');
  if (!me) return fail(c, 401, 'sign_in');
  const { row, allowed } = await canView(c, c.req.param('id') ?? '', kind);
  if (!row || !allowed) return fail(c, 404, 'not_found');
  if (row.access !== 'invited' || !row.updates || row.updates === 'off') {
    return fail(c, 403, 'not_offered', { message: 'This page does not offer email updates.' });
  }
  const email = me.email.toLowerCase();
  if (on === true) {
    const n = await c.env.DB.prepare('SELECT count(*) AS n FROM recipients WHERE dashboard_id = ?1 AND active = 1 AND email != ?2')
      .bind(row.id, email)
      .first<{ n: number }>();
    if ((n?.n ?? 0) >= LIMITS.recipients) {
      return fail(c, 403, 'limit', { message: `This page already sends updates to ${LIMITS.recipients} people, the most it can.` });
    }
    await c.env.DB.prepare(
      'INSERT INTO recipients (dashboard_id, email, token, created_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (dashboard_id, email) DO UPDATE SET active = 1',
    )
      .bind(row.id, email, rid(32), now())
      .run();
  } else if (on === false) {
    await c.env.DB.prepare('UPDATE recipients SET active = 0 WHERE dashboard_id = ?1 AND email = ?2').bind(row.id, email).run();
  }
  const r = await c.env.DB.prepare('SELECT active FROM recipients WHERE dashboard_id = ?1 AND email = ?2').bind(row.id, email).first<{ active: number }>();
  return json(c, 200, { on: r?.active === 1, updates: row.updates });
}

// ---- the hosted dashboard's data -------------------------------------------

/** Can the current visitor see this dashboard? A password one opens with the cookie a right password set (v5 build 11). */
export async function canView(
  c: Context<HonoEnv>,
  id: string,
  kind: Kind = 'dashboard',
): Promise<{ row: DashRow | null; allowed: boolean; needsSignIn: boolean; needsPassword: boolean }> {
  const row = await c.env.DB.prepare(`SELECT ${ROW_COLUMNS}, account_id, pw_version FROM dashboards WHERE id = ?1 AND kind = ?2`)
    .bind(id, kind)
    .first<DashRow & { account_id: string; pw_version: number }>();
  if (!row) return { row: null, allowed: false, needsSignIn: false, needsPassword: false };
  if (row.access === 'link') return { row, allowed: true, needsSignIn: false, needsPassword: false };
  const me = c.get('account');
  if (row.access === 'password') {
    // Its owner needs no password; anyone else needs the cookie of the current password.
    const open = (me && me.id === row.account_id) || (await cookieOpens(c.env, row.id, row.pw_version, cookie(c.req.raw, cookieName(row.id)) ?? undefined));
    return { row, allowed: !!open, needsSignIn: false, needsPassword: !open };
  }
  if (!me) return { row, allowed: false, needsSignIn: true, needsPassword: false };
  if (me.id === row.account_id) return { row, allowed: true, needsSignIn: false, needsPassword: false };
  const inv = await c.env.DB.prepare('SELECT 1 AS ok FROM invites WHERE dashboard_id = ?1 AND email = ?2').bind(id, me.email.toLowerCase()).first();
  return { row, allowed: !!inv, needsSignIn: false, needsPassword: false };
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
      // Only an open link may sit in a shared cache; a password or an invitation is per reader.
      'cache-control': row.access === 'link' ? 'public, max-age=300' : 'private, max-age=60',
      'x-content-type-options': 'nosniff',
    },
    encodeBody: 'manual',
  });
}
