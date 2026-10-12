// The numbers a tile shows, as a plain table with a header row. CSV downloads, the Excel request and
// the focus sheet's table view all read this one shape. Pure code: no app state.

import type { Cell, QueryResult, Tile } from './api';
import { nameOf } from './measures.ts';

/** The key the engine gives the "Other" group of a top N (same as `OTHER_KEY` in format.ts). */
const OTHER_KEY = '\u0000other';

export interface Plain {
  heads: string[];
  /** Same order as `heads`. A group-by value is as the engine sent it (ISO dates, numbers as numbers). */
  rows: Cell[][];
  /** A bold "Total" row, when the tile has totals. */
  total: Cell[] | null;
  /** Which columns hold numbers. */
  numeric: boolean[];
}

/** The history of a number tile, as a two-column result. A number tile has no rows of its own. */
export function kpiRows(tile: Tile, res: QueryResult): QueryResult {
  const bars = res.kpi?.bars ?? [];
  return {
    ...res,
    columns: [
      { name: 'Period', role: 'dimension', dtype: 'text' },
      { name: tile.measures[0]?.named ? nameOf(tile.measures[0].named) : (tile.measures[0]?.column ?? 'Value'), role: 'measure', dtype: 'number' },
    ],
    rows: bars.map((b) => [b.label, b.value]),
  };
}

/**
 * What the tile shows, after show-as and top N (so with its "Other" group), one row per mark.
 * `heads` are the column names the tile shows (`columnHeads`; for a number tile, Period and the value's name).
 */
export function tileTable(tile: Tile, res: QueryResult, heads: string[]): Plain {
  if (tile.type === 'kpi') {
    return { heads, rows: kpiRows(tile, res).rows, total: null, numeric: [false, true] };
  }
  heads = [...heads];
  const numeric = res.columns.map((c) => c.dtype === 'number');
  let rows = res.rows.map((r) => [...r]);
  // The same question one year earlier: one more column, matched on the first group.
  if (res.compare) {
    const by = new Map(res.compare.rows.map((r) => [String(r[0]), r[r.length - 1]]));
    rows = rows.map((r) => [...r, by.get(String(r[0])) ?? null]);
    heads.push(res.compare.label);
    numeric.push(true);
  }
  const groups = res.columns.filter((c) => c.role === 'dimension').length;
  const hasTotals = (tile.type === 'table' && tile.totals === true) || (tile.type === 'pivot' && tile.totals !== false);
  let total: Cell[] | null = null;
  if (hasTotals && groups > 0 && res.totals) {
    total = res.columns.map((c, i) => {
      if (i === 0) return 'Total';
      const j = i - groups;
      // A value shown another way has no total.
      if (j < 0 || (tile.showAs?.[j] && tile.showAs[j] !== 'value')) return null;
      return res.totals!.grand[j] ?? null;
    });
    if (res.compare) total.push(null);
  }
  return { heads, rows, total, numeric };
}

/** One CSV cell. Text that a spreadsheet would run as a formula gets a quote in front. */
export function csvCell(v: Cell, text: boolean): string {
  if (v === null || v === undefined) return '';
  let s = v === OTHER_KEY ? 'Other' : String(v);
  if (text && /^[=+\-@\t\r]/.test(s)) s = `'${s}`;
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** UTF-8 with a byte order mark so Excel reads accents, CRLF lines, dot decimals, ISO dates. */
export function toCsv(t: Plain): string {
  const line = (r: Cell[]) => r.map((v, i) => csvCell(v, typeof v === 'string' && !t.numeric[i])).join(',');
  const body = [...t.rows, ...(t.total ? [t.total] : [])].map(line);
  return '﻿' + [t.heads.map((h) => csvCell(h, true)).join(','), ...body].join('\r\n') + '\r\n';
}

/** A file name from a title: no characters a file system refuses. */
export function fileName(title: string, ext: string, fallback = 'tenun'): string {
  const base = title.replace(/[\\/:*?"<>|\u0000-\u001f]/g, '_').trim().slice(0, 100);
  return `${base || fallback}.${ext}`;
}
