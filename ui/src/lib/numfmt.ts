// How a number is written: every field of a format (formats §2). Pure functions, so the app, the viewer and the
// node tests share them. The engine writes the same text (crates/tenun-core/src/numtext.rs);
// ui/tests/fixtures/numfmt.json holds both to it. `locale.ts` keeps the names the rest of the app calls.

import { formatLocale, type FormatLocaleObject } from 'd3-format';
import type { Locale } from './locale';
import type { NumFormat } from './api';
import CURRENCIES from './currencies.json' with { type: 'json' };
import { runCode, type Marks, type Tone } from './numcode.ts';

export type { Tone } from './numcode.ts';
export { codeProblem } from './numcode.ts';

/** How digits are grouped and which mark is the decimal (formats §2.2). Unset: from the language. */
export type Digits = 'comma_dot' | 'dot_comma' | 'space_comma' | 'apos_dot' | 'indian';

export const DIGITS: { id: Digits; example: string }[] = [
  { id: 'comma_dot', example: '1,234,567.5' },
  { id: 'dot_comma', example: '1.234.567,5' },
  { id: 'space_comma', example: '1 234 567,5' },
  { id: 'apos_dot', example: '1’234’567.5' },
  { id: 'indian', example: '12,34,567.5' },
];

const MARKS: Record<Digits, Marks> = {
  comma_dot: { decimal: '.', thousands: ',', grouping: [3] },
  dot_comma: { decimal: ',', thousands: '.', grouping: [3] },
  space_comma: { decimal: ',', thousands: ' ', grouping: [3] },
  apos_dot: { decimal: '.', thousands: '’', grouping: [3] },
  indian: { decimal: '.', thousands: ',', grouping: [3, 2] },
};

export const isDigits = (d: unknown): d is Digits => typeof d === 'string' && d in MARKS;
export const digitsFor = (loc: Locale, d?: Digits | null): Digits => (d && isDigits(d) ? d : loc === 'id' ? 'dot_comma' : 'comma_dot');

const D3 = new Map<Digits, FormatLocaleObject>();
function d3For(d: Digits): FormatLocaleObject {
  let f = D3.get(d);
  if (!f) {
    const m = MARKS[d];
    // d3 cycles the group sizes; Indian groups are 3, then 2 again and again.
    const grouping = m.grouping.length > 1 ? [m.grouping[0], ...Array(12).fill(m.grouping[1])] : m.grouping;
    f = formatLocale({ decimal: m.decimal, thousands: m.thousands, grouping, currency: ['', ''] });
    D3.set(d, f);
  }
  return f;
}
const d3 = (d: Digits, spec: string) => d3For(d).format(spec);

// ---- currencies -------------------------------------------------------------------------------------------

const SYMBOLS = CURRENCIES as Record<string, string>;
/** Every currency Tenun knows, by ISO 4217 code. */
export const CURRENCY_CODES = Object.keys(SYMBOLS);
/** The ones most people pick, shown first. */
export const COMMON_CURRENCIES = ['IDR', 'USD', 'EUR', 'SGD', 'MYR', 'JPY', 'CNY', 'GBP', 'AUD', 'MZN', 'ZAR', 'INR'];

/** Where a currency goes, [before the number, after it]. The first five keep how Tenun wrote them before. */
export function currencyParts(code: string, loc: Locale, iso = false): [string, string] {
  if (iso) return [`${code} `, ''];
  switch (code) {
    case 'IDR': return ['Rp ', ''];
    case 'USD': return [loc === 'id' ? 'US$' : '$', ''];
    case 'EUR': return ['€', ''];
    case 'SGD': return ['S$', ''];
    case 'MZN': return loc === 'id' ? ['', ' MT'] : ['MT ', ''];
  }
  const s = SYMBOLS[code];
  if (!s) return [`${code} `, ''];
  return [/\p{L}$/u.test(s) ? `${s} ` : s, ''];
}

// ---- units --------------------------------------------------------------------------------------------------

