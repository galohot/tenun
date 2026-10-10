const ALPHABET = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';

/** n random letters from `alphabet`, every letter equally likely (rejection sampling). */
function draw(alphabet: string, n: number): string {
  const limit = 256 - (256 % alphabet.length);
  let out = '';
  while (out.length < n) {
    for (const b of crypto.getRandomValues(new Uint8Array(n * 2))) {
      if (b < limit && out.length < n) out += alphabet[b % alphabet.length];
    }
  }
  return out;
}

/** A random id; 22 letters of 62 ≈ 131 bits. */
export const rid = (n = 22) => draw(ALPHABET, n);

/** ABCD-EFGH without letters that look alike (no 0/O, 1/I/L). */
export function userCode(): string {
  const s = draw('ABCDEFGHJKMNPQRSTUVWXYZ23456789', 8);
  return `${s.slice(0, 4)}-${s.slice(4)}`;
}

export async function sha256(s: string): Promise<string> {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s));
  return Array.from(new Uint8Array(d), (x) => x.toString(16).padStart(2, '0')).join('');
}

export const now = () => Date.now();

export function esc(s: unknown): string {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export const validEmail = (e: string) => /^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+$/.test(e) && e.length <= 254;
