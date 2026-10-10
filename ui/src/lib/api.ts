// Typed calls to the Tenun server. Every change sends X-Tenun, which the server
// requires so that another web page cannot post to it.

export type DType = 'number' | 'date' | 'datetime' | 'text' | 'bool';

export interface Column {
  name: string;
  dtype: DType;
  raw_type: string;
}

export interface Table {
  id: string;
  source_id: string;
  name: string;
  label: string;
  columns: Column[];
  rows: number | null;
}

export interface FieldRef {
  table: string;
  column: string;
}

export interface Relationship {
  id: string;
  from: FieldRef;
  to: FieldRef;
}

export interface Source {
  id: string;
  name: string;
  kind: 'excel' | 'csv' | 'parquet' | 'sqlite' | 'duckdb' | 'postgres' | 'mysql';
  path?: string;
  host?: string;
  database?: string;
}

/** `stdev` and `variance` are Advanced (see tiers.ts). */
export type Agg = 'sum' | 'avg' | 'min' | 'max' | 'count' | 'count_distinct' | 'stdev' | 'variance';
export type Grain = 'year' | 'quarter' | 'month' | 'week' | 'day';

export interface Measure {
  table: string;
  column: string | null;
  agg: Agg;
}

export interface Dim {
  table: string;
  column: string;
  grain?: Grain | null;
}

export interface Filter {
  table: string;
  column: string;
  grain?: Grain | null;
  op: 'in' | 'not_in' | 'between' | 'gte' | 'lte';
  values: (string | number | boolean | null)[];
}

export interface SemanticQuery {
  measures: Measure[];
  dimensions: Dim[];
  filters?: Filter[];
  sort?: { by: number; desc: boolean }[];
  limit?: number;
}

export interface OutColumn {
  name: string;
  role: 'dimension' | 'measure';
  dtype: DType;
}

export type Cell = string | number | boolean | null;

export interface QueryResult {
  columns: OutColumn[];
  rows: Cell[][];
  ignored_filters: number[];
  ms: number;
  /** Number tiles only: the computed figure and its comparison. */
  kpi?: KpiInfo;
  /** Time charts: the last period is not over yet (data runs to `until`). */
  partial?: { key: string; until: string };
}

export interface KpiInfo {
  value: number | null;
  /** What the value covers: "September 2026", "Oct 1–14, 2026", or "All data". */
  period: string;
  /** What it is compared with, or null when there is nothing to compare. */
  versus: string | null;
  prev: number | null;
  /** Relative change, e.g. 0.04 for +4 %. */
  delta: number | null;
  partial: boolean;
  /** The last periods, oldest first, for the column chart. */
  bars: { label: string; value: number | null }[];
}

/** `donut` is the old name of `share`; tiles saved with it render as `share`. */
export type TileType =
  | 'kpi' | 'bar' | 'line' | 'area' | 'share' | 'donut' | 'table'
  // Simple types added in v3.
  | 'scatter' | 'histogram' | 'waterfall' | 'pivot' | 'text' | 'image' | 'filter'
  // Advanced types. They always render; only editing them needs the Advanced switch (V3-D2).
  | 'heatmap' | 'funnel' | 'box' | 'bullet' | 'compare' | 'sankey' | 'calendar' | 'cohort' | 'gantt';

/** A reference line on a chart. `band` (value to `to`) is Advanced. */
export interface TileLine {
  kind: 'target' | 'average' | 'median' | 'min' | 'max' | 'trend' | 'band';
  value?: number;
  to?: number;
  label?: string;
}

export interface Tile {
  id: string;
  type: TileType;
  /** Empty = generated from the fields. */
  title: string;
  x: number;
  y: number;
  w: number;
  h: number;
  measures: Measure[];
  /** [group by, split by] for charts; any number for tables. */
  dims: Dim[];
  horizontal?: boolean;
  /** Number tiles: which direction of change is good. Default up. */
  better?: 'up' | 'down';
  /** `manual` is Advanced. */
  sort?: { by: 'value' | 'label' | 'manual'; desc?: boolean; order?: string[] };
  lines?: TileLine[];
  /** Small multiples (Advanced). */
  repeat?: Dim;
  /** Several measures: `panels` and `indexed` are Advanced. */
  layout?: 'together' | 'panels' | 'indexed';
  /** Bars plus a line, index-aligned with measures (Advanced). */
  mix?: ('bar' | 'line')[];
}

/** Settings of this computer (not of the workspace). */
export interface Settings {
  /** Show the editing controls for Advanced features. */
  advanced: boolean;
}

export interface PublishSettings {
  /** Set by the publisher: the cloud dashboard, its link, the last success and the last error. */
  id?: string;
  url?: string;
  last?: string;
  error?: string;
  access: 'link' | 'invited';
  invites: string[];
  every: 'manual' | 'change' | 'hour' | 'day';
  /** Overrides this computer's name on the hosted page. */
  source: string;
}

export interface Dashboard {
  id: string;
  title: string;
  /** Seconds between refreshes of database data; 0 = off. */
  refresh: number;
  tiles: Tile[];
  publish?: PublishSettings;
}

export interface CloudStatus {
  base: string;
  connected: boolean;
  email: string | null;
  source: string;
  /** Free-plan usage; null when tenun.cloud could not be reached. */
  usage: { dashboards: number; limit: number } | null;
}

