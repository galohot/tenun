// Tile helpers: the query a tile sends, sensible defaults, and grid layout.

import type { Dashboard, Dim, DType, Filter, Measure, SemanticQuery, Table, Tile, TileType } from './api';

export const COLS = 12;

export interface DashFilter extends Filter {
  /** The tile the click came from; it is not filtered by its own selection. */
  fromTile: string;
}

export function tileQuery(tile: Tile, filters: DashFilter[]): SemanticQuery | null {
  if (!tile.measures.length && !tile.dims.length) return null;
  const dims = tile.type === 'kpi' ? tile.dims.slice(0, 1) : tile.type === 'donut' ? tile.dims.slice(0, 1) : tile.type === 'table' ? tile.dims : tile.dims.slice(0, 2);
  return {
    measures: tile.measures.length ? (tile.type === 'table' ? tile.measures : tile.measures.slice(0, 1)) : [],
    dimensions: dims,
    filters: filters.filter((f) => f.fromTile !== tile.id).map(({ fromTile: _, ...f }) => f),
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
