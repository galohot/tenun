// Unusual values (v5 build 09, V5-D11): the words for a value the engine marked. The engine sends the numbers (how many
// robust spreads from the median of the periods around it, that median, how many periods); the words are written here,
// in the reader's number format. Pure, with only type imports, so the node tests load it.

import type { Grain, QueryResult } from './api';

export type UnusualPoint = NonNullable<QueryResult['unusual']>[number];

const PERIODS: Partial<Record<Grain, [string, string]>> = {
  year: ['year', 'years'],
  quarter: ['quarter', 'quarters'],
  month: ['month', 'months'],
  week: ['week', 'weeks'],
  day: ['day', 'days'],
};

/** The engine writes "the periods around it are all the same" as ±1e308: JSON has no infinity. */
const FLAT = 1e300;

/** "About 4 times the usual spread above the 6 months around it (median 267M)." */
export function unusualWords(u: UnusualPoint, grain: Grain | null | undefined, short: (n: number) => string): string {
  const [one, many] = (grain && PERIODS[grain]) || ['period', 'periods'];
  const periods = `the ${u.around} ${u.around === 1 ? one : many} around it`;
  if (Math.abs(u.spreads) >= FLAT) return `Different from ${periods}, which are all ${short(u.median)}.`;
  const times = Math.max(1, Math.round(Math.abs(u.spreads)));
  return `About ${times} times the usual spread ${u.spreads > 0 ? 'above' : 'below'} ${periods} (median ${short(u.median)}).`;
}

/** Whether a mark is above the usual (drawn in the bad colour) or below it (the warn colour). */
export const isHigh = (u: UnusualPoint): boolean => u.spreads > 0;

/** "1 unusual value" · "3 unusual values". */
export const unusualCount = (n: number): string => `${n} unusual ${n === 1 ? 'value' : 'values'}`;
