// Filters of a dashboard: what a filter tile selects, the filters a click on a mark makes, "Keep only"
// and "Exclude", and the one grain for every time tile. Pure code with no imports at run time, so the
// node tests can load it. The engine does the same on its side (`filters.rs`); keep the two in step.

import type { Bins, Cell, DashFilter, Dim, DType, Filter, Grain, Tile, TileFilter } from './api';

export const isPeriod = (g: Grain | null | undefined): boolean => !!g && !['month_of_year', 'weekday', 'hour'].includes(g);

/** The grains the toolbar offers, smallest first. */
export const PERIODS: Grain[] = ['day', 'week', 'month', 'quarter', 'year'];

/** The ways to cut a date range quickly. The engine resolves each against today's date. */
export const DATE_PRESETS: { id: string; label: string }[] = [
  { id: 'last_7_days', label: 'Last 7 days' },
  { id: 'last_30_days', label: 'Last 30 days' },
  { id: 'this_month', label: 'This month so far' },
  { id: 'last_month', label: 'Last month' },
  { id: 'this_quarter', label: 'This quarter' },
  { id: 'last_quarter', label: 'Last quarter' },
  { id: 'ytd', label: 'Year to date' },
  { id: 'last_12_months', label: 'Last 12 months' },
];

export const presetLabel = (id: string | undefined): string => DATE_PRESETS.find((p) => p.id === id)?.label ?? 'Custom';

/** The key of a value, as the charts use it. `null` (blank) has a key of its own. */
export const keyOf = (v: Cell): string => (v === null || v === undefined ? '\u0000null' : String(v));

/** Gives a tile the dashboard's grain when its first group is a date cut into periods. */
export function withGrain<T extends Pick<Tile, 'dims'>>(tile: T, grain: Grain | null | undefined): T {
  const d = tile.dims[0];
  if (!grain || !isPeriod(grain) || !d || !isPeriod(d.grain) || d.grain === grain) return tile;
  return { ...tile, dims: [{ ...d, grain }, ...tile.dims.slice(1)] };
}

/** A tile with a date cut into periods: the toolbar offers one grain for all of them. */
export const hasPeriod = (tiles: Pick<Tile, 'dims'>[]): boolean => tiles.some((t) => isPeriod(t.dims[0]?.grain));

/** The next day, as `YYYY-MM-DD`. A span of whole days keeps its last day in full by ending at the next one. */
export function nextDay(day: string): string | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(day);
  if (!m) return null;
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3] + 1));
  return d.toISOString().slice(0, 10);
}

const isDay = (v: Cell | undefined): v is string => typeof v === 'string' && /^\d{4}-\d{2}-\d{2}/.test(v);

/** The filter a filter tile stands for, or null while it keeps everything. `dtype` is its field's type. */
export function tileFilter(tile: Tile, dtype: DType | undefined): DashFilter | null {
  const f = tile.filter;
  const d = tile.dims[0];
  if (tile.type !== 'filter' || !f || !d) return null;
  const make = (op: Filter['op'], values: Cell[]): DashFilter => ({ table: d.table, column: d.column, grain: null, op, values, fromTile: tile.id });
  if (f.style === 'range') {
    const [lo, hi] = f.range ?? [null, null];
    if (lo === null && hi === null) return null;
    if (hi === null) return make('gte', [lo]);
    if (lo === null) return make('lte', [hi]);
    return make('between', [lo, hi]);
  }
  if (f.style === 'dates') {
    if (f.preset && f.preset !== 'custom') return make('relative', [f.preset]);
    const [lo, hi] = f.range ?? [null, null];
    const end = isDay(hi) ? nextDay(hi) : null;
    if (isDay(lo) && end) return make('range', [lo, end]);
    if (isDay(lo)) return make('gte', [lo]);
    if (end) return make('range', ['0001-01-01', end]);
    return null;
  }
  const values = f.values ?? [];
  if (!values.length) return null;
  // The blank value stands for empty text too.
  return make('in', values.includes(null) && dtype === 'text' && !values.includes('') ? [...values, ''] : values);
}

