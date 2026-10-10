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

export type Agg = 'sum' | 'avg' | 'min' | 'max' | 'count' | 'count_distinct';
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
}

export type TileType = 'kpi' | 'bar' | 'line' | 'area' | 'donut' | 'table';

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
}

export interface Dashboard {
  id: string;
  title: string;
  /** Seconds between refreshes of database data; 0 = off. */
  refresh: number;
  tiles: Tile[];
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
  viewer: () => call<{ js: string; css: string }>('GET', '/viewer'),
  writeExport: (path: string, html: string) => call<{ ok: boolean }>('POST', '/export-file', { path, html }),
};
