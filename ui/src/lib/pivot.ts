// The layout of a pivot table from an engine result: which rows, which columns, which cell holds
// what. The engine computes every number, including the totals; this only arranges them. Pure code
// with no imports at run time, so the node tests can load it.

import type { Cell, QueryResult, Tile } from './api';

/** At most this many columns are drawn. */
export const MAX_PIVOT_COLS = 30;
/** The engine reads at most this many rows (cells) for a pivot. */
export const MAX_PIVOT_ROWS = 5000;

const keyOf = (v: Cell | undefined): string => (v === null || v === undefined ? '\u0000null' : String(v));
const tupleKey = (vs: Cell[]): string => vs.map(keyOf).join('\u0001');

export interface PivotModel {
  /** How many fields make the rows, and how many values sit under each column. */
  nRows: number;
  nValues: number;
  /** The columns drawn. With no column field there is one column with no heading. */
  cols: { raw: Cell; key: string }[];
  /** All the column values there are, and whether some were left out. */
  colCount: number;
  cut: boolean;
  /** The engine stopped reading cells at its row limit, so some cells may be missing. */
  limited: boolean;
  /** The column field has a heading (false: a plain list of the row fields). */
  hasCols: boolean;
  rows: {
    raw: Cell[];
    key: string;
    /** [column][value]; null where the row has no data under that column. */
    cells: (Cell[] | null)[];
    /** Measure totals of the row, when the pivot has totals and a column field. */
    total: Cell[] | null;
  }[];
  /** [column] → measure totals of that column; null where unknown. */
  colTotals: (Cell[] | null)[];
  grand: Cell[] | null;
}

const naturalOrder = (a: Cell, b: Cell): number => {
  if (a === b) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  return String(a).localeCompare(String(b), undefined, { numeric: true });
};

export function pivotModel(res: QueryResult, tile: Pick<Tile, 'cols'>): PivotModel {
  const nd = res.columns.filter((c) => c.role === 'dimension').length;
  const hasCols = !!tile.cols && nd > 0;
  const nRows = hasCols ? nd - 1 : nd;
  const nValues = res.columns.length - nd;
  const totals = res.totals;
  const split = (r: Cell[]) => ({
    row: r.slice(0, nRows),
    col: hasCols ? r[nRows] : null,
    values: r.slice(nd),
  });

  // Columns: the engine's order (the column totals list it), else sorted. Left out beyond 30.
  const seenCols = new Map<string, Cell>();
  for (const r of res.rows) {
    const { col } = split(r);
    if (!seenCols.has(keyOf(col))) seenCols.set(keyOf(col), col);
  }
  let colList: Cell[];
  if (!hasCols) colList = [null];
  else if (totals?.cols) {
    colList = totals.cols.map((c) => c[0]).filter((c) => seenCols.has(keyOf(c)));
    for (const [k, v] of seenCols) if (!colList.some((c) => keyOf(c) === k)) colList.push(v);
  } else colList = [...seenCols.values()].sort(naturalOrder);
  const cols = colList.slice(0, MAX_PIVOT_COLS).map((raw) => ({ raw, key: keyOf(raw) }));

  // Rows: the engine's order when it sent row totals, else as they first appear.
  const byRow = new Map<string, { raw: Cell[]; cells: Map<string, Cell[]> }>();
  for (const r of res.rows) {
    const { row, col, values } = split(r);
    const k = tupleKey(row);
    const entry = byRow.get(k) ?? { raw: row, cells: new Map() };
    entry.cells.set(keyOf(col), values);
    byRow.set(k, entry);
  }
  const rowTotals = new Map<string, Cell[]>();
  for (const t of totals?.rows ?? []) rowTotals.set(tupleKey(t.slice(0, nRows)), t.slice(nRows));
  let order = [...byRow.keys()];
  if (totals?.rows) {
    const wanted = totals.rows.map((t) => tupleKey(t.slice(0, nRows))).filter((k) => byRow.has(k));
    const rest = order.filter((k) => !wanted.includes(k));
    order = [...wanted, ...rest];
  }
  // With more than one row field, rows of the same first value sit together. The first values keep
  // the order they first appear in, and so does every row inside a group.
  if (nRows > 1) {
    const groups = new Map<string, string[]>();
    for (const k of order) {
      const g = keyOf(byRow.get(k)!.raw[0]);
      groups.set(g, [...(groups.get(g) ?? []), k]);
    }
    order = [...groups.values()].flat();
  }
  // The engine's column list counts every column, also those it did not read cells for.
  const colCount = hasCols && totals?.cols ? Math.max(totals.cols.length, colList.length) : colList.length;
  const colTotalOf = new Map<string, Cell[]>();
  for (const t of totals?.cols ?? []) colTotalOf.set(keyOf(t[0]), t.slice(1));

  return {
    nRows,
    nValues,
    cols,
    colCount,
    cut: colCount > cols.length,
    limited: res.rows.length >= MAX_PIVOT_ROWS,
    hasCols,
    rows: order.map((k) => {
      const e = byRow.get(k)!;
      return {
        raw: e.raw,
        key: k,
        cells: cols.map((c) => e.cells.get(c.key) ?? null),
        // With no row field the one row is the whole data.
        total: rowTotals.get(k) ?? (nRows === 0 ? (totals?.grand ?? null) : null),
      };
    }),
    colTotals: cols.map((c) => colTotalOf.get(c.key) ?? null),
    grand: totals?.grand ?? null,
  };
}
