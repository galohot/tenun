// Human formatting for numbers, dates and field names.

import { MAX_CHART_MEASURES } from './tiles';
import { isPart, type Agg, type Cell, type Dim, type DType, type Grain, type Measure, type NumFormat, type Table, type Tile } from './api';
import * as L from './locale';
import type { Locale } from './locale';
import { localeState } from './localeState.svelte';

export type { Locale } from './locale';

/** The number and date format of the open workspace. The viewer sets it from the snapshot. */
export function setLocale(l: Locale | null | undefined): void {
  localeState.value = l === 'id' ? 'id' : 'en';
}

export const getLocale = (): Locale => localeState.value;

/**
 * The short form of a number: 1,284 · 12.9K · 4.2M · 1.3B in English, 1.284 · 12,9 rb · 4,2 jt · 1,3 M
 * in Indonesian. A format can add a currency or decimals, or ask for every digit (`compact: false`).
 */
export function compact(n: number, f?: NumFormat | null, loc: Locale = getLocale()): string {
  return L.shortNumber(n, loc, f);
}

/** Grouped digits, up to 2 decimals. A format can add a currency or fix the decimals. */
export function full(n: number, f?: NumFormat | null, loc: Locale = getLocale()): string {
  return L.longNumber(n, loc, f);
}

/** For tooltips: the full number, and in Indonesian its unit word: "1.234.567 (1,2 jt)". */
export function tip(n: number, f?: NumFormat | null, loc: Locale = getLocale()): string {
  return L.tipNumber(n, loc, f);
}

/**
 * The formats of one value, bound to a locale. A chart builds this once, so a change of
 * locale makes it build again. A table, a pivot and a tooltip default to the full number.
 */
export function numberFormatter(f?: NumFormat | null, loc: Locale = getLocale()) {
  return {
    short: (n: number) => L.shortNumber(n, loc, f),
    long: (n: number) => L.longNumber(n, loc, f),
    tip: (n: number) => L.tipNumber(n, loc, f),
  };
}

/** A share of a whole, 0–1, as a percentage with a locale decimal mark: "4,2%". */
export function pctShare(x: number, loc: Locale = getLocale()): string {
  if (x >= 0.995) return '100%';
  if (x < 0.001) return `<${L.fixedNumber(0.1, 1, loc)}%`;
  return `${L.fixedNumber(x * 100, x < 0.1 ? 1 : 0, loc)}%`;
}

/** A relative change, 0.042 → "4,2%" (no sign). */
export function pctChange(delta: number, loc: Locale = getLocale()): string {
  return `${L.fixedNumber(Math.abs(delta * 100), Math.abs(delta) < 0.1 ? 1 : 0, loc)}%`;
}

