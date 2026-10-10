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

/** `median` and `p90` (the 90th percentile) take numbers only. `stdev` and `variance` are Advanced (see tiers.ts). */
export type Agg = 'sum' | 'avg' | 'min' | 'max' | 'count' | 'count_distinct' | 'median' | 'p90' | 'stdev' | 'variance';
export type Grain = 'year' | 'quarter' | 'month' | 'week' | 'day' | 'month_of_year' | 'weekday' | 'hour';
/** Date parts come back as numbers: months 1–12, weekdays 1–7 (Monday = 1), hours 0–23. */
export const DATE_PARTS: readonly Grain[] = ['month_of_year', 'weekday', 'hour'];
export const isPart = (g: Grain | null | undefined): boolean => !!g && DATE_PARTS.includes(g);

export interface Measure {
  table: string;
  column: string | null;
  agg: Agg;
  /** Makes a ratio: agg(column) divided by agg(per). A zero below gives a gap. */
  per?: { table: string; column: string | null; agg: Agg };
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
  op: 'in' | 'not_in' | 'between' | 'gte' | 'lte' | 'range' | 'contains' | 'starts_with' | 'blank' | 'not_blank' | 'relative';
  values: (string | number | boolean | null)[];
}

/**
 * A filter on a dashboard. `fromTile` is the tile it came from, which it does not filter: a click on
 * a mark, or a filter tile's selection. A filter the dashboard saves ("Keep only", "Exclude") names
 * no tile, so it filters them all. `label` is what a chip says when the values alone do not tell.
 */
export interface DashFilter extends Filter {
  fromTile: string;
  label?: string;
}

export interface SemanticQuery {
  measures: Measure[];
  dimensions: Dim[];
  filters?: Filter[];
  sort?: { by: number; desc: boolean }[];
  limit?: number;
}

/** What a value shown another way counts. A percent is a fraction: 0.25 reads as 25 %. */
export type Unit = 'percent' | 'rank';

export interface OutColumn {
  name: string;
  role: 'dimension' | 'measure';
  dtype: DType;
  /** Set by "Show as": the value is a share (`percent`) or a place in the ranking (`rank`). */
  unit?: Unit;
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
  /** The last group is "Other" (key `\u0000other`): everything outside the top N. */
  other?: boolean;
  /** The groups that are not in "Other", for a `not_in` filter. */
  otherOf?: Cell[];
  /** Histograms: bin `i` covers `[start + i·width, start + (i+1)·width)`. */
  bins?: Bins;
  /** Tables with a total row, and pivots. Computed by the engine from the same measures. */
  totals?: Totals;
  /** Time charts with `tile.compare`: the earlier series, its keys moved forward to line up with `rows`. */
  compare?: { label: string; rows: Cell[][] };
}

/**
 * Every entry holds measure values in the order of the tile's measures. `rows` entries start with
 * the values of the row fields, `cols` entries with the column value. `cols` also lists the column
 * values in the engine's order.
 */
export interface Totals {
  grand: Cell[];
  rows?: Cell[][];
  cols?: Cell[][];
}

