// End-to-end tests of tenun.cloud in Miniflare (workerd + local D1 and R2).
import { readdirSync, readFileSync } from 'node:fs';
import { gzipSync, gunzipSync } from 'node:zlib';
import { Miniflare } from 'miniflare';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

let mf: Miniflare;
const ORIGIN = 'https://tenun.cloud';

/** Emails the Worker hands to its `MAIL_OUTBOX` stand-in, for each instance. */
type Sent = { to: string; subject: string; text: string; html: string; headers: Record<string, string> };

async function boot(vars: Record<string, string>, outbox: Sent[] = []) {
  const m = new Miniflare({
    modules: true,
    scriptPath: '.build/worker.js',
    compatibilityDate: '2026-08-01',
    d1Databases: { DB: 'db' },
    r2Buckets: ['SNAPSHOTS'],
    bindings: { SESSION_SECRET: 'test-secret-that-is-long-enough', ...vars },
    serviceBindings: {
      ASSETS: async () => new Response('missing', { status: 404 }),
      MAIL_OUTBOX: async (r: Request) => {
        outbox.push((await r.json()) as Sent);
        return new Response('ok');
      },
    },
  });
  const db = await m.getD1Database('DB');
  // Every migration, in order.
  const sql = ['0001_init.sql', '0002_sites.sql', '0003_v5.sql']
    .map((f) => readFileSync(`migrations/${f}`, 'utf8'))
    .join('\n')
    .split('\n')
    .map((l) => l.replace(/--.*$/, ''))
    .join('\n')
    .split(';')
    .map((s) => s.trim())
    .filter(Boolean);
  await db.batch(sql.map((s) => db.prepare(s)));
  return m;
}

const req = (path: string, init: RequestInit & { cookie?: string } = {}) => {
  const headers = new Headers(init.headers);
  if (init.cookie) headers.set('cookie', init.cookie);
  return mf.dispatchFetch(`${ORIGIN}${path}`, { ...init, headers, redirect: 'manual' } as never);
};

async function signIn(email: string): Promise<string> {
  const r = await req(`/auth/dev?email=${encodeURIComponent(email)}&return=/account`);
  expect(r.status).toBe(302);
  const set = r.headers.get('set-cookie') ?? '';
  const m = /tn_s=([^;]+)/.exec(set);
  expect(m).toBeTruthy();
  return `tn_s=${m![1]}`;
}

/** Device sign-in, end to end: returns a publish token for `email`. */
async function connect(email: string): Promise<string> {
  const d = (await (await req('/api/v1/devices', { method: 'POST', body: JSON.stringify({ label: 'Test-PC' }) })).json()) as {
    device_code: string;
    user_code: string;
  };
  const pending = await req('/api/v1/devices/token', { method: 'POST', body: JSON.stringify({ device_code: d.device_code }) });
  expect(((await pending.json()) as { error: string }).error).toBe('authorization_pending');
  const cookie = await signIn(email);
  const page = await req(`/connect?code=${d.user_code}`, { cookie });
  expect(await page.text()).toContain('Test-PC');
  const ok = await req('/connect', {
    method: 'POST',
    cookie,
    headers: { origin: ORIGIN, 'content-type': 'application/x-www-form-urlencoded' },
    body: `code=${d.user_code}&ok=yes`,
  });
  expect(ok.status).toBe(200);
  const tok = (await (await req('/api/v1/devices/token', { method: 'POST', body: JSON.stringify({ device_code: d.device_code }) })).json()) as {
    token: string;
    email: string;
  };
  expect(tok.email).toBe(email);
  // Handed over once only.
  const again = await req('/api/v1/devices/token', { method: 'POST', body: JSON.stringify({ device_code: d.device_code }) });
  expect(again.status).toBe(400);
  return tok.token;
}

const auth = (t: string) => ({ authorization: `Bearer ${t}` });
const snapshot = (n: number) => gzipSync(JSON.stringify({ version: 2, title: 'Coffee', tiles: [], base: {}, cross: {}, filters: [], n }));

async function publish(token: string, id: string, n: number) {
  return req(`/api/v1/dashboards/${id}/snapshot`, {
    method: 'PUT',
    headers: { ...auth(token), 'content-type': 'application/gzip', 'x-tenun-source': 'Test-PC', 'x-tenun-every': '60', 'x-tenun-generated': new Date().toISOString() },
    body: snapshot(n),
  });
}

async function objects(prefix: string) {
  const bucket = await mf.getR2Bucket('SNAPSHOTS');
  return (await bucket.list({ prefix })).objects.map((o) => o.key);
}