/** A file size: "512 B" · "48 KB" · "1,5 MB". */
export function fileSize(b: number, loc: Locale = getLocale()): string {
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${L.fixedNumber(b / 1024, 0, loc)} KB`;
  return `${L.fixedNumber(b / 1048576, 1, loc)} MB`;
}

export function formatDate(v: string, grain?: Grain | null, dtype?: DType, loc: Locale = getLocale()): string {
  return L.formatDate(v, loc, grain, dtype);
}

/** The key the engine gives the "Other" group of a top N (the first column of its row). */
export const OTHER_KEY = '\u0000other';

/** Month of year 1–12 → Jan…Dec · weekday 1–7 (Monday = 1) → Mon…Sun · hour 0–23 → "09:00". Names follow the locale. */
export function partName(n: number, grain: Grain, loc: Locale = getLocale()): string {
  return L.partName(n, grain, loc);
}

/** "September 2026" · "Q3 2026" · "Minggu 7 Sep 2026": the name of a period that starts on `start`. */
export function periodName(start: Date, grain: Grain, loc: Locale = getLocale()): string {
  return L.periodName(start, grain, loc);
}

/** "Oct 14" or "14 Okt". */
export function dayMonth(d: Date, loc: Locale = getLocale()): string {
  return L.dayMonth(d, loc);
}

const tag = (loc: Locale) => (loc === 'id' ? 'id-ID' : 'en-US');

/** A clock time, "14:46" in Indonesian (Intl would write "14.46"), "2:46 PM" in English. */
export function timeShort(when: string | Date, loc: Locale = getLocale()): string {
  const d = new Date(when);
  if (loc === 'id') return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  return d.toLocaleTimeString(tag(loc), { hour: 'numeric', minute: '2-digit' });
}

/** A date and time in the style of the locale, from an ISO time or a Date. */
export function dateTimeMedium(when: string | Date, loc: Locale = getLocale()): string {
  if (loc === 'id') return `${dateMedium(when, loc)}, ${timeShort(when, loc)}`;
  return new Date(when).toLocaleString(tag(loc), { dateStyle: 'medium', timeStyle: 'short' });
}

export function dateMedium(when: string | Date, loc: Locale = getLocale()): string {
  return new Date(when).toLocaleDateString(tag(loc), { dateStyle: 'medium' });
}

/** Formats one result cell for display. */
export function cell(v: Cell, dtype: DType, grain?: Grain | null, short = false, f?: NumFormat | null): string {
  if (v === null || v === undefined) return '(blank)';
  if (v === OTHER_KEY) return 'Other';
  if (grain && isPart(grain) && typeof v === 'number') return partName(v, grain);
  if (dtype === 'number' && typeof v === 'number') return short ? compact(v, f) : full(v, f);
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
  stdev: 'Spread of',
  variance: 'Variance of',
};

export const aggNames: Record<Agg, string> = {
  sum: 'Sum',
  avg: 'Average',
  min: 'Minimum',
  max: 'Maximum',
  count: 'Count',
  count_distinct: 'Count unique',
  stdev: 'Standard deviation',
  variance: 'Variance',
};

const singular = (s: string) => s.replace(/ies$/i, 'y').replace(/(ss)$/i, '$1').replace(/s$/i, '');

/** "customer_id" → "customer"; null when the name is not an id. */
function idBase(column: string): string | null {
  const m = /^(.+?)[ _-]?id$/i.exec(column);
  return m && m[1].length > 1 ? pretty(m[1]).toLowerCase() : null;
}

const GENERIC = new Set(['name', 'title', 'label', 'code', 'id', 'key', 'type', 'description']);

export type Named = Pick<Table, 'id' | 'name' | 'label'>;

export function measureLabel(m: Measure, tables: Named[]): string {
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
export function dimLabel(d: Dim, tables: Named[] = []): string {
  if (d.grain === 'month_of_year') return 'month of year';
  if (d.grain) return d.grain;
  if (GENERIC.has(d.column.toLowerCase())) {
    const t = tables.find((x) => x.id === d.table);
    if (t) return singular(t.name.split('.').pop()!).toLowerCase();
  }
  return pretty(d.column).toLowerCase();
}

export function tileTitle(t: Tile, tables: Named[]): string {
  if (t.title) return t.title;
  if (!t.measures.length && !t.dims.length) return 'New tile';
  let m = t.measures[0] ? measureLabel(t.measures[0], tables) : pretty(t.dims[0].column);
  // Several values: "Sales and cost by month".
  if ((t.type === 'bar' || t.type === 'line' || t.type === 'area') && t.measures.length > 1) {
    const names = t.measures.slice(0, MAX_CHART_MEASURES).map((x, i) => (i ? measureLabel(x, tables).toLowerCase() : measureLabel(x, tables)));
    m = names.length > 2 ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}` : names.join(' and ');
  }
  if (t.type === 'table') {
    return t.dims.length ? `${m} by ${t.dims.map((d) => dimLabel(d, tables)).join(' and ')}` : m;
  }
  const by = t.dims[0] && t.type !== 'kpi' ? ` by ${dimLabel(t.dims[0], tables)}` : '';
  const split = t.dims[1] && t.measures.length < 2 && (t.type === 'bar' || t.type === 'line' || t.type === 'area') ? ` and ${dimLabel(t.dims[1], tables)}` : '';
  return m + by + split;
}

/** When data was loaded, for the "Updated" stamp: "10:32" today, "Oct 9, 10:32" or "9 Okt, 10:32" on other days. */
export function stampLabel(iso: string, loc: Locale = getLocale()): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const time = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  if (d.toDateString() === new Date().toDateString()) return time;
  const day = L.MONTHS_SHORT[loc][d.getMonth()];
  return `${loc === 'id' ? `${d.getDate()} ${day}` : `${day} ${d.getDate()}`}, ${time}`;
}
