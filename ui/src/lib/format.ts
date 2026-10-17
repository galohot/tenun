// Human formatting for numbers, dates and field names.

import { MAX_CHART_MEASURES } from './tiles';
import { isPart, type Agg, type Cell, type Column, type Dim, type DType, type Grain, type Measure, type NumFormat, type QueryResult, type ShowAs, type Table, type Tile, type Unit } from './api';
import { dim0Kind, effectiveShowAs, SHOW_AS_WORDS, type Dim0Kind } from './showas';
import { labelOf } from './prep';
import { fieldWithLink } from './links';
import { columnFormat, nameOf } from './measures';
import * as L from './locale';
import { toneOf, type Tone } from './numfmt';
import type { Locale } from './locale';
import { localeState } from './localeState.svelte';
import { t } from './i18n';

export type { Locale } from './locale';

/** The number and date format of the open workspace. The viewer sets it from the snapshot. */
export function setLocale(l: Locale | null | undefined): void {
  localeState.value = l === 'id' ? 'id' : 'en';
}

/** The digit style of the open workspace (formats §2.2). The viewer sets it from the snapshot. */
export function setDigits(d: string | null | undefined): void {
  L.setDigits(d as L.Digits);
  localeState.digits = L.getDigits();
}

/** The locale; reading it also follows the digit style, so whatever draws a number draws again when either changes. */
export const getLocale = (): Locale => (void localeState.digits, localeState.value);

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

/** What a format marks a value as (red negatives, a code's [Red] or [Green]), in the open workspace. */
export function toneFor(n: number, f?: NumFormat | null): Tone | null {
  return toneOf(n, getLocale(), f, L.getDigits());
}

