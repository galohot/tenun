// How numbers and dates read in Indonesian and in English. Pure functions with the
// locale passed in, so the viewer, the app and the node tests share them.
// `format.ts` wraps these with the workspace locale (V3-D9).

import { formatLocale } from 'd3-format';
import { timeFormatLocale } from 'd3-time-format';
import type { Grain, NumFormat } from './api';

export type Locale = 'id' | 'en';

/** `id` for a language tag that starts with "id" ("id", "id-ID"), else `en`. */
export const guessLocale = (lang: string | null | undefined): Locale => (/^id/i.test((lang ?? '').trim()) ? 'id' : 'en');

// ---- numbers ---------------------------------------------------------------

const NUM = {
  en: formatLocale({ decimal: '.', thousands: ',', grouping: [3], currency: ['$', ''] }),
  id: formatLocale({ decimal: ',', thousands: '.', grouping: [3], currency: ['Rp ', ''] }),
};

/** Where each currency sits, per locale: [before the number, after it]. */
const CURRENCY: Record<string, Record<Locale, [string, string]>> = {
  IDR: { id: ['Rp ', ''], en: ['Rp ', ''] },
  USD: { id: ['US$', ''], en: ['$', ''] },
  EUR: { id: ['€', ''], en: ['€', ''] },
  SGD: { id: ['S$', ''], en: ['S$', ''] },
  MZN: { id: ['', ' MT'], en: ['MT ', ''] },
};

/** Compact units for 10³, 10⁶, 10⁹, 10¹² (d3 writes them k, M, G, T). */
const UNITS: Record<Locale, Record<string, string>> = {
  en: { k: 'K', M: 'M', G: 'B', T: 'T' },
  id: { k: ' rb', M: ' jt', G: ' M', T: ' T' },
};
const STEPS: [number, string][] = [
  [1e12, 'T'],
  [1e9, 'G'],
  [1e6, 'M'],
  [1e3, 'k'],
];

/** Below this a compact number keeps all its digits. */
const COMPACT_FROM = 10_000;

const digitsFor = (loc: Locale, spec: string) => NUM[loc].format(spec);

/** A minus sign (d3 writes U+2212) is taken off so a currency can go before it. */
function unsign(s: string): [boolean, string] {
  return s.startsWith('−') ? [true, s.slice(1)] : [false, s];
}

/** Digits grouped, with a fixed number of decimals. */
const fixedGroup = (n: number, d: number, loc: Locale) => digitsFor(loc, `,.${d}f`)(n);

function fullBody(n: number, loc: Locale, decimals?: number): string {
  if (decimals !== undefined) return fixedGroup(n, decimals, loc);
  return Number.isInteger(n) ? digitsFor(loc, ',d')(n) : digitsFor(loc, ',.2~f')(n);
}

function compactBody(n: number, loc: Locale, decimals?: number): string {
  const a = Math.abs(n);
  if (a < COMPACT_FROM) {
    if (decimals !== undefined) return fixedGroup(n, decimals, loc);
    return a >= 100 || Number.isInteger(n) ? digitsFor(loc, ',d')(Math.round(n)) : digitsFor(loc, ',.2~f')(n);
  }
  if (decimals === undefined) {
    // Three significant digits: 12.9K · 4.2M · 1.3B.
    const m = /^(.*?)([kMGT])?$/.exec(digitsFor(loc, '.3~s')(n))!;
    return m[1] + (m[2] ? UNITS[loc][m[2]] : '');
  }
  let i = STEPS.findIndex(([v]) => a >= v);
  if (i < 0) return fixedGroup(n, decimals, loc);
  // 999,999 rounds up to 1,000 thousand: use the next unit instead.
  if (i > 0 && Number((a / STEPS[i][0]).toFixed(decimals)) >= 1000) i--;
  const [v, u] = STEPS[i];
  return fixedGroup(n / v, decimals, loc) + UNITS[loc][u];
}

function decorate(signed: string, loc: Locale, currency: string | undefined): string {
  const [neg, body] = unsign(signed);
  const [pre, post] = currency ? (CURRENCY[currency]?.[loc] ?? [`${currency} `, '']) : ['', ''];
  return (neg ? '−' : '') + pre + body + post;
}

function percentText(n: number, loc: Locale, f: NumFormat): string {
  const v = n * 100;
  const body = f.decimals === undefined ? digitsFor(loc, ',.1~f')(v) : fixedGroup(v, f.decimals, loc);
  return body + '%';
}

/** The short form: 12.9K, 1,2 jt, Rp 1,2 jt. `compact: false` keeps every digit. */
export function shortNumber(n: number, loc: Locale, f?: NumFormat | null): string {
  if (!isFinite(n)) return '–';
  if (f?.percent) return percentText(n, loc, f);
  const body = f?.compact === false ? fullBody(n, loc, f.decimals) : compactBody(n, loc, f?.decimals);
  return decorate(body, loc, f?.currency);
}

/** The full form: grouped digits, up to 2 decimals. `compact: true` asks for the short form. */
export function longNumber(n: number, loc: Locale, f?: NumFormat | null): string {
  if (!isFinite(n)) return '–';
  if (f?.percent) return percentText(n, loc, f);
  const body = f?.compact === true ? compactBody(n, loc, f.decimals) : fullBody(n, loc, f?.decimals);
  return decorate(body, loc, f?.currency);
}

/** For tooltips: always the full number. In Indonesian a big one adds its unit word: "1.234.567 (1,2 jt)". */
export function tipNumber(n: number, loc: Locale, f?: NumFormat | null): string {
  if (!isFinite(n)) return '–';
  const full = longNumber(n, loc, { ...f, compact: false });
  if (loc !== 'id' || f?.percent || Math.abs(n) < COMPACT_FROM) return full;
  return `${full} (${shortNumber(n, loc, { ...f, currency: undefined, compact: true })})`;
}

/** A number with a fixed number of decimals and no grouping: "4,2" or "4.2". */
export const fixedNumber = (n: number, d: number, loc: Locale) => digitsFor(loc, `.${d}f`)(n);

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
