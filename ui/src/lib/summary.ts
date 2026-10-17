// The sentence a screen reader gets for a chart, made from its data: its kind, its title, how many
// marks it has, and the highest and lowest. Pure code: the caller supplies the number and label words.

import type { Cell, QueryResult, Tile } from './api';
import { t, tn } from './i18n.ts';

const OTHER_KEY = '\u0000other';

/** Each kind's name, and how many marks it has: "12 bars". */
const KINDS: Record<string, { name: () => string; count: (n: number) => string }> = {
  bar: { name: () => t('Bar chart'), count: (n) => tn('{n} bar', '{n} bars', n) },
  line: { name: () => t('Line chart'), count: (n) => tn('{n} point', '{n} points', n) },
  area: { name: () => t('Area chart'), count: (n) => tn('{n} point', '{n} points', n) },
  scatter: { name: () => t('Scatter chart'), count: (n) => tn('{n} dot', '{n} dots', n) },
  histogram: { name: () => t('Histogram'), count: (n) => tn('{n} bin', '{n} bins', n) },
  waterfall: { name: () => t('Waterfall chart'), count: (n) => tn('{n} step', '{n} steps', n) },
  share: { name: () => t('Share of total'), count: (n) => tn('{n} part', '{n} parts', n) },
  donut: { name: () => t('Donut chart'), count: (n) => tn('{n} slice', '{n} slices', n) },
  pie: { name: () => t('Pie chart'), count: (n) => tn('{n} slice', '{n} slices', n) },
};

export interface Words {
  /** A group-by value, written the way the chart writes it. */
  label: (v: Cell, column: number) => string;
  /** A number of the first value column, written the way the chart writes it. */
  value: (v: number, column: number) => string;
}

const noPeriod = (s: string) => s.replace(/\.+$/, '');

export function chartSummary(tile: Pick<Tile, 'type'>, res: QueryResult, title: string, words: Words): string {
  const kind = KINDS[tile.type];
  if (!kind) return noPeriod(title);
  const out = [kind.name() + '.'];
  if (title) out.push(noPeriod(title) + '.');
  const groups = res.columns.filter((c) => c.role === 'dimension').length;
  // A split chart has one mark for each group of each category; the sentence counts the categories.
  const n = groups > 1 && tile.type !== 'scatter' ? new Set(res.rows.map((r) => String(r[0]))).size : res.rows.length;
  if (!res.rows.length) return [...out, t('No data.')].join(' ');
  out.push(`${kind.count(n)}.`);

  const mi = res.columns.findIndex((c) => c.role === 'measure');
  // A scatter has two values and no single "highest". Other charts name their extremes by the first value.
  if (tile.type === 'scatter' || mi < 0) return out.join(' ');
  const bins = res.bins;
  const name = (r: Cell[]) =>
    bins && typeof r[0] === 'number'
      ? t('{from} to {to}', { from: words.label(r[0], 0), to: words.label(Math.round((r[0] + bins.width) * 1e10) / 1e10, 0) })
      : r
          .slice(0, Math.max(1, groups))
          .map((v, i) => (v === OTHER_KEY ? t('Other') : words.label(v, i)))
          .join(', ');
  const rows = res.rows.filter((r) => typeof r[mi] === 'number' && !(res.other && r[0] === OTHER_KEY));
  if (!rows.length) return out.join(' ');
  const by = (pick: (a: number, b: number) => boolean) => rows.reduce((best, r) => (pick(r[mi] as number, best[mi] as number) ? r : best));
  const hi = by((a, b) => a > b);
  const lo = by((a, b) => a < b);
  const say = (r: Cell[]) => `${name(r)}, ${words.value(r[mi] as number, mi)}`;
  if (rows.length === 1) out.push(t('Value: {value}.', { value: say(hi) }));
  else out.push(t('Highest: {value}.', { value: say(hi) }), t('Lowest: {value}.', { value: say(lo) }));
  return out.join(' ');
}
