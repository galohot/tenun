// "Show value as": which ways a tile offers and how they read. The engine computes the numbers
// (`shape::show_as`); this file only decides what the Inspector lists and what the words are.
// Pure code with only pure imports at run time, so the node tests can load it.

import type { ShowAs, Tile } from './api';
import { addsUpAlong, addsUpHere, ADDING_WAYS, ALONG_WAYS, type Parts } from './measures.ts';

/** What the first group-by field is: a date, a part of a date (month of year…), or something else. */
export type Dim0Kind = 'date' | 'part' | 'other' | 'none';

/** The tile types that can show a value another way. A number tile has its own comparison. */
export const SHOW_AS_TYPES: readonly string[] = ['bar', 'line', 'area', 'table', 'pivot'];

/** The ways that read the points in time order. The engine ignores them unless the first field is a date or a date part. */
const ORDERED: readonly ShowAs[] = ['running', 'moving_3', 'moving_7', 'moving_12', 'change', 'pct_change'];

/** The choices in the Inspector. */
export const SHOW_AS_LABELS: Record<ShowAs, string> = {
  value: 'Value',
  pct_total: '% of total',
  pct_group: '% of group',
  running: 'Running total',
  moving_3: 'Moving average, 3 points',
  moving_7: 'Moving average, 7 points',
  moving_12: 'Moving average, 12 points',
  change: 'Change from the one before',
  pct_change: '% change from the one before',
  rank: 'Rank',
  ytd: 'Year to date',
};

/** The words that follow the name of a value: "Revenue (% of total)". Same as the engine's. */
export const SHOW_AS_WORDS: Record<ShowAs, string> = {
  value: '',
  pct_total: '% of total',
  pct_group: '% of group',
  running: 'running total',
  moving_3: '3-period average',
  moving_7: '7-period average',
  moving_12: '12-period average',
  change: 'change',
  pct_change: '% change',
  rank: 'rank',
  ytd: 'year to date',
};

/** How many fields the engine groups by for this tile. Same as `snapshot::tile_query`. */
export function groupCount(tile: Pick<Tile, 'type' | 'dims' | 'measures' | 'cols'>): number {
  if (tile.type === 'table') return tile.dims.length;
  if (tile.type === 'pivot') return Math.min(tile.dims.length, 3) + (tile.cols ? 1 : 0);
  if (tile.measures.length > 1) return Math.min(tile.dims.length, 1);
  return Math.min(tile.dims.length, 2);
}

/** The kind of a group-by field, from its type and grain. */
export function dim0Kind(dtype: string | undefined, grain: string | null | undefined): Dim0Kind {
  if (dtype !== 'date' && dtype !== 'datetime') return 'other';
  return grain === 'month_of_year' || grain === 'weekday' || grain === 'hour' ? 'part' : 'date';
}

/**
 * The ways a value can be shown on this tile. Share and rank always work. A group share needs a
 * split. Running, moving, change and percent change only make sense in an order, so they need a
 * date or a part of a date. Year to date needs a date.
 */
export function showAsOptions(tile: Pick<Tile, 'type' | 'dims' | 'measures' | 'cols'>, dim0: Dim0Kind, i?: number, parts?: Parts): ShowAs[] {
  if (!SHOW_AS_TYPES.includes(tile.type) || !groupCount(tile)) return [];
  const out: ShowAs[] = ['value', 'pct_total'];
  if (groupCount(tile) > 1) out.push('pct_group');
  if (dim0 === 'date' || dim0 === 'part') out.push(...ORDERED);
  out.push('rank');
  if (dim0 === 'date') out.push('ytd');
  // The ways that add values up are not offered for a value that does not add up (V4-D99). A running total and a
  // year to date only need it to add up along the first group (V4-D123).
  if (i === undefined) return out;
  return out.filter((o) => !ADDING_WAYS.includes(o) || (ALONG_WAYS.includes(o) ? addsUpAlong(tile, i, parts) : addsUpHere(tile, i, parts)));
}

/**
 * What the engine really does for value `i`: `null` when it shows the plain value. A way that does not
 * fit the tile is ignored by the engine, and a group share without a split is a share of the total.
 */
export function effectiveShowAs(
  tile: Pick<Tile, 'type' | 'dims' | 'measures' | 'cols' | 'showAs'>,
  i: number,
  dim0: Dim0Kind,
  parts?: Parts,
): ShowAs | null {
  const how = tile.showAs?.[i];
  if (!how || how === 'value' || !SHOW_AS_TYPES.includes(tile.type) || !groupCount(tile)) return null;
  // A share of the whole, a running total and a year to date add values up. The engine shows a value that does
  // not add up as it is, with words (V4-D85d, V4-D99), so its name does not take the suffix.
  if (ADDING_WAYS.includes(how) && !(ALONG_WAYS.includes(how) ? addsUpAlong(tile, i, parts) : addsUpHere(tile, i, parts))) return null;
  if (how === 'ytd' && dim0 !== 'date') return null;
  if (ORDERED.includes(how) && dim0 !== 'date' && dim0 !== 'part') return null;
  if (how === 'pct_group' && groupCount(tile) < 2) return 'pct_total';
  return how;
}

/** A share of the whole (or of the group): the axis runs from 0 to 100 %. */
export const isShare = (how: ShowAs | null | undefined): boolean => how === 'pct_total' || how === 'pct_group';

/** Cumulative shows add up when series are stacked; a rank or a percent change does not. */
export const addsUp = (how: ShowAs | null | undefined): boolean => how !== 'rank' && how !== 'pct_change';

/** Keeps `showAs` in step with `measures`: no trailing nulls, and gone when nothing is set. */
export function withShowAs(showAs: Tile['showAs'], i: number, how: ShowAs | null, count: number): Tile['showAs'] {
  const next: (ShowAs | null)[] = Array.from({ length: Math.max(count, i + 1) }, (_, j) => showAs?.[j] ?? null);
  next[i] = how === 'value' ? null : how;
  while (next.length && next[next.length - 1] === null) next.pop();
  return next.length ? next : undefined;
}
