// Runs one tile's queries. Used by the dashboard and by the export, so an
// exported file holds exactly what the dashboard showed.

import { api, type Dim, type Filter, type Grain, type KpiInfo, type QueryResult, type Tile } from './api';
import { tileQuery, type DashFilter } from './tiles';

export async function runTile(tile: Tile, filters: DashFilter[]): Promise<QueryResult> {
  if (tile.type === 'kpi') return runKpi(tile, filters);
  const q = tileQuery(tile, filters)!;
  const d = tile.dims[0];
  const timeChart = d?.grain && (tile.type === 'bar' || tile.type === 'line' || tile.type === 'area');
  if (!timeChart) return api.query(q);
  // A time chart also asks where the data ends, so an unfinished last period
  // is shown as unfinished instead of as a drop.
  const [res, latest] = await Promise.all([
    api.query(q),
    api.query({ measures: [{ table: d.table, column: d.column, agg: 'max' }], dimensions: [], filters: q.filters }),
  ]);
  const maxRaw = latest.rows[0]?.[0];
  const keys = res.rows.map((r) => r[0]).filter((k): k is string => typeof k === 'string');
  const last = keys.sort().at(-1);
  if (typeof maxRaw === 'string' && last && d.grain !== 'day') {
    const end = new Date(addGrain(parse(last), d.grain!, 1).getTime() - DAY);
    if (parse(maxRaw).getTime() < end.getTime()) res.partial = { key: last, until: maxRaw.slice(0, 10) };
  }
  return res;
}

// ---- dates (UTC, ISO strings) -------------------------------------------

const DAY = 86_400_000;

function parse(s: string): Date {
  const [y, m, d] = s.slice(0, 10).split('-').map(Number);
  return new Date(Date.UTC(y, m - 1, d));
}

const iso = (d: Date) => d.toISOString().slice(0, 10);

export function addGrain(d: Date, g: Grain, n: number): Date {
  const r = new Date(d);
  if (g === 'day') r.setUTCDate(r.getUTCDate() + n);
  else if (g === 'week') r.setUTCDate(r.getUTCDate() + 7 * n);
  else if (g === 'month') r.setUTCMonth(r.getUTCMonth() + n);
  else if (g === 'quarter') r.setUTCMonth(r.getUTCMonth() + 3 * n);
  else r.setUTCFullYear(r.getUTCFullYear() + n);
  return r;
}

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const SHORT = MONTHS.map((m) => m.slice(0, 3));

/** "September 2026", "Q3 2026", "2026", "Week of Sep 7, 2026", "Sep 7, 2026". */
export function periodName(start: Date, g: Grain, short = false): string {
  const y = start.getUTCFullYear();
  const m = start.getUTCMonth();
  const day = start.getUTCDate();
  switch (g) {
    case 'year':
      return String(y);
    case 'quarter':
      return `Q${Math.floor(m / 3) + 1} ${y}`;
    case 'month':
      return short ? SHORT[m] : `${MONTHS[m]} ${y}`;
    case 'week':
      return short ? `${SHORT[m]} ${day}` : `Week of ${SHORT[m]} ${day}, ${y}`;
    default:
      return `${SHORT[m]} ${day}, ${y}`;
  }
}

/** "Oct 1–14" (same month) or "Sep 28 – Oct 4". */
function span(a: Date, b: Date, withYear: boolean): string {
  const left = `${SHORT[a.getUTCMonth()]} ${a.getUTCDate()}`;
  const right = a.getUTCMonth() === b.getUTCMonth() ? `${b.getUTCDate()}` : `${SHORT[b.getUTCMonth()]} ${b.getUTCDate()}`;
  const sep = a.getUTCMonth() === b.getUTCMonth() ? '–' : ' – ';
  return `${left}${sep}${right}${withYear ? `, ${b.getUTCFullYear()}` : ''}`;
}

// ---- the number tile ------------------------------------------------------

