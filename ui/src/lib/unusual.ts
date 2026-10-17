// Unusual values (v5 build 09, V5-D11): the words for a value the engine marked. The engine sends the numbers (how many
// robust spreads from the median of the periods around it, that median, how many periods); the words are written here,
// in the reader's number format. Pure, with type imports and t(), so the node tests load it.

import type { Grain, QueryResult } from './api';
import { t, tn } from './i18n.ts';

export type UnusualPoint = NonNullable<QueryResult['unusual']>[number];

/** "the 6 months around it", in the grain of the chart. */
function around(n: number, grain: Grain | null | undefined): string {
  switch (grain) {
    case 'year': return tn('the {n} year around it', 'the {n} years around it', n);
    case 'quarter': return tn('the {n} quarter around it', 'the {n} quarters around it', n);
    case 'month': return tn('the {n} month around it', 'the {n} months around it', n);
    case 'week': return tn('the {n} week around it', 'the {n} weeks around it', n);
    case 'day': return tn('the {n} day around it', 'the {n} days around it', n);
    default: return tn('the {n} period around it', 'the {n} periods around it', n);
  }
}

/** The engine writes "the periods around it are all the same" as ±1e308: JSON has no infinity. */
const FLAT = 1e300;

/** "About 4 times the usual spread above the 6 months around it (median 267M)." */
export function unusualWords(u: UnusualPoint, grain: Grain | null | undefined, short: (n: number) => string): string {
  const periods = around(u.around, grain);
  if (Math.abs(u.spreads) >= FLAT) return t('Different from {periods}, which are all {median}.', { periods, median: short(u.median) });
  const times = Math.max(1, Math.round(Math.abs(u.spreads)));
  return u.spreads > 0
    ? t('About {times} times the usual spread above {periods} (median {median}).', { times, periods, median: short(u.median) })
    : t('About {times} times the usual spread below {periods} (median {median}).', { times, periods, median: short(u.median) });
}

/** Whether a mark is above the usual (drawn in the bad colour) or below it (the warn colour). */
export const isHigh = (u: UnusualPoint): boolean => u.spreads > 0;

/** "1 unusual value" · "3 unusual values". */
export const unusualCount = (n: number): string => tn('{n} unusual value', '{n} unusual values', n);
