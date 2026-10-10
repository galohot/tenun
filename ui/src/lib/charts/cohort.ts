// The grid of a cohort table from the engine's rows `[cohort, periods since the start, value, size]`:
// one row for each cohort, one column for each period since it started. A cell the cohort has not
// reached yet stays empty. Pure code with no imports at run time.

export interface CohortRow {
  /** The first period of the cohort, as the engine writes it ("2026-01-01"). */
  key: string;
  /** How many entities the cohort has. */
  size: number;
  /** The value in each period since the start; null where the cohort has not got that far. */
  cells: (number | null)[];
}

export interface CohortGrid {
  rows: CohortRow[];
  /** The number of columns: the most periods any cohort has, counting the first. */
  columns: number;
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

export function cohortGrid(rows: readonly (readonly unknown[])[]): CohortGrid {
  const byCohort = new Map<string, CohortRow>();
  let columns = 0;
  for (const r of rows) {
    const [key, k, v, size] = [String(r[0]), Number(r[1]), r[2], r[3]];
    if (!Number.isInteger(k) || k < 0) continue;
    const row = byCohort.get(key) ?? { key, size: finite(size) ? size : 0, cells: [] };
    while (row.cells.length <= k) row.cells.push(null);
    row.cells[k] = finite(v) ? v : null;
    byCohort.set(key, row);
    columns = Math.max(columns, k + 1);
  }
  const out = [...byCohort.values()].sort((a, b) => a.key.localeCompare(b.key));
  for (const row of out) while (row.cells.length < columns) row.cells.push(null);
  return { rows: out, columns };
}

/**
 * The range the colours cover. The first column is the cohort itself (always 100 %, or its first
 * total), and would hide everything else, so the range comes from the later columns. When there
 * are none it comes from the first.
 */
export function cohortRange(grid: CohortGrid): { lo: number; hi: number } | null {
  const later = grid.rows.flatMap((r) => r.cells.slice(1)).filter(finite);
  const all = later.length ? later : grid.rows.flatMap((r) => r.cells).filter(finite);
  if (!all.length) return null;
  return { lo: Math.min(0, ...all), hi: Math.max(...all) };
}

/** What a period is called in the heading: "Month 3", from the grain of the dates. */
export function periodWord(grain: string | null | undefined): string {
  switch (grain) {
    case 'year':
      return 'Year';
    case 'quarter':
      return 'Quarter';
    case 'week':
      return 'Week';
    case 'day':
      return 'Day';
    default:
      return 'Month';
  }
}
