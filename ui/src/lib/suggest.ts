// "Also try": other tile types that suit the fields a tile already has. Types that do not fit are
// never offered. Pure code with only pure imports at run time, so the node tests can load it.

import type { Dim, DType, Table, Tile, TileType } from './api';
import { addsUp, type Parts } from './measures.ts';
import { placeLevelOf } from './places.ts';

export interface Suggestion {
  type: TileType;
  /** The tile as it would be after a click. */
  variant: Tile;
}

/** A group-by field with at most this many values counts as a category small enough for a bar or a share strip. */
export const SMALL_CATEGORY = 12;
export const MAX_SUGGESTIONS = 3;

const dtypeOf = (tables: Pick<Table, 'id' | 'columns'>[], table: string, column: string | null): DType | undefined =>
  column === null ? 'number' : tables.find((t) => t.id === table)?.columns.find((c) => c.name === column)?.dtype;

const isDate = (d: DType | undefined) => d === 'date' || d === 'datetime';

/**
 * `groups` is how many different values the first group-by field has in the current result, or
 * null when it is not known. The rules, in this order:
 * - a date and a value: line, area, bar
 * - a category of 12 values or fewer: bar, share, table
 * - two values and a category: scatter
 * - one number column alone: histogram, number
 * - a category and a date: pivot
 */
export function suggestTypes(tile: Tile, tables: Pick<Table, 'id' | 'columns'>[], groups: number | null, parts?: Parts): Suggestion[] {
  const out: Suggestion[] = [];
  const add = (type: TileType, change: Partial<Tile> = {}) => {
    if (out.some((s) => s.type === type) || type === tile.type) return;
    // A preview is a plain chart: Advanced layouts (small multiples, panels, indexed, combo) stay with the tile.
    out.push({ type, variant: { ...tile, repeat: undefined, ownScale: undefined, layout: undefined, mix: undefined, periods: undefined, spark: undefined, expand: undefined, ...change, type } });
  };
  const d0: Dim | undefined = tile.dims[0];
  const d1: Dim | undefined = tile.dims[1];
  const t0 = d0 ? dtypeOf(tables, d0.table, d0.column) : undefined;
  const t1 = d1 ? dtypeOf(tables, d1.table, d1.column) : undefined;
  const m0 = tile.measures[0];
  const hasValue = tile.measures.length > 0;
  const group = (d: DType | undefined) => d === 'text' || d === 'bool';
  // Tiles with no fields to suit, and tiles that are not charts of fields, get nothing.
  // Notes, pictures and the Advanced charts of build 13 read their fields in their own way; they get no previews.
  if (['text', 'image', 'filter', 'bullet', 'compare', 'sankey', 'calendar', 'cohort', 'gantt'].includes(tile.type) || (!hasValue && !d0)) return out;

  // A column of places, by its name, with one value: a map (build 06).
  const level = d0 && group(t0) && !d1 && tile.measures.length <= 1 ? placeLevelOf(d0.column) : null;
  if (level) add('map', { dims: [d0], measures: tile.measures.slice(0, 1), map: { level, ...(level === 'country' ? {} : { country: 'ID' }) } });
  if (d0 && isDate(t0)) {
    for (const type of ['line', 'area', 'bar'] as const) add(type, { horizontal: false });
  }
  if (d0 && group(t0) && groups !== null && groups <= SMALL_CATEGORY) {
    add('bar', { horizontal: true });
    // Shares are the parts of a whole: only values that add up here (V4-D85d, V4-D112).
    if (!m0 || addsUp(tile, parts)) add('share', { measures: tile.measures.slice(0, 1) });
    add('table');
  }
  if (d0 && group(t0) && tile.measures.length >= 2) {
    add('scatter', { measures: tile.measures.slice(0, 2), dims: [d0] });
  }
  if (!d0 && m0 && m0.column !== null && dtypeOf(tables, m0.table, m0.column) === 'number') {
    add('histogram', { measures: [m0], dims: [] });
    add('kpi', { measures: [m0], dims: [] });
  }
  // A category and a date: the category down the side, the dates across the top.
  if (d0 && d1 && ((group(t0) && isDate(t1)) || (isDate(t0) && group(t1)))) {
    const [rows, cols] = group(t0) ? [d0, d1] : [d1, d0];
    add('pivot', { dims: [rows], cols, measures: tile.measures.slice(0, 4) });
  }
  return out.slice(0, MAX_SUGGESTIONS);
}
