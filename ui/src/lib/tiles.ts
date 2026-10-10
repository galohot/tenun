// Tile helpers: the query a tile sends, sensible defaults, and grid layout.

import type { Dashboard, Dim, DType, Filter, Measure, QueryResult, SemanticQuery, Table, Tile, TileType } from './api';

export const COLS = 12;

export interface DashFilter extends Filter {
  /** The tile the click came from; it is not filtered by its own selection. */
  fromTile: string;
}

/** The most values a bar, line or area tile shows side by side. */
export const MAX_CHART_MEASURES = 4;

/** A pivot reads up to this many row fields and values; its column field is one more. Same as the engine. */
export const PIVOT_MAX_ROWS = 3;
export const PIVOT_MAX_MEASURES = 4;
export const PIVOT_LIMIT = 5000;
/** A Gantt chart draws at most this many rows. Same as the engine. */
export const MAX_GANTT_ROWS = 100;

/** Tiles that show something the author typed or picked. They ask nothing and have no result. */
export const STATIC_TYPES: readonly TileType[] = ['text', 'image', 'filter'];
export const isStatic = (t: Tile | TileType): boolean => STATIC_TYPES.includes(typeof t === 'string' ? t : t.type);

/** What a static tile is drawn from: it has no rows. */
export const EMPTY_RESULT: QueryResult = { columns: [], rows: [], ignored_filters: [], ms: 0 };

/** Small multiples need a bar, line or area with a group. The engine and this file agree on it. */
export const isRepeated = (tile: Tile) =>
  !!tile.repeat && tile.dims.length > 0 && (tile.type === 'bar' || tile.type === 'line' || tile.type === 'area');

export const hasManyValues = (tile: Tile) =>
  (tile.type === 'bar' || tile.type === 'line' || tile.type === 'area') && tile.measures.length > 1 && !isRepeated(tile);

/** A pivot and a heat map ask the same kind of question: rows by one column field. */
export const isPivotLike = (type: TileType) => type === 'pivot' || type === 'heatmap';

/**
 * The question a tile asks. The server builds its own copy of this (`snapshot::tile_query`); this one
 * decides when a tile must ask again, so keep the two the same.
 */
export function tileQuery(tile: Tile, filters: DashFilter[]): SemanticQuery | null {
  if (isStatic(tile) || (!tile.measures.length && !tile.dims.length)) return null;
  const one = ['kpi', 'share', 'donut', 'waterfall', 'funnel', 'box', 'bullet', 'gantt', 'cohort'].includes(tile.type);
  const many = hasManyValues(tile);
  // A compare chart with one value reads two periods of its second field.
  const periods = tile.type === 'compare' && tile.measures.length < 2;
  const dims = ((): Dim[] => {
    // A histogram has no groups: it bins one number column.
    if (tile.type === 'histogram') return [];
    // A calendar has one cell for each day, whatever grain the field was saved with.
    if (tile.type === 'calendar') return tile.dims.slice(0, 1).map((d) => ({ ...d, grain: 'day' as const }));
    if (tile.type === 'sankey') return tile.dims.slice(0, 3); // the stages
    if (tile.type === 'compare') return tile.dims.slice(0, periods ? 2 : 1);
    // A heat map has one row field.
    if (isPivotLike(tile.type)) return [...tile.dims.slice(0, tile.type === 'heatmap' ? 1 : PIVOT_MAX_ROWS), ...(tile.cols ? [tile.cols] : [])]; // rows, then columns
    // The group, then the field that makes a chart for each of its values.
    if (isRepeated(tile)) return [tile.dims[0], tile.repeat!];
    if (one || many) return tile.dims.slice(0, 1);
    return tile.type === 'table' ? tile.dims : tile.dims.slice(0, 2);
  })();
  // A chart with a group but no value counts rows of the grouped table.
  const measures = ((): Measure[] => {
    if (!tile.measures.length) return tile.type === 'table' ? [] : [{ table: tile.dims[0].table, column: null, agg: 'count' as const }];
    if (tile.type === 'table') return tile.measures;
    if (tile.type === 'pivot') return tile.measures.slice(0, PIVOT_MAX_MEASURES);
    if (many) return tile.measures.slice(0, MAX_CHART_MEASURES);
    // Across and up, and a third value for the size of the dot.
    if (tile.type === 'scatter') return tile.measures.slice(0, 3);
    // Bullet: the value, then its target. Gantt: start, then end. Cohort: the entity, then a value to add up. Compare: the two values.
    if (['bullet', 'gantt', 'cohort'].includes(tile.type) || (tile.type === 'compare' && !periods)) return tile.measures.slice(0, 2);
    return tile.measures.slice(0, 1);
  })();
  const sort: { by: number; desc: boolean }[] = [];
  // A pivot keeps the engine's order: its rows and columns are not one list to sort. A funnel
  // with no chosen order puts the biggest stage first, and a Gantt chart the task that starts first.
  // A compare chart is ordered by the engine, and a calendar keeps its days in time order.
  const by = tile.sort ?? (tile.type === 'funnel' ? { by: 'value' as const } : tile.type === 'gantt' ? { by: 'value' as const, desc: false } : undefined);
  if (dims.length && by && !isPivotLike(tile.type) && !['compare', 'calendar', 'sankey'].includes(tile.type)) {
    if (by.by === 'value' && measures.length) sort.push({ by: dims.length, desc: by.desc ?? true });
    else if (by.by === 'label') sort.push({ by: 0, desc: by.desc ?? false });
  }
  // A calendar reads the latest days first and the engine cuts them to the last 12 months.
  if (tile.type === 'calendar') sort.push({ by: 0, desc: true });
  return {
    measures,
    dimensions: dims,
    filters: filters.filter((f) => f.fromTile !== tile.id).map(({ fromTile: _, ...f }) => f),
    sort,
    limit:
      tile.type === 'table'
        ? 500
        : isPivotLike(tile.type)
          ? PIVOT_LIMIT
          : tile.type === 'kpi' || tile.type === 'calendar'
            ? 400
            : tile.type === 'gantt'
              ? MAX_GANTT_ROWS
              : 2000,
  };
}

/**
 * What a tile's result depends on. The type, the top N, the bin width, the totals, the column field,
 * "Show as" and the comparison change what the server sends back, beside the question itself.
 */
export function resultKey(tile: Tile, filters: DashFilter[]): string {
  return JSON.stringify([
    tileQuery(tile, filters),
    tile.type,
    tile.type === 'kpi' ? tile.dims : null,
    tile.top ?? null,
    tile.bins ?? null,
    tile.totals ?? null,
    tile.cols ?? null,
    tile.showAs ?? null,
    tile.compare ?? null,
    tile.repeat ?? null,
    tile.layout ?? null,
    tile.sort ?? null,
    tile.periods ?? null,
    tile.spark ?? null,
    tile.expand ?? null,
  ]);
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
    case 'pivot':
    case 'heatmap':
      return { w: 8, h: 5 };
    case 'funnel':
      return { w: 6, h: 5 };
    case 'bullet':
      return { w: 6, h: 4 };
    case 'compare':
      return { w: 6, h: 5 };
    case 'sankey':
    case 'cohort':
    case 'gantt':
      return { w: 8, h: 5 };
    case 'calendar':
      return { w: 12, h: 3 };
    case 'text':
      return { w: 4, h: 2 };
    case 'image':
      return { w: 4, h: 3 };
    default:
      return { w: 6, h: 4 };
  }
}

/** The fewest rows a tile may be resized to: a note can be one row high. */
export const minHeight = (type: TileType): number => (type === 'text' ? 1 : 2);

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
