// How numbers and dates read in Indonesian and in English. Pure functions with the
// locale passed in, so the viewer, the app and the node tests share them.
// `format.ts` wraps these with the workspace locale (V3-D9).

import { formatLocale } from 'd3-format';
import { timeFormatLocale } from 'd3-time-format';
import type { Grain, NumFormat } from './api';
import { digitsFor, isDigits, oneForm, writeNumber, type Digits } from './numfmt.ts';

export type { Digits } from './numfmt.ts';

export type Locale = 'id' | 'en';

/** `id` for a language tag that starts with "id" ("id", "id-ID"), else `en`. */
export const guessLocale = (lang: string | null | undefined): Locale => (/^id/i.test((lang ?? '').trim()) ? 'id' : 'en');

// ---- numbers ---------------------------------------------------------------
// The writing itself lives in `numfmt.ts` (every field of a format, the digit styles, format codes).

let digits: Digits | null = null;

/** Sets the digit style numbers follow (the app: the workspace's; an exported file: the one it carries). Unset: from the language. */
export function setDigits(d: Digits | null | undefined): void {
  digits = isDigits(d) ? d : null;
}
export const getDigits = (): Digits | null => digits;

/** The short form: 12.9K, 1,2 jt, Rp 1,2 jt. `compact: false` keeps every digit. */
export function shortNumber(n: number, loc: Locale, f?: NumFormat | null, d: Digits | null = digits): string {
  return writeNumber(n, loc, f, d, true).text;
}

/** The full form: grouped digits, up to 2 decimals. `compact: true` asks for the short form. */
export function longNumber(n: number, loc: Locale, f?: NumFormat | null, d: Digits | null = digits): string {
  return writeNumber(n, loc, f, d, false).text;
}

/** For tooltips: always the full number. In Indonesian a big one adds its unit word: "1.234.567 (1,2 jt)". */
export function tipNumber(n: number, loc: Locale, f?: NumFormat | null, d: Digits | null = digits): string {
  if (!isFinite(n)) return '–';
  const full = longNumber(n, loc, { ...f, compact: false }, d);
  if (loc !== 'id' || oneForm(f) || Math.abs(n) < 10_000 || (f?.zero && n === 0)) return full;
  const { prefix: _p, suffix: _s, currency: _c, ...bare } = f ?? {};
  return `${full} (${shortNumber(n, loc, { ...bare, negative: 'minus', compact: true }, d)})`;
}

/** A number with a fixed number of decimals and no grouping: "4,2" or "4.2". */
export const fixedNumber = (n: number, dec: number, loc: Locale) =>
  formatLocale({ decimal: digitsFor(loc, digits) === 'dot_comma' || digitsFor(loc, digits) === 'space_comma' ? ',' : '.', thousands: '', grouping: [3], currency: ['', ''] }).format(`.${dec}f`)(n);

// ---- dates -----------------------------------------------------------------

type Week = [string, string, string, string, string, string, string];
type Year = [string, string, string, string, string, string, string, string, string, string, string, string];

export const MONTHS_LONG: Record<Locale, Year> = {
  en: ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'],
  id: ['Januari', 'Februari', 'Maret', 'April', 'Mei', 'Juni', 'Juli', 'Agustus', 'September', 'Oktober', 'November', 'Desember'],
};
export const MONTHS_SHORT: Record<Locale, Year> = {
  en: ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'],
  id: ['Jan', 'Feb', 'Mar', 'Apr', 'Mei', 'Jun', 'Jul', 'Agu', 'Sep', 'Okt', 'Nov', 'Des'],
};
// Sunday first, as d3 expects.
const DAYS_LONG: Record<Locale, Week> = {
  en: ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'],
  id: ['Minggu', 'Senin', 'Selasa', 'Rabu', 'Kamis', 'Jumat', 'Sabtu'],
};
const DAYS_SHORT: Record<Locale, Week> = {
  en: ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'],
  id: ['Min', 'Sen', 'Sel', 'Rab', 'Kam', 'Jum', 'Sab'],
};

