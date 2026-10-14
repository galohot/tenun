// Pie and donut (build 07, V4-D7): a result as slices, drawn by `pieparts.ts`. Whether a value can be cut into
// slices is the one gate's question (measures.ts `tileProblem`), asked before this draws.

import { cell, getLocale, measureLabel, numberFormatter, OTHER_KEY } from '../format';
import { formatOf } from '../measures';
import { tipHtml, tipKey } from '../tips';
import { esc, key, slotsFor, type BuildArgs, type Built } from './common';
import { pieOption, slicesOf, type Slice } from './pieparts';

export { DONUT_HOLE, INSIDE_AT, LIST_BELOW, LIST_TILE, centre, hiddenLabels, labelInside, shareText, type Slice } from './pieparts';

export interface PieArgs extends BuildArgs {
  /** The slice under the pointer, for a donut's centre. */
  hovered?: number | null;
  /** Labels under the chart instead of on it (a phone). */
  listed?: boolean;
}

export function buildPie(a: PieArgs): Built & { slices: Slice[] } {
  const { res, tile, colors } = a;
  const dtype = res.columns[0]?.dtype ?? 'text';
  const fmt = tile.formats?.[0] ?? null;
  const format = (v: number) => cell(v, 'number', null, false, fmt);
  const short = (v: number) => cell(v, 'number', null, true, fmt);
  const slices = slicesOf(res.rows, (v) => (v === OTHER_KEY ? 'Other' : cell(v, dtype, tile.dims[0]?.grain ?? null)), (keys) => slotsFor(tile, keys));
  const option = pieOption({
    slices,
    colors,
    donut: tile.type === 'donut',
    listed: !!a.listed,
    picked: a.selected?.length ? new Set(a.selected.map((v) => key(v))) : null,
    format,
    short,
    measure: tile.measures[0] ? measureLabel(tile.measures[0], a.tables ?? []) : 'Rows',
    hovered: a.hovered ?? null,
    loc: getLocale(),
    esc,
    size: Math.min(a.width, a.height),
    // More in the tooltip (v5 build 10).
    tipFor: res.tips
      ? (raw, color) =>
          tipHtml(
            res.tips?.[tipKey([raw])],
            (tile.tooltip?.extra ?? []).map((m) => measureLabel(m, a.tables ?? [])),
            (tile.tooltip?.extra ?? []).map((m) => numberFormatter(formatOf({ measures: [m] }, 0)).tip),
            tile.tooltip?.trend ? `Last 12 ${tile.tooltip.trend.grain === 'week' ? 'weeks' : tile.tooltip.trend.grain === 'quarter' ? 'quarters' : tile.tooltip.trend.grain === 'day' ? 'days' : 'months'}` : '',
            color,
            colors.text2,
            esc,
          )
      : undefined,
  });
  return {
    option,
    slices,
    categories: slices.map((s) => s.raw),
    pick: { by: 'item', value: (_s, i) => slices[i]?.raw },
  };
}
