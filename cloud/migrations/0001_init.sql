-- tenun.cloud: accounts, publish tokens, device sign-in, dashboards and their versions.
CREATE TABLE accounts (
  id TEXT PRIMARY KEY,            -- WorkOS user id (or dev_… in local development)
  email TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX accounts_email ON accounts (email);

CREATE TABLE tokens (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  hash TEXT NOT NULL UNIQUE,      -- SHA-256 of the token; the token itself is never stored
  label TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  last_used_at INTEGER
);
CREATE INDEX tokens_account ON tokens (account_id);

-- Device sign-in (like `gh auth login`): a short code the person approves in the browser.
CREATE TABLE devices (
  code_hash TEXT PRIMARY KEY,     -- SHA-256 of the secret device code the app polls with
  user_code TEXT NOT NULL UNIQUE, -- ABCD-EFGH, typed or opened by the person
  label TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  account_id TEXT,                -- set on approval
  token TEXT                      -- the new token, handed over once on the next poll, then deleted
);

CREATE TABLE dashboards (
  id TEXT PRIMARY KEY,            -- 22 random characters; the public link
  account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  title TEXT NOT NULL DEFAULT 'Dashboard',
  access TEXT NOT NULL DEFAULT 'link' CHECK (access IN ('link', 'invited')),
  source TEXT NOT NULL DEFAULT '', -- "Accounting-PC": where updates come from
  every_minutes INTEGER,           -- the publisher's schedule; null = manual
  version INTEGER NOT NULL DEFAULT 0,
  generated_at TEXT,               -- when the current snapshot was built
  bytes INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX dashboards_account ON dashboards (account_id);

CREATE TABLE versions (
  dashboard_id TEXT NOT NULL REFERENCES dashboards (id) ON DELETE CASCADE,
  version INTEGER NOT NULL,
  bytes INTEGER NOT NULL,
  generated_at TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (dashboard_id, version)
);

CREATE TABLE invites (
  dashboard_id TEXT NOT NULL REFERENCES dashboards (id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  PRIMARY KEY (dashboard_id, email)
);

-- One row per accepted publish, kept for a day: the rate limits count these.
CREATE TABLE publishes (
  dashboard_id TEXT NOT NULL REFERENCES dashboards (id) ON DELETE CASCADE,
  at INTEGER NOT NULL
);
CREATE INDEX publishes_dashboard ON publishes (dashboard_id, at);
