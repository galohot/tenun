// Copied from galohot/akun by `akun vendor`. Do not edit here: change akun, then copy again.
/**
 * Signed tokens: `v1.<base64url JSON>.<base64url HMAC-SHA256>` (the irawan-auth / MUSIM format).
 * Every product signs with ITS OWN secret (GRANDPLAN D5). `k` says what a token is for, so one kind
 * can never pass as another (a state token is never a session).
 */
const enc = new TextEncoder();

const b64url = (bytes: Uint8Array): string =>
  btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');

function fromB64url(s: string): Uint8Array {
  const b = atob(s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4));
  return Uint8Array.from(b, (c) => c.charCodeAt(0));
}

const key = (secret: string) =>
  crypto.subtle.importKey('raw', enc.encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign', 'verify']);

export type Claims = { k: 'session' | 'state'; exp: number } & Record<string, unknown>;

export async function sign(claims: Claims, secret: string): Promise<string> {
  const body = b64url(enc.encode(JSON.stringify(claims)));
  const sig = new Uint8Array(await crypto.subtle.sign('HMAC', await key(secret), enc.encode(`v1.${body}`)));
  return `v1.${body}.${b64url(sig)}`;
}

/** The claims of a token that is well formed, correctly signed, of kind `k` and not expired; else null. */
export async function verify(token: string | null | undefined, k: Claims['k'], secret: string): Promise<Claims | null> {
  if (!token) return null;
  const [v, body, sig] = token.split('.');
  if (v !== 'v1' || !body || !sig) return null;
  try {
    const ok = await crypto.subtle.verify('HMAC', await key(secret), fromB64url(sig) as BufferSource, enc.encode(`v1.${body}`));
    if (!ok) return null;
    const c = JSON.parse(new TextDecoder().decode(fromB64url(body))) as Claims;
    return c.k === k && typeof c.exp === 'number' && c.exp > Date.now() / 1000 ? c : null;
  } catch {
    return null;
  }
}

export function cookie(req: Request, name: string): string | null {
  const m = (req.headers.get('cookie') ?? '').match(new RegExp(`(?:^|;\\s*)${name}=([^;]+)`));
  return m ? m[1]! : null;
}
