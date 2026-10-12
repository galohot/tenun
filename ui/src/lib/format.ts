// Human formatting for numbers, dates and field names.

import { MAX_CHART_MEASURES } from './tiles';
import { isPart, type Agg, type Cell, type Column, type Dim, type DType, type Grain, type Measure, type NumFormat, type QueryResult, type ShowAs, type Table, type Tile, type Unit } from './api';
import { dim0Kind, effectiveShowAs, SHOW_AS_WORDS, type Dim0Kind } from './showas';
import { labelOf } from './prep';
import { fieldWithLink } from './links';
import { nameOf } from './measures';
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
export function numberFormatter(f?: NumFormat | null, loc: Locale = getLocale(), unit?: Unit | null) {
  // A rank reads "#3". A share reads as a percent, whatever currency the value has.
  if (unit === 'rank') {
    const rank = (n: number) => (Number.isFinite(n) ? `#${Math.round(n)}` : '–');
    return { short: rank, long: rank, tip: rank };
  }
  const g = unit === 'percent' ? { ...f, percent: true, currency: undefined } : f;
  return {
    short: (n: number) => L.shortNumber(n, loc, g),
    long: (n: number) => L.longNumber(n, loc, g),
    tip: (n: number) => L.tipNumber(n, loc, g),
  };
}

/** A share of a whole, 0–1, as a percentage with a locale decimal mark: "4,2%". */
export function pctShare(x: number, loc: Locale = getLocale()): string {
  if (x >= 0.995) return '100%';
  if (x < 0.001) return `<${L.fixedNumber(0.1, 1, loc)}%`;
  return `${L.fixedNumber(x * 100, x < 0.1 ? 1 : 0, loc)}%`;
}

