// Histogram: how many rows fall in each slice of a number column. The engine picks the bins
// (`shape::histogram`); this draws them as touching bars with a 1 px surface line between them,
// and puts the x labels on the bin edges.

import type { Cell } from '../api';
import { getLocale, numberFormatter } from '../format';
import { axisCommon, esc, key, tooltipBase, type BuildArgs, type Built } from './common';

export function buildHistogram({ res, tile, colors: c, selected, width }: BuildArgs): Built {
  const loc = getLocale();
  const fmt = numberFormatter(tile.formats?.[0], loc);
  const count = numberFormatter(null, loc);
  const bins = res.bins;
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  if (!bins || !res.rows.length) return { option: { series: [] }, categories: [] };

  const { start, width: step } = bins;
  const n = res.rows.length;
  const edge = (i: number) => Math.round((start + i * step) * 1e10) / 1e10;
  // Each bar is [from, to, rows]. A custom series lets the bars touch and sit on number edges.
  const data = res.rows.map((r, i) => ({
    value: [edge(i), edge(i + 1), Number(r[1])],
    itemStyle: { opacity: isSel(r[0]) ? 1 : 0.28 },
  }));
  // About one label for every 70 px, always on a bin edge.
  const stride = Math.max(1, Math.ceil(n / Math.max(2, Math.floor(width / 70))));
  const [lo, hi] = [edge(0), edge(n)];

  const option = {
    animationDuration: 350,
    grid: { left: 4, right: 16, top: 14, bottom: 4, containLabel: true },
    tooltip: {
      ...tooltipBase(c),
      trigger: 'item',
      formatter: (p: { value: number[] }) =>
        `<b>${esc(fmt.tip(p.value[0]))} to ${esc(fmt.tip(p.value[1]))}</b><br/>${esc(count.tip(p.value[2]))} ${p.value[2] === 1 ? 'row' : 'rows'}`,
    },
    xAxis: {
      type: 'value' as const,
      ...axisCommon(c),
      splitLine: { show: false },
      min: lo,
      max: hi,
      interval: step * stride,
      axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => fmt.short(Math.round(v * 1e10) / 1e10) },
    },
    yAxis: { type: 'value' as const, ...axisCommon(c), min: 0, splitNumber: 4, axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => count.short(v) } },
    series: [
      {
        type: 'custom',
        name: 'Rows',
        encode: { x: [0, 1], y: 2 },
        data,
        // The bar runs from its left edge to its right edge, so there is no gap between bars.
        renderItem: (
          _: unknown,
          api: { value: (i: number) => number; coord: (p: number[]) => number[]; style: (o?: object) => object },
        ) => {
          const a = api.coord([api.value(0), api.value(2)]);
          const b = api.coord([api.value(1), 0]);
          return {
            type: 'rect',
            shape: { x: a[0], y: a[1], width: b[0] - a[0], height: b[1] - a[1] },
            style: api.style({ fill: c.series[0], stroke: c.surface, lineWidth: 1 }),
          };
        },
      },
    ],
  };
  // A click finds the bin under the pointer.
  const pick = (x: number): Cell | undefined => {
    const i = Math.floor((x - start) / step + 1e-9);
    return i >= 0 && i < n ? res.rows[i][0] : undefined;
  };
  return { option, categories: [], pick: { by: 'x', value: pick } };
}
