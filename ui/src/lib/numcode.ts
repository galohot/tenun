// A number format code, the part of Excel's syntax a dashboard needs (formats §2.1):
//   #,##0.00;(#,##0.00);"–"    [>=1000000]0.0,,"M";[>=1000]0.0,"K";0    [Red]-0    0.00E+00    000-0000
// The engine writes the same text (crates/tenun-core/src/numcode.rs); ui/tests/fixtures/numfmt.json holds both to it.
// The marks of the result follow the digit style, as Excel follows the reader's locale.

import { getLang, t } from './i18n.ts';

/** The marks a number is written with: decimal mark, thousands mark, group sizes (Indian: 3 then 2). */
export interface Marks {
  decimal: string;
  thousands: string;
  grouping: number[];
}

/** What a code marks its value as: `[Red]` bad, `[Green]` good. */
export type Tone = 'bad' | 'good';

type Tok =
  | { t: 'lit'; s: string }
  | { t: 'digit'; c: '0' | '#' | '?' }
  | { t: 'point' }
  | { t: 'comma' }
  | { t: 'pct' }
  | { t: 'exp'; plus: boolean }
  | { t: 'general' };

interface Section {
  toks: Tok[];
  tone: Tone | null;
  cond: ((n: number) => boolean) | null;
}

const MINUS = '−';
const FIGURE = ' ';
const COLOURS = /^(black|blue|cyan|green|magenta|red|white|yellow|color\s*\d+)$/i;
const COND = /^(<=|>=|<>|<|>|=)\s*(-?\d+(?:\.\d+)?(?:e[+-]?\d+)?)$/i;

const parsed = new Map<string, { sections: Section[] } | { problem: string }>();

/** Why a code cannot be used, in words, or null when it can. */
export function codeProblem(code: string): string | null {
  const p = parse(code);
  return 'problem' in p ? p.problem : null;
}

/** The text of `n` in a code and its tone, or null when the code cannot be used (the caller writes the default). */
export function runCode(
  code: string,
  n: number,
  marks: Marks,
  general: (a: number) => string,
): { text: string; tone: Tone | null } | null {
  const p = parse(code);
  if ('problem' in p) return null;
  const { sections } = p;
  let sec: Section | undefined;
  let signed: boolean;
  if (sections.some((s) => s.cond)) {
    // Conditions: the first section whose condition holds; the first one without a condition is the rest.
    // No condition holds and every part has one: the last part, as Excel does.
    sec = sections.find((s) => s.cond?.(n)) ?? sections.find((s) => !s.cond) ?? sections[sections.length - 1];
    signed = true;
  } else if (sections.length === 1) {
    sec = sections[0];
    signed = true;
  } else {
    sec = n > 0 ? sections[0] : n < 0 ? sections[1] : sections.length >= 3 ? sections[2] : sections[0];
    signed = false;
  }
  if (!sec) return { text: '', tone: null };
  const { text, zero } = render(sec.toks, Math.abs(n), marks, general);
  const neg = signed && n < 0 && !zero;
  return { text: (neg ? MINUS : '') + text, tone: sec.tone };
}

function parse(code: string): { sections: Section[] } | { problem: string } {
  // A problem is written in the language of the screens, so the key holds it.
  const key = `${getLang()}\u0000${code}`;
  let p = parsed.get(key);
  if (!p) {
    p = parseNow(code);
    if (parsed.size > 500) parsed.clear();
    parsed.set(key, p);
  }
  return p;
}

