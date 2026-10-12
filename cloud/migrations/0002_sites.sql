-- Sites (build 14, V4-D17): a published site is a row of `dashboards` with kind 'site'. It is versioned, shared by
-- link or by invitation, rate-limited and deleted exactly as a dashboard is, and counts as one item toward the free
-- plan. `pages` is how many pages it holds (the app says so when it publishes; at most 10).
ALTER TABLE dashboards ADD COLUMN kind TEXT NOT NULL DEFAULT 'dashboard' CHECK (kind IN ('dashboard', 'site'));
ALTER TABLE dashboards ADD COLUMN pages INTEGER NOT NULL DEFAULT 1;
