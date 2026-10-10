// The grid of a calendar heat map: weeks are columns and weekdays are rows. The engine sends one
// value for each day of the last 12 months with data; this puts each day in its cell and works out the
// month outlines. Pure code with no imports at run time.

export interface CalCell {
  /** The day as the engine writes it: "2026-03-20". */
  key: string;
  week: number;
  row: number;
  /** Null: no data for this day. */
  value: number | null;
  /** The day of the month, 1 to 31. */
  day: number;
}

export interface CalMonth {
  /** 0 to 11. */
  month: number;
  year: number;
  /** The column of the first day, where the month's name is written. */
  week: number;
  /** The outline of the month's days, as corners in cell units (x across, y down). */
  outline: [number, number][];
}

export interface Calendar {
  weeks: number;
  cells: CalCell[];
  months: CalMonth[];
}

const DAY = 86_400_000;
const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

const parse = (s: string): number | null => {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s);
  return m ? Date.UTC(+m[1], +m[2] - 1, +m[3]) : null;
};

const iso = (ms: number): string => new Date(ms).toISOString().slice(0, 10);

/** The row of a weekday: Monday first puts Monday at the top, Sunday first puts Sunday there. */
export const rowOf = (ms: number, mondayFirst: boolean): number => {
  const dow = new Date(ms).getUTCDay(); // Sunday = 0
  return mondayFirst ? (dow + 6) % 7 : dow;
};

/** The weekday number (Monday = 1 … Sunday = 7) shown on a row. */
export const weekdayOfRow = (row: number, mondayFirst: boolean): number => (mondayFirst ? row + 1 : row === 0 ? 7 : row);

/**
 * Lays out the days. The grid runs from the first day of the first month that has a row to the last
 * day of the last month, so every month is whole. A day with no row has no value. Monday-first for
 * Indonesian, Sunday-first for English.
 */
export function calendarModel(rows: readonly (readonly unknown[])[], mondayFirst: boolean): Calendar {
  const byDay = new Map<number, number | null>();
  for (const r of rows) {
    const ms = typeof r[0] === 'string' ? parse(r[0]) : null;
    if (ms !== null) byDay.set(ms, finite(r[1]) ? r[1] : null);
  }
  if (!byDay.size) return { weeks: 0, cells: [], months: [] };
  const days = [...byDay.keys()];
  const first = new Date(Math.min(...days));
  const last = new Date(Math.max(...days));
  const from = Date.UTC(first.getUTCFullYear(), first.getUTCMonth(), 1);
  const to = Date.UTC(last.getUTCFullYear(), last.getUTCMonth() + 1, 0);
  const lead = rowOf(from, mondayFirst);
  const cells: CalCell[] = [];
  for (let ms = from; ms <= to; ms += DAY) {
    const i = Math.round((ms - from) / DAY) + lead;
    cells.push({ key: iso(ms), week: Math.floor(i / 7), row: i % 7, value: byDay.get(ms) ?? null, day: new Date(ms).getUTCDate() });
  }
  const months: CalMonth[] = [];
  let at = new Date(from);
  while (at.getTime() <= to) {
    const [y, m] = [at.getUTCFullYear(), at.getUTCMonth()];
    const a = cells.find((c) => c.key === iso(Date.UTC(y, m, 1)))!;
    const b = cells.find((c) => c.key === iso(Date.UTC(y, m + 1, 0)))!;
    months.push({ month: m, year: y, week: a.week, outline: outline(a, b) });
    at = new Date(Date.UTC(y, m + 1, 1));
  }
  return { weeks: Math.max(...cells.map((c) => c.week)) + 1, cells, months };
}

/**
 * The corners of the shape a month covers, from its first day `a` to its last day `b`: the part of the
 * first column below `a`, the whole columns between, and the part of the last column down to `b`.
 */
function outline(a: CalCell, b: CalCell): [number, number][] {
  return [
    [a.week, a.row],
    [a.week + 1, a.row],
    [a.week + 1, 0],
    [b.week + 1, 0],
    [b.week + 1, b.row + 1],
    [b.week, b.row + 1],
    [b.week, 7],
    [a.week, 7],
  ];
}