function parseNow(code: string): { sections: Section[] } | { problem: string } {
  const sections: Section[] = [];
  let toks: Tok[] = [];
  let tone: Tone | null = null;
  let cond: Section['cond'] = null;
  let points = 0;
  const close = () => {
    sections.push({ toks, tone, cond });
    toks = [];
    tone = null;
    cond = null;
    points = 0;
  };
  for (let i = 0; i < code.length; i++) {
    const c = code[i];
    if (c === '"') {
      const end = code.indexOf('"', i + 1);
      if (end < 0) return { problem: t('A quote is not closed.') };
      toks.push({ t: 'lit', s: code.slice(i + 1, end) });
      i = end;
    } else if (c === '\\') {
      if (i + 1 < code.length) toks.push({ t: 'lit', s: code[++i] });
    } else if (c === '_') {
      i++;
      toks.push({ t: 'lit', s: ' ' });
    } else if (c === '*') {
      i++;
    } else if (c === '[') {
      const end = code.indexOf(']', i + 1);
      if (end < 0) return { problem: t('A [ is not closed.') };
      const inner = code.slice(i + 1, end).trim();
      i = end;
      const m = COND.exec(inner);
      if (COLOURS.test(inner)) {
        const w = inner.toLowerCase();
        tone = w === 'red' ? 'bad' : w === 'green' ? 'good' : tone;
      } else if (m) {
        const v = Number(m[2]);
        const op = m[1];
        cond = (n) =>
          op === '<' ? n < v : op === '<=' ? n <= v : op === '>' ? n > v : op === '>=' ? n >= v : op === '=' ? n === v : n !== v;
      } else if (inner.startsWith('$')) {
        // [$€-407]: a currency symbol with a locale; the symbol is written.
        toks.push({ t: 'lit', s: inner.slice(1).split('-')[0] });
      } else {
        return { problem: t('Tenun does not know [{code}].', { code: inner }) };
      }
    } else if (c === ';') {
      close();
      if (sections.length > 3) return { problem: t('A code has 4 parts at most.') };
    } else if (c === '0' || c === '#' || c === '?') {
      toks.push({ t: 'digit', c });
    } else if (c === '.') {
      if (++points > 1) return { problem: t('A part has one decimal point at most.') };
      toks.push({ t: 'point' });
    } else if (c === ',') {
      toks.push({ t: 'comma' });
    } else if (c === '%') {
      toks.push({ t: 'pct' });
    } else if ((c === 'E' || c === 'e') && (code[i + 1] === '+' || code[i + 1] === '-') && /[0#?]/.test(code[i + 2] ?? '')) {
      toks.push({ t: 'exp', plus: code[i + 1] === '+' });
      i++;
    } else if (c === '@') {
      // Text: a number has none.
    } else if (/^general/i.test(code.slice(i))) {
      toks.push({ t: 'general' });
      i += 6;
    } else {
      toks.push({ t: 'lit', s: c });
    }
  }
  close();
  if (sections.length > 4) return { problem: t('A code has 4 parts at most.') };
  return { sections };
}

/** JavaScript's rounding (toFixed), which the rest of the app uses: the exact value, a tie up. */
const fixed = (a: number, d: number) => a.toFixed(d);

function group(int: string, m: Marks): string {
  const out: string[] = [];
  let end = int.length;
  let g = 0;
  while (end > 0) {
    const size = m.grouping[Math.min(g, m.grouping.length - 1)];
    out.unshift(int.slice(Math.max(0, end - size), end));
    end -= size;
    g++;
  }
  return out.join(m.thousands);
}

function render(toks: Tok[], a: number, m: Marks, general: (a: number) => string): { text: string; zero: boolean } {
  const expAt = toks.findIndex((t) => t.t === 'exp');
  const mant = expAt < 0 ? toks : toks.slice(0, expAt);
  const pointAt = mant.findIndex((t) => t.t === 'point');
  const digitIdx = mant.flatMap((t, i) => (t.t === 'digit' ? [i] : []));
  const intIdx = digitIdx.filter((i) => pointAt < 0 || i < pointAt);
  const fracIdx = digitIdx.filter((i) => pointAt >= 0 && i > pointAt);
  const lastDigit = digitIdx.length ? digitIdx[digitIdx.length - 1] : -1;
  // A comma after the last placeholder divides by 1000; one between integer placeholders groups the digits.
  let scale = 0;
  let grouped = false;
  const silent = new Set<number>();
  mant.forEach((t, i) => {
    if (t.t !== 'comma') return;
    if (lastDigit >= 0 && i > lastDigit) {
      scale++;
      silent.add(i);
    } else if (intIdx.length && i > intIdx[0] && i < intIdx[intIdx.length - 1]) {
      grouped = true;
      silent.add(i);
    } else if (pointAt >= 0 && i === pointAt - 1 && intIdx.length) {
      scale++;
      silent.add(i);
    }
  });
  const pcts = toks.filter((t) => t.t === 'pct').length;
  let v = a * 100 ** pcts / 1000 ** scale;

  let expText = '';
  if (expAt >= 0) {
    const ex = toks[expAt] as Extract<Tok, { t: 'exp' }>;
    const expDigits = toks.slice(expAt + 1).filter((t) => t.t === 'digit').length;
    const ints = Math.max(1, intIdx.length);
    let e = v === 0 ? 0 : Math.floor(Math.log10(v)) - (ints - 1);
    let mv = v / 10 ** e;
    if (Number(fixed(mv, fracIdx.length)) >= 10 ** ints) {
      e += 1;
      mv = v / 10 ** e;
    }
    v = mv;
    const sign = e < 0 ? '-' : ex.plus ? '+' : '';
    expText = 'E' + sign + String(Math.abs(e)).padStart(expDigits, '0');
  }

  if (!digitIdx.length) {
    const hasGeneral = toks.some((t) => t.t === 'general');
    const text = toks.map((t) => (t.t === 'lit' ? t.s : t.t === 'general' ? general(a) : t.t === 'pct' ? '%' : '')).join('');
    return { text, zero: hasGeneral ? a === 0 : true };
  }

  const s = fixed(v, fracIdx.length);
  const [intRaw, fracRaw = ''] = s.split('.');
  const zero = Number(s) === 0;
  // Fraction: '#' drops a trailing zero, '?' turns it into a figure space.
  const frac = fracRaw.split('');
  const fracOut = fracIdx.map((_, k) => frac[k]);
  for (let k = fracIdx.length - 1; k >= 0; k--) {
    const c = (mant[fracIdx[k]] as Extract<Tok, { t: 'digit' }>).c;
    if (fracOut[k] !== '0' || c === '0') break;
    fracOut[k] = c === '?' ? FIGURE : '';
  }
  // Integer: filled from the right; the leftmost placeholder takes every digit left over.
  const digits = intRaw === '0' ? '' : intRaw;
  const intOut = new Map<number, string>();
  if (grouped) {
    const zeros = intIdx.filter((i) => (mant[i] as Extract<Tok, { t: 'digit' }>).c === '0').length;
    const body = digits.padStart(zeros, '0');
    intIdx.forEach((i, k) => intOut.set(i, k === 0 ? (body ? group(body, m) : '') : ''));
  } else {
    let rest = digits;
    for (let k = intIdx.length - 1; k >= 0; k--) {
      const c = (mant[intIdx[k]] as Extract<Tok, { t: 'digit' }>).c;
      let put: string;
      if (k === 0) put = rest || (c === '0' ? '0' : c === '?' ? FIGURE : '');
      else if (rest) put = rest.slice(-1);
      else put = c === '0' ? '0' : c === '?' ? FIGURE : '';
      if (k > 0) rest = rest.slice(0, -1);
      intOut.set(intIdx[k], put);
    }
    if (!intIdx.length && digits) intOut.set(-1, digits);
  }

  let out = '';
  let fk = 0;
  toks.forEach((t, i) => {
    if (expAt >= 0 && i > expAt) return;
    if (t.t === 'lit') out += t.s;
    else if (t.t === 'pct') out += '%';
    else if (t.t === 'point') out += (intOut.get(-1) ?? '') + m.decimal;
    else if (t.t === 'exp') out += expText;
    else if (t.t === 'general') out += general(a);
    else if (t.t === 'digit') out += intIdx.includes(i) ? intOut.get(i) ?? '' : fracOut[fk++] ?? '';
    else if (t.t === 'comma' && !silent.has(i)) out += ',';
  });
  return { text: out, zero };
}
