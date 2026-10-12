// The maths behind reference lines and the trend line. Pure functions with no imports at run time,
// so the node tests can load this file on its own. The UI draws; nothing here touches ECharts.

import type { Grain } from '../api';
/** The middle value. An even count averages the two in the middle. */
export function median(values: number[]): number | null {
  if (!values.length) return null;
  const v = [...values].sort((a, b) => a - b);
  const mid = v.length >> 1;
  return v.length % 2 ? v[mid] : (v[mid - 1] + v[mid]) / 2;
}

export type StatKind = 'average' | 'median' | 'min' | 'max';

/** A tile has at most this many lines (a trend counts as one). */
export const MAX_LINES = 4;

/**
 * One number for each category, as the chart shows it: the first series, or the total of all series
 * when they pile up on each other or are the groups of a split. A category with no value is null.
 */
export function shownSeries(series: { values: (number | null)[] }[], totals: boolean): (number | null)[] {
  if (!series.length) return [];
  if (!totals) return [...series[0].values];
  return series[0].values.map((_, i) => {
    const vs = series.map((s) => s.values[i]).filter((v): v is number => typeof v === 'number');
    return vs.length ? vs.reduce((a, b) => a + b, 0) : null;
  });
}

/**
 * Reference lines and a trend read the numbers shown. A 100 % chart has none. On a split chart they read
 * the total of the groups, which is right only when the values add up (V4-D85d): for a ratio or an average
 * there is no such total, so the chart draws no lines.
 */
export function linesAllowed(o: { percent: boolean; perValue: boolean; multi: boolean; addsUp: boolean }): boolean {
  return !o.percent && (o.addsUp || o.perValue || !o.multi);
}

/** The numbers a statistic is read from: no gaps, and none from a category that `skip` names ("Other"). */
export function usableValues<C>(values: (number | null)[], categories: C[], skip: (c: C) => boolean): number[] {
  return values.filter((v, i): v is number => typeof v === 'number' && Number.isFinite(v) && !skip(categories[i]));
}

/** Average, median, lowest or highest of the numbers; null when there are none. */
export function statOf(kind: StatKind, values: number[]): number | null {
  if (!values.length) return null;
  switch (kind) {
    case 'average':
      return values.reduce((a, b) => a + b, 0) / values.length;
    case 'median':
      return median(values);
    case 'min':
      return Math.min(...values);
    case 'max':
      return Math.max(...values);
  }
}

export interface TrendFit {
  slope: number;
  intercept: number;
  /** R²: how much of the ups and downs the straight line explains, 0 to 1. */
  r2: number;
  /** How many points the fit used. */
  n: number;
}

/**
 * Ordinary least squares. `ys` are the values; `xs` are their places (the index when not given, so
 * equal steps). A point with no number is left out, and so is any index in `exclude` (the period
 * that is not over yet). Needs three points and some spread in x; otherwise null.
 */
export function trendFit(ys: (number | null)[], xs?: (number | null)[], exclude: ReadonlySet<number> = new Set()): TrendFit | null {
  const pts: [number, number][] = [];
  ys.forEach((y, i) => {
    const x = xs ? xs[i] : i;
    if (exclude.has(i) || typeof y !== 'number' || typeof x !== 'number' || !Number.isFinite(y) || !Number.isFinite(x)) return;
    pts.push([x, y]);
  });
  const n = pts.length;
  if (n < 3) return null;
  const mx = pts.reduce((s, p) => s + p[0], 0) / n;
  const my = pts.reduce((s, p) => s + p[1], 0) / n;
  let sxx = 0;
  let sxy = 0;
  let syy = 0;
  for (const [x, y] of pts) {
    sxx += (x - mx) ** 2;
    sxy += (x - mx) * (y - my);
    syy += (y - my) ** 2;
  }
  if (sxx === 0) return null;
  const slope = sxy / sxx;
  // A flat set of values has no ups and downs to explain: no trend.
  const r2 = syy === 0 ? 0 : Math.min(1, (sxy * sxy) / (sxx * syy));
  return { slope, intercept: my - slope * mx, r2, n };
}