/** What the instance with email updates off was asked to send: it must stay empty. */
const offOutbox: Sent[] = [];

beforeAll(async () => {
  mf = await boot({ SIGNIN: 'dev', DEV_LOGIN: '1' }, offOutbox);
});
afterAll(async () => {
  await mf.dispose();
});

describe('tenun.cloud', () => {
  it('answers health with security headers', async () => {
    const r = await req('/api/v1/health');
    expect(r.status).toBe(200);
    expect(r.headers.get('content-security-policy')).toContain("script-src 'self'");
    // A site holds up to 10 pages (build 14).
    expect(((await r.json()) as { limits: { sitePages: number } }).limits.sitePages).toBe(10);
  });

  it('publishes, serves, prunes, rate-limits and deletes', async () => {
    const token = await connect('owner@example.com');
    const created = await req('/api/v1/dashboards', { method: 'POST', headers: auth(token), body: JSON.stringify({ title: 'Coffee' }) });
    expect(created.status).toBe(201);
    const { id } = (await created.json()) as { id: string };
    expect(id).toMatch(/^[A-Za-z0-9]{22}$/);

    const bad = await req(`/api/v1/dashboards/${id}/snapshot`, { method: 'PUT', headers: auth(token), body: '{"not":"gzip"}' });
    expect(bad.status).toBe(400);

    for (let n = 1; n <= 6; n++) expect((await publish(token, id, n)).status).toBe(200);
    const limited = await publish(token, id, 7);
    expect(limited.status).toBe(429);
    expect(limited.headers.get('retry-after')).toBe('3600');

    // Only the last five versions are kept.
    expect((await objects(`snap/${id}/`)).sort()).toEqual([2, 3, 4, 5, 6].map((v) => `snap/${id}/${v}.json.gz`).sort());

    const shell = await req(`/d/${id}`);
    expect(shell.status).toBe(200);
    const html = await shell.text();
    expect(html).toContain(`/api/v1/d/${id}/snapshot?v=6`);
    expect(html).toContain('content="Test-PC"');
    expect(html).toContain('noindex');

    const data = await req(`/api/v1/d/${id}/snapshot?v=6`);
    expect(data.status).toBe(200);
    const raw = Buffer.from(await data.arrayBuffer());
    const text = raw[0] === 0x1f ? gunzipSync(raw).toString() : raw.toString();
    expect(JSON.parse(text).n).toBe(6);

    const del = await req(`/api/v1/dashboards/${id}`, { method: 'DELETE', headers: auth(token) });
    expect(del.status).toBe(200);
    expect(await objects(`snap/${id}/`)).toEqual([]);
    expect((await req(`/d/${id}`)).status).toBe(404);
  });

  it('enforces the dashboard limit and ownership', async () => {
    const a = await connect('limit@example.com');
    for (let i = 0; i < 3; i++) expect((await req('/api/v1/dashboards', { method: 'POST', headers: auth(a), body: '{}' })).status).toBe(201);
    expect((await req('/api/v1/dashboards', { method: 'POST', headers: auth(a), body: '{}' })).status).toBe(403);

    const b = await connect('other@example.com');
    const me = (await (await req('/api/v1/me', { headers: auth(a) })).json()) as { dashboards: { id: string }[] };
    const victim = me.dashboards[0].id;
    expect((await publish(b, victim, 1)).status).toBe(404);
    expect((await req(`/api/v1/dashboards/${victim}`, { method: 'DELETE', headers: auth(b) })).status).toBe(404);
    expect((await req('/api/v1/me', { headers: { authorization: 'Bearer tnn_' + 'x'.repeat(40) } })).status).toBe(401);
  });

  it('shows invited dashboards only to invited people', async () => {
    const owner = await connect('boss@example.com');
    const { id } = (await (await req('/api/v1/dashboards', { method: 'POST', headers: auth(owner), body: '{}' })).json()) as { id: string };
    expect((await publish(owner, id, 1)).status).toBe(200);
    const patch = await req(`/api/v1/dashboards/${id}`, {
      method: 'PATCH',
      headers: auth(owner),
      body: JSON.stringify({ access: 'invited', invites: ['Guest@Example.com'] }),
    });
    expect(patch.status).toBe(200);

    expect((await req(`/d/${id}`)).status).toBe(302); // sign in first
    expect((await req(`/api/v1/d/${id}/snapshot`)).status).toBe(404);
    const stranger = await signIn('stranger@example.com');
    expect((await req(`/d/${id}`, { cookie: stranger })).status).toBe(403);
    expect((await req(`/api/v1/d/${id}/snapshot`, { cookie: stranger })).status).toBe(404);
    const guest = await signIn('guest@example.com');
    expect((await req(`/d/${id}`, { cookie: guest })).status).toBe(200);
    expect((await req(`/api/v1/d/${id}/snapshot`, { cookie: guest })).status).toBe(200);
  });

  it('publishes a site: its own link, up to 10 pages, a bigger size, one item toward the limit, the same access', async () => {
    const token = await connect('site@example.com');
    const site = (b: Record<string, unknown>) => gzipSync(JSON.stringify({ version: 2, kind: 'site', title: 'Coffee', order: ['d1', 'd2'], pages: {}, drill: {}, ...b }));
    const put = (id: string, pages: string, body: Uint8Array, extra: Record<string, string> = {}) =>
      req(`/api/v1/sites/${id}/snapshot`, {
        method: 'PUT',
        headers: { ...auth(token), 'content-type': 'application/gzip', 'x-tenun-pages': pages, 'x-tenun-generated': new Date().toISOString(), ...extra },
        body,
      });
    const created = await req('/api/v1/sites', { method: 'POST', headers: auth(token), body: JSON.stringify({ title: 'Coffee site' }) });
    expect(created.status).toBe(201);
    const { id, url } = (await created.json()) as { id: string; url: string };
    expect(url).toBe(`${ORIGIN}/s/${id}`);
    // Eleven pages, or none said, are refused; ten pass.
    expect((await put(id, '11', site({}))).status).toBe(413);
    expect((await put(id, '', site({}))).status).toBe(413);
    const ok = await put(id, '10', site({ n: 1 }));
    expect(ok.status).toBe(200);
    expect(((await ok.json()) as { url: string }).url).toBe(`${ORIGIN}/s/${id}`);
    // A site may be bigger than a dashboard (10 MB), up to 25 MB. The Worker checks the size and the gzip mark only.
    const blob = (mb: number) => {
      const b = new Uint8Array(mb * 1024 * 1024);
      b[0] = 0x1f;
      b[1] = 0x8b;
      return b;
    };
    expect((await put(id, '2', blob(12))).status).toBe(200);
    expect((await put(id, '2', blob(26))).status).toBe(413);
    expect((await put(id, '2', site({ n: 1 }))).status).toBe(200);
    // The page: the viewer reads the site's snapshot; a page of the site goes to the site at that page.
    const shell = await req(`/s/${id}`);
    expect(shell.status).toBe(200);
    expect(await shell.text()).toContain(`/api/v1/s/${id}/snapshot?v=3`);
    const page = await req(`/s/${id}/d2`);
    expect(page.status).toBe(302);
    expect(page.headers.get('location')).toBe(`/s/${id}#/p/d2`);
    const data = await req(`/api/v1/s/${id}/snapshot?v=3`);
    const raw = Buffer.from(await data.arrayBuffer());
    expect(JSON.parse((raw[0] === 0x1f ? gunzipSync(raw) : raw).toString()).n).toBe(1);
    // A site is not a dashboard: the other link and the other API do not reach it.
    expect((await req(`/d/${id}`)).status).toBe(404);
    expect((await req(`/api/v1/dashboards/${id}/snapshot`, { method: 'PUT', headers: { ...auth(token), 'content-type': 'application/gzip' }, body: site({}) })).status).toBe(404);
    // The account lists it with its pages; /me gives its link.
    const me = (await (await req('/api/v1/me', { headers: auth(token) })).json()) as { dashboards: { id: string; kind: string; pages: number; url: string }[]; limits: { sitePages: number } };
    expect(me.dashboards[0]).toMatchObject({ id, kind: 'site', pages: 2, url: `${ORIGIN}/s/${id}` });
    expect(me.limits.sitePages).toBe(10);
    const cookie = await signIn('site@example.com');
    const account = await (await req('/account', { cookie })).text();
    expect(account).toContain(`href="/s/${id}"`);
    expect(account).toContain('Site of 2 pages');
    // One item toward the free plan: two dashboards more, then the next is refused.
    for (let i = 0; i < 2; i++) expect((await req('/api/v1/dashboards', { method: 'POST', headers: auth(token), body: '{}' })).status).toBe(201);
    expect((await req('/api/v1/sites', { method: 'POST', headers: auth(token), body: '{}' })).status).toBe(403);
    // Invited only, as a dashboard.
    expect((await req(`/api/v1/sites/${id}`, { method: 'PATCH', headers: auth(token), body: JSON.stringify({ access: 'invited', invites: ['friend@example.com'] }) })).status).toBe(200);
    expect((await req(`/s/${id}`)).status).toBe(302);
    expect((await req(`/s/${id}`, { cookie: await signIn('nobody@example.com') })).status).toBe(403);
    expect((await req(`/s/${id}`, { cookie: await signIn('friend@example.com') })).status).toBe(200);
    // Versions are pruned as a dashboard's, and a delete takes them all.
    for (let n = 4; n <= 6; n++) expect((await put(id, '2', site({ n }))).status).toBe(200);
    expect((await objects(`snap/${id}/`)).sort()).toEqual([2, 3, 4, 5, 6].map((v) => `snap/${id}/${v}.json.gz`).sort());
    expect((await req(`/api/v1/sites/${id}`, { method: 'DELETE', headers: auth(token) })).status).toBe(200);
    expect(await objects(`snap/${id}/`)).toEqual([]);
    expect((await req(`/s/${id}`)).status).toBe(404);
  });

  it('opens a password dashboard only with its right password, for that dashboard only, until the password changes', async () => {
    const owner = await connect('pw@example.com');
    const make = async (title: string) => {
      const { id } = (await (await req('/api/v1/dashboards', { method: 'POST', headers: auth(owner), body: JSON.stringify({ title }) })).json()) as { id: string };
      expect((await publish(owner, id, 1)).status).toBe(200);
      return id;
    };
    const a = await make('Secret plan');
    const b = await make('Other plan');
    const patch = (id: string, body: Record<string, unknown>) =>
      req(`/api/v1/dashboards/${id}`, { method: 'PATCH', headers: auth(owner), body: JSON.stringify(body) });
    // A password item needs a password of 4 to 200 characters.
    expect((await patch(a, { access: 'password' })).status).toBe(400);
    expect((await patch(a, { access: 'password', password: 'abc' })).status).toBe(400);
    expect((await patch(a, { access: 'password', password: 'x'.repeat(201) })).status).toBe(400);
    expect((await patch(a, { access: 'password', password: 'open sesame' })).status).toBe(200);
    expect((await patch(b, { access: 'password', password: 'open sesame' })).status).toBe(200);
    // The owner sees it as a password item; no part of the password leaves the server.
    const me = await (await req('/api/v1/me', { headers: auth(owner) })).text();
    expect(JSON.parse(me).dashboards[0]).toMatchObject({ id: a, access: 'password', embed: 0 });
    expect(me).not.toContain('pw_');
    // Without the password: a page with no data and no title, and no snapshot.
    const locked = await req(`/d/${a}`);
    expect(locked.status).toBe(401);
    expect(locked.headers.get('cache-control')).toBe('private, no-store');
    const html = await locked.text();
    expect(html).toContain('This dashboard has a password');
    expect(html).not.toContain('Secret plan');
    expect(html).not.toContain('snapshot');
    expect((await req(`/api/v1/d/${a}/snapshot`)).status).toBe(404);
    const unlock = (id: string, password: string, headers: Record<string, string> = { origin: ORIGIN }) =>
      req(`/d/${id}/unlock`, {
        method: 'POST',
        headers: { 'content-type': 'application/x-www-form-urlencoded', 'cf-connecting-ip': '198.51.100.1', ...headers },
        body: `password=${encodeURIComponent(password)}`,
      });
    // Only from its own page; a wrong one says so.
    expect((await unlock(a, 'open sesame', {})).status).toBe(403);
    const wrong = await unlock(a, 'open says me');
    expect(wrong.status).toBe(401);
    expect(await wrong.text()).toContain('That password is not right.');
    expect(wrong.headers.get('set-cookie')).toBeNull();
    // The right one: a cookie for this dashboard alone, then back to it.
    const right = await unlock(a, 'open sesame');
    expect(right.status).toBe(303);
    expect(right.headers.get('location')).toBe(`/d/${a}`);
    const set = right.headers.get('set-cookie') ?? '';
    expect(set).toMatch(new RegExp(`^tn_pw_${a}=1\\.[A-Za-z0-9_-]+; Path=/; Max-Age=604800; HttpOnly; Secure; SameSite=Lax$`));
    const cookie = set.split(';')[0];
    expect((await req(`/d/${a}`, { cookie })).status).toBe(200);
    const snap = await req(`/api/v1/d/${a}/snapshot`, { cookie });
    expect(snap.status).toBe(200);
    // Never in a shared cache.
    expect(snap.headers.get('cache-control')).toBe('private, max-age=60');
    // The same password on another dashboard: its cookie does not open that one, under either name.
    expect((await req(`/d/${b}`, { cookie })).status).toBe(401);
    expect((await req(`/d/${b}`, { cookie: cookie.replace(a, b) })).status).toBe(401);
    // Its owner needs no password.
    expect((await req(`/d/${a}`, { cookie: await signIn('pw@example.com') })).status).toBe(200);
    // Saying "password" again keeps it; a new password closes the old cookies.
    expect((await patch(a, { access: 'password', title: 'Secret plan' })).status).toBe(200);
    expect((await req(`/d/${a}`, { cookie })).status).toBe(200);
    expect((await patch(a, { access: 'password', password: 'new words' })).status).toBe(200);
    expect((await req(`/d/${a}`, { cookie })).status).toBe(401);
    // An old cookie with its number changed to the new password's does not open it either: the number is signed.
    expect((await req(`/d/${a}`, { cookie: cookie.replace('=1.', '=2.') })).status).toBe(401);
    expect((await unlock(a, 'open sesame')).status).toBe(401);
    expect((await unlock(a, 'new words')).headers.get('set-cookie')).toContain(`tn_pw_${a}=2.`);
    // Back to "anyone with the link": open, and the password is gone for good.
    expect((await patch(a, { access: 'link' })).status).toBe(200);
    expect((await req(`/d/${a}`)).status).toBe(200);
    expect((await unlock(a, 'new words')).status).toBe(303);
    expect((await patch(a, { access: 'password' })).status).toBe(400);
    // A site has the same lock, at its own link.
    const { id: s } = (await (await req('/api/v1/sites', { method: 'POST', headers: auth(owner), body: '{}' })).json()) as { id: string };
    expect((await req(`/api/v1/sites/${s}`, { method: 'PATCH', headers: auth(owner), body: JSON.stringify({ access: 'password', password: 'site words' }) })).status).toBe(200);
    expect(await (await req(`/s/${s}`)).text()).toContain('This site has a password');
    const site = await req(`/s/${s}/unlock`, {
      method: 'POST',
      headers: { origin: ORIGIN, 'content-type': 'application/x-www-form-urlencoded', 'cf-connecting-ip': '198.51.100.2' },
      body: 'password=site+words',
    });
    expect(site.headers.get('location')).toBe(`/s/${s}`);
  });

  it('stops an address after five wrong passwords in a minute, and keeps no address', async () => {
    const owner = await connect('tries@example.com');
    const { id } = (await (await req('/api/v1/dashboards', { method: 'POST', headers: auth(owner), body: '{}' })).json()) as { id: string };
    await publish(owner, id, 1);
    await req(`/api/v1/dashboards/${id}`, { method: 'PATCH', headers: auth(owner), body: JSON.stringify({ access: 'password', password: 'right one' }) });
    const unlock = (password: string, ip: string) =>
      req(`/d/${id}/unlock`, {
        method: 'POST',
        headers: { origin: ORIGIN, 'content-type': 'application/x-www-form-urlencoded', 'cf-connecting-ip': ip },
        body: `password=${encodeURIComponent(password)}`,
      });
    for (let i = 0; i < 5; i++) expect((await unlock(`wrong ${i}`, '203.0.113.9')).status).toBe(401);
    // The sixth waits, even when it is right.
    const wait = await unlock('right one', '203.0.113.9');
    expect(wait.status).toBe(429);
    expect(await wait.text()).toContain('Wait a minute');
    expect((await unlock('right one', '203.0.113.10')).status).toBe(303);
    const db = await mf.getD1Database('DB');
    const { results } = await db.prepare('SELECT who FROM unlock_tries').all<{ who: string }>();
    expect(results.length).toBeGreaterThanOrEqual(5);
    expect(JSON.stringify(results)).not.toContain('203.0.113');
  });

  it('lets other sites frame an item only when its owner allows it, and never one for invited people', async () => {
    const owner = await connect('embed@example.com');
    const { id } = (await (await req('/api/v1/dashboards', { method: 'POST', headers: auth(owner), body: '{}' })).json()) as { id: string };
    await publish(owner, id, 1);
    const patch = (body: Record<string, unknown>) => req(`/api/v1/dashboards/${id}`, { method: 'PATCH', headers: auth(owner), body: JSON.stringify(body) });
    const frame = async (path: string, cookie?: string) => (await req(path, { cookie })).headers.get('content-security-policy') ?? '';
    expect(await frame(`/d/${id}`)).toContain("frame-ancestors 'none'");
    expect((await patch({ embed: true })).status).toBe(200);
    expect(await frame(`/d/${id}`)).toContain('frame-ancestors *');
    // Only that item: every other page still refuses a frame.
    expect(await frame('/api/v1/health')).toContain("frame-ancestors 'none'");
    expect(await frame('/account')).toContain("frame-ancestors 'none'");
    // Invited people sign in with Google, which does not work inside another site.
    const refused = await patch({ access: 'invited', invites: ['a@example.com'] });
    expect(refused.status).toBe(400);
    expect(((await refused.json()) as { error: string }).error).toBe('embed_invited');
    expect((await patch({ access: 'invited', invites: ['a@example.com'], embed: false })).status).toBe(200);
    expect((await patch({ embed: true })).status).toBe(400);
    // Even when the database says embed, an item for invited people is never framed.
    const db = await mf.getD1Database('DB');
    await db.prepare('UPDATE dashboards SET embed = 1 WHERE id = ?1').bind(id).run();
    expect(await frame(`/d/${id}`)).toContain("frame-ancestors 'none'");
    // A password item inside another site: its cookie is sent there, kept to that site.
    expect((await patch({ access: 'password', password: 'framed', embed: true })).status).toBe(200);
    expect(await frame(`/d/${id}`)).toContain('frame-ancestors *');
    const right = await req(`/d/${id}/unlock`, {
      method: 'POST',
      headers: { origin: ORIGIN, 'content-type': 'application/x-www-form-urlencoded', 'cf-connecting-ip': '198.51.100.3' },
      body: 'password=framed',
    });
    expect(right.headers.get('set-cookie')).toMatch(/; HttpOnly; Secure; SameSite=None; Partitioned$/);
    const account = await (await req('/account', { cookie: await signIn('embed@example.com') })).text();
    expect(account).toContain('Anyone with the password · can be embedded');
  });

  it('applies every migration file the folder holds', () => {
    expect(readdirSync('migrations').sort()).toEqual(['0001_init.sql', '0002_sites.sql', '0003_v5.sql']);
  });

  it('refuses page forms from other sites', async () => {
    const cookie = await signIn('csrf@example.com');
    const r = await req('/account/delete', { method: 'POST', cookie, headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: 'sure=yes' });
    expect(r.status).toBe(403);
    const evil = await req('/account/delete', {
      method: 'POST',
      cookie,
      headers: { origin: 'https://evil.example', 'content-type': 'application/x-www-form-urlencoded' },
      body: 'sure=yes',
    });
    expect(evil.status).toBe(403);
  });

  it('deletes an account and everything in it', async () => {
    const token = await connect('leaving@example.com');
    const { id } = (await (await req('/api/v1/dashboards', { method: 'POST', headers: auth(token), body: '{}' })).json()) as { id: string };
    await publish(token, id, 1);
    const cookie = await signIn('leaving@example.com');
    const r = await req('/account/delete', {
      method: 'POST',
      cookie,
      headers: { origin: ORIGIN, 'content-type': 'application/x-www-form-urlencoded' },
      body: 'sure=yes',
    });
    expect(r.status).toBe(302);
    expect(await objects(`snap/${id}/`)).toEqual([]);
    expect((await req('/api/v1/me', { headers: auth(token) })).status).toBe(401);
  });
});

