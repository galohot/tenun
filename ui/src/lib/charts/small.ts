// Small multiples: one small chart for each value of a field. The engine sends one result whose rows
// are `[group, repeat value, value…]`; this file cuts it into one result for each chart and works out
// the scale the charts share. Pure code with no imports at run time, so the node tests can load it.

import type { Cell, QueryResult } from '../api';

const OTHER_KEY = '\u0000other';
const keyOf = (v: Cell | undefined): string => (v === null || v === undefined ? '\u0000null' : String(v));

export interface Part {
  /** The repeat value this chart is for. */
  key: Cell;
  /** The rows of this chart as a normal one-group result: `[group, value]`. */
  res: QueryResult;
}

/** One result for each repeat value the engine kept, in its drawing order. */
export function splitCharts(res: QueryResult): Part[] {
  if (!res.repeat) return [];
  const columns = [res.columns[0], ...res.columns.slice(2)];
  return res.repeat.keys.map((key) => {
    const rows = res.rows.filter((r) => keyOf(r[1]) === keyOf(key)).map((r) => [r[0], ...r.slice(2)]);
    const { repeat: _r, compare: _c, real: _real, ...rest } = res;
    return {
      key,
      res: { ...rest, columns, rows, other: rows.some((r) => r[0] === OTHER_KEY) || undefined },
    };
  });
}

/** The smallest of 1, 2, 2.5, 5 or 10 times a power of ten that is at least `x`. */
export function niceUp(x: number): number {
  if (!(x > 0)) return 1;
  const base = 10 ** Math.floor(Math.log10(x));
  return [1, 2, 2.5, 5, 10].map((n) => n * base).find((n) => n >= x * (1 - 1e-9)) ?? base * 10;
}

/**
 * The value axis that all the charts share: from zero (or lower, when a value is negative) to a round
 * number above the biggest value of any chart, in four steps of a round size so the ticks read cleanly.
 * Gaps are left out. Null when there is nothing to draw.
 */
export function sharedScale(parts: readonly Pick<Part, 'res'>[]): { min: number; max: number; interval: number } | null {
  const nums: number[] = [];
  for (const p of parts) for (const r of p.res.rows) for (const v of r.slice(1)) if (typeof v === 'number' && Number.isFinite(v)) nums.push(v);
  if (!nums.length) return null;
  const lo = Math.min(0, ...nums);
  const hi = Math.max(0, ...nums);
  if (hi === 0 && lo === 0) return { min: 0, max: 1, interval: 0.25 };
  const interval = niceUp((hi - lo) / 4);
  return { min: Math.floor(lo / interval) * interval, max: Math.ceil(hi / interval) * interval, interval };
}

/** How many charts sit side by side in a tile this wide. */
export function chartColumns(width: number, count: number): number {
  return Math.max(1, Math.min(count, Math.floor(width / 200), 4));
}

/**
 * Fixes the value axis of a chart option to a shared scale. An axis that already has its own range
 * (a share runs 0 to 100 %) is left alone. `horizontal` is a bar chart turned on its side, where the
 * values run along x.
 */
export function withScale<T extends object>(option: T, scale: { min: number; max: number; interval?: number } | null, horizontal: boolean): T {
  if (!scale) return option;
  const key = horizontal ? 'xAxis' : 'yAxis';
  const axis = (option as Record<string, unknown>)[key] as Record<string, unknown> | Record<string, unknown>[] | undefined;
  const one = Array.isArray(axis) ? axis[0] : axis;
  if (!one || one.max !== undefined) return option;
  const fixed = { ...one, min: scale.min, max: scale.max, ...(scale.interval ? { interval: scale.interval } : {}) };
  return { ...option, [key]: Array.isArray(axis) ? [fixed, ...axis.slice(1)] : fixed };
}

/**
 * Lets a small chart use its whole cell. The chart builder leaves room on the right for the name of a
 * line and under the axis for long labels; a cell has neither. A date axis keeps its labels (the
 * builder hides the ones that touch). The labels of a category axis are rotated and crowded in a cell
 * this small, so they go and the shared order stands in for them. `horizontal` is a bar chart on its side.
 */
export function fitToCell<T extends object>(option: T, dates: boolean, horizontal: boolean): T {
  const o = option as Record<string, unknown>;
  const grid = (o.grid ?? {}) as Record<string, unknown>;
  const key = horizontal ? 'yAxis' : 'xAxis';
  const axis = o[key] as Record<string, unknown> | Record<string, unknown>[] | undefined;
  const bare = (a: Record<string, unknown>) => ({ ...a, axisLabel: { ...((a.axisLabel ?? {}) as object), show: false } });
  const fixed = dates || !axis ? axis : Array.isArray(axis) ? axis.map(bare) : bare(axis);
  return { ...option, grid: { ...grid, left: 4, right: 8, top: 8 }, ...(fixed ? { [key]: fixed } : {}) };
}
