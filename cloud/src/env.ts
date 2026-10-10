export type Env = {
  DB: D1Database;
  SNAPSHOTS: R2Bucket;
  ASSETS: Fetcher;
  /** HMAC key for our own session cookies (wrangler secret). */
  SESSION_SECRET: string;
  /** Akun (WorkOS) keys; sign-in stays closed until both exist. */
  WORKOS_CLIENT_ID?: string;
  WORKOS_API_KEY?: string;
  /** 'open' = Akun sign-in · 'dev' = local test sign-in (needs DEV_LOGIN=1) · anything else = closed. */
  SIGNIN?: string;
  DEV_LOGIN?: string;
};

export type Account = { id: string; email: string; name: string };

export type HonoEnv = { Bindings: Env; Variables: { account: Account | null } };

/** The free plan (V2-D3). Numbers are tuned at build time, not promises. */
export const LIMITS = {
  dashboards: 3,
  /** Compressed snapshot size. */
  maxBytes: 10 * 1024 * 1024,
  perHour: 6,
  perDay: 48,
  keepVersions: 5,
  /** Schedules faster than this are refused; the app offers hourly as the fastest. */
  minEveryMinutes: 60,
  /** Device sign-ins waiting for approval, across everyone. */
  openDevices: 500,
};