/** The words for 10³, 10⁶, 10⁹, 10¹². They follow the language, not the digit style. */
const UNITS: Record<Locale, Record<number, string>> = {
  en: { 3: 'K', 6: 'M', 9: 'B', 12: 'T' },
  id: { 3: ' rb', 6: ' jt', 9: ' M', 12: ' T' },
};
const D3_POWER: Record<string, number> = { k: 3, M: 6, G: 9, T: 12 };
const STEPS = [12, 9, 6, 3];

/** Below this a short number keeps all its digits. */
const COMPACT_FROM = 10_000;

const MINUS = '−';

// ---- bodies -------------------------------------------------------------------------------------------------

const fixedGroup = (n: number, dec: number, d: Digits) => d3(d, `,.${dec}f`)(n);
const sigGroup = (n: number, sig: number, d: Digits) => d3(d, `,.${sig}r`)(n);
const clampInt = (v: unknown, lo: number, hi: number): number | undefined =>
  typeof v === 'number' && Number.isFinite(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : undefined;

function fullBody(n: number, d: Digits, dec?: number, sig?: number): string {
  if (sig !== undefined) return sigGroup(n, sig, d);
  if (dec !== undefined) return fixedGroup(n, dec, d);
  return Number.isInteger(n) ? d3(d, ',d')(n) : d3(d, ',.2~f')(n);
}

function compactBody(n: number, loc: Locale, d: Digits, dec?: number, sig?: number): string {
  const a = Math.abs(n);
  if (a < COMPACT_FROM) {
    if (sig !== undefined) return sigGroup(n, sig, d);
    if (dec !== undefined) return fixedGroup(n, dec, d);
    return a >= 100 || Number.isInteger(n) ? d3(d, ',d')(Math.round(n)) : d3(d, ',.2~f')(n);
  }
  if (dec === undefined) {
    // Three significant digits by default: 12.9K · 4.2M · 1.3B. `sig` keeps its zeros: 1.0M.
    const m = /^(.*?)([kMGTPEZY])?$/.exec(d3(d, sig !== undefined ? `.${sig}s` : '.3~s')(n))!;
    return m[1] + (m[2] ? UNITS[loc][D3_POWER[m[2]]] ?? m[2] : '');
  }
  let i = STEPS.findIndex((p) => a >= 10 ** p);
  if (i < 0) return fixedGroup(n, dec, d);
  // 999,999 rounds up to 1,000 thousand: use the next unit instead.
  if (i > 0 && Number((a / 10 ** STEPS[i]).toFixed(dec)) >= 1000) i--;
  const p = STEPS[i];
  return fixedGroup(n / 10 ** p, dec, d) + UNITS[loc][p];
}

// ---- durations ----------------------------------------------------------------------------------------------

const SECONDS: Record<string, number> = { s: 1, m: 60, h: 3600, d: 86400 };
const LETTERS: Record<Locale, string[]> = { en: ['d', 'h', 'm', 's'], id: ['h', 'j', 'm', 'd'] };
const pad2 = (v: number) => String(v).padStart(2, '0');

/** A duration: "1:02:05" in full, "1h 2m" short. Seconds are kept only for a value in seconds. */
function durationBody(n: number, unit: string, loc: Locale, short: boolean): string {
  const secs = Math.abs(n) * SECONDS[unit];
  const t = unit === 's' ? Math.round(secs) : Math.round(secs / 60) * 60;
  const sign = n < 0 && t > 0 ? MINUS : '';
  if (!short) {
    const h = Math.floor(t / 3600);
    const m = Math.floor((t % 3600) / 60);
    return sign + (unit === 's' ? `${h}:${pad2(m)}:${pad2(t % 60)}` : `${h}:${pad2(m)}`);
  }
  const parts = [Math.floor(t / 86400), Math.floor((t % 86400) / 3600), Math.floor((t % 3600) / 60), t % 60];
  const L = LETTERS[loc];
  const i = parts.findIndex((p) => p > 0);
  if (i < 0) return `0${L[unit === 's' ? 3 : 2]}`;
  let out = `${parts[i]}${L[i]}`;
  if (i + 1 < 4 && parts[i + 1] > 0) out += ` ${parts[i + 1]}${L[i + 1]}`;
  return sign + out;
}

// ---- the whole ----------------------------------------------------------------------------------------------

const touches = /[\p{L}\p{N}]/u;
function withAffixes(s: string, f: NumFormat): string {
  const pre = f.prefix ?? '';
  const post = f.suffix ?? '';
  const a = pre && touches.test(pre.slice(-1)) ? `${pre} ` : pre;
  const b = post && touches.test(post[0]) ? ` ${post}` : post;
  return a + s + b;
}

/** A minus sign (d3 writes U+2212) is taken off, so a currency or a bracket can go around the number. */
const unsign = (s: string): [boolean, string] => (s.startsWith(MINUS) ? [true, s.slice(1)] : [false, s]);

export interface Written {
  text: string;
  tone: Tone | null;
}

/**
 * `n` in format `f`: the short form (charts, number tiles: 1.2M) or the full form (tables, tooltips: 1,234,567).
 * A format code decides everything when it can be read; otherwise the fields do, in this order of parts:
 * [prefix][− or (][currency before][body][unit word][currency after][)][suffix].
 */
export function writeNumber(n: number, loc: Locale, f: NumFormat | null | undefined, digits: Digits | null | undefined, short: boolean): Written {
  if (!Number.isFinite(n)) return { text: '–', tone: null };
  const d = digitsFor(loc, digits);
  if (f?.code) {
    const done = runCode(f.code, n, MARKS[d], (a) => fullBody(a, d));
    if (done) return done;
  }
  const g: NumFormat = f ?? {};
  if (typeof g.zero === 'string' && g.zero !== '' && n === 0) return { text: g.zero, tone: null };
  const dec = clampInt(g.decimals, 0, 6);
  const sig = typeof g.sig === 'number' && g.sig >= 1 ? clampInt(g.sig, 1, 6) : undefined;
  let body: string;
  let cur: [string, string] = ['', ''];
  if (g.duration && g.duration in SECONDS) {
    body = durationBody(n, g.duration, loc, short);
  } else if (g.percent) {
    const v = n * 100;
    body = (sig !== undefined ? sigGroup(v, sig, d) : dec !== undefined ? fixedGroup(v, dec, d) : d3(d, ',.1~f')(v)) + '%';
  } else {
    if (g.currency) cur = currencyParts(g.currency, loc, !!g.iso);
    const scale = g.scale === 3 || g.scale === 6 || g.scale === 9 || g.scale === 12 ? g.scale : undefined;
    if (scale) {
      const v = n / 10 ** scale;
      body = (sig !== undefined ? sigGroup(v, sig, d) : fixedGroup(v, dec ?? 1, d)) + UNITS[loc][scale];
    } else if (short ? g.compact !== false : g.compact === true) {
      body = compactBody(n, loc, d, dec, sig);
    } else {
      body = fullBody(n, d, dec, sig);
    }
  }
  const [neg, rest] = unsign(body);
  const inner = cur[0] + rest + cur[1];
  const style = g.negative ?? 'minus';
  const parens = neg && (style === 'parens' || style === 'parens_red');
  const text = withAffixes(neg ? (parens ? `(${inner})` : MINUS + inner) : inner, g);
  return { text, tone: neg && (style === 'red' || style === 'parens_red') ? 'bad' : null };
}

/** The tone of a value in a format, for a cell or a number tile that colours it. */
export function toneOf(n: number, loc: Locale, f: NumFormat | null | undefined, digits?: Digits | null): Tone | null {
  if (!f || (!f.code && f.negative !== 'red' && f.negative !== 'parens_red')) return null;
  return writeNumber(n, loc, f, digits, false).tone;
}

/** The text of an empty cell in a format: its `blank` text, or nothing. */
export const blankText = (f: NumFormat | null | undefined): string => (typeof f?.blank === 'string' ? f.blank : '');

/** The format writes the same text in both forms (a code, a fixed unit, a duration, a percent): a tooltip adds nothing. */
export const oneForm = (f: NumFormat | null | undefined): boolean => !!(f?.code || f?.scale || f?.duration || f?.percent);
