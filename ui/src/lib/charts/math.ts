// The arithmetic behind the stack modes and the waterfall. Pure functions with no imports
// at run time, so the node tests can load this file on its own.

import type { Measure, NumFormat, ShowAs, Tile } from '../api';

/** Sums and counts add up, so they can be stacked. An average, a minimum or a distinct count cannot. */
export const STACKABLE: readonly string[] = ['sum', 'count'];

export type StackMode = 'stacked' | 'side' | 'percent';

/** Can the values of this tile be piled on top of each other? */
export const stackable = (aggs: readonly string[]): boolean => aggs.every((a) => STACKABLE.includes(a));

/**
 * The unit a value is counted in. Two values with the same unit can be piled up and the total means
 * something. A currency is a unit of its own (Rp and Rp), a percent is another. Plain numbers share a
 * unit only when they come from the same column, and rows counted in one table share it too.
 * A sum of Amount and a sum of Quantity are different units: a stack of them adds money to units.
 */
export function unitOf(m: Pick<Measure, 'table' | 'column' | 'agg' | 'per'>, f?: NumFormat | null, how?: ShowAs | null): string {
  // A share or a rank is its own unit, whatever the value was.
  if (how === 'rank') return 'rank';
  if (how === 'pct_total' || how === 'pct_group' || how === 'pct_change') return 'percent';
  if (f?.currency) return `currency:${f.currency}`;
  if (f?.percent) return 'percent';
  if (m.per) return `ratio:${m.table}.${m.column}/${m.per.table}.${m.per.column}`;
  if (m.agg === 'count' || m.column === null) return `rows:${m.table}`;
  return `column:${m.table}.${m.column}`;
}

/**
 * How a bar or area with several series is drawn when the user has not chosen. A split on one value
 * is stacked. Several values (one series for each) are stacked only when every one is a sum or a
 * count and they all share one unit; otherwise they sit side by side.
 */
export function defaultStack(tile: Pick<Tile, 'measures' | 'formats' | 'showAs'>, perValue: boolean): StackMode {
  if (!perValue) return 'stacked';
  const units = new Set(tile.measures.map((m, i) => unitOf(m, tile.formats?.[i], tile.showAs?.[i])));
  return units.size <= 1 ? 'stacked' : 'side';
}

/**
 * How a bar or area with several series is drawn. A saved choice wins. A choice that cannot hold
 * (stacking averages) is ignored. `perValue` is true when the series are the tile's values, false
 * when they are the split groups.
 */
export function stackMode(tile: Pick<Tile, 'stack' | 'measures' | 'formats' | 'showAs'>, perValue: boolean): StackMode {
  if (!stackableTile(tile, perValue)) return 'side';
  return tile.stack ?? defaultStack(tile, perValue);
}

/**
 * Can the series of this tile be piled up? Every value has to add up: a sum or a count, not a ratio,
 * and not shown as a rank or a percent change (those do not add up either).
 */
export function stackableTile(tile: Pick<Tile, 'measures' | 'showAs'>, perValue: boolean): boolean {
  const used = perValue ? tile.measures : tile.measures.slice(0, 1);
  return used.every((m, i) => !m.per && stackable([m.agg]) && tile.showAs?.[i] !== 'rank' && tile.showAs?.[i] !== 'pct_change');
}

/**
 * Colour slots that follow the entity, not its position (a filter that removes a group must not
 * repaint the groups that stay). `memory` maps a series key to its palette slot and lives as long
 * as the tile does. Keys are given in first-seen order; a key never seen before takes the lowest
 * slot nobody holds. When every slot is held, it takes the lowest slot no shown key holds and the
 * entity that held it forgets it. With more keys than slots the last ones wrap round and are not
 * remembered. Returns the slot of each key, in order.
 */
export function assignSlots(memory: Map<string, number>, keys: readonly string[], slots: number): number[] {
  const shown = new Set(keys);
  return keys.map((k, i) => {
    const known = memory.get(k);
    if (known !== undefined) return known;
    const held = new Set(memory.values());
    let slot = Array.from({ length: slots }, (_, s) => s).find((s) => !held.has(s));
    if (slot === undefined) {
      const inUse = new Set(keys.filter((x) => memory.has(x)).map((x) => memory.get(x)!));
      slot = Array.from({ length: slots }, (_, s) => s).find((s) => !inUse.has(s));
      if (slot === undefined) return i % slots;
      for (const [other, s] of memory) if (s === slot && !shown.has(other)) memory.delete(other);
    }
    memory.set(k, slot);
    return slot;
  });
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

/** True when every value sits far from zero, so an axis from zero would squeeze the dots. */
export function farFromZero(values: number[]): boolean {
  if (!values.length) return false;
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  if (lo > 0) return lo / hi > 0.5;
  if (hi < 0) return hi / lo > 0.5;
  return false;
}