function times(loc: Locale) {
  const t = timeFormatLocale({
    dateTime: '%x, %X',
    date: '%d/%m/%Y',
    time: '%H:%M:%S',
    periods: ['AM', 'PM'],
    days: DAYS_LONG[loc],
    shortDays: DAYS_SHORT[loc],
    months: MONTHS_LONG[loc],
    shortMonths: MONTHS_SHORT[loc],
  });
  // Day first in Indonesian: "14 Okt 2026". English stays "Oct 14, 2026".
  const day = loc === 'id' ? '%-d %b %Y' : '%b %-d, %Y';
  return { year: t.utcFormat('%Y'), month: t.utcFormat('%b %Y'), day: t.utcFormat(day), time: t.utcFormat(`${day} %H:%M`) };
}
const TIMES: Record<Locale, ReturnType<typeof times>> = { en: times('en'), id: times('id') };

export function parseDate(s: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2}))?)?/.exec(s);
  if (!m) return null;
  return new Date(Date.UTC(+m[1], +m[2] - 1, +m[3], +(m[4] ?? 0), +(m[5] ?? 0), +(m[6] ?? 0)));
}

/** "Q3 2026" · "Okt 2026" · "Minggu 7 Sep 2026" · "14 Okt 2026" by the grain. */
export function formatDate(v: string, loc: Locale, grain?: Grain | null, dtype?: string): string {
  const d = parseDate(v);
  if (!d) return v;
  // A year or quarter of a year that starts after January: "FY 2027", "Q1 FY 2027" (v5 build 04).
  const fiscal = fiscalName(d, grain);
  if (fiscal) return fiscal;
  const f = TIMES[loc];
  switch (grain) {
    case 'year':
      return f.year(d);
    case 'quarter':
      return `Q${Math.floor(d.getUTCMonth() / 3) + 1} ${d.getUTCFullYear()}`;
    case 'month':
      return f.month(d);
    case 'week':
      return `${loc === 'id' ? 'Minggu' : 'Week of'} ${f.day(d)}`;
    case 'day':
      return f.day(d);
  }
  return dtype === 'datetime' ? f.time(d) : f.day(d);
}

const PATTERN = /'[^']*'|yyyy|yy|MMMM|MMM|MM|M|dd|d|EEEE|EEE|HH|H|hh|h|mm|ss|a/g;

