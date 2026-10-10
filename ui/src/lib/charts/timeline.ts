// Dates on a Gantt chart: a day as milliseconds, the span the axis covers, and how long a bar is.
// Pure code with no imports at run time.

const DAY = 86_400_000;

/** A date or time from the engine ("2026-03-01" or "2026-03-01T09:30:00") as milliseconds, read as UTC. */
export function dayMs(v: unknown): number | null {
  if (typeof v !== 'string') return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2}))?)?/.exec(v);
  if (!m) return null;
  return Date.UTC(+m[1], +m[2] - 1, +m[3], +(m[4] ?? 0), +(m[5] ?? 0), +(m[6] ?? 0));
}

export interface Bar {
  start: number;
  /** The end of the bar. A date with no time ends at the end of its day, so a task on one day is one day long. */
  end: number;
  /** The length in whole days, at least 1 for a date with no time. */
  days: number;
}

/** The bar for a start and an end; null when either is missing or the end is before the start. */
export function barOf(start: unknown, end: unknown): Bar | null {
  const s = dayMs(start);
  let e = dayMs(end);
  if (s === null || e === null) return null;
  if (typeof end === 'string' && end.length === 10) e += DAY;
  if (e < s) return null;
  return { start: s, end: e, days: Math.max(1, Math.round((e - s) / DAY)) };
}

/** The span the axis covers: the earliest start to the latest end. */
export function spanOf(bars: readonly (Bar | null)[]): { min: number; max: number } | null {
  const real = bars.filter((b): b is Bar => b !== null);
  if (!real.length) return null;
  return { min: Math.min(...real.map((b) => b.start)), max: Math.max(...real.map((b) => b.end)) };
}

/** Is today inside the span? Then the chart draws its hairline. */
export const todayIn = (span: { min: number; max: number } | null, now: number): boolean => !!span && now >= span.min && now <= span.max;

/** Midnight UTC of the day that `now` falls in. */
export const dayStart = (now: number): number => Math.floor(now / DAY) * DAY;
