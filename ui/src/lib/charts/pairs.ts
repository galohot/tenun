// The numbers behind a compare chart: which way a pair moved, by how much, and in what words. Used
// for the tooltips and for the colour of a slope line. Pure code; its only import at run time is the words.

import { t } from '../i18n.ts';

export type Move = 'up' | 'down' | 'flat' | 'unknown';

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/** Which way the second value went from the first. A missing value on either side is `unknown`. */
export function moveOf(a: unknown, b: unknown): Move {
  if (!finite(a) || !finite(b)) return 'unknown';
  return b > a ? 'up' : b < a ? 'down' : 'flat';
}

/** Is the move good news? `better` is the direction that is good for this tile (default up). A flat or unknown move is neither. */
export function isGood(move: Move, better: 'up' | 'down' | undefined): boolean | null {
  if (move === 'flat' || move === 'unknown') return null;
  return (move === 'up') === ((better ?? 'up') === 'up');
}

/** The word and sign of a move: "▲ rose", "▼ fell". The sign comes with a word, so colour is never alone. */
export function moveWords(move: Move): { glyph: string; word: string } {
  switch (move) {
    case 'up':
      return { glyph: '▲', word: t('rose') };
    case 'down':
      return { glyph: '▼', word: t('fell') };
    case 'flat':
      return { glyph: '–', word: t('did not change') };
    default:
      return { glyph: '', word: t('has no value for one side') };
  }
}

/** The relative change from `a` to `b`, or null when there is no base to compare with. */
export function relative(a: unknown, b: unknown): number | null {
  return finite(a) && finite(b) && a !== 0 ? (b - a) / Math.abs(a) : null;
}

/** The rows that fit a chart: the ones with the biggest gap come first in the engine's order, so keep the first `n`. */
export const fitRows = <T>(rows: readonly T[], n: number): T[] => rows.slice(0, Math.max(1, n));