/** A digest as the app sends it beside a snapshot: base64 (URL alphabet) of JSON. */
const digestHeader = (d: unknown) => Buffer.from(JSON.stringify(d)).toString('base64url');
const DIGEST = {
  title: 'Shops',
  items: [
    { name: 'Amount', value: '18.4K', change: '▲ 6.3% vs Sep 1–14', period: 'Oct 1–14, 2026, so far' },
    { name: 'Number of orders', value: '475' },
  ],
};

async function publishWith(token: string, id: string, n: number, digest: unknown = DIGEST) {
  return req(`/api/v1/dashboards/${id}/snapshot`, {
    method: 'PUT',
    headers: { ...auth(token), 'content-type': 'application/gzip', 'x-tenun-generated': '2026-10-14T09:00:00Z', 'x-tenun-digest': digestHeader(digest) },
    body: snapshot(n),
  });
}

async function invitedItem(owner: string, updates: string, invites: string[]) {
  const token = await connect(owner);
  const { id } = (await (await req('/api/v1/dashboards', { method: 'POST', headers: auth(token), body: JSON.stringify({ title: 'Shops' }) })).json()) as { id: string };
  const patch = await req(`/api/v1/dashboards/${id}`, { method: 'PATCH', headers: auth(token), body: JSON.stringify({ access: 'invited', invites, updates }) });
  expect(patch.status).toBe(200);
  return { token, id };
}

