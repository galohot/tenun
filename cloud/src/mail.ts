// Email updates (v5 build 13, V5-D16): the numbers of a published item, sent to its owner and to invited people who
// asked for them. Sending needs Cloudflare's Email Service, which needs the paid Workers plan: until the owner of
// tenun.cloud turns `EMAIL_UPDATES` on, nothing here sends anything.
//
// The Worker never parses a snapshot (the free plan allows ~10 ms of CPU a request). The app sends a small digest
// with each snapshot instead: the number tiles' names, values, changes and periods, already written in the
// workspace's format. An email holds that digest and the link, nothing else.
import type { HonoEnv } from './env';
import { esc, now, rid } from './util';

type Env = HonoEnv['Bindings'];

/** One number tile of the digest, as the app wrote it. */
export type DigestItem = { name: string; value: string; change?: string; period?: string };
export type Digest = { title?: string; items: DigestItem[] };

/** How often an item's readers get an email. */
export const UPDATES = ['off', 'each', 'daily', 'weekly'] as const;
export type Updates = (typeof UPDATES)[number];

/** The sender. The domain is set up for sending by the Email Service (SPF, DKIM and DMARC). */
export const FROM = { email: 'updates@tenun.cloud', name: 'Tenun' };
/** Where links in an email point when there is no request to read the address from (the daily cron). */
export const PUBLIC_ORIGIN = 'https://tenun.cloud';

export const emailOn = (env: Env) => env.EMAIL_UPDATES === 'on';

/** The R2 key of an item's digest, beside its snapshots. */
export const digestKey = (id: string) => `snap/${id}/digest.json`;

const MAX_DIGEST = 8192;
const MAX_ITEMS = 12;

/** Reads the `x-tenun-digest` header (base64 of JSON). Anything that is not a small, well-formed digest is dropped. */
export function readDigest(header: string | undefined): Digest | null {
  if (!header || header.length > MAX_DIGEST) return null;
  try {
    const text = new TextDecoder().decode(Uint8Array.from(atob(header.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0)));
    const d = JSON.parse(text) as Digest;
    if (!d || !Array.isArray(d.items)) return null;
    const s = (v: unknown, n: number) => (typeof v === 'string' ? v.slice(0, n) : undefined);
    return {
      title: s(d.title, 120),
      items: d.items.slice(0, MAX_ITEMS).flatMap((i) =>
        i && typeof i.name === 'string' && typeof i.value === 'string'
          ? [{ name: i.name.slice(0, 120), value: i.value.slice(0, 40), change: s(i.change, 80), period: s(i.period, 80) }]
          : [],
      ),
    };
  } catch {
    return null;
  }
}

/** The owner's address on the item's recipients, once updates are on. */
export async function addOwner(env: Env, id: string, email: string) {
  await env.DB.prepare('INSERT OR IGNORE INTO recipients (dashboard_id, email, token, created_at) VALUES (?1, ?2, ?3, ?4)')
    .bind(id, email.toLowerCase(), rid(32), now())
    .run();
}

export type Mail = {
  to: string;
  from: { email: string; name: string };
  subject: string;
  text: string;
  html: string;
  headers: Record<string, string>;
};

