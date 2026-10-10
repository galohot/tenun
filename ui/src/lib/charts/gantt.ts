// Gantt chart (Advanced): one bar for each row from its start to its end, along a time axis, with a
// hairline at today when today is in range. The engine sends `[label, start, end]` ordered by start;
// this only draws them.

import type { Cell } from '../api';
import { cell, dayMonth, getLocale } from '../format';
import { axisCommon, esc, key, makeLabel, tooltipBase, type BuildArgs, type Built } from './common';
import { barOf, dayStart, spanOf, todayIn, type Bar } from './timeline';

type Api = {
  value: (i: number) => number;
  coord: (p: number[]) => number[];
  size: (p: number[]) => number[];
};

export const GANTT_ROW_PX = 26;

export function buildGantt({ res, tile, colors: c, selected, width }: BuildArgs): Built {
  const label = makeLabel(res, tile);
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  const groups = res.rows.map((r) => r[0]);
  const names = groups.map(label);
  const bars: (Bar | null)[] = res.rows.map((r) => barOf(r[1], r[2]));
  const span = spanOf(bars);
  const now = dayStart(Date.now());
  const loc = getLocale();
  const [startName, endName] = ['Start', 'End'];
  const dtype = res.columns[1]?.dtype ?? 'date';
  const dayText = (ms: number) => cell(new Date(ms).toISOString().slice(0, 10), dtype === 'datetime' ? 'date' : dtype, 'day');

  const bar = {
    type: 'custom',
    name: 'Rows',
    z: 3,
    encode: { x: [1, 2], y: 0 },
    renderItem: (_: unknown, api: Api) => {
      const i = api.value(0);
      const b = bars[i];
      if (!b) return { type: 'group', children: [] };
      const [p, q] = [api.coord([b.start, i]), api.coord([b.end, i])];
      const thick = Math.min(api.size([0, 1])[1] * 0.55, 18);
      return {
        type: 'rect',
        shape: { x: p[0], y: p[1] - thick / 2, width: Math.max(2, q[0] - p[0]), height: thick, r: 3 },
        style: { fill: c.series[0], opacity: isSel(groups[i]) ? 1 : 0.28 },
      };
    },
    data: bars.map((b, i) => ({ value: [i, b?.start ?? 0, b?.end ?? 0] })),
  };
  // Today: a hairline across the whole plot.
  const today = {
    type: 'custom',
    name: 'Today',
    z: 5,
    silent: true,
    tooltip: { show: false },
    renderItem: (params: { coordSys: { x: number; y: number; width: number; height: number } }, api: Api) => {
      const x = api.coord([now, 0])[0];
      const s = params.coordSys;
      return {
        type: 'group',
        children: [
          { type: 'line', shape: { x1: x, y1: s.y, x2: x, y2: s.y + s.height }, style: { stroke: c.text, lineWidth: 1, opacity: 0.7 } },
          { type: 'text', x: x + 4, y: s.y + 2, style: { text: 'Today', fill: c.text2, fontSize: 11, textVerticalAlign: 'top' } },
        ],
      };
    },
    data: todayIn(span, now) ? [{ value: [0, now, now] }] : [],
  };

  const option = {
    animationDuration: 350,
    grid: { left: 4, right: 20, top: 8, bottom: 4, containLabel: true },
    tooltip: {
      ...tooltipBase(c),
      trigger: 'item',
      formatter: (p: { dataIndex: number; seriesName: string }) => {
        const b = bars[p.dataIndex];
        const r = res.rows[p.dataIndex];
        if (!b || !r) return '';
        // An end with no time of day is a whole day, so the bar runs a day past its end ms.
        const lastDay = typeof r[2] === 'string' && r[2].length === 10 ? b.end - 86_400_000 : b.end;
        return `<b>${esc(names[p.dataIndex])}</b><br/>${esc(startName)}: ${esc(dayText(b.start))}<br/>${esc(endName)}: ${esc(dayText(lastDay))}<br/>${b.days} ${b.days === 1 ? 'day' : 'days'}`;
      },
    },
    xAxis: {
      type: 'time' as const,
      ...axisCommon(c),
      min: span?.min,
      max: span?.max,
      axisLabel: {
        ...axisCommon(c).axisLabel,
        // Month and day by the locale; the year too when the bars run over a year end.
        formatter: (v: number) => {
          const d = new Date(v);
          const wide = span ? span.max - span.min > 300 * 86_400_000 : false;
          return wide ? `${dayMonth(d, loc)} ${String(d.getUTCFullYear()).slice(2)}` : dayMonth(d, loc);
        },
      },
    },
    yAxis: {
      type: 'category' as const,
      data: names,
      inverse: true,
      ...axisCommon(c),
      splitLine: { show: true, lineStyle: { color: c.grid, width: 1 } },
      axisLabel: { ...axisCommon(c).axisLabel, width: Math.min(150, Math.max(60, width * 0.25)), overflow: 'truncate' as const, interval: 0 },
      axisPointer: { type: 'shadow' as const },
      boundaryGap: true,
    },
    series: [bar, today],
  };
  const cut = res.rows.length >= 100 ? 'Showing the first 100 rows.' : '';
  const missing = bars.filter((b) => b === null).length;
  const bad = missing ? `${missing} ${missing === 1 ? 'row has' : 'rows have'} no start or an end before its start, so ${missing === 1 ? 'it has' : 'they have'} no bar.` : '';
  return { option, categories: groups, horizontal: true, rows: res.rows.length, note: [cut, bad].filter(Boolean).join(' ') || null };
}