export interface Bins {
  start: number;
  width: number;
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
  /** The share of the period that has data, 0–1. 1 for a finished period; not set for "All data". */
  elapsed?: number;
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

/** How one value is written. Index-aligned with `Tile.measures`; every field is optional. */
export interface NumFormat {
  /** 0–4. Unset: the number as it is, up to 2 decimals. */
  decimals?: number;
  /** IDR, USD, EUR, SGD or MZN. */
  currency?: string;
  /** Short (1,2 jt) or full. Unset: short on charts and number tiles, full in tables. */
  compact?: boolean;
  /** Multiply by 100 and add %. */
  percent?: boolean;
}

/** How a value is shown next to the others. The engine computes it; a total cell is blank for any but `value`. */
export type ShowAs =
  | 'value' | 'pct_total' | 'pct_group' | 'running' | 'moving_3' | 'moving_7' | 'moving_12'
  | 'change' | 'pct_change' | 'rank' | 'ytd';

/**
 * A filter tile: its field is `dims[0]`. `values` (list, dropdown), `range` (range: lowest and highest
 * number; dates: first and last day, either may be null) and `preset` (dates) hold the selection. A
 * filter with none of them keeps everything.
 */
export interface TileFilter {
  style: 'list' | 'dropdown' | 'range' | 'dates';
  /** List and dropdown: pick one value at most. */
  single?: boolean;
  values?: Cell[];
  range?: [Cell, Cell];
  /** A relative date preset, or `custom` while the two days are being picked. */
  preset?: string;
}

/** A picture on a dashboard. */
export interface TileImage {
  /** A data URL: PNG, JPEG or WebP, at most 1 MB. */
  data: string;
  alt: string;
  fit?: 'contain' | 'cover';
}

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
  /** Keep the biggest N groups; the rest become one "Other" bar. `other` defaults to true. */
  top?: { n: number; other?: boolean };
  lines?: TileLine[];
  /** How each value is written, index-aligned with `measures`. */
  formats?: (NumFormat | null)[];
  /** Small multiples (Advanced). */
  repeat?: Dim;
  /** Several measures: `panels` and `indexed` are Advanced. */
  layout?: 'together' | 'panels' | 'indexed';
  /** Bars plus a line, index-aligned with measures (Advanced). */
  mix?: ('bar' | 'line')[];
  /** Bars and areas with a split or several values: pile them up, put them side by side, or fill the height. */
  stack?: 'stacked' | 'side' | 'percent';
  /** Histogram: the width of a bin. Not set: the engine picks one. */
  bins?: { width?: number };
  /** Number tiles: the goal for the period. */
  target?: { value: number };
  /** Table: add a total row. Pivot: totals are on unless this is false. */
  totals?: boolean;
  /** Pivot: the field whose values become the columns. The rows are `dims`. */
  cols?: Dim;
  /** How each value is shown, index-aligned with `measures`. */
  showAs?: (ShowAs | null)[];
  /** Line, area and bar over time: also draw the same question one year (or one period) earlier. */
  compare?: 'prev_year' | 'prev_period' | null;
  /** Filter tile: its style and what it has selected. */
  filter?: TileFilter;
  /** Text tile: a small Markdown subset (see markdown.ts). */
  text?: string;
  /** Image tile. */
  image?: TileImage;
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
  /** One grain for every time tile. Not set: each tile keeps its own ("Auto"). */
  grain?: Grain | null;
  /** Filters the dashboard keeps ("Keep only", "Exclude"). They apply to every tile. */
  filters?: DashFilter[];
}

/** The raw rows behind a tile or one of its marks. */
export interface RowsBehind {
  columns: { label: string; dtype: DType }[];
  rows: Cell[][];
  /** How many rows match. `rows` holds the first 500. */
  total: number;
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
  /** How numbers and dates read. A file without it reads as English. */
  locale?: 'id' | 'en';
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
  newWorkspace: () => call<Snapshot>('POST', '/workspace/new', { lang: navigator.language }),
  setLocale: (locale: 'id' | 'en') => call<Snapshot>('PUT', '/workspace/locale', { locale }),
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
    req: { kind: 'data_files' } | { kind: 'image' } | { kind: 'open_workspace' } | { kind: 'save_workspace' | 'save_html' | 'save_xlsx'; name: string },
  ) =>
    call<{ paths: string[] }>('POST', '/pick', req),
  /** The picture the native dialog chose, for an image tile. */
  readImage: async (path: string): Promise<Blob> => {
    const res = await fetch('/api/image/read', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Tenun': '1' },
      body: JSON.stringify({ path }),
    });
    if (!res.ok) throw new ApiError((await res.json().catch(() => null))?.error ?? res.statusText);
    return res.blob();
  },
  openBrowser: () => call<{ ok: boolean }>('POST', '/open-browser'),
  tile: (tile: Tile, filters: unknown[]) => call<QueryResult>('POST', '/tile', { tile, filters }),
  /** The raw rows behind a tile; `click` is the filter a click on the mark makes. */
  rows: (tile: Tile, filters: unknown[], click: Filter | null) => call<RowsBehind>('POST', '/rows', { tile, filters, click }),
  cloud: () => call<CloudStatus>('GET', '/cloud'),
  cloudConnect: (open: boolean) => call<{ user_code: string; url: string; expires_in: number }>('POST', '/cloud/connect', { open }),
  cloudDisconnect: () => call<{ ok: boolean }>('POST', '/cloud/disconnect', {}),
  publish: (dashboard: string) => call<{ ok: boolean; url: string; version: number; snapshot: Snapshot }>('POST', '/cloud/publish', { dashboard }),
  unpublish: (dashboard: string) => call<Snapshot>('POST', '/cloud/unpublish', { dashboard }),
  /** Writes the HTML file (desktop app) when `path` is given. */
  exportTo: (dashboard: string, filters: unknown[], path: string) =>
    call<{ ok: boolean }>('POST', '/export-html', { dashboard, filters, path }),
  /** The Excel file of one tile. With `path` (desktop app) the server writes it there and nothing comes back. */
  exportXlsx: async (body: { tile: Tile; filters: unknown[]; headers: string[]; title: string }, path?: string): Promise<Uint8Array> => {
    const res = await fetch('/api/export-xlsx', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Tenun': '1' },
      body: JSON.stringify({ ...body, path }),
    });
    if (!res.ok) throw new ApiError((await res.json().catch(() => null))?.error ?? res.statusText);
    return path ? new Uint8Array() : new Uint8Array(await res.arrayBuffer());
  },
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
