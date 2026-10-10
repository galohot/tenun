// Shared by every chart builder: colours, the pivot of a result into series, axis and
// tooltip styling, and the labels of categories.
//
// Mark specs (dataviz method): bars ≤ 24 px thick with a 4 px rounded end and a
// square base; 2 px lines; area fill at 10 %; a 2 px surface gap between stacked
// segments; hairline solid grid; text in text colours, never in series colours.

import type { EChartsCoreOption } from 'echarts/core';
import type { Cell, QueryResult, Tile } from '../api';
import { cell, measureLabel, OTHER_KEY, type Named } from '../format';

export interface Colors {
  surface: string;
  text: string;
  text2: string;
  grid: string;
  series: string[];
  raised: string;
  /** Status colours. Always paired with a word, a sign or an icon. */
  good: string;
  bad: string;
}

export function readColors(el: Element = document.documentElement): Colors {
  const cs = getComputedStyle(el);
  const v = (n: string) => cs.getPropertyValue(n).trim();
  return {
    surface: v('--surface'),
    raised: v('--raised'),
    text: v('--text'),
    text2: v('--axis-text'),
    grid: v('--grid-line'),
    series: [1, 2, 3, 4, 5, 6, 7, 8].map((i) => v(`--s${i}`)),
    good: v('--success'),
    bad: v('--danger'),
  };
}

const MAX_SERIES = 8;
export const OTHER = 'Other';

/** Key used to compare a category value with a filter value. */
export const key = (v: Cell) => (v === null ? '\u0000null' : String(v));

export interface Pivot {
  categories: Cell[];
  series: { name: string; raw: Cell; values: (number | null)[] }[];
}

/** Several values on one group: each value is its own series. */
export const bySeriesPerMeasure = (res: QueryResult) => {
  const nd = res.columns.filter((c) => c.role === 'dimension').length;
  return nd < 2 && res.columns.length - nd > 1;
};

/** Turns rows [dim0, dim1?, measure...] into categories × series. */
export function pivot(res: QueryResult, tile: Tile, tables: Named[] = []): Pivot {
  const nd = res.columns.filter((c) => c.role === 'dimension').length;
  const mi = nd; // first measure column
  const catIdx = new Map<string, number>();
  const categories: Cell[] = [];
  for (const r of res.rows) {
    const k = key(r[0]);
    if (!catIdx.has(k)) {
      catIdx.set(k, categories.length);
      categories.push(r[0]);
    }
  }
  if (nd < 2 || tile.dims.length < 2) {
    // One series for each value column; a chart with a single value has one.
    const series: Pivot['series'] = [];
    for (let k = mi; k < Math.max(res.columns.length, mi + 1); k++) {
      const values = categories.map(() => null as number | null);
      for (const r of res.rows) values[catIdx.get(key(r[0]))!] = r[k] as number | null;
      const m = tile.measures[k - mi];
      const many = res.columns.length - mi > 1;
      const name = many && m ? measureLabel(m, tables) : (res.columns[k]?.name ?? '');
      series.push({ name, raw: name, values });
    }
    return { categories, series };
  }
  // Split series: keep the biggest seven, fold the rest into Other.
  const totals = new Map<string, { raw: Cell; total: number }>();
  for (const r of res.rows) {
    const k = key(r[1]);
    const t = totals.get(k) ?? { raw: r[1], total: 0 };
    t.total += Math.abs((r[mi] as number) ?? 0);
    totals.set(k, t);
  }
  const ranked = [...totals.entries()].sort((a, b) => b[1].total - a[1].total);
  const keep = ranked.length > MAX_SERIES ? ranked.slice(0, MAX_SERIES - 1) : ranked;
  // Stable colour per entity: order kept series by their value, not by rank.
  keep.sort((a, b) => String(a[1].raw).localeCompare(String(b[1].raw), undefined, { numeric: true }));
  const names = new Map(keep.map(([k], i) => [k, i]));
  const series: Pivot['series'] = keep.map(([, v]) => ({
    name: cell(v.raw, res.columns[1].dtype, tile.dims[1]?.grain),
    raw: v.raw,
    values: categories.map(() => null),
  }));
  const hasOther = ranked.length > keep.length;
  if (hasOther) series.push({ name: OTHER, raw: OTHER, values: categories.map(() => null) });
  for (const r of res.rows) {
    const si = names.get(key(r[1])) ?? (hasOther ? series.length - 1 : -1);
    if (si < 0) continue;
    const ci = catIdx.get(key(r[0]))!;
    const prev = series[si].values[ci] ?? 0;
    series[si].values[ci] = prev + ((r[mi] as number) ?? 0);
  }
  return { categories, series };
}

export function tooltipBase(c: Colors) {
  return {
    backgroundColor: c.raised,
    borderWidth: 0,
    padding: [8, 10],
    textStyle: { color: c.text, fontSize: 12, fontFamily: 'inherit' },
    extraCssText: 'box-shadow: 0 2px 12px rgb(0 0 0 / 14%); border-radius: 8px;',
  };
}

export function axisCommon(c: Colors) {
  return {
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: { color: c.text2, fontSize: 11, hideOverlap: true },
    splitLine: { lineStyle: { color: c.grid, width: 1, type: 'solid' as const } },
  };
}

export interface BuildArgs {
  res: QueryResult;
  tile: Tile;
  colors: Colors;
  /** Category values selected by a cross-filter from this tile. */
  selected: Cell[] | null;
  /** Size in px, for label density and how many categories fit. */
  width: number;
  height: number;
  /** For the names of several values. */
  tables?: Named[];
}

export interface Built {
  option: EChartsCoreOption;
  /** The category behind each axis position; null for the folded "Other". */
  categories: (Cell | undefined)[];
  /**
   * How a click finds the value to filter by. Not set: the position on the category axis,
   * looked up in `categories`. `item`: the dot or bar that was hit. `x`: a point on a number axis.
   */
  pick?:
    | { by: 'item'; value: (series: number, index: number) => Cell | undefined }
    | { by: 'x'; value: (x: number) => Cell | undefined };
  /** A line under the chart that says what the reader might misread. */
  note?: string | null;
}

/** Escapes a value from the data before it goes into tooltip HTML. */
export const esc = (v: unknown): string =>
  String(v).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

/** The text of a category on an axis or in a tooltip: dates by grain, "Other" for the folded bar. */
export function makeLabel(res: QueryResult, tile: Tile) {
  const dim0 = res.columns[0];
  const grain = tile.dims[0]?.grain;
  const partialKey = res.partial?.key;
  return (v: Cell) =>
    v === FOLDED ? 'Other' : cell(v, dim0.dtype, grain, true) + (partialKey !== undefined && v === partialKey ? '*' : '');
}

export const FOLDED = OTHER_KEY;

/** Keeps the biggest categories that fit and folds the rest into one "Other" bar. */
export function fold(p: Pivot, max: number): Pivot {
  if (p.categories.length <= max || p.series.length !== 1) return p;
  const keep = max - 1;
  const values = p.series[0].values;
  const rest = values.slice(keep).reduce<number>((s, v) => s + (v ?? 0), 0);
  return {
    categories: [...p.categories.slice(0, keep), FOLDED],
    series: [{ ...p.series[0], values: [...values.slice(0, keep), rest] }],
  };
}