/** The value as a share of its target, 0.82 → "82 %". It can pass 100. */
export function pctOfTarget(share: number, loc: Locale = getLocale()): string {
  return `${L.fixedNumber(share * 100, 0, loc)} %`;
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
export function cell(v: Cell, dtype: DType, grain?: Grain | null, short = false, f?: NumFormat | null, unit?: Unit | null): string {
  if (v === null || v === undefined) return '(blank)';
  if (v === OTHER_KEY) return 'Other';
  if (grain && isPart(grain) && typeof v === 'number') return partName(v, grain);
  if (dtype === 'number' && typeof v === 'number') {
    if (unit) return short ? numberFormatter(f, getLocale(), unit).short(v) : numberFormatter(f, getLocale(), unit).long(v);
    return short ? compact(v, f) : full(v, f);
  }
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

/** The name of a field: the label the user gave it, else the prettified column name. */
export function fieldLabel(tables: Named[] | undefined, table: string, column: string): string {
  return labelOf(tables, table, column) ?? pretty(column);
}

/** A field inside a sentence: the user's label as typed, else the column name in lower case. */
export function fieldWord(tables: Named[] | undefined, table: string, column: string): string {
  return labelOf(tables, table, column) ?? pretty(column).toLowerCase();
}

const aggWords: Record<Agg, string> = {
  sum: 'Total',
  avg: 'Average',
  min: 'Lowest',
  max: 'Highest',
  count: 'Count of',
  count_distinct: 'Unique',
  median: 'Median',
  p90: '90th percentile of',
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
  median: 'Median',
  p90: '90th percentile',
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

export type Named = Pick<Table, 'id' | 'name' | 'label'> & { columns?: Column[] };

/**
 * The name of a value: "Revenue", "Average amount". A ratio reads "Revenue per order". A value shown
 * another way adds its words: "Revenue (% of total)".
 */
export function measureLabel(m: Measure, tables: Named[], how?: ShowAs | null): string {
  // A named measure is called what the user called it; its formula says the rest.
  const base = m.named ? nameOf(m.named) : ratioLabel(m, tables);
  return how && how !== 'value' ? `${base} (${SHOW_AS_WORDS[how]})` : base;
}

/**
 * What a ratio divides by, in words: "order" for a count of rows, else the name of the value. A count
 * of rows of another table counts the joined rows of the first table, so it reads "row" (V3-D36).
 */
function perWord(p: NonNullable<Measure['per']>, top: string, tables: Named[]): string {
  if (p.column == null && p.table !== top) return 'row';
  if (p.column == null) return singular((tables.find((x) => x.id === p.table)?.label ?? 'row').toLowerCase());
  return plainLabel({ table: p.table, column: p.column, agg: p.agg }, tables).toLowerCase();
}

function ratioLabel(m: Measure, tables: Named[]): string {
  const top = plainLabel(m, tables);
  return m.per ? `${top} per ${perWord(m.per, m.table, tables)}` : top;
}

function plainLabel(m: Measure, tables: Named[]): string {
  if (m.named) return nameOf(m.named);
  const t = tables.find((x) => x.id === m.table);
  if (m.column === null) return `Number of ${(t?.label ?? 'rows').toLowerCase()}`;
  const base = labelOf(tables, m.table, m.column) ? null : idBase(m.column);
  if (base && (m.agg === 'count_distinct' || m.agg === 'count')) {
    return m.agg === 'count_distinct' ? `Unique ${base}s` : `Number of ${base}s`;
  }
  if (m.agg === 'sum') return fieldLabel(tables, m.table, m.column);
  return `${aggWords[m.agg]} ${fieldWord(tables, m.table, m.column)}`;
}

/**
 * The heading of each column of a table or pivot result: a field reads as in the pivot ("Category"),
 * a value as in the charts ("Amount", "Average amount", "Number of rows"). Two values from one column
 * differ by their way of adding up, so they are never both "amount".
 */
export function columnHeads(
  res: Pick<QueryResult, 'columns' | 'parts'>,
  tile: Pick<Tile, 'type' | 'dims' | 'measures' | 'cols' | 'showAs'>,
  tables: Named[],
): string[] {
  const kind = resultDim0(res, tile);
  // The fields behind the group-by columns: a pivot lists its rows, then its column field.
  const fields = tile.type === 'pivot' ? [...tile.dims, ...(tile.cols ? [tile.cols] : [])] : tile.dims;
  let j = 0;
  let d = 0;
  return res.columns.map((c) => {
    if (c.role !== 'measure') {
      const f = fields[d++];
      return f ? fieldWithLink(fieldLabel(tables, f.table, f.column), f.via) : pretty(c.name);
    }
    const i = j++;
    const m = tile.measures[i];
    return m ? measureLabel(m, tables, effectiveShowAs(tile, i, kind, res.parts)) : pretty(c.name);
  });
}

/** What the first group-by column of a result is, read from the result itself. */
export function resultDim0(res: Pick<QueryResult, 'columns'>, tile: Pick<Tile, 'dims'>): Dim0Kind {
  const c = res.columns[0];
  if (!c || c.role !== 'dimension') return 'none';
  if (c.dtype === 'date' || c.dtype === 'datetime') return 'date';
  return isPart(tile.dims[0]?.grain) ? 'part' : 'other';
}

/** "name" of the stores table reads as "store". */
export function dimLabel(d: Dim, tables: Named[] = []): string {
  // A field that crosses a named link says so, or two tiles of the same table read the same.
  return fieldWithLink(dimWord(d, tables), d.via);
}

function dimWord(d: Dim, tables: Named[]): string {
  if (d.grain === 'month_of_year') return 'month of year';
  if (d.grain) return d.grain;
  if (GENERIC.has(d.column.toLowerCase())) {
    const t = tables.find((x) => x.id === d.table);
    if (t) return singular(t.name.split('.').pop()!).toLowerCase();
  }
  return fieldWord(tables, d.table, d.column);
}

export function tileTitle(t: Tile, tables: Named[], parts?: QueryResult['parts']): string {
  if (t.title) return t.title;
  // A value shown another way adds its words to its name; the engine ignores a way that does not fit.
  const d0 = t.dims[0];
  const dtype = d0 ? tables.find((x) => x.id === d0.table)?.columns?.find((c) => c.name === d0.column)?.dtype : undefined;
  const kind = d0 ? dim0Kind(dtype, d0.grain) : 'none';
  const label = (x: Measure, i: number) => measureLabel(x, tables, effectiveShowAs(t, i, kind, parts));
  // A note or a picture needs no title.
  if (t.type === 'text' || t.type === 'image' || t.type === 'filter') return '';
  if (!t.measures.length && !t.dims.length) return 'New tile';
  let m = t.measures[0] ? label(t.measures[0], 0) : fieldLabel(tables, t.dims[0].table, t.dims[0].column);
  // Several values: "Sales and cost by month".
  if ((t.type === 'bar' || t.type === 'line' || t.type === 'area') && t.measures.length > 1) {
    const names = t.measures.slice(0, MAX_CHART_MEASURES).map((x, i) => (i ? label(x, i).toLowerCase() : label(x, i)));
    m = names.length > 2 ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}` : names.join(' and ');
  }
  // "Revenue by category and channel": rows, then columns.
  if (t.type === 'pivot' || t.type === 'heatmap') {
    const fields = [...t.dims.slice(0, 3), ...(t.cols ? [t.cols] : [])].map((d) => dimLabel(d, tables));
    return fields.length ? `${m} by ${fields.join(' and ')}` : m;
  }
  if (t.type === 'table') {
    return t.dims.length ? `${m} by ${t.dims.map((d) => dimLabel(d, tables)).join(' and ')}` : m;
  }
  // "Spread of amount by region": a box plot reads one number column for each group.
  if (t.type === 'box') {
    const col = t.measures[0]?.column ? pretty(t.measures[0].column).toLowerCase() : null;
    return col ? `Spread of ${col}${t.dims[0] ? ` by ${dimLabel(t.dims[0], tables)}` : ''}` : m;
  }
  // "Distribution of amount": a histogram bins one column, whatever the way it is added up.
  if (t.type === 'histogram') return t.measures[0]?.column ? `Distribution of ${fieldWord(tables, t.measures[0].table, t.measures[0].column)}` : m;
  // "Actual against target by region": a bullet chart reads a value and its target.
  if (t.type === 'bullet') {
    const by = t.dims[0] ? ` by ${dimLabel(t.dims[0], tables)}` : '';
    return t.measures[1] || t.target ? `${m} against target${by}` : m + by;
  }
  // "Actual vs target by region", or "Revenue by region, two periods" for one value over two.
  if (t.type === 'compare') {
    const by = t.dims[0] ? ` by ${dimLabel(t.dims[0], tables)}` : '';
    return t.measures[1] ? `${m} vs ${label(t.measures[1], 1).toLowerCase()}${by}` : `${m}${by}, two periods`;
  }
  // "Revenue from channel to category to status".
  if (t.type === 'sankey') return t.dims.length > 1 ? `${m} from ${t.dims.slice(0, 3).map((d) => dimLabel(d, tables)).join(' to ')}` : m;
  if (t.type === 'calendar') return `${m} by day`;
  // "Cohorts of customer by month".
  if (t.type === 'cohort') {
    const who = t.measures[0]?.column ? fieldWord(tables, t.measures[0].table, t.measures[0].column) : 'entity';
    return `Cohorts of ${who}${t.dims[0] ? ` by ${dimLabel(t.dims[0], tables)}` : ''}`;
  }
  // "Timeline by task".
  if (t.type === 'gantt') return t.dims[0] ? `Timeline by ${dimLabel(t.dims[0], tables)}` : 'Timeline';
  // "Revenue vs quantity by product": one dot for each product.
  if (t.type === 'scatter' && t.measures[1]) m = `${m} vs ${label(t.measures[1], 1).toLowerCase()}`;
  // A third value sets the size of the dots: "Revenue vs quantity, sized by margin".
  const sized = t.type === 'scatter' && t.measures[2] ? `, sized by ${label(t.measures[2], 2).toLowerCase()}` : '';
  const by = t.dims[0] && t.type !== 'kpi' ? ` by ${dimLabel(t.dims[0], tables)}` : '';
  const chart = t.type === 'bar' || t.type === 'line' || t.type === 'area';
  const repeated = !!t.repeat && !!t.dims[0] && chart;
  const split = t.dims[1] && t.measures.length < 2 && chart && !repeated ? ` and ${dimLabel(t.dims[1], tables)}` : '';
  // Small multiples: "Revenue by month, a chart for each region".
  const each = repeated ? `, a chart for each ${dimLabel(t.repeat!, tables)}` : '';
  return m + by + split + each + sized;
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