const optIn = (id: string, cookie: string, on?: boolean) =>
  on === undefined
    ? req(`/api/v1/d/${id}/updates`, { cookie })
    : req(`/api/v1/d/${id}/updates`, { method: 'POST', cookie, headers: { origin: ORIGIN }, body: JSON.stringify({ on }) });

describe('email updates (v5 build 13)', () => {
  const outbox: Sent[] = [];
  let off: Miniflare;
  beforeAll(async () => {
    off = mf;
    mf = await boot({ SIGNIN: 'dev', DEV_LOGIN: '1', EMAIL_UPDATES: 'on' }, outbox);
  });
  afterAll(async () => {
    await mf.dispose();
    mf = off;
  });

  it('sends the new numbers after each update, at most once a day, to the owner and the invited people who asked', async () => {
    const { token, id } = await invitedItem('mail-owner@example.com', 'each', ['reader@example.com']);
    expect((await publishWith(token, id, 1)).status).toBe(200);
    expect(outbox.map((m) => m.to)).toEqual(['mail-owner@example.com']);
    const m = outbox[0];
    expect(m.subject).toBe('Shops: updated');
    // The numbers are the digest's, as the app wrote them, and the link opens the page.
    expect(m.text).toContain('Amount: 18.4K (Oct 1–14, 2026, so far), ▲ 6.3% vs Sep 1–14');
    expect(m.text).toContain('Number of orders: 475');
    expect(m.text).toContain(`Open it: ${ORIGIN}/d/${id}`);
    expect(m.html).toContain('18.4K');
    expect(m.headers['List-Unsubscribe']).toMatch(new RegExp(`^<${ORIGIN}/u/[A-Za-z0-9]{32}>$`));
    expect(m.headers['List-Unsubscribe-Post']).toBe('List-Unsubscribe=One-Click');

    // An invited reader asks on the page (from the page only); a stranger cannot.
    const reader = await signIn('reader@example.com');
    expect(((await (await optIn(id, reader)).json()) as { on: boolean }).on).toBe(false);
    expect((await req(`/api/v1/d/${id}/updates`, { method: 'POST', cookie: reader, body: '{"on":true}' })).status).toBe(403);
    expect(((await (await optIn(id, reader, true)).json()) as { on: boolean }).on).toBe(true);
    expect((await optIn(id, await signIn('stranger@example.com'), true)).status).toBe(404);
    // The page offers it to them.
    expect(await (await req(`/d/${id}`, { cookie: reader })).text()).toContain(`<meta name="tenun-updates" content="d/${id}">`);

    // The next update today: the owner had one today, the reader had none.
    await publishWith(token, id, 2);
    expect(outbox.map((m) => m.to)).toEqual(['mail-owner@example.com', 'reader@example.com']);
    await publishWith(token, id, 3);
    expect(outbox.length).toBe(2);
    // The reader switches it off again.
    expect(((await (await optIn(id, reader, false)).json()) as { on: boolean }).on).toBe(false);
  });

  it('stops in one click, and sends daily and weekly updates from the daily run', async () => {
    outbox.length = 0;
    const { token, id } = await invitedItem('daily-owner@example.com', 'daily', ['fan@example.com']);
    await publishWith(token, id, 1);
    // "Daily" sends nothing after an update.
    expect(outbox.length).toBe(0);
    expect(((await (await optIn(id, await signIn('fan@example.com'), true)).json()) as { on: boolean }).on).toBe(true);
    const worker = await mf.getWorker();
    const run = (day: string) => worker.scheduled({ scheduledTime: new Date(`${day}T03:17:00Z`), cron: '17 3 * * *' });
    await run('2026-10-20');
    expect(outbox.map((m) => m.to).sort()).toEqual(['daily-owner@example.com', 'fan@example.com']);
    // The owner stops them with the link of the email, as a mail app does (a POST), or by opening it.
    const link = outbox.find((m) => m.to === 'daily-owner@example.com')!.headers['List-Unsubscribe'].slice(1, -1);
    const path = new URL(link).pathname;
    expect((await req(path, { method: 'POST' })).status).toBe(200);
    const page = await req(path);
    expect(page.status).toBe(200);
    expect(await page.text()).toContain('no more email updates about “Shops”');
    expect((await req('/u/' + 'x'.repeat(32))).status).toBe(404);
    // Once a day at most: the same day again sends nothing; the next day only the fan gets one.
    await run('2026-10-20');
    expect(outbox.length).toBe(2);
    await run('2026-10-21');
    expect(outbox.slice(2).map((m) => m.to)).toEqual(['fan@example.com']);
    // Weekly: on Mondays only.
    await req(`/api/v1/dashboards/${id}`, { method: 'PATCH', headers: auth(token), body: JSON.stringify({ updates: 'weekly' }) });
    await run('2026-10-22');
    expect(outbox.length).toBe(3);
    await run('2026-10-26');
    expect(outbox.slice(3).map((m) => m.to)).toEqual(['fan@example.com']);
  });

  it('offers updates on invited pages only, to at most 10 people with the owner', async () => {
    const readers = Array.from({ length: 10 }, (_, i) => `r${i}@example.com`);
    const { token, id } = await invitedItem('many-owner@example.com', 'each', readers);
    await publishWith(token, id, 1);
    for (const r of readers.slice(0, 9)) expect((await optIn(id, await signIn(r), true)).status).toBe(200);
    const last = await optIn(id, await signIn(readers[9]), true);
    expect(last.status).toBe(403);
    expect(((await last.json()) as { error: string }).error).toBe('limit');
    // A page for anyone with the link offers nothing: there is no list of readers to ask.
    const { id: open } = (await (await req('/api/v1/dashboards', { method: 'POST', headers: auth(token), body: '{}' })).json()) as { id: string };
    await req(`/api/v1/dashboards/${open}`, { method: 'PATCH', headers: auth(token), body: JSON.stringify({ updates: 'each' }) });
    expect((await optIn(open, await signIn('r0@example.com'), true)).status).toBe(403);
    expect((await req(`/api/v1/dashboards/${open}`, { method: 'PATCH', headers: auth(token), body: JSON.stringify({ updates: 'hourly' }) })).status).toBe(400);
    expect(((await (await req('/api/v1/health')).json()) as { emailUpdates: boolean; limits: { recipients: number } }).limits.recipients).toBe(10);
  });
});

