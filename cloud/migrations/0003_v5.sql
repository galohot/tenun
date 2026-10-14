-- v5 (builds 11 and 13). Password links and embedding: a password is stored as a PBKDF2-SHA256 hash with its own
-- salt and iteration count, beside `access` 'link' (the API calls it access 'password'); `pw_version` goes up with
-- each new password, so the cookies of the old one stop working. `embed` lets other sites frame the item. `updates`
-- and `recipients` are for email updates (build 13): empty until then.
ALTER TABLE dashboards ADD COLUMN pw_hash TEXT;
ALTER TABLE dashboards ADD COLUMN pw_salt TEXT;
ALTER TABLE dashboards ADD COLUMN pw_iter INTEGER;
ALTER TABLE dashboards ADD COLUMN pw_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE dashboards ADD COLUMN embed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE dashboards ADD COLUMN updates TEXT NOT NULL DEFAULT 'off';

CREATE TABLE recipients (
  dashboard_id TEXT NOT NULL REFERENCES dashboards (id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  token TEXT NOT NULL UNIQUE,     -- the one-click unsubscribe link's secret
  active INTEGER NOT NULL DEFAULT 1,  -- 0 after an unsubscribe, so a later list does not add it back
  sent_at INTEGER,                -- the last email, for at most one a day
  created_at INTEGER NOT NULL,
  PRIMARY KEY (dashboard_id, email)
);

-- Wrong passwords, for the limit of five a minute from one address. The address is stored as its SHA-256 only.
CREATE TABLE unlock_tries (
  who TEXT NOT NULL,
  at INTEGER NOT NULL
);
CREATE INDEX unlock_tries_who ON unlock_tries (who, at);
