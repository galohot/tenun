// The arithmetic under a bullet chart: where the bars, the target ticks and the bands sit on one
// scale, and what share of its target a value reached. This is arithmetic on the values on screen
// (like the funnel steps), so it lives in the UI. Pure code with no imports at run time.

import type { TileLine } from '../api';

/** At most this many bands (muted ranges such as poor, fair and good) sit behind the bars. */
export const MAX_BANDS = 3;

/** The smallest of 1, 2, 2.5, 5 or 10 times a power of ten that is at least `x`. */
function niceUp(x: number): number {
  if (!(x > 0)) return 1;
  const base = 10 ** Math.floor(Math.log10(x));
  return [1, 2, 2.5, 5, 10].map((n) => n * base).find((n) => n >= x * (1 - 1e-9)) ?? base * 10;
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

export interface Band {
  from: number;
  to: number;
  label: string;
}

/** The bands of a tile: its `band` lines, at most three, lowest first. A band with no range is left out. */
export function bandsOf(lines: readonly TileLine[] | undefined): Band[] {
  return (lines ?? [])
    .filter((l) => l.kind === 'band' && finite(l.value) && finite(l.to) && l.value !== l.to)
    .map((l) => ({ from: Math.min(l.value!, l.to!), to: Math.max(l.value!, l.to!), label: l.label?.trim() ?? '' }))
    .sort((a, b) => a.from - b.from || a.to - b.to)
    .slice(0, MAX_BANDS);
}

/** The scale every bar of a bullet chart shares: from zero (or a round number below, for a negative value) to a round number above everything drawn. */
export function bulletDomain(nums: readonly (number | null | undefined)[]): { lo: number; hi: number } {
  const v = nums.filter(finite);
  const top = Math.max(0, ...v);
  const bottom = Math.min(0, ...v);
  return { lo: bottom < 0 ? -niceUp(-bottom) : 0, hi: top > 0 ? niceUp(top) : 1 };
}

/** Where a value falls on the scale, 0 to 1. A value outside the scale is held at the edge. */
export function at(v: number, d: { lo: number; hi: number }): number {
  return Math.max(0, Math.min(1, (v - d.lo) / (d.hi - d.lo)));
}

/** What share of its target a value reached: 0.82 for 82 % of the target. A missing value, or a target of zero or less, has none. */
export function shareOfTarget(value: number | null | undefined, target: number | null | undefined): number | null {
  return finite(value) && finite(target) && target > 0 ? value / target : null;
}

/** The numbers of the scale's axis: the lowest, the highest and the steps between. */
export function bulletTicks(d: { lo: number; hi: number }, count = 4): number[] {
  return Array.from({ length: count + 1 }, (_, i) => d.lo + ((d.hi - d.lo) * i) / count);
}