/** A share of a whole, 0–1, as a percentage with a locale decimal mark: "4,2%". */
export function pctShare(x: number, loc: Locale = getLocale()): string {
  return L.pctShareIn(x, loc);
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

/** A column a cell comes from, so it can be written in that column's format and date pattern (formats §2.3). */
export type ColRef = { table?: string | null; column?: string | null } | null | undefined;

/** Formats one result cell for display. `col` names its column: a number with no format of its own, an empty cell
 *  and a date at day grain then follow that column's settings. */
export function cell(v: Cell, dtype: DType, grain?: Grain | null, short = false, f?: NumFormat | null, unit?: Unit | null, col?: ColRef): string {
  const own = col ? columnFormat(col.table, col.column) : null;
  if (v === null || v === undefined) return own?.format?.blank || t('(blank)');
  if (v === OTHER_KEY) return t('Other');
  if (grain && isPart(grain) && typeof v === 'number') return partName(v, grain);
  if (dtype === 'number' && typeof v === 'number') {
    const g = f ?? own?.format ?? null;
    if (unit) return short ? numberFormatter(g, getLocale(), unit).short(v) : numberFormatter(g, getLocale(), unit).long(v);
    return short ? compact(v, g) : full(v, g);
  }
  if ((dtype === 'date' || dtype === 'datetime') && typeof v === 'string') {
    if (own?.date_pattern && (!grain || grain === 'day')) return L.datePattern(v, own.date_pattern, getLocale());
    return formatDate(v, grain, dtype);
  }
  if (typeof v === 'boolean') return v ? t('Yes') : t('No');
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

/** A value added up a way, in words: "Average amount". */
function aggWord(agg: Agg, name: string): string {
  switch (agg) {
    case 'sum': return t('Total {name}', { name });
    case 'avg': return t('Average {name}', { name });
    case 'min': return t('Lowest {name}', { name });
    case 'max': return t('Highest {name}', { name });
    case 'count': return t('Count of {name}', { name });
    case 'count_distinct': return t('Unique {name}', { name });
    case 'median': return t('Median {name}', { name });
    case 'p90': return t('90th percentile of {name}', { name });
    case 'stdev': return t('Spread of {name}', { name });
    case 'variance': return t('Variance of {name}', { name });
    case 'closing': return t('Closing {name}', { name });
    case 'opening': return t('Opening {name}', { name });
  }
}

export const aggNames: Record<Agg, string> = {
  get sum() { return t('Sum'); },
  get avg() { return t('Average'); },
  get min() { return t('Minimum'); },
  get max() { return t('Maximum'); },
  get count() { return t('Count'); },
  get count_distinct() { return t('Count unique'); },
  get median() { return t('Median'); },
  get p90() { return t('90th percentile'); },
  get stdev() { return t('Standard deviation'); },
  get variance() { return t('Variance'); },
  get closing() { return t('Closing value'); },
  get opening() { return t('Opening value'); },
};

/** "a and b and c": names joined in the language of the screens. */
const and = (names: string[]) => names.reduce((a, b) => t('{a} and {b}', { a, b }));

/** A period grain inside a title: "Revenue by month". */
function grainWord(g: Grain): string {
  switch (g) {
    case 'year': return t('year');
    case 'quarter': return t('quarter');
    case 'month': return t('month');
    case 'week': return t('week');
    case 'day': return t('day');
    case 'month_of_year': return t('month of year');
    case 'weekday': return t('weekday');
    case 'hour': return t('hour');
  }
}

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
  return how && how !== 'value' ? t('{name} ({how})', { name: base, how: SHOW_AS_WORDS[how] }) : base;
}

/**
 * What a ratio divides by, in words: "order" for a count of rows, else the name of the value. A count
 * of rows of another table counts the joined rows of the first table, so it reads "row" (V3-D36).
 */
function perWord(p: NonNullable<Measure['per']>, top: string, tables: Named[]): string {
  if (p.column == null && p.table !== top) return t('row');
  if (p.column == null) {
    const label = tables.find((x) => x.id === p.table)?.label;
    return label ? singular(label.toLowerCase()) : t('row');
  }
  return plainLabel({ table: p.table, column: p.column, agg: p.agg }, tables).toLowerCase();
}

function ratioLabel(m: Measure, tables: Named[]): string {
  const top = plainLabel(m, tables);
  return m.per ? t('{value} per {per}', { value: top, per: perWord(m.per, m.table, tables) }) : top;
}

function plainLabel(m: Measure, tables: Named[]): string {
  if (m.named) return nameOf(m.named);
  const tb = tables.find((x) => x.id === m.table);
  if (m.column === null) return t('Number of {name}', { name: tb?.label ? tb.label.toLowerCase() : t('rows') });
  const base = labelOf(tables, m.table, m.column) ? null : idBase(m.column);
  if (base && (m.agg === 'count_distinct' || m.agg === 'count')) {
    return m.agg === 'count_distinct' ? t('Unique {name}s', { name: base }) : t('Number of {name}s', { name: base });
  }
  if (m.agg === 'sum') return fieldLabel(tables, m.table, m.column);
  return aggWord(m.agg, fieldWord(tables, m.table, m.column));
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
  if (d.grain) return grainWord(d.grain);
  if (GENERIC.has(d.column.toLowerCase())) {
    const tb = tables.find((x) => x.id === d.table);
    if (tb) return singular(tb.name.split('.').pop()!).toLowerCase();
  }
  return fieldWord(tables, d.table, d.column);
}

export function tileTitle(tl: Tile, tables: Named[], parts?: QueryResult['parts']): string {
  if (tl.title) return tl.title;
  // A value shown another way adds its words to its name; the engine ignores a way that does not fit.
  const d0 = tl.dims[0];
  const dtype = d0 ? tables.find((x) => x.id === d0.table)?.columns?.find((c) => c.name === d0.column)?.dtype : undefined;
  const kind = d0 ? dim0Kind(dtype, d0.grain) : 'none';
  const label = (x: Measure, i: number) => measureLabel(x, tables, effectiveShowAs(tl, i, kind, parts));
  // "Revenue by region": a title and the field its groups come from.
  const by = (title: string, d: Dim | undefined) => (d ? t('{value} by {field}', { value: title, field: dimLabel(d, tables) }) : title);
  // A note or a picture needs no title; a filter and a parameter's control write their own name.
  if (tl.type === 'text' || tl.type === 'image' || tl.type === 'filter' || tl.type === 'param') return '';
  if (!tl.measures.length && !tl.dims.length) return t('New tile');
  let m = tl.measures[0] ? label(tl.measures[0], 0) : fieldLabel(tables, tl.dims[0].table, tl.dims[0].column);
  // Several values: "Sales and cost by month".
  if ((tl.type === 'bar' || tl.type === 'line' || tl.type === 'area') && tl.measures.length > 1) {
    const names = tl.measures.slice(0, MAX_CHART_MEASURES).map((x, i) => (i ? label(x, i).toLowerCase() : label(x, i)));
    m = names.length > 2 ? t('{list} and {last}', { list: names.slice(0, -1).join(', '), last: names.at(-1)! }) : and(names);
  }
  // "Revenue by category and channel": rows, then columns.
  if (tl.type === 'pivot' || tl.type === 'heatmap') {
    const fields = [...tl.dims.slice(0, 3), ...(tl.cols ? [tl.cols] : [])].map((d) => dimLabel(d, tables));
    return fields.length ? t('{value} by {field}', { value: m, field: and(fields) }) : m;
  }
  if (tl.type === 'table') {
    return tl.dims.length ? t('{value} by {field}', { value: m, field: and(tl.dims.map((d) => dimLabel(d, tables))) }) : m;
  }
  // "Spread of amount by region": a box plot reads one number column for each group.
  if (tl.type === 'box') {
    const col = tl.measures[0]?.column ? pretty(tl.measures[0].column).toLowerCase() : null;
    return col ? by(t('Spread of {name}', { name: col }), tl.dims[0]) : m;
  }
  // "Distribution of amount": a histogram bins one column, whatever the way it is added up.
  if (tl.type === 'histogram') {
    return tl.measures[0]?.column ? t('Distribution of {name}', { name: fieldWord(tables, tl.measures[0].table, tl.measures[0].column) }) : m;
  }
  // "Actual against target by region": a bullet chart reads a value and its target.
  if (tl.type === 'bullet') return by(tl.measures[1] || tl.target ? t('{value} against target', { value: m }) : m, tl.dims[0]);
  // "Actual vs target by region", or "Revenue by region, two periods" for one value over two.
  if (tl.type === 'compare') {
    return tl.measures[1]
      ? by(t('{a} vs {b}', { a: m, b: label(tl.measures[1], 1).toLowerCase() }), tl.dims[0])
      : t('{title}, two periods', { title: by(m, tl.dims[0]) });
  }
  // "Revenue from channel to category to status".
  if (tl.type === 'sankey') {
    if (tl.dims.length < 2) return m;
    const path = tl.dims.slice(0, 3).map((d) => dimLabel(d, tables)).reduce((a, b) => t('{a} to {b}', { a, b }));
    return t('{value} from {path}', { value: m, path });
  }
  if (tl.type === 'calendar') return t('{value} by day', { value: m });
  // "Cohorts of customer by month".
  if (tl.type === 'cohort') {
    const who = tl.measures[0]?.column ? fieldWord(tables, tl.measures[0].table, tl.measures[0].column) : t('entity');
    return by(t('Cohorts of {name}', { name: who }), tl.dims[0]);
  }
  // "Timeline by task".
  if (tl.type === 'gantt') return tl.dims[0] ? t('Timeline by {field}', { field: dimLabel(tl.dims[0], tables) }) : t('Timeline');
  // "Revenue vs quantity by product": one dot for each product.
  if (tl.type === 'scatter' && tl.measures[1]) m = t('{a} vs {b}', { a: m, b: label(tl.measures[1], 1).toLowerCase() });
  const chart = tl.type === 'bar' || tl.type === 'line' || tl.type === 'area';
  const repeated = !!tl.repeat && !!tl.dims[0] && chart;
  let title = tl.type !== 'kpi' ? by(m, tl.dims[0]) : m;
  if (tl.dims[1] && tl.measures.length < 2 && chart && !repeated) title = and([title, dimLabel(tl.dims[1], tables)]);
  // Small multiples: "Revenue by month, a chart for each region".
  if (repeated) title = t('{title}, a chart for each {field}', { title, field: dimLabel(tl.repeat!, tables) });
  // A third value sets the size of the dots: "Revenue vs quantity, sized by margin".
  if (tl.type === 'scatter' && tl.measures[2]) title = t('{title}, sized by {value}', { title, value: label(tl.measures[2], 2).toLowerCase() });
  return title;
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