const plain = (filters: DashFilter[], tile: Tile): Filter[] =>
  filters.filter((f) => f.fromTile !== tile.id).map(({ fromTile: _, ...f }) => f);

const first = (r: QueryResult): number | null => {
  const v = r.rows[0]?.[r.columns.findIndex((c) => c.role === 'measure')];
  return typeof v === 'number' ? v : null;
};

async function runKpi(tile: Tile, filters: DashFilter[]): Promise<QueryResult> {
  const dim: Dim | undefined = tile.dims[0];
  // A number tile with only a date counts rows, like a chart with no value.
  const m = tile.measures[0] ?? { table: dim!.table, column: null, agg: 'count' as const };
  const fs = plain(filters, tile);
  const total = await api.query({ measures: [m], dimensions: [], filters: fs });
  const res: QueryResult = { ...total };

  // A number over time needs a date grouped by a grain; anything else is a plain total.
  if (!dim?.grain) {
    res.kpi = { value: first(total), period: 'All data', versus: null, prev: null, delta: null, partial: false, bars: [] };
    return res;
  }
  const g = dim.grain;
  const [series, latest] = await Promise.all([
    api.query({ measures: [m], dimensions: [dim], filters: fs, limit: 5000 }),
    api.query({ measures: [{ table: dim.table, column: dim.column, agg: 'max' }], dimensions: [], filters: fs }),
  ]);
  const rows = series.rows.filter((r) => typeof r[0] === 'string') as [string, number | null][];
  if (!rows.length) {
    res.kpi = { value: null, period: 'No data', versus: null, prev: null, delta: null, partial: false, bars: [] };
    return res;
  }
  const [lastKey, value] = rows[rows.length - 1];
  const start = parse(lastKey);
  const end = new Date(addGrain(start, g, 1).getTime() - DAY);
  const maxRaw = latest.rows[0]?.[0];
  const maxDay = typeof maxRaw === 'string' ? parse(maxRaw) : end;
  const partial = maxDay.getTime() < end.getTime() && g !== 'day';
  const prevStart = addGrain(start, g, -1);

  let prev: number | null;
  let period: string;
  let versus: string;
  if (partial) {
    // Compare the same number of days: Oct 1–14 against Sep 1–14.
    const days = Math.round((maxDay.getTime() - start.getTime()) / DAY);
    let prevEnd = new Date(prevStart.getTime() + days * DAY);
    const prevLast = new Date(start.getTime() - DAY);
    if (prevEnd > prevLast) prevEnd = prevLast;
    const dtype = series.columns[0]?.dtype;
    const hi = iso(prevEnd) + (dtype === 'datetime' || String(maxRaw).includes('T') ? 'T23:59:59' : '');
    const window: Filter = { table: dim.table, column: dim.column, grain: null, op: 'between', values: [iso(prevStart), hi] };
    prev = first(await api.query({ measures: [m], dimensions: [], filters: [...fs, window] }));
    period = span(start, maxDay, true);
    versus = span(prevStart, prevEnd, false);
  } else {
    const prevRow = rows.find(([k]) => k.slice(0, 10) === iso(prevStart));
    prev = prevRow ? prevRow[1] : null;
    period = periodName(start, g);
    versus = periodName(prevStart, g);
  }
  const bars = rows.slice(-12).map(([k, v]) => ({ label: periodName(parse(k), g), value: v }));
  res.kpi = {
    value,
    period,
    versus,
    prev,
    delta: value !== null && prev !== null && prev !== 0 ? (value - prev) / Math.abs(prev) : null,
    partial,
    bars,
  };
  return res;
}

/** "* Oct 2026 has data up to Oct 14." for a chart whose last period is unfinished. */
export function partialNote(res: QueryResult, grain: Grain | null | undefined): string | null {
  if (!res.partial || !grain) return null;
  const until = parse(res.partial.until);
  return `* ${periodName(parse(res.partial.key), grain)} has data up to ${SHORT[until.getUTCMonth()]} ${until.getUTCDate()}.`;
}
