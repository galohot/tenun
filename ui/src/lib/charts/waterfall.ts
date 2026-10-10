// Waterfall: bars that step from one level to the next. By category the steps start at 0 and end
// in a "Total" bar. By date the first period is the start, each next bar is the change from the
// period before, and the last bar is the "End" level. The step maths is in `math.ts`.

import { isPart, type Cell } from '../api';
import { getLocale, numberFormatter } from '../format';
import { axisCommon, esc, key, makeLabel, tooltipBase, type BuildArgs, type Built } from './common';
import { labelledSteps, waterfallSteps, type Step } from './math';

// How the bars read: ink for levels, the good colour for a step that helps and the bad colour for a
// step that hurts. Which way is good follows `tile.better`, like a number tile.
const INK_OPACITY = 0.7;
const PARTIAL_FADE = 0.6;

type Api = {
  value: (i: number) => number;
  coord: (p: number[]) => number[];
  size: (p: number[]) => number[];
  style: (o?: object) => object;
};

export function buildWaterfall({ res, tile, colors: c, selected, width }: BuildArgs): Built {
  const nd = res.columns.filter((x) => x.role === 'dimension').length;
  const loc = getLocale();
  const fmt = numberFormatter(tile.formats?.[0], loc);
  const label = makeLabel(res, tile);
  const isSel = (v: Cell | undefined) => !selected || v === undefined || selected.some((s) => key(s) === key(v));
  const grain = tile.dims[0]?.grain;
  const byDate = res.columns[0]?.dtype === 'date' || res.columns[0]?.dtype === 'datetime' || isPart(grain);

  const values = res.rows.map((r) => (typeof r[nd] === 'number' ? (r[nd] as number) : null));
  const steps = waterfallSteps(values, byDate);
  const cats: (Cell | undefined)[] = steps.map((s) => (s.row === null ? undefined : res.rows[s.row][0]));
  const names = steps.map((s, i) => (s.row === null ? (byDate ? 'End' : 'Total') : label(cats[i] as Cell)));
  const showNumber = labelledSteps(steps);
  const lastRow = res.rows.length - 1;
  const partial = res.partial !== undefined && res.rows.length > 0 && key(res.rows[lastRow][0]) === res.partial.key;

  const better = tile.better ?? 'up';
  const good = better === 'up' ? 'up' : 'down';
  const colorOf = (s: Step) => (s.kind === 'up' || s.kind === 'down' ? (s.kind === good ? c.good : c.bad) : c.text);
  // The unfinished last period is drawn fainter, with the end bar that repeats it.
  const opacityOf = (s: Step) =>
    (s.kind === 'up' || s.kind === 'down' ? 1 : INK_OPACITY) *
    (isSel(cats[s.index]) ? 1 : 0.28) *
    (partial && (s.row === lastRow || s.kind === 'end') ? PARTIAL_FADE : 1);

  const text = (s: Step) =>
    s.kind === 'up' ? `+${fmt.short(s.value)}` : fmt.short(s.value); // a down step already starts with the minus sign
  const barWidth = (api: Api) => Math.min(api.size([1, 0])[0] * 0.62, 44);

  /** One series for each kind of bar, so the legend can name them. */
  const series = (name: string, kinds: Step['kind'][]) => {
    const own = steps.filter((s) => kinds.includes(s.kind));
    return {
      name,
      own,
      option: {
        type: 'custom',
        name,
        encode: { x: 0, y: [1, 2] },
        data: own.map((s) => ({ value: [s.index, s.from, s.to], itemStyle: { opacity: opacityOf(s) } })),
        tooltip: {
          formatter: (p: { dataIndex: number }) => {
            const s = own[p.dataIndex];
            const head = `<b>${esc(names[s.index])}</b><br/>`;
            if (s.kind === 'up' || s.kind === 'down') {
              return `${head}${s.kind === 'up' ? 'Up' : 'Down'} ${esc(fmt.tip(Math.abs(s.value)))}<br/>From ${esc(fmt.tip(s.from))} to ${esc(fmt.tip(s.to))}`;
            }
            return `${head}${s.kind === 'flat' ? 'No change' : esc(fmt.tip(s.value))}`;
          },
        },
        renderItem: (params: { dataIndex: number }, api: Api) => {
          const s = own[params.dataIndex];
          const hi = api.coord([s.index, Math.max(s.from, s.to)]);
          const lo = api.coord([s.index, Math.min(s.from, s.to)]);
          const w = barWidth(api);
          const children: object[] = [
            {
              type: 'rect',
              shape: { x: hi[0] - w / 2, y: hi[1], width: w, height: Math.max(lo[1] - hi[1], 1), r: 2 },
              style: api.style({ fill: colorOf(s), stroke: c.surface, lineWidth: 1 }),
            },
          ];
          if (showNumber.has(s.index)) {
            // Above the bar, or below it when the whole bar sits under zero.
            const below = Math.max(s.from, s.to) <= 0;
            children.push({
              type: 'text',
              style: {
                text: text(s),
                x: hi[0],
                y: below ? lo[1] + 4 : hi[1] - 4,
                textAlign: 'center',
                textVerticalAlign: below ? 'top' : 'bottom',
                fill: c.text2,
                fontSize: 11,
              },
            });
          }
          return { type: 'group', children };
        },
      },
    };
  };

  const up = series('Increase', ['up']);
  const down = series('Decrease', ['down']);
  const ink = series(byDate ? 'Start and end' : 'Total', ['start', 'end', 'total', 'flat']);
  const present = [
    { s: ink, color: c.text, opacity: INK_OPACITY },
    { s: up, color: good === 'up' ? c.good : c.bad },
    { s: down, color: good === 'down' ? c.good : c.bad },
  ].filter((x) => x.s.own.length);

  // A hairline from the end of each bar to the start of the next.
  const connectors = {
    type: 'custom',
    silent: true,
    z: 1,
    encode: { x: 0, y: 1 },
    data: steps.slice(0, -1).map((s) => [s.index, s.to]),
    renderItem: (_: unknown, api: Api) => {
      const [i, level] = [api.value(0), api.value(1)];
      const [a, b] = [api.coord([i, level]), api.coord([i + 1, level])];
      const w = barWidth(api);
      return {
        type: 'line',
        shape: { x1: a[0] + w / 2, y1: a[1], x2: b[0] - w / 2, y2: a[1] },
        style: { stroke: c.text2, lineWidth: 1, opacity: 0.6 },
      };
    },
  };

  const longest = Math.max(0, ...names.map((x) => x.length));
  const option = {
    animationDuration: 350,
    grid: { left: 4, right: 24, top: present.length > 1 ? 34 : 14, bottom: 4, containLabel: true },
    legend:
      present.length > 1
        ? {
            top: 0,
            left: 0,
            icon: 'roundRect',
            itemWidth: 10,
            itemHeight: 10,
            textStyle: { color: c.text, fontSize: 12 },
            data: present.map((x) => ({ name: x.s.name, itemStyle: { color: x.color, opacity: x.opacity ?? 1 } })),
          }
        : undefined,
    tooltip: { ...tooltipBase(c), trigger: 'item' },
    xAxis: {
      type: 'category' as const,
      data: names,
      ...axisCommon(c),
      splitLine: { show: false },
      // Few bars: show every name, cut to the width of its bar. Many bars: let overlapping names drop out.
      axisLabel: {
        ...axisCommon(c).axisLabel,
        overflow: 'truncate' as const,
        rotate: !byDate && longest > 10 && names.length > 6 ? 30 : 0,
        ...(names.length <= 8 ? { interval: 0, width: Math.max(40, (width - 70) / names.length - 8) } : {}),
      },
      boundaryGap: true,
    },
    yAxis: {
      type: 'value' as const,
      ...axisCommon(c),
      splitNumber: 4,
      axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => fmt.short(v) },
    },
    series: [connectors, ...present.map((x) => x.s.option)],
  };
  return { option, categories: cats };
}
