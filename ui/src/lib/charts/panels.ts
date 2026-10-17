// Separate panels (Advanced): one plot for each value, stacked, sharing the x axis. Values in
// different units each get an honest axis of their own, and a hover line runs through all the plots.
// Bars, lines and areas as the tile type says. At most four values.

import type { Cell } from '../api';
import { getLocale, measureLabel, numberFormatter, resultDim0 } from '../format';
import { effectiveShowAs } from '../showas';
import { axisCommon, esc, key, makeLabel, pivot, tooltipBase, type BuildArgs, type Built } from './common';
import { t } from '../i18n';

export const MAX_PANELS = 4;
const GAP = 12;
/** Room above each plot for its name. */
const NAME = 20;
/** Room under the last plot for the x axis labels. */
const AXIS = 24;

export function buildPanels({ res, tile, colors: c, selected, height, tables = [] }: BuildArgs): Built {
  const loc = getLocale();
  const label = makeLabel(res, tile);
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  const { categories, series: all } = pivot(res, tile, tables);
  const series = all.slice(0, MAX_PANELS);
  const n = series.length;
  const kind = resultDim0(res, tile);
  const partialKey = res.partial?.key;
  const fmts = series.map((_, i) => numberFormatter(tile.formats?.[i], loc, res.columns[res.columns.length - all.length + i]?.unit));
  const names = series.map((_, i) => (tile.measures[i] ? measureLabel(tile.measures[i], tables, effectiveShowAs(tile, i, kind, res.parts)) : series[i].name));

  // One plot height for all: what is left after the names, the gaps and the x axis.
  const plot = Math.max(34, Math.floor((height - 4 - n * NAME - (n - 1) * GAP - AXIS) / n));
  const top = (i: number) => 4 + i * (NAME + plot + GAP) + NAME;
  // Every plot has the same left edge, so the same category sits straight above itself.
  const widest = Math.max(
    ...series.map((s, i) => {
      const peak = Math.max(0, ...s.values.map((v) => Math.abs(v ?? 0)));
      return Math.max(fmts[i].short(peak).length, fmts[i].short(-peak).length * (s.values.some((v) => (v ?? 0) < 0) ? 1 : 0));
    }),
  );
  const left = Math.max(36, Math.ceil(widest * 6.3) + 12);

  const isBar = tile.type === 'bar';
  const catData = categories.map(label);
  const unfinished = partialKey !== undefined && categories.at(-1) === partialKey;

  const grids = series.map((_, i) => ({ left, right: 44, top: top(i), height: plot, containLabel: false }));
  const xAxes = series.map((_, i) => ({
    type: 'category' as const,
    gridIndex: i,
    data: catData,
    ...axisCommon(c),
    splitLine: { show: false },
    axisLine: { show: i === n - 1, lineStyle: { color: c.grid } },
    axisLabel: { ...axisCommon(c).axisLabel, show: i === n - 1 },
    boundaryGap: isBar,
    axisPointer: { show: true, type: 'line' as const, lineStyle: { color: c.text2, width: 1 }, label: { show: false } },
  }));
  const yAxes = series.map((_, i) => ({
    type: 'value' as const,
    gridIndex: i,
    ...axisCommon(c),
    splitNumber: 2,
    axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => fmts[i].short(v) },
    // The name of the value sits above its plot, at the left edge.
    name: names[i],
    nameLocation: 'end' as const,
    nameGap: 6,
    nameTextStyle: { color: c.text, fontSize: 12, fontWeight: 600, align: 'left' as const },
  }));

  const built = series.flatMap((s, i): object[] => {
    const color = c.series[(s.slot ?? i) % c.series.length];
    const data = s.values.map((v, ci) => ({
      value: v,
      itemStyle: { opacity: (isSel(categories[ci]) ? 1 : 0.28) * (categories[ci] === partialKey ? 0.45 : 1) },
    }));
    const common = { xAxisIndex: i, yAxisIndex: i, name: names[i], emphasis: { focus: 'none' as const } };
    if (isBar) {
      return [{ ...common, type: 'bar', barMaxWidth: 24, barCategoryGap: '30%', itemStyle: { color, borderRadius: [4, 4, 0, 0] }, data }];
    }
    const lineStyle = { width: 2, color, cap: 'round', join: 'round' };
    const itemStyle = { color, borderColor: c.surface, borderWidth: 2 };
    const endLabel = { show: categories.length > 2, color: c.text2, fontSize: 11, formatter: (p: { value: number }) => fmts[i].short(p.value) };
    const main = {
      ...common,
      type: 'line',
      smooth: false,
      showSymbol: categories.length <= 2,
      symbol: 'circle',
      symbolSize: 8,
      lineStyle,
      itemStyle,
      areaStyle: tile.type === 'area' ? { color, opacity: 0.1 } : undefined,
      connectNulls: false,
      data: unfinished && s.values.length > 1 ? [...s.values.slice(0, -1), null] : s.values,
      endLabel: unfinished ? { show: false } : endLabel,
    };
    if (!(unfinished && s.values.length > 1)) return [main];
    const k = s.values.length;
    return [
      main,
      {
        ...common,
        type: 'line',
        silent: true,
        showSymbol: false,
        lineStyle: { ...lineStyle, type: [4, 4] },
        itemStyle,
        tooltip: { show: false },
        data: s.values.map((v, ci) => (ci >= k - 2 ? v : null)),
        endLabel,
      },
    ];
  });

  const option = {
    animationDuration: 350,
    grid: grids,
    xAxis: xAxes,
    yAxis: yAxes,
    series: built,
    // One hover line through every plot.
    axisPointer: { link: [{ xAxisIndex: 'all' }] },
    tooltip: {
      ...tooltipBase(c),
      trigger: 'axis',
      axisPointer: { type: 'line', lineStyle: { color: c.text2, width: 1 } },
      // Every value at this category, each in its own format.
      formatter: (ps: { dataIndex: number }[]) => {
        const at = ps[0]?.dataIndex ?? 0;
        const dot = (i: number) =>
          `<span style="display:inline-block;margin-right:4px;border-radius:10px;width:10px;height:10px;background-color:${c.series[(series[i].slot ?? i) % c.series.length]}"></span>`;
        return (
          `${esc(catData[at] ?? '')}<br/>` +
          series
            .map((s, i) => `${dot(i)} ${esc(names[i])} <b>${typeof s.values[at] === 'number' ? esc(fmts[i].tip(s.values[at] as number)) : '–'}</b>`)
            .join('<br/>')
        );
      },
    },
  };
  return {
    option,
    categories,
    grids: n,
    horizontal: false,
    note: all.length > MAX_PANELS ? t('Panels show the first {n} values.', { n: MAX_PANELS }) : null,
  };
}