describe('email updates while they are off (the default)', () => {
  it('keeps the setting and the numbers, and sends and offers nothing', async () => {
    const { token, id } = await invitedItem('quiet-owner@example.com', 'each', ['quiet@example.com']);
    expect((await publishWith(token, id, 1)).status).toBe(200);
    expect(offOutbox).toEqual([]);
    expect(((await (await req('/api/v1/health')).json()) as { emailUpdates: boolean }).emailUpdates).toBe(false);
    const me = (await (await req('/api/v1/me', { headers: auth(token) })).json()) as { emailUpdates: boolean; dashboards: { updates: string }[] };
    expect(me.emailUpdates).toBe(false);
    expect(me.dashboards[0].updates).toBe('each');
    const reader = await signIn('quiet@example.com');
    expect((await optIn(id, reader, true)).status).toBe(404);
    expect(await (await req(`/d/${id}`, { cookie: reader })).text()).not.toContain('tenun-updates');
    // The digest is kept beside the snapshots, and goes with them.
    expect(await objects(`snap/${id}/`)).toContain(`snap/${id}/digest.json`);
    await (await mf.getWorker()).scheduled({ scheduledTime: new Date('2026-10-26T03:17:00Z'), cron: '17 3 * * *' });
    expect(offOutbox).toEqual([]);
    expect((await req(`/api/v1/dashboards/${id}`, { method: 'DELETE', headers: auth(token) })).status).toBe(200);
    expect(await objects(`snap/${id}/`)).toEqual([]);
  });
});

describe('sign-in closed (production default)', () => {
  it('shows "opens soon" and has no test sign-in', async () => {
    const closed = await boot({ SIGNIN: 'closed' });
    const r = await closed.dispatchFetch(`${ORIGIN}/auth/login`, { redirect: 'manual' } as never);
    expect(r.status).toBe(503);
    const dev = await closed.dispatchFetch(`${ORIGIN}/auth/dev?email=a@b.co`, { redirect: 'manual' } as never);
    expect(dev.status).toBe(404);
    // Even with DEV_LOGIN set, "open" without keys never falls back to the test sign-in.
    const open = await boot({ SIGNIN: 'open', DEV_LOGIN: '1' });
    expect((await open.dispatchFetch(`${ORIGIN}/auth/dev?email=a@b.co`, { redirect: 'manual' } as never)).status).toBe(404);
    await closed.dispose();
    await open.dispose();
  });
});
