// End-to-end tests of tenun.cloud in Miniflare (workerd + local D1 and R2).
import { readFileSync } from 'node:fs';
import { gzipSync, gunzipSync } from 'node:zlib';
import { Miniflare } from 'miniflare';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

let mf: Miniflare;
const ORIGIN = 'https://tenun.cloud';

async function boot(vars: Record<string, string>) {
  const m = new Miniflare({
    modules: true,
    scriptPath: '.build/worker.js',
    compatibilityDate: '2026-08-01',
    d1Databases: { DB: 'db' },
    r2Buckets: ['SNAPSHOTS'],
    bindings: { SESSION_SECRET: 'test-secret-that-is-long-enough', ...vars },
    serviceBindings: { ASSETS: async () => new Response('missing', { status: 404 }) },
  });
  const db = await m.getD1Database('DB');
  const sql = readFileSync('migrations/0001_init.sql', 'utf8')
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

beforeAll(async () => {
  mf = await boot({ SIGNIN: 'dev', DEV_LOGIN: '1' });
});
afterAll(async () => {
  await mf.dispose();
});

describe('tenun.cloud', () => {
  it('answers health with security headers', async () => {
    const r = await req('/api/v1/health');
    expect(r.status).toBe(200);
    expect(r.headers.get('content-security-policy')).toContain("script-src 'self'");
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
