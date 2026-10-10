// Human formatting for numbers, dates and field names.

import { format as d3format } from 'd3-format';
import { utcFormat } from 'd3-time-format';
import type { Agg, Cell, Dim, DType, Grain, Measure, Table, Tile } from './api';

const compactFmt = d3format('.3~s');
const groupFmt = d3format(',.2~f');
const intFmt = d3format(',d');

/** 1,284 · 12.9K · 4.2M · 1.3B (G is shown as B). */
export function compact(n: number): string {
  if (!isFinite(n)) return '–';
  const a = Math.abs(n);
  if (a < 10_000) return a >= 100 || Number.isInteger(n) ? intFmt(Math.round(n)) : groupFmt(n);
  return compactFmt(n).replace('G', 'B').replace('k', 'K');
}

export function full(n: number): string {
  if (!isFinite(n)) return '–';
  return Number.isInteger(n) ? intFmt(n) : groupFmt(n);
}

function parseDate(s: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2}))?)?/.exec(s);
  if (!m) return null;
  return new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +(m[4] ?? 0), +(m[5] ?? 0), +(m[6] ?? 0)));
}

const fmtYear = utcFormat('%Y');
const fmtMonth = utcFormat('%b %Y');
const fmtDay = utcFormat('%b %-d, %Y');
const fmtTime = utcFormat('%b %-d, %Y %H:%M');

export function formatDate(v: string, grain?: Grain | null, dtype?: DType): string {
  const d = parseDate(v);
  if (!d) return v;
  switch (grain) {
    case 'year':
      return fmtYear(d);
    case 'quarter':
      return `Q${Math.floor(d.getUTCMonth() / 3) + 1} ${d.getUTCFullYear()}`;
    case 'month':
      return fmtMonth(d);
    case 'week':
      return `Week of ${fmtDay(d)}`;
    case 'day':
      return fmtDay(d);
  }
  return dtype === 'datetime' ? fmtTime(d) : fmtDay(d);
}

/** Formats one result cell for display. */
export function cell(v: Cell, dtype: DType, grain?: Grain | null, short = false): string {
  if (v === null || v === undefined) return '(blank)';
  if (dtype === 'number' && typeof v === 'number') return short ? compact(v) : full(v);
  if ((dtype === 'date' || dtype === 'datetime') && typeof v === 'string') return formatDate(v, grain, dtype);
  if (typeof v === 'boolean') return v ? 'Yes' : 'No';
  return String(v);
}

/** "customer_id" → "Customer id"; keeps names that already have capitals. */
export function pretty(name: string): string {
  const s = name.replace(/[_]+/g, ' ').trim();
  if (/[A-Z]/.test(s.slice(1))) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}

const aggWords: Record<Agg, string> = {
  sum: 'Total',
  avg: 'Average',
  min: 'Lowest',
  max: 'Highest',
  count: 'Count of',
  count_distinct: 'Unique',
};

export const aggNames: Record<Agg, string> = {
  sum: 'Sum',
  avg: 'Average',
  min: 'Minimum',
  max: 'Maximum',
  count: 'Count',
  count_distinct: 'Count unique',
};

const singular = (s: string) => s.replace(/ies$/i, 'y').replace(/(ss)$/i, '$1').replace(/s$/i, '');

/** "customer_id" → "customer"; null when the name is not an id. */
function idBase(column: string): string | null {
  const m = /^(.+?)[ _-]?id$/i.exec(column);
  return m && m[1].length > 1 ? pretty(m[1]).toLowerCase() : null;
}

const GENERIC = new Set(['name', 'title', 'label', 'code', 'id', 'key', 'type', 'description']);

export function measureLabel(m: Measure, tables: Table[]): string {
  const t = tables.find((x) => x.id === m.table);
  if (m.column === null) return `Number of ${(t?.label ?? 'rows').toLowerCase()}`;
  const base = idBase(m.column);
  if (base && (m.agg === 'count_distinct' || m.agg === 'count')) {
    return m.agg === 'count_distinct' ? `Unique ${base}s` : `Number of ${base}s`;
  }
  if (m.agg === 'sum') return pretty(m.column);
  return `${aggWords[m.agg]} ${pretty(m.column).toLowerCase()}`;
}

/** "name" of the stores table reads as "store". */
export function dimLabel(d: Dim, tables: Pick<Table, 'id' | 'name'>[] = []): string {
  if (d.grain) return d.grain;
  if (GENERIC.has(d.column.toLowerCase())) {
    const t = tables.find((x) => x.id === d.table);
    if (t) return singular(t.name.split('.').pop()!).toLowerCase();
  }
  return pretty(d.column).toLowerCase();
}

export function tileTitle(t: Tile, tables: Table[]): string {
  if (t.title) return t.title;
  if (!t.measures.length && !t.dims.length) return 'New tile';
  const m = t.measures[0] ? measureLabel(t.measures[0], tables) : pretty(t.dims[0].column);
  if (t.type === 'table') {
    return t.dims.length ? `${m} by ${t.dims.map((d) => dimLabel(d, tables)).join(' and ')}` : m;
  }
  const by = t.dims[0] && t.type !== 'kpi' ? ` by ${dimLabel(t.dims[0], tables)}` : '';
  const split = t.dims[1] && (t.type === 'bar' || t.type === 'line' || t.type === 'area') ? ` and ${dimLabel(t.dims[1], tables)}` : '';
  return m + by + split;
}
