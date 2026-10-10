// Copied from galohot/akun by `akun vendor`. Do not edit here: change akun, then copy again.
/**
 * The WorkOS side of sign-in for Dedi's public products (GRANDPLAN D1, D4, D6).
 * Plain fetch, no SDK: authorize URL → code exchange → session id → revoke on sign-out.
 * The hosted AuthKit page keeps its own session, so a person signed in to one product is signed in
 * to the next one without typing anything. Revoking the session ends that for every product.
 */
export const WORKOS = 'https://api.workos.com';

export type WorkosUser = { id: string; email: string; name: string };
export type SignedIn = { user: WorkosUser; sid: string | null };

export type AuthorizeOpts = {
  clientId: string;
  /** `<product origin>/auth/callback` — must be registered (`akun add <origin>`). */
  redirectUri: string;
  state: string;
  /** Open the sign-up screen instead of sign-in. */
  screenHint?: 'sign-in' | 'sign-up';
  /** 0 = ask again even when the AuthKit session is still alive ("use another account"). */
  maxAge?: number;
};

export function authorizeUrl(o: AuthorizeOpts): string {
  const u = new URL(`${WORKOS}/user_management/authorize`);
  u.searchParams.set('client_id', o.clientId);
  u.searchParams.set('redirect_uri', o.redirectUri);
  u.searchParams.set('response_type', 'code');
  u.searchParams.set('provider', 'authkit');
  u.searchParams.set('state', o.state);
  if (o.screenHint) u.searchParams.set('screen_hint', o.screenHint);
  if (o.maxAge !== undefined) u.searchParams.set('max_age', String(o.maxAge));
  return u.toString();
}

type RawUser = { id?: string; email?: string; first_name?: string | null; last_name?: string | null };

/** Exchanges the callback code. Null on any failure: the product shows its own "try again". */
export async function authenticate(code: string, clientId: string, apiKey: string, f: typeof fetch = fetch): Promise<SignedIn | null> {
  try {
    const r = await f(`${WORKOS}/user_management/authenticate`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ client_id: clientId, client_secret: apiKey, grant_type: 'authorization_code', code }),
    });
    if (!r.ok) return null;
    const j = (await r.json()) as { user?: RawUser; access_token?: string };
    const w = j.user;
    if (!w?.id || !w.email) return null;
    return {
      user: { id: w.id, email: w.email, name: [w.first_name, w.last_name].filter(Boolean).join(' ') },
      sid: sessionId(j.access_token),
    };
  } catch {
    return null;
  }
}

/** The `sid` claim of a WorkOS access token (a JWT). The signature is not checked: the token came
 *  straight from WorkOS over TLS, and the id is only used to end that same session. */
export function sessionId(accessToken: string | undefined): string | null {
  const body = accessToken?.split('.')[1];
  if (!body) return null;
  try {
    const json = atob(body.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (body.length % 4)) % 4));
    const sid = (JSON.parse(json) as { sid?: unknown }).sid;
    return typeof sid === 'string' && sid ? sid : null;
  } catch {
    return null;
  }
}

/** Ends the WorkOS session, so the next "Masuk" asks again (D6). True when WorkOS accepted it. */
export async function revokeSession(sid: string, apiKey: string, f: typeof fetch = fetch): Promise<boolean> {
  try {
    const r = await f(`${WORKOS}/user_management/sessions/revoke`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${apiKey}` },
      body: JSON.stringify({ session_id: sid }),
    });
    return r.ok;
  } catch {
    return false;
  }
}
