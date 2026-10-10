// The look of the cells of a table or pivot (build 07): a data bar behind the number, or a number
// coloured good or bad. Pure functions with no imports at run time, so the node tests can load them.

import type { CondFormat, ShowAs } from './api';

/** The range the bars of one value are drawn against. Zero is always inside it. */
export interface BarScale {
  min: number;
  max: number;
}

/** The scale of a column of numbers: from the lowest below zero to the highest above it. */
export function barScale(values: (number | null | undefined)[]): BarScale {
  const nums = values.filter((v): v is number => typeof v === 'number' && Number.isFinite(v));
  return { min: Math.min(0, ...nums), max: Math.max(0, ...nums) };
}

/**
 * Where a bar sits inside its cell, as percent of the cell's width. The bar starts at zero and runs
 * to the value: right for a positive value, left for a negative one. null where there is no bar.
 */
export function barSpan(v: number | null | undefined, scale: BarScale): { from: number; to: number } | null {
  if (typeof v !== 'number' || !Number.isFinite(v) || v === 0) return null;
  const range = scale.max - scale.min;
  if (range <= 0) return null;
  const at = (x: number) => ((x - scale.min) / range) * 100;
  return v > 0 ? { from: at(0), to: at(v) } : { from: at(v), to: at(0) };
}

/** Ink grey at 18 %, drawn as the background of the cell: a thin bar in the middle of its height, inside the padding. */
export function barStyle(span: { from: number; to: number } | null): string {
  if (!span) return '';
  const ink = 'color-mix(in srgb, var(--text) 18%, transparent)';
  const { from, to } = span;
  return (
    `background-image:linear-gradient(to right,transparent ${from}%,${ink} ${from}%,${ink} ${to}%,transparent ${to}%);` +
    // The bar runs inside the padding of the cell, so the bars of two columns never touch and none passes the number's edge.
    'background-size:100% 10px;background-repeat:no-repeat;background-position:center;background-origin:content-box'
  );
}

export interface Verdict {
  /** The glyph that always goes with the colour. */
  glyph: '▲' | '▼';
  tone: 'good' | 'bad';
  /** For a tooltip and a screen reader: "Above 100" or "Below 100". */
  word: 'Above' | 'Below';
}

/** A change or a % change is compared with 0; any other value with the threshold (0 when none). */
export const isChange = (how: ShowAs | null | undefined): boolean => how === 'change' || how === 'pct_change';

/**
 * Does the number, as it reads on screen, say the same as the line? "0%" reads as 0 whatever the
 * digits behind it; "−0.04%" does not. `text` and `lineText` are the written value and the written line.
 */
export function readsAsLine(text: string, line: number, lineText: string): boolean {
  const bare = (t: string) => t.replace(/[-\u2212+\s]/g, '');
  return bare(text) === bare(lineText) || (line === 0 && !/[1-9]/.test(text));
}

/**
 * Is this number good or bad? Above the line is good when higher is better (the default) and bad when
 * lower is better. A number on the line has no verdict; neither has one that is written the same as
 * the line (pass `shown` and `lineShown`), so a red ▼ never sits beside "0%". The glyph says the way,
 * the tone says the judgement, so colour is never alone.
 */
export function goodBad(v: number | null | undefined, cond: CondFormat, how?: ShowAs | null, shown?: string, lineShown?: string): Verdict | null {
  if (typeof v !== 'number' || !Number.isFinite(v)) return null;
  const line = isChange(how) ? 0 : (cond.threshold ?? 0);
  if (v === line) return null;
  if (shown !== undefined && lineShown !== undefined && readsAsLine(shown, line, lineShown)) return null;
  const up = v > line;
  const goodWhenUp = (cond.better ?? 'up') === 'up';
  return { glyph: up ? '▲' : '▼', tone: up === goodWhenUp ? 'good' : 'bad', word: up ? 'Above' : 'Below' };
}
