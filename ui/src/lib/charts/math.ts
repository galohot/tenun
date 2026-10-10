// The arithmetic behind the stack modes and the waterfall. Pure functions with no imports
// at run time, so the node tests can load this file on its own.

import type { Tile } from '../api';

/** Sums and counts add up, so they can be stacked. An average, a minimum or a distinct count cannot. */
export const STACKABLE: readonly string[] = ['sum', 'count'];

export type StackMode = 'stacked' | 'side' | 'percent';

/** Can the values of this tile be piled on top of each other? */
export const stackable = (aggs: readonly string[]): boolean => aggs.every((a) => STACKABLE.includes(a));

/**
 * How a bar or area with several series is drawn. The default is stacked for sums and counts and
 * side by side for everything else. A saved choice that cannot hold (stacking averages) is ignored.
 * `perValue` is true when the series are the tile's values, false when they are the split groups.
 */
export function stackMode(tile: Pick<Tile, 'stack' | 'measures'>, perValue: boolean): StackMode {
  const used = perValue ? tile.measures : tile.measures.slice(0, 1);
  if (!stackable(used.map((m) => m.agg))) return 'side';
  return tile.stack ?? 'stacked';
}

/**
 * The 100 % view: every category is scaled so its series add up to 1. `series` is series × category.
 * Negative values count by their size, so the shares of a category add up to 1 in absolute terms.
 * A category with no total has no shares (null). Nothing is rounded.
 */
export function normalizeShares(series: (number | null)[][]): { shares: (number | null)[][]; totals: number[] } {
  const n = Math.max(0, ...series.map((s) => s.length));
  const totals = Array.from({ length: n }, (_, i) => series.reduce((sum, s) => sum + Math.abs(s[i] ?? 0), 0));
  const shares = series.map((s) => s.map((v, i) => (v === null || !totals[i] ? null : v / totals[i])));
  return { shares, totals };
}

export type StepKind = 'start' | 'up' | 'down' | 'flat' | 'end' | 'total';

export interface Step {
  /** Position on the axis. */
  index: number;
  /** The row this bar stands for, or null for the closing "Total" or "End" bar. */
  row: number | null;
  kind: StepKind;
  /** The bar covers `from` to `to`. A change is drawn from where the last bar ended. */
  from: number;
  to: number;
  /** The change for up, down and flat steps; the level for the others. */
  value: number;
}

/**
 * The bars of a waterfall.
 * By category: each value moves the running total up or down, starting at 0, and a closing
 * "total" bar shows where it ended.
 * By date: the first period is a start bar at its own value, every next period is the change from
 * the one before, and the closing "end" bar is the last value. A period with no value keeps the
 * level of the one before (it is a change of zero).
 */
export function waterfallSteps(values: (number | null)[], byDate: boolean): Step[] {
  const steps: Step[] = [];
  let level = 0;
  values.forEach((raw, i) => {
    const v = raw ?? (byDate ? level : 0);
    if (byDate && i === 0) {
      steps.push({ index: 0, row: 0, kind: 'start', from: 0, to: v, value: v });
      level = v;
      return;
    }
    const change = byDate ? v - level : v;
    const kind: StepKind = change > 0 ? 'up' : change < 0 ? 'down' : 'flat';
    steps.push({ index: i, row: i, kind, from: level, to: level + change, value: change });
    level += change;
  });
  if (steps.length) steps.push({ index: steps.length, row: null, kind: byDate ? 'end' : 'total', from: 0, to: level, value: level });
  return steps;
}

/** Which bars carry a number: the start, end and total bars, and the `n` biggest changes. */
export function labelledSteps(steps: Step[], n = 3): Set<number> {
  const out = new Set<number>();
  for (const s of steps) if (s.kind === 'start' || s.kind === 'end' || s.kind === 'total') out.add(s.index);
  steps
    .filter((s) => s.kind === 'up' || s.kind === 'down')
    .sort((a, b) => Math.abs(b.value) - Math.abs(a.value) || a.index - b.index)
    .slice(0, n)
    .forEach((s) => out.add(s.index));
  return out;
}

/** The middle value. An even count averages the two in the middle. */
export function median(values: number[]): number | null {
  if (!values.length) return null;
  const v = [...values].sort((a, b) => a - b);
  const mid = v.length >> 1;
  return v.length % 2 ? v[mid] : (v[mid - 1] + v[mid]) / 2;
}

/** True when every value sits far from zero, so an axis from zero would squeeze the dots. */
export function farFromZero(values: number[]): boolean {
  if (!values.length) return false;
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  if (lo > 0) return lo / hi > 0.5;
  if (hi < 0) return hi / lo > 0.5;
  return false;
}
