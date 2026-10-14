// Shared by every chart builder: colours, the pivot of a result into series, axis and
// tooltip styling, and the labels of categories.
//
// Mark specs (dataviz method): bars ≤ 24 px thick with a 4 px rounded end and a
// square base; 2 px lines; area fill at 10 %; a 2 px surface gap between stacked
// segments; hairline solid grid; text in text colours, never in series colours.

import type { EChartsCoreOption } from 'echarts/core';
import type { Cell, QueryResult, Tile } from '../api';
import { cell, measureLabel, OTHER_KEY, resultDim0, type Named } from '../format';
import { effectiveShowAs } from '../showas';
import { assignSlots } from './math';
import { foldSeries, splitKept } from './fold';
import { addsUp as tileAddsUp } from '../measures';

export interface Colors {
  surface: string;
  text: string;
  text2: string;
  grid: string;
  series: string[];
  /** The sequential ramp of the heat map, light to dark (dark mode has its own steps). */
  ramp: string[];
  /** The diverging ramp of a map of a change: below zero, near it, above it (build 06). */
  div: string[];
  /** The colour of a hole in the surface: a shape of a map with no data. */
  inset: string;
  raised: string;
  /** Status colours. Always paired with a word, a sign or an icon. */
  good: string;
  bad: string;
  /** A value below the usual (v5 build 09): the warning colour. */
  warn: string;
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
    ramp: [1, 2, 3, 4, 5, 6, 7].map((i) => v(`--ramp-${i}`)),
    div: [1, 2, 3, 4, 5, 6, 7].map((i) => v(`--div-${i}`)),
    inset: v('--inset'),
    good: v('--success'),
    bad: v('--danger'),
    warn: v('--warn'),
  };
}

export const MAX_SERIES = 8;
export const OTHER = 'Other';

/** Key used to compare a category value with a filter value. */
export const key = (v: Cell) => (v === null ? '\u0000null' : String(v));

export interface Pivot {
  categories: Cell[];
  /** `slot` is the palette slot of the colour; null for the folded "Other", which is grey. */
  series: { name: string; raw: Cell; values: (number | null)[]; slot: number | null }[];
}

// The colours of a tile's split groups, kept for the session. A filter that takes a group away
// must not repaint the groups that stay, so a group keeps the slot it had when first drawn.
const slotMemory = new Map<string, Map<string, number>>();

/** The palette slot of each key, kept per tile, per split field and per source table (see `assignSlots`). */
export function slotsFor(tile: Tile, keys: string[]): number[] {
  const d = tile.dims[1];
  const source = [...new Set(tile.measures.map((m) => m.table))].join(',');
  const id = `${tile.id}|${source}|${d ? `${d.table}.${d.column}.${d.grain ?? ''}` : ''}`;
  let memory = slotMemory.get(id);
  if (!memory) slotMemory.set(id, (memory = new Map()));
  return assignSlots(memory, keys, MAX_SERIES);
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
      const name = many && m ? measureLabel(m, tables, effectiveShowAs(tile, k - mi, resultDim0(res, tile), res.parts)) : (res.columns[k]?.name ?? '');
      series.push({ name, raw: name, values, slot: k - mi });
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
  // "Other" adds the small groups up. That is right for a sum or a count and wrong for a ratio, an
  // average or any value that does not add up (V4-D85d): those keep every group.
  const keep = splitKept(ranked, MAX_SERIES, tileAddsUp(tile, res.parts)).kept;
  // Order kept series by name. Their colour comes from the tile's colour memory, not from this order.
  keep.sort((a, b) => String(a[1].raw).localeCompare(String(b[1].raw), undefined, { numeric: true }));
  const names = new Map(keep.map(([k], i) => [k, i]));
  const slots = slotsFor(tile, keep.map(([k]) => k));
  const series: Pivot['series'] = keep.map(([, v], i) => ({
    name: cell(v.raw, res.columns[1].dtype, tile.dims[1]?.grain),
    raw: v.raw,
    values: categories.map(() => null),
    slot: slots[i],
  }));
  const hasOther = ranked.length > keep.length;
  if (hasOther) series.push({ name: OTHER, raw: OTHER, values: categories.map(() => null), slot: null });
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
  /** Small multiples: keep every bar of a chart; the charts share a scale and nothing may fold into "Other". */
  noFold?: boolean;
  /** Sankey: the stage (index of the field in the tile) the selection belongs to. */
  selectedStage?: number | null;
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
    | {
        by: 'item';
        /** `type` is ECharts' `dataType` of the mark ("node" or "edge" on a Sankey chart). */
        value: (series: number, index: number, type?: string) => Cell | undefined;
        /** Sankey: which field of the tile the value belongs to (a node is a value of the field of its stage). */
        stage?: (series: number, index: number, type?: string) => number | undefined;
      }
    | { by: 'x'; value: (x: number) => Cell | undefined };
  /** A line under the chart that says what the reader might misread. */
  note?: string | null;
  /** The way out the note offers, as a link beside it (Advanced): draw the values in separate panels. */
  noteAction?: 'panels';
  /** What a value that does not add up made the chart leave out, in one line ("Averages are not added up, so there is no Other bar."). */
  gateNote?: string | null;
  /** How many plots stand above each other, when more than one. A click is found in any of them. */
  grids?: number;
  /** The categories run down the side (horizontal bars, box plots). Not set: read it from the tile. */
  horizontal?: boolean;
  /** How many rows run down the side, when the chart needs room for each (compare, Gantt). The tile scrolls when they do not fit. */
  rows?: number;
  /** Scatter with a third value: three sample dots that show what size means. */
  sizeLegend?: { title: string; items: { size: number; label: string }[] };
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

/** Keeps the biggest categories that fit and folds the rest into one "Other" bar. Only values that add up are folded. */
export function fold(p: Pivot, max: number, addsUp = true): Pivot {
  // The rest are added up into one bar, which is wrong for a value that does not add up (V4-D85d).
  const f = foldSeries(p.categories, p.series[0]?.values ?? [], p.series.length, max, addsUp, FOLDED);
  return f ? { categories: f.categories, series: [{ ...p.series[0], values: f.values }] } : p;
}

