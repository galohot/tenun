// Password links (v5 build 11, V5-D13): the hash of a password, its check, the cookie a right password sets, and the
// limit on wrong tries. The plain password is never stored and never logged.
import type { HonoEnv } from './env';
import { now, sha256 } from './util';

/**
 * PBKDF2-SHA256 iterations for a new password. 100,000 (the Workers maximum) takes about 40 ms of CPU, four times the
 * free plan's 10 ms a request, so the free plan uses 10,000 (about 5 ms). Each hash keeps its own count, so a later,
 * larger number applies to passwords set after it (V5-D49).
 */
export const ITERATIONS = 10_000;
/** A password is 4 to 200 characters. */
export const MIN_PASSWORD = 4;
export const MAX_PASSWORD = 200;
/** Wrong tries from one address in a minute before it must wait. */
export const TRIES = 5;
/** How long a right password opens its item: 7 days. */
export const OPEN_SECONDS = 7 * 86400;

const b64 = (b: ArrayBuffer | Uint8Array) => btoa(String.fromCharCode(...new Uint8Array(b)));
const unb64 = (s: string) => Uint8Array.from(atob(s), (ch) => ch.charCodeAt(0));

async function derive(password: string, salt: Uint8Array, iterations: number): Promise<ArrayBuffer> {
  const key = await crypto.subtle.importKey('raw', new TextEncoder().encode(password), 'PBKDF2', false, ['deriveBits']);
  return crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations }, key, 256);
}

/** The stored form of a new password: its hash, a random salt and the iteration count. */
export async function hashPassword(password: string): Promise<{ hash: string; salt: string; iter: number }> {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  return { hash: b64(await derive(password, salt, ITERATIONS)), salt: b64(salt), iter: ITERATIONS };
}

/** Equal bytes, compared without stopping at the first difference. */
function same(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a[i] ^ b[i];
  return d === 0;
}

export async function checkPassword(password: string, row: { pw_hash: string; pw_salt: string; pw_iter: number }): Promise<boolean> {
  const got = new Uint8Array(await derive(password, unb64(row.pw_salt), row.pw_iter));
  return same(got, unb64(row.pw_hash));
}

async function hmac(secret: string, text: string): Promise<string> {
  const key = await crypto.subtle.importKey('raw', new TextEncoder().encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return b64(await crypto.subtle.sign('HMAC', key, new TextEncoder().encode(text)))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

/** The name of the cookie that opens one item. */
export const cookieName = (id: string) => `tn_pw_${id}`;

/** The value of the cookie for an item at a password version: the version and an HMAC of both. */
export async function cookieValue(env: HonoEnv['Bindings'], id: string, version: number): Promise<string> {
  return `${version}.${await hmac(env.SESSION_SECRET, `pw|${id}|${version}`)}`;
}

/** Whether a cookie value opens the item at its current password version. */
export async function cookieOpens(env: HonoEnv['Bindings'], id: string, version: number, value: string | undefined): Promise<boolean> {
  if (!value) return false;
  const expected = await cookieValue(env, id, version);
  return same(new TextEncoder().encode(value), new TextEncoder().encode(expected));
}

/** Who is trying, as a hash of the address: no address is stored. */
export const whoOf = async (ip: string | undefined) => sha256(`try|${ip ?? 'unknown'}`);

/** Wrong tries from this address in the last minute. */
export async function triesOf(env: HonoEnv['Bindings'], who: string): Promise<number> {
  const r = await env.DB.prepare('SELECT count(*) AS n FROM unlock_tries WHERE who = ?1 AND at > ?2').bind(who, now() - 60_000).first<{ n: number }>();
  return r?.n ?? 0;
}

export async function wrongTry(env: HonoEnv['Bindings'], who: string) {
  await env.DB.prepare('INSERT INTO unlock_tries (who, at) VALUES (?1, ?2)').bind(who, now()).run();
}
