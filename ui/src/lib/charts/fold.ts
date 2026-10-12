// Folding the small bars into one "Other" (V4-D85d, V4-D99). The sum of the bars it hides is the value of
// "Other", which is right only for values that add up. Pure functions with only pure imports at run time, so
// the node tests can load this file; `common.ts` calls them.

import { foldable, splitKeep } from './math.ts';

/** A bar chart of one series after its small bars were folded: the categories and the values. */
export interface Folded<C> {
  categories: C[];
  values: (number | null)[];
}

/**
 * Keeps the first `max - 1` bars and puts the rest in one bar called `marker`, which is their sum. Returns null
 * when nothing is folded: the bars fit, there is more than one series, or the values do not add up (a ratio, an
 * average, or a value that reads a table repeated across the groups).
 */
export function foldSeries<C>(categories: C[], values: (number | null)[], series: number, max: number, addsUp: boolean, marker: C): Folded<C> | null {
  if (!foldable({ addsUp, categories: categories.length, max, series })) return null;
  const keep = max - 1;
  const rest = values.slice(keep).reduce<number>((s, v) => s + (v ?? 0), 0);
  return { categories: [...categories.slice(0, keep), marker], values: [...values.slice(0, keep), rest] };
}

/**
 * The groups of a split chart that get a series of their own, biggest first, and whether the rest become one
 * "Other" series. With more than `max` groups the biggest `max - 1` stay when the values add up; otherwise
 * every group stays.
 */
export function splitKept<T>(ranked: T[], max: number, addsUp: boolean): { kept: T[]; other: boolean } {
  const kept = ranked.slice(0, splitKeep(ranked.length, max, addsUp));
  return { kept, other: kept.length < ranked.length };
}
