// Tile helpers: the query a tile sends, sensible defaults, and grid layout.

import type { Dashboard, Dim, DType, Filter, Measure, SemanticQuery, Table, Tile, TileType } from './api';

export const COLS = 12;

export interface DashFilter extends Filter {
  /** The tile the click came from; it is not filtered by its own selection. */
  fromTile: string;
}

/** The most values a bar, line or area tile shows side by side. */
export const MAX_CHART_MEASURES = 4;

export const hasManyValues = (tile: Tile) =>
  (tile.type === 'bar' || tile.type === 'line' || tile.type === 'area') && tile.measures.length > 1;

/**
 * The question a tile asks. The server builds its own copy of this (`snapshot::tile_query`); this one
 * decides when a tile must ask again, so keep the two the same.
 */
export function tileQuery(tile: Tile, filters: DashFilter[]): SemanticQuery | null {
  if (!tile.measures.length && !tile.dims.length) return null;
  const one = tile.type === 'kpi' || tile.type === 'share' || tile.type === 'donut' || tile.type === 'waterfall';
  const many = hasManyValues(tile);
  // A histogram has no groups: it bins one number column.
  const dims =
    tile.type === 'histogram' ? [] : one || many ? tile.dims.slice(0, 1) : tile.type === 'table' ? tile.dims : tile.dims.slice(0, 2);
  // A chart with a group but no value counts rows of the grouped table.
  const measures = tile.measures.length
    ? tile.type === 'table'
      ? tile.measures
      : many
        ? tile.measures.slice(0, MAX_CHART_MEASURES)
        : tile.type === 'scatter'
          ? tile.measures.slice(0, 2) // across and up
          : tile.measures.slice(0, 1)
    : tile.type === 'table'
      ? []
      : [{ table: tile.dims[0].table, column: null, agg: 'count' as const }];
  const sort: { by: number; desc: boolean }[] = [];
  if (dims.length && tile.sort) {
    if (tile.sort.by === 'value' && measures.length) sort.push({ by: dims.length, desc: tile.sort.desc ?? true });
    else if (tile.sort.by === 'label') sort.push({ by: 0, desc: tile.sort.desc ?? false });
  }
  return {
    measures,
    dimensions: dims,
    filters: filters.filter((f) => f.fromTile !== tile.id).map(({ fromTile: _, ...f }) => f),
    sort,
    limit: tile.type === 'table' ? 500 : tile.type === 'kpi' ? 400 : 2000,
  };
}

/** The filter list a tile actually received (its own selection excluded). */
export function appliedFilters(tile: Tile, filters: DashFilter[]): DashFilter[] {
  return filters.filter((f) => f.fromTile !== tile.id);
}

export function dtypeOf(tables: Table[], table: string, column: string | null): DType | undefined {
  if (column === null) return 'number';
  return tables.find((t) => t.id === table)?.columns.find((c) => c.name === column)?.dtype;
}

/** Picks a chart type that suits the fields, the way a careful analyst would. */
export function autoType(tables: Table[], measures: Measure[], dims: Dim[]): TileType {
  if (!dims.length) return 'kpi';
  const d = dtypeOf(tables, dims[0].table, dims[0].column);
  if (d === 'date' || d === 'datetime') return 'line';
  return 'bar';
}

export function defaultMeasure(tables: Table[], table: string, column: string | null): Measure {
  const d = dtypeOf(tables, table, column);
  return { table, column, agg: column === null ? 'count' : d === 'number' ? 'sum' : 'count_distinct' };
}

export function defaultDim(tables: Table[], table: string, column: string): Dim {
  const d = dtypeOf(tables, table, column);
  return { table, column, grain: d === 'date' || d === 'datetime' ? 'month' : null };
}

export function defaultSize(type: TileType): { w: number; h: number } {
  switch (type) {
    case 'kpi':
      return { w: 3, h: 2 };
    case 'share':
    case 'donut':
      return { w: 4, h: 4 };
    case 'table':
      return { w: 6, h: 4 };
    default:
      return { w: 6, h: 4 };
  }
}

export function newId(prefix: string) {
  return prefix + Math.random().toString(36).slice(2, 8);
}

const overlaps = (a: Tile, b: Tile) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/**
 * Vertical compaction: every tile moves up as far as it can without overlapping.
 * `pinned` (the tile being dragged) keeps its place and others flow around it.
 */
export function compact(tiles: Tile[], pinned?: string): Tile[] {
  const out: Tile[] = [];
  const fixed = tiles.find((t) => t.id === pinned);
  if (fixed) out.push({ ...fixed, x: clamp(fixed.x, 0, COLS - fixed.w), y: Math.max(0, fixed.y) });
  const rest = tiles
    .filter((t) => t.id !== pinned)
    .sort((a, b) => a.y - b.y || a.x - b.x);
  for (const t of rest) {
    const c = { ...t, x: clamp(t.x, 0, COLS - Math.min(t.w, COLS)), w: Math.min(t.w, COLS) };
    c.y = 0;
    while (out.some((o) => overlaps(o, c))) c.y++;
    out.push(c);
  }
  return out;
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/** First free spot for a new tile of size w × h, scanning rows top-down. */
export function place(tiles: Tile[], w: number, h: number): { x: number; y: number } {
  for (let y = 0; ; y++) {
    for (let x = 0; x + w <= COLS; x++) {
      const probe = { x, y, w, h } as Tile;
      if (!tiles.some((t) => overlaps(t, probe))) return { x, y };
    }
  }
}

export function newDashboard(n: number): Dashboard {
  return { id: newId('d'), title: n ? `Dashboard ${n + 1}` : 'Overview', refresh: 60, tiles: [] };
}

/** Tables a query based on `base` can use: base plus everything reachable many → one. */
export function reachableFrom(base: string, rels: { from: { table: string }; to: { table: string } }[]): Set<string> {
  const seen = new Set([base]);
  const queue = [base];
  while (queue.length) {
    const t = queue.shift()!;
    for (const r of rels) {
      if (r.from.table === t && !seen.has(r.to.table)) {
        seen.add(r.to.table);
        queue.push(r.to.table);
      }
    }
  }
  return seen;
}