/** One email: the title, when it was updated, each number, the link, and a one-click unsubscribe. */
export function compose(item: { title: string; generated_at: string | null; url: string }, d: Digest | null, to: string, unsubscribe: string): Mail {
  const when = item.generated_at ? new Date(item.generated_at).toUTCString().replace(/:\d\d GMT$/, ' UTC') : '';
  const items = d?.items ?? [];
  const lines = items.map((i) => `${i.name}: ${i.value}${i.period ? ` (${i.period})` : ''}${i.change ? `, ${i.change}` : ''}`);
  const text = [
    item.title,
    when ? `Updated ${when}` : '',
    '',
    ...(lines.length ? lines : ['This page has no number tiles. Open it to see the update.']),
    '',
    `Open it: ${item.url}`,
    '',
    `You get this because you asked for updates of this page on tenun.cloud. Stop them: ${unsubscribe}`,
  ].join('\n');
  const rows = items
    .map(
      (i) =>
        `<tr><td style="padding:6px 12px 6px 0;color:#555">${esc(i.name)}${i.period ? `<br><span style="font-size:12px;color:#888">${esc(i.period)}</span>` : ''}</td>` +
        `<td style="padding:6px 0;font-size:20px;font-weight:600;text-align:right">${esc(i.value)}</td>` +
        `<td style="padding:6px 0 6px 12px;font-size:12px;color:#555">${esc(i.change ?? '')}</td></tr>`,
    )
    .join('');
  const html = `<!doctype html><html><body style="font-family:system-ui,-apple-system,Segoe UI,sans-serif;color:#1d1d1f;background:#fff;margin:0;padding:24px">
<h1 style="font-size:20px;margin:0 0 4px">${esc(item.title)}</h1>
${when ? `<p style="margin:0 0 16px;color:#555;font-size:13px">Updated ${esc(when)}</p>` : ''}
${rows ? `<table style="border-collapse:collapse;margin:0 0 20px">${rows}</table>` : '<p>This page has no number tiles. Open it to see the update.</p>'}
<p style="margin:0 0 24px"><a href="${esc(item.url)}" style="background:#0068da;color:#fff;padding:9px 16px;border-radius:999px;text-decoration:none">Open it</a></p>
<p style="font-size:12px;color:#888">You get this because you asked for updates of this page on tenun.cloud. <a href="${esc(unsubscribe)}" style="color:#888">Stop these emails</a>.</p>
</body></html>`;
  return {
    to,
    from: FROM,
    subject: `${item.title}: updated`,
    text,
    html,
    headers: { 'List-Unsubscribe': `<${unsubscribe}>`, 'List-Unsubscribe-Post': 'List-Unsubscribe=One-Click' },
  };
}

/** Hands one email to the Email Service; `MAIL_OUTBOX` (a service that takes each email as JSON) stands in for it in tests. */
async function deliver(env: Env, m: Mail): Promise<boolean> {
  if (!emailOn(env)) return false;
  if (env.MAIL_OUTBOX) {
    const r = await env.MAIL_OUTBOX.fetch('https://outbox.invalid/send', { method: 'POST', body: JSON.stringify(m) });
    return r.ok;
  }
  if (env.EMAIL) {
    await env.EMAIL.send(m);
    return true;
  }
  return false;
}

/** The start of a UTC day, for at most one email a day for each recipient of an item. */
const dayStart = (t: number) => t - (t % 86400_000);

/**
 * Sends an item's update to its recipients who have not had one today. `each`: after a publish. `daily` and
 * `weekly`: from the daily cron (weekly on Mondays). Returns how many went.
 */
export async function sendUpdates(env: Env, id: string, when: Exclude<Updates, 'off'>, origin = PUBLIC_ORIGIN, t = now()): Promise<number> {
  if (!emailOn(env)) return 0;
  const item = await env.DB.prepare('SELECT id, title, kind, version, generated_at, updates FROM dashboards WHERE id = ?1')
    .bind(id)
    .first<{ id: string; title: string; kind: string; version: number; generated_at: string | null; updates: string }>();
  if (!item || !item.version || item.updates !== when) return 0;
  const { results } = await env.DB.prepare('SELECT email, token FROM recipients WHERE dashboard_id = ?1 AND active = 1 AND (sent_at IS NULL OR sent_at < ?2)')
    .bind(id, dayStart(t))
    .all<{ email: string; token: string }>();
  if (!results.length) return 0;
  const obj = await env.SNAPSHOTS.get(digestKey(id));
  const digest = obj ? (JSON.parse(await obj.text()) as Digest) : null;
  const url = `${origin}/${item.kind === 'site' ? 's' : 'd'}/${item.id}`;
  let sent = 0;
  for (const r of results) {
    const ok = await deliver(env, compose({ title: item.title, generated_at: item.generated_at, url }, digest, r.email, `${origin}/u/${r.token}`));
    if (!ok) continue;
    sent++;
    await env.DB.prepare('UPDATE recipients SET sent_at = ?1 WHERE dashboard_id = ?2 AND email = ?3').bind(t, id, r.email).run();
  }
  return sent;
}

/** The daily cron: every item on "daily", and on Mondays every item on "weekly". */
export async function sendScheduled(env: Env, t = now()): Promise<number> {
  if (!emailOn(env)) return 0;
  const monday = new Date(t).getUTCDay() === 1;
  const { results } = await env.DB.prepare("SELECT id, updates FROM dashboards WHERE updates IN ('daily', 'weekly') AND version > 0").all<{ id: string; updates: string }>();
  let n = 0;
  for (const r of results) {
    if (r.updates === 'daily' || monday) n += await sendUpdates(env, r.id, r.updates as 'daily' | 'weekly', PUBLIC_ORIGIN, t);
  }
  return n;
}