/** The height of the fitted line at `x`. */
export const fitAt = (fit: TrendFit, x: number): number => fit.intercept + fit.slope * x;

/** R² in words: above 0.7 well, from 0.3 to 0.7 loosely, below 0.3 no clear trend. */
export function fitWords(r2: number): 'fits well' | 'fits loosely' | 'no clear trend' {
  return r2 > 0.7 ? 'fits well' : r2 >= 0.3 ? 'fits loosely' : 'no clear trend';
}

/** The word for one step along a date axis: "month" for a monthly chart. */
export function stepWord(grain: Grain | null | undefined): string {
  switch (grain) {
    case 'year':
      return 'year';
    case 'quarter':
      return 'quarter';
    case 'month':
    case 'month_of_year':
      return 'month';
    case 'week':
      return 'week';
    case 'day':
    case 'weekday':
      return 'day';
    case 'hour':
      return 'hour';
    default:
      return 'step';
  }
}

/**
 * What the hover text says: "Rises about 1,2 jt per month. It fits well." `amount` is the size of
 * the slope, already written as a number; `per` is what one step is ("month"). Below R² 0.3 the
 * words say there is no clear trend.
 */
export function trendSentence(fit: TrendFit, amount: string, per: string): string {
  const way = fit.slope < 0 ? 'Falls' : 'Rises';
  const words = fitWords(fit.r2);
  const tail = words === 'no clear trend' ? 'There is no clear trend.' : `It ${words}.`;
  return `${way} about ${amount} ${per}. ${tail}`;
}

export type Place = 'above' | 'below' | 'hide';

/**
 * Where the name of each level line goes, so names never sit on each other. `values` are the levels
 * of the lines; the value axis runs `lo` to `hi` over `length` px, higher values nearer the top.
 * A name is `size` px tall and goes above its line, else below it, else it is left out (the line
 * stays). The highest line chooses first.
 */
export function placeLevelLabels(values: number[], lo: number, hi: number, length: number, size = 15): Place[] {
  const span = hi - lo || 1;
  const y = (v: number) => ((hi - v) / span) * length;
  const taken: [number, number][] = [];
  const free = (a: number, b: number) => a >= -1 && b <= length + 1 && taken.every(([c, d]) => b <= c || a >= d);
  const out: Place[] = values.map(() => 'hide');
  [...values.keys()]
    .sort((i, j) => values[j] - values[i])
    .forEach((i) => {
      const at = y(values[i]);
      for (const [place, a, b] of [['above', at - size, at], ['below', at, at + size]] as const) {
        if (free(a, b)) {
          taken.push([a, b]);
          out[i] = place;
          return;
        }
      }
    });
  return out;
}

/**
 * The same for upright lines, whose names sit side by side along the top. A name is `size` px wide
 * and centred on its line. It goes in the first row ('above'); when that is taken it goes in a second
 * row ('below') and only when both are taken is it left out. The leftmost line chooses first.
 * `overhang` is the room in px beyond each end of the plot where a name may reach.
 */
export function placeUprightLabels(values: number[], lo: number, hi: number, length: number, size: (i: number) => number, overhang = 0): Place[] {
  const span = hi - lo || 1;
  const x = (v: number) => ((v - lo) / span) * length;
  const rows: [number, number][][] = [[], []];
  const out: Place[] = values.map(() => 'hide');
  [...values.keys()]
    .sort((i, j) => values[i] - values[j])
    .forEach((i) => {
      const [a, b] = [x(values[i]) - size(i) / 2, x(values[i]) + size(i) / 2];
      const row = a >= -1 - overhang && b <= length + 1 + overhang ? rows.findIndex((taken) => taken.every(([c, d]) => b <= c || a >= d)) : -1;
      if (row >= 0) {
        rows[row].push([a, b]);
        out[i] = row === 0 ? 'above' : 'below';
      }
    });
  return out;
}