export interface Workspace {
  version: number;
  name: string;
  sources: Source[];
  relationships: Relationship[];
  dashboards: Dashboard[];
  layout: { model?: Record<string, { x: number; y: number }> } | null;
}

export interface Snapshot {
  workspace: Workspace;
  path: string | null;
  tables: Table[];
  errors: Record<string, string>;
  /** Source id → when it last loaded without error (ISO 8601, UTC). */
  loaded_at?: Record<string, string>;
  dirty: boolean;
}

export interface Suggestion {
  from: FieldRef;
  to: FieldRef;
  score: number;
  reason: string;
}

export interface Health {
  ok: boolean;
  version: string;
  duckdb: string;
  native: boolean;
}

export interface FsListing {
  path: string;
  parent: string | null;
  entries: { name: string; path: string; dir: boolean; size: number; supported: boolean }[];
  places: { name: string; path: string }[];
}

export class ApiError extends Error {}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: body === undefined ? { 'X-Tenun': '1' } : { 'Content-Type': 'application/json', 'X-Tenun': '1' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    throw new ApiError(text || res.statusText);
  }
  if (!res.ok) {
    throw new ApiError((data as { error?: string })?.error ?? res.statusText);
  }
  return data as T;
}

export const api = {
  health: () => call<Health>('GET', '/health'),
  settings: () => call<Settings>('GET', '/settings'),
  saveSettings: (s: Settings) => call<Settings>('PUT', '/settings', s),
  workspace: () => call<Snapshot>('GET', '/workspace'),
  newWorkspace: () => call<Snapshot>('POST', '/workspace/new'),
  open: (path: string) => call<Snapshot>('POST', '/workspace/open', { path }),
  save: (path?: string) => call<Snapshot>('POST', '/workspace/save', { path: path ?? null }),
  rename: (name: string) => call<Snapshot>('POST', '/workspace/rename', { name }),
  sample: () => call<Snapshot>('POST', '/sample'),
  addFile: (path: string) => call<Snapshot & { added: string }>('POST', '/sources', { kind: 'file', path }),
  addDatabase: (kind: 'postgres' | 'mysql', conn: Record<string, unknown>) =>
    call<Snapshot & { added: string }>('POST', '/sources', { kind, ...conn }),
  removeSource: (id: string) => call<Snapshot>('DELETE', `/sources/${encodeURIComponent(id)}`),
  reloadSource: (id: string) => call<Snapshot>('POST', `/sources/${encodeURIComponent(id)}/reload`),
  preview: (id: string, limit = 200) =>
    call<{ columns: Column[]; rows: Cell[][] }>('GET', `/tables/preview?id=${encodeURIComponent(id)}&limit=${limit}`),
  addRelationship: (from: FieldRef, to: FieldRef) => call<Snapshot>('POST', '/relationships', { from, to }),
  removeRelationship: (id: string) => call<Snapshot>('DELETE', `/relationships/${encodeURIComponent(id)}`),
  suggest: () => call<Suggestion[]>('GET', '/relationships/suggest'),
  putDashboards: (d: Dashboard[]) => call<{ ok: boolean }>('PUT', '/dashboards', d),
  putLayout: (l: unknown) => call<{ ok: boolean }>('PUT', '/layout', l),
  query: (q: SemanticQuery) => call<QueryResult>('POST', '/query', q),
  fs: (path?: string) => call<FsListing>('GET', `/fs${path ? `?path=${encodeURIComponent(path)}` : ''}`),
  pick: (
    req: { kind: 'data_files' } | { kind: 'open_workspace' } | { kind: 'save_workspace' | 'save_html'; name: string },
  ) =>
    call<{ paths: string[] }>('POST', '/pick', req),
  openBrowser: () => call<{ ok: boolean }>('POST', '/open-browser'),
  tile: (tile: Tile, filters: unknown[]) => call<QueryResult>('POST', '/tile', { tile, filters }),
  cloud: () => call<CloudStatus>('GET', '/cloud'),
  cloudConnect: (open: boolean) => call<{ user_code: string; url: string; expires_in: number }>('POST', '/cloud/connect', { open }),
  cloudDisconnect: () => call<{ ok: boolean }>('POST', '/cloud/disconnect', {}),
  publish: (dashboard: string) => call<{ ok: boolean; url: string; version: number; snapshot: Snapshot }>('POST', '/cloud/publish', { dashboard }),
  unpublish: (dashboard: string) => call<Snapshot>('POST', '/cloud/unpublish', { dashboard }),
  /** Writes the HTML file (desktop app) when `path` is given. */
  exportTo: (dashboard: string, filters: unknown[], path: string) =>
    call<{ ok: boolean }>('POST', '/export-html', { dashboard, filters, path }),
  /** Returns the HTML file's text (browser mode). */
  exportHtml: async (dashboard: string, filters: unknown[]): Promise<string> => {
    const res = await fetch('/api/export-html', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Tenun': '1' },
      body: JSON.stringify({ dashboard, filters }),
    });
    if (!res.ok) throw new ApiError((await res.json().catch(() => null))?.error ?? res.statusText);
    return res.text();
  },
};