/** Why a date pattern cannot be used, or null. */
export function patternProblem(p: string): string | null {
  if ((p.match(/'/g) ?? []).length % 2) return 'A quote is not closed.';
  return /yy|M|d|E|H|h|m|s/.test(p.replace(/'[^']*'/g, '')) ? null : 'The pattern names no part of a date.';
}

/**
 * A date in a pattern (formats §2.3): yyyy yy · M MM MMM MMMM · d dd · EEE EEEE · H HH h hh mm ss a · 'text'.
 * Month and day names follow the language. Anything else is written as it is.
 */
export function datePattern(v: string, pattern: string, loc: Locale): string {
  const d = parseDate(v);
  if (!d || patternProblem(pattern)) return v;
  const y = d.getUTCFullYear(), mo = d.getUTCMonth(), day = d.getUTCDate(), wd = d.getUTCDay();
  const H = d.getUTCHours(), mi = d.getUTCMinutes(), se = d.getUTCSeconds();
  const p2 = (n: number) => String(n).padStart(2, '0');
  const h12 = H % 12 || 12;
  return pattern.replace(PATTERN, (t) => {
    switch (t) {
      case 'yyyy': return String(y);
      case 'yy': return p2(y % 100);
      case 'MMMM': return MONTHS_LONG[loc][mo];
      case 'MMM': return MONTHS_SHORT[loc][mo];
      case 'MM': return p2(mo + 1);
      case 'M': return String(mo + 1);
      case 'dd': return p2(day);
      case 'd': return String(day);
      case 'EEEE': return DAYS_LONG[loc][wd];
      case 'EEE': return DAYS_SHORT[loc][wd];
      case 'HH': return p2(H);
      case 'H': return String(H);
      case 'hh': return p2(h12);
      case 'h': return String(h12);
      case 'mm': return p2(mi);
      case 'ss': return p2(se);
      case 'a': return H < 12 ? 'AM' : 'PM';
      default: return t.slice(1, -1);
    }
  });
}

/** The name of a period that starts on `d`: "September 2026", "Q3 2026", "Minggu 7 Sep 2026". */
export function periodName(d: Date, grain: Grain, loc: Locale): string {
  if (grain === 'month') return `${MONTHS_LONG[loc][d.getUTCMonth()]} ${d.getUTCFullYear()}`;
  return formatDate(d.toISOString().slice(0, 10), loc, grain === 'year' || grain === 'quarter' || grain === 'week' ? grain : 'day');
}

/** A day and month in the order the locale reads them: "Oct 14" or "14 Okt". */
export function dayMonth(d: Date, loc: Locale): string {
  const m = MONTHS_SHORT[loc][d.getUTCMonth()];
  return loc === 'id' ? `${d.getUTCDate()} ${m}` : `${m} ${d.getUTCDate()}`;
}

/** Month of year 1–12 → Jan…Dec / Jan…Des · weekday 1–7 (Monday = 1) → Mon…Sun / Sen…Min · hour 0–23 → "09:00". */
export function partName(n: number, grain: Grain, loc: Locale): string {
  const i = Math.round(n);
  switch (grain) {
    case 'month_of_year':
      return MONTHS_SHORT[loc][i - 1] ?? String(n);
    case 'weekday':
      // The table starts on Sunday; the engine counts from Monday.
      return DAYS_SHORT[loc][i % 7] ?? String(n);
    default:
      return `${String(i).padStart(2, '0')}:00`;
  }
}

/** A share of a whole, 0.074 → "7.4%", 0.23 → "23%": one decimal under 10 %, nothing below 0.1 % but "<0.1%". */
export function pctShareIn(x: number, loc: Locale): string {
  if (x >= 0.995) return '100%';
  if (x < 0.001) return `<${fixedNumber(0.1, 1, loc)}%`;
  return `${fixedNumber(x * 100, x < 0.1 ? 1 : 0, loc)}%`;
}

/** The month the year starts in (1–12) and the day the week starts on (1 Monday, 6 Saturday, 7 Sunday). */
export interface Calendar {
  yearStart?: number;
  weekStart?: number;
}

let calendar: Required<Calendar> = { yearStart: 1, weekStart: 1 };

/** Sets the calendar the period names follow (the app: the workspace's; an exported file: the one it carries). */
export function setCalendar(c: Calendar | null | undefined): void {
  const y = c?.yearStart ?? 1;
  const w = c?.weekStart ?? 1;
  calendar = { yearStart: y >= 1 && y <= 12 ? Math.round(y) : 1, weekStart: w === 6 || w === 7 ? w : 1 };
  weekSet = c?.weekStart !== undefined;
}

export const getCalendar = (): Required<Calendar> => calendar;

let weekSet = false;
/** The week start the workspace sets, or null when it sets none (a view may then follow the language). */
export const weekStartSet = (): number | null => (weekSet ? calendar.weekStart : null);

/** "FY 2027" or "Q1 FY 2027" for a period of a year that starts after January; null otherwise. `d` is its first day. */
export function fiscalName(d: Date, grain: Grain | null | undefined): string | null {
  const { yearStart } = calendar;
  if (yearStart === 1 || (grain !== 'year' && grain !== 'quarter')) return null;
  const m = d.getUTCMonth() + 1;
  const fy = m >= yearStart ? d.getUTCFullYear() + 1 : d.getUTCFullYear();
  if (grain === 'year') return `FY ${fy}`;
  const q = Math.floor(((m + 12 - yearStart) % 12) / 3) + 1;
  return `Q${q} FY ${fy}`;
}
