// Box plot (Advanced): one horizontal box for each group. The engine computes the five numbers, the
// whiskers (the furthest values within 1.5 × the box length) and the outlier dots; this only draws them.
// Rows are `[group, min, q1, median, q3, max, lower whisker, upper whisker, count]`.

import type { Cell } from '../api';
import { getLocale, numberFormatter } from '../format';
import { axisCommon, esc, key, makeLabel, tooltipBase, type BuildArgs, type Built } from './common';
import { farFromZero } from './math';

type Api = {
  value: (i: number) => number;
  coord: (p: number[]) => number[];
  size: (p: number[]) => number[];
};

/** A #rrggbb colour with an opacity, for the tinted inside of a box. */
function tint(hex: string, alpha: number): string {
  const h = hex.trim().replace('#', '');
  if (h.length !== 6) return hex;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

export function buildBox({ res, tile, colors: c, selected, width }: BuildArgs): Built {
  const loc = getLocale();
  const fmt = numberFormatter(tile.formats?.[0], loc);
  const label = makeLabel(res, tile);
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  const num = (v: Cell | undefined) => (typeof v === 'number' && Number.isFinite(v) ? v : 0);
  const groups = res.rows.map((r) => r[0]);
  const names = groups.map(label);
  const color = c.series[0];
  const at = new Map(groups.map((g, i) => [key(g), i]));

  const dots = (res.outliers ?? []).filter((d) => at.has(key(d[0])) && typeof d[1] === 'number');
  const everything = [...res.rows.flatMap((r) => [num(r[1]), num(r[5])]), ...dots.map((d) => d[1] as number)];
  const far = farFromZero(everything);

  const box = {
    type: 'custom',
    name: 'Spread',
    z: 3,
    // The data is [place, lower whisker, q1, median, q3, upper whisker]; the axis takes its range from them.
    encode: { x: [1, 2, 3, 4, 5], y: 0 },
    renderItem: (_: unknown, api: Api) => {
      const i = api.value(0);
      const [lo, q1, med, q3, hi] = [1, 2, 3, 4, 5].map((k) => api.coord([api.value(k), i]));
      const y = lo[1];
      const thick = Math.min(api.size([0, 1])[1] * 0.5, 28);
      const dim = isSel(groups[i]) ? 1 : 0.28;
      const ink = { stroke: c.text2, lineWidth: 1.5, opacity: dim };
      const line = (x1: number, y1: number, x2: number, y2: number, style: object) => ({ type: 'line', shape: { x1, y1, x2, y2 }, style });
      return {
        type: 'group',
        children: [
          line(lo[0], y, q1[0], y, ink),
          line(q3[0], y, hi[0], y, ink),
          line(lo[0], y - thick / 4, lo[0], y + thick / 4, ink),
          line(hi[0], y - thick / 4, hi[0], y + thick / 4, ink),
          {
            type: 'rect',
            shape: { x: q1[0], y: y - thick / 2, width: Math.max(1, q3[0] - q1[0]), height: thick, r: 3 },
            style: { fill: tint(color, 0.25), stroke: color, lineWidth: 1.5, opacity: dim },
          },
          line(med[0], y - thick / 2, med[0], y + thick / 2, { stroke: c.text, lineWidth: 2.5, opacity: dim }),
        ],
      };
    },
    data: res.rows.map((r, i) => ({ value: [i, num(r[6]), num(r[2]), num(r[3]), num(r[4]), num(r[7])] })),
  };
  const outliers = {
    type: 'scatter',
    name: 'Outliers',
    z: 4,
    symbol: 'circle',
    symbolSize: 7,
    itemStyle: { color, borderColor: c.surface, borderWidth: 1.5 },
    emphasis: { scale: 1.3 },
    data: dots.map((d) => ({ value: [d[1] as number, at.get(key(d[0]))!], group: d[0], itemStyle: { opacity: isSel(d[0]) ? 1 : 0.28 } })),
  };

  // The tooltip says the five numbers in words, then what half of the values do.
  const words = (r: Cell[], i: number) => {
    const shown = dots.filter((d) => key(d[0]) === key(r[0])).length;
    const row = (name: string, v: Cell | undefined) => `${esc(name)} <b>${esc(fmt.tip(num(v)))}</b>`;
    return (
      `<b>${esc(names[i])}</b> · ${esc(String(num(r[8])))} values<br/>` +
      [row('Lowest', r[1]), row('Lower quartile', r[2]), row('Median', r[3]), row('Upper quartile', r[4]), row('Highest', r[5])].join('<br/>') +
      `<br/>Half of the values lie between ${esc(fmt.tip(num(r[2])))} and ${esc(fmt.tip(num(r[4])))}.` +
      (shown ? `<br/>${shown} ${shown === 1 ? 'value is' : 'values are'} far from the rest.` : '')
    );
  };

  const option = {
    animationDuration: 350,
    grid: { left: 4, right: 20, top: 10, bottom: 4, containLabel: true },
    tooltip: {
      ...tooltipBase(c),
      trigger: 'item',
      formatter: (p: { seriesType: string; dataIndex: number; data: { group?: Cell; value: number[] } }) => {
        if (p.seriesType === 'scatter') {
          const g = p.data.group;
          return `<b>${esc(label(g as Cell))}</b><br/>${esc(fmt.tip(p.data.value[0]))}<br/>Far from the rest of this group`;
        }
        return words(res.rows[p.dataIndex], p.dataIndex);
      },
    },
    xAxis: {
      type: 'value' as const,
      ...axisCommon(c),
      scale: far,
      splitNumber: 5,
      axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => fmt.short(v) },
    },
    yAxis: {
      type: 'category' as const,
      data: names,
      inverse: true,
      ...axisCommon(c),
      splitLine: { show: false },
      axisLabel: { ...axisCommon(c).axisLabel, width: Math.min(140, Math.max(60, width * 0.25)), overflow: 'truncate' as const, interval: 0 },
      axisPointer: { type: 'shadow' as const },
      boundaryGap: true,
    },
    series: [box, ...(dots.length ? [outliers] : [])],
  };
  return { option, categories: groups, horizontal: true, note: far ? 'The axis does not start at zero.' : null };
}
