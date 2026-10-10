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
type Coord = { x: number; y: number; width: number; height: number };

const LABEL_H = 13; // 11 px text
const TURNED_W = 10; // the height of the letters when the text is turned on its side
/** About how wide a label is at 11 px: digits, then the narrow marks and the rest. */
const labelWidth = (t: string) => 2 + [...t].reduce((w, ch) => w + (/[0-9]/.test(ch) ? 6.3 : /[.,\s]/.test(ch) ? 3.2 : 7), 0);

interface Box {
  x0: number;
  x1: number;
  y0: number;
  y1: number;
}
const hits = (a: Box, b: Box, pad = 1) => a.x0 < b.x1 + pad && a.x1 > b.x0 - pad && a.y0 < b.y1 + pad && a.y1 > b.y0 - pad;

/** Where one number goes: its centre, and how the text sits and reads. */
interface Place {
  x: number;
  y: number;
  align: 'center';
  valign: 'top' | 'bottom' | 'middle';
  /** Inside a bar: the bar's own colour decides the text colour. */
  inside: boolean;
  rotate: boolean;
}

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
  // Auto: the start, end and total bars and the biggest steps. All: every bar, where its number fits
  // (the layout leaves out a number with no free place). None: no numbers.
  const showNumber =
    tile.labels === 'none' ? new Set<number>() : tile.labels === 'all' ? new Set(steps.map((s) => s.index)) : labelledSteps(steps);
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

  const isInk = (s: Step) => s.kind === 'start' || s.kind === 'end' || s.kind === 'total';
  const text = (s: Step) =>
    s.kind === 'up' ? `+${fmt.short(s.value)}` : fmt.short(s.value); // a down step already starts with the minus sign
  const barWidth = (api: Api) => Math.min(api.size([1, 0])[0] * 0.62, 44);

  // Where the numbers go. No number may touch a bar or another number, so every bar is measured
  // first. The start, end and total bars go first: above the bar, else inside it (turned on its side
  // when the bar is narrow), else nowhere. Then the biggest steps: above an up step and below a down
  // step, else the other side. A number with no free place is left out. The layout is computed once
  // for a size of the plot and reused by every bar.
  let layout: { key: string; places: Map<number, Place> } | null = null;
  const placeLabels = (api: Api, cs: Coord): Map<number, Place> => {
    const key = `${cs.x}|${cs.y}|${cs.width}|${cs.height}`;
    if (layout?.key === key) return layout.places;
    const w = barWidth(api);
    const bars = steps.map((s) => {
      const hi = api.coord([s.index, Math.max(s.from, s.to)]);
      const lo = api.coord([s.index, Math.min(s.from, s.to)]);
      return { x0: hi[0] - w / 2, x1: hi[0] + w / 2, y0: hi[1], y1: Math.max(lo[1], hi[1] + 1) };
    });
    const [top, bottom] = [cs.y - 8, cs.y + cs.height];
    const taken: Box[] = [];
    const places = new Map<number, Place>();
    const order = [...showNumber].sort((a, b) => Number(isInk(steps[b])) - Number(isInk(steps[a])));
    for (const i of order) {
      const s = steps[i];
      const bar = bars[i];
      const cx = (bar.x0 + bar.x1) / 2;
      const [tw, bw, bh] = [labelWidth(text(s)), bar.x1 - bar.x0, bar.y1 - bar.y0];
      // A number above or below a bar may slide sideways, so it never reaches over the axis numbers.
      const ax = Math.max(cs.x + tw / 2, Math.min(cx, cs.x + cs.width - tw / 2));
      const above = { box: { x0: ax - tw / 2, x1: ax + tw / 2, y0: bar.y0 - 4 - LABEL_H, y1: bar.y0 - 4 }, p: { x: ax, y: bar.y0 - 4, valign: 'bottom' as const, inside: false, rotate: false } };
      const below = { box: { x0: ax - tw / 2, x1: ax + tw / 2, y0: bar.y1 + 4, y1: bar.y1 + 4 + LABEL_H }, p: { x: ax, y: bar.y1 + 4, valign: 'top' as const, inside: false, rotate: false } };
      const within = { box: { x0: cx - tw / 2, x1: cx + tw / 2, y0: (bar.y0 + bar.y1) / 2 - LABEL_H / 2, y1: (bar.y0 + bar.y1) / 2 + LABEL_H / 2 }, p: { x: cx, y: (bar.y0 + bar.y1) / 2, valign: 'middle' as const, inside: true, rotate: false } };
      const turned = { box: { x0: cx - TURNED_W / 2, x1: cx + TURNED_W / 2, y0: (bar.y0 + bar.y1) / 2 - tw / 2, y1: (bar.y0 + bar.y1) / 2 + tw / 2 }, p: { x: cx, y: (bar.y0 + bar.y1) / 2, valign: 'middle' as const, inside: true, rotate: true } };
      // A bar wholly under zero hangs from the axis, so its number goes under it first.
      const under = Math.max(s.from, s.to) <= 0;
      const candidates = isInk(s) ? (under ? [below, within, turned] : [above, within, turned]) : under ? [below, above] : [above, below];
      const fits = (c: (typeof candidates)[number]) => {
        const inBar = c.p.inside;
        if (inBar && (c.box.x1 - c.box.x0 + 2 > bw || c.box.y1 - c.box.y0 + 4 > bh)) return false;
        if (c.box.y0 < top || c.box.y1 > bottom) return false;
        // Inside its own bar is fine; every other bar and every placed number is not.
        if (bars.some((b, j) => j !== i && hits(c.box, b))) return false;
        if (!inBar && hits(c.box, bar, 0)) return false;
        return !taken.some((t) => hits(c.box, t, 2));
      };
      const pick = candidates.find(fits);
      if (!pick) continue;
      taken.push(pick.box);
      places.set(i, { ...pick.p, align: 'center' });
    }
    layout = { key, places };
    return places;
  };

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
        renderItem: (params: { dataIndex: number; coordSys: Coord }, api: Api) => {
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
          const at = placeLabels(api, params.coordSys).get(s.index);
          if (at) {
            // Inside a bar the text takes the colour that reads on it; the faded bar of an unfinished period is light.
            const faded = opacityOf(s) < 0.5;
            children.push({
              type: 'text',
              x: at.x,
              y: at.y,
              rotation: at.rotate ? -Math.PI / 2 : 0,
              style: {
                text: text(s),
                x: 0,
                y: 0,
                textAlign: at.align,
                textVerticalAlign: at.valign,
                fill: at.inside ? (faded ? c.text : c.surface) : c.text2,
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