/** Every filter tile's selection, for the whole dashboard. */
export function tileFilters(tiles: Tile[], dtypeOf: (table: string, column: string) => DType | undefined): DashFilter[] {
  return tiles.flatMap((t) => {
    const f = tileFilter(t, t.dims[0] ? dtypeOf(t.dims[0].table, t.dims[0].column) : undefined);
    return f ? [f] : [];
  });
}

/** A filter tile with nothing selected. */
export const isCleared = (f: TileFilter | undefined): boolean =>
  !f || (!f.values?.length && !(f.range && (f.range[0] !== null || f.range[1] !== null)) && !f.preset);

/** The same question on the same field: a second click on a mark takes the filter off. */
export function sameFilter(a: Filter, b: Filter): boolean {
  return (
    a.table === b.table &&
    a.column === b.column &&
    a.op === b.op &&
    a.values.length === b.values.length &&
    a.values.every((v, i) => keyOf(v) === keyOf(b.values[i]))
  );
}

/** The key the engine gives the "Other" group of a top N. */
export const OTHER = '\u0000other';

/**
 * The filter a click on a mark makes: a group of the first field (a Sankey node: the field of its stage), "Other" (everything outside the top
 * N), or for a histogram the range of the bin. Null when the click means nothing.
 */
export function markFilter(
  tile: Pick<Tile, 'type' | 'dims' | 'measures'>,
  value: Cell,
  otherOf?: Cell[],
  bins?: Bins,
  stage?: number,
): Omit<DashFilter, 'fromTile'> | null {
  if (tile.type === 'histogram') {
    const m = tile.measures[0];
    if (!m?.column || !bins || typeof value !== 'number') return null;
    return { table: m.table, column: m.column, grain: null, op: 'range', values: [value, Math.round((value + bins.width) * 1e10) / 1e10] };
  }
  // A Sankey node is a value of the field of its stage; every other tile filters by its first field.
  const d = tile.type === 'sankey' && stage !== undefined ? tile.dims[stage] : tile.dims[0];
  if (!d) return null;
  // A calendar is always read by day.
  const base = { table: d.table, column: d.column, grain: tile.type === 'calendar' ? ('day' as const) : (d.grain ?? null) };
  if (value === OTHER && otherOf) return { ...base, op: 'not_in', values: [...otherOf], label: 'Other' };
  return { ...base, op: 'in', values: [value] };
}

/** A click on a pivot's column header. */
export function colFilter(c: Dim | undefined, value: Cell): Omit<DashFilter, 'fromTile'> | null {
  return c ? { table: c.table, column: c.column, grain: c.grain ?? null, op: 'in', values: [value] } : null;
}

/**
 * A click filter made permanent. "Keep only" keeps the marks; "Exclude" drops them. It names no
 * tile, so it filters every tile, the one it came from too. A bin of a histogram cannot be excluded.
 */
export function pinFilter(f: DashFilter, mode: 'keep' | 'exclude', id: string): DashFilter | null {
  const { fromTile: _, label, ...rest } = f;
  if (mode === 'keep') return { ...rest, ...(label ? { label } : {}), fromTile: id };
  if (f.op === 'in') return { ...rest, op: 'not_in', fromTile: id };
  // Excluding "Other" keeps the groups that are in the top N.
  if (f.op === 'not_in' && label === 'Other') return { ...rest, op: 'in', label: 'Not Other', fromTile: id };
  if (f.op === 'not_in') return { ...rest, op: 'in', fromTile: id };
  return null;
}

/** Excluding is possible for these filters. */
export const canExclude = (f: Filter): boolean => f.op === 'in' || f.op === 'not_in';

/** Ids of saved filters start with this, so they never match a tile. */
export const PIN_PREFIX = 'pin_';
export const newPinId = (): string => PIN_PREFIX + Math.random().toString(36).slice(2, 8);
