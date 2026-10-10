// The size of a dot on a scatter chart with a third value. The area follows the value, so a dot
// with four times the value is twice as wide. Pure code with no imports at run time.

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/** The smallest and largest dot, in px across. */
export const BUBBLE_MIN = 5;
export const BUBBLE_MAX = 36;

/** The width in px of a dot for `v`, when the biggest value is `max`. A missing or negative value gets the smallest dot. */
export function bubbleSize(v: number | null | undefined, max: number, smallest = BUBBLE_MIN, biggest = BUBBLE_MAX): number {
  if (!finite(v) || !(max > 0) || v <= 0) return smallest;
  return Math.max(smallest, biggest * Math.sqrt(Math.min(v, max) / max));
}

/** Rounds to two significant digits: 8,432 → 8,400. */
const round2 = (x: number) => Number(x.toPrecision(2));

/** Three values for the size legend: the biggest, a half and a tenth of it, rounded so they read cleanly. */
export function bubbleSamples(max: number): number[] {
  if (!(max > 0)) return [];
  const top = round2(max);
  return [top, round2(top / 2), round2(top / 10)].filter((v, i, all) => v > 0 && all.indexOf(v) === i);
}
