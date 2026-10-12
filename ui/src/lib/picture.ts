import type { Cell, Tile } from './api';
import { stepsByDate, tileProblem, type Parts } from './measures.ts';

// What a picture of a tile says under it (V4-D52, V4-D54). A picture leaves the computer, so it takes
// the notes that have no key value, no file name and no formula: `publicNotes`. The notes of the app
// (`notes`) name them and stay on the screen.

/** The line under a picture: the parts that are words already, then the public notes of the result. */
export function pictureNote(res: { notes?: string[]; publicNotes?: string[] } | null | undefined, ...parts: (string | null | undefined)[]): string {
  return [...parts, ...(res?.publicNotes ?? [])].filter((p): p is string => !!p).join(' ');
}

/**
 * How a tile becomes a picture (V4-D99): `chart` when the chart is drawn again off screen, `screen` when what is
 * on the screen is copied. A tile that the gate refuses (a share chart or a waterfall of a value that does not
 * add up) is a message on the screen, so it is copied as it is: the chart code is never asked to draw it.
 */
export function pictureKind(
  tile: Pick<Tile, 'type' | 'measures' | 'dims'>,
  res: { rows: unknown[]; columns: { dtype: string }[]; parts?: Parts } | null | undefined,
  chartTypes: readonly string[],
): 'chart' | 'screen' {
  if (!res || !chartTypes.includes(tile.type) || res.rows.length === 0) return 'screen';
  return tileProblem(tile, res.parts, stepsByDate(tile, res), res.rows as Cell[][]) ? 'screen' : 'chart';
}
