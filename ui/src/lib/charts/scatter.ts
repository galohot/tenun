// Scatter: one dot for each row of the first group, across and up by two values, coloured by an
// optional second group. 8 px dots with a 2 px surface ring, so dots that touch stay apart.

import type { Cell } from '../api';
import { cell, getLocale, measureLabel, numberFormatter } from '../format';
import { bubbleSamples, bubbleSize } from './bubble';
import { axisCommon, esc, key, OTHER, slotsFor, tooltipBase, type BuildArgs, type Built } from './common';
import { axisSpan, levelMarkLine, resolveLevels, wantsTrend, type Level } from './guides';
import { fitAt, trendFit, trendSentence } from './lines';
import { farFromZero } from './math';

/** The query asks for this many dots at most (`snapshot::tile_query`). */
const DOT_LIMIT = 2000;
const MAX_GROUPS = 8;

export function buildScatter({ res, tile, colors: c, selected, width, height, tables = [] }: BuildArgs): Built {
  const nd = res.columns.filter((x) => x.role === 'dimension').length;
  const [xi, yi] = [nd, nd + 1];
  const hasColour = nd > 1 && tile.dims.length > 1;
  const loc = getLocale();
  const fx = numberFormatter(tile.formats?.[0], loc);
  const fy = numberFormatter(tile.formats?.[1], loc);
  const xName = tile.measures[0] ? measureLabel(tile.measures[0], tables) : (res.columns[xi]?.name ?? '');
  const yName = tile.measures[1] ? measureLabel(tile.measures[1], tables) : (res.columns[yi]?.name ?? '');
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));

  // A third value sets the size of the dot (Advanced): its area follows the value.
  const si = nd + 2;
  const sized = !!tile.measures[2] && res.columns.length > si;
  const fs = numberFormatter(tile.formats?.[2], loc);
  const sizeName = tile.measures[2] ? measureLabel(tile.measures[2], tables) : '';
  const sizeOf = (r: Cell[]) => (typeof r[si] === 'number' ? (r[si] as number) : null);

  // Dots with a missing value cannot be placed. With sizes, the big dots are drawn first so small ones stay in view.
  const rows = res.rows.filter((r) => typeof r[xi] === 'number' && typeof r[yi] === 'number');
  const sizeMax = sized ? Math.max(0, ...rows.map((r) => sizeOf(r) ?? 0)) : 0;
  if (sized) rows.sort((a, b) => (sizeOf(b) ?? -1) - (sizeOf(a) ?? -1));

  // Colour groups: the seven most common stay, the rest become one grey "Other".
  // The colour follows the group, not its place in the list: the tile remembers the slot each group
  // had, so a filter that removes a group leaves the colours of the others as they were.
  const counts = new Map<string, { raw: Cell; n: number }>();
  if (hasColour) {
    for (const r of rows) {
      const k = key(r[1]);
      const g = counts.get(k) ?? { raw: r[1], n: 0 };
      g.n++;
      counts.set(k, g);
    }
  }
  const ranked = [...counts.entries()].sort((a, b) => b[1].n - a[1].n);
  const kept = ranked.length > MAX_GROUPS ? ranked.slice(0, MAX_GROUPS - 1) : ranked;
  const colourLabel = (raw: Cell) => cell(raw, res.columns[1]?.dtype ?? 'text', tile.dims[1]?.grain);
  kept.sort((a, b) => colourLabel(a[1].raw).localeCompare(colourLabel(b[1].raw), undefined, { numeric: true }));
  const groupOf = new Map(kept.map(([k], i) => [k, i]));
  const slots = slotsFor(tile, kept.map(([k]) => k));
  const groups = hasColour
    ? kept.map(([, g], i) => ({ name: colourLabel(g.raw), color: c.series[slots[i] % c.series.length] }))
    : [{ name: '', color: c.series[0] }];
  const hasOther = ranked.length > kept.length;
  if (hasOther) groups.push({ name: OTHER, color: c.text2 });

  type Dot = { value: [number, number]; name: string; group: string; size: number | null; symbolSize?: number; itemStyle: { opacity: number } };
  const data: Dot[][] = groups.map(() => []);
  const dots: Cell[][] = groups.map(() => []);
  for (const r of rows) {
    const gi = hasColour ? (groupOf.get(key(r[1])) ?? (hasOther ? groups.length - 1 : 0)) : 0;
    data[gi].push({
      value: [r[xi] as number, r[yi] as number],
      name: cell(r[0], res.columns[0].dtype, tile.dims[0]?.grain),
      group: groups[gi].name,
      size: sized ? sizeOf(r) : null,
      ...(sized ? { symbolSize: bubbleSize(sizeOf(r), sizeMax) } : {}),
      itemStyle: { opacity: isSel(r[0]) ? (sized ? 0.85 : 1) : 0.28 },
    });
    dots[gi].push(r[0]);
  }
  const multi = groups.length > 1;

  const xs = rows.map((r) => r[xi] as number);
  const ys = rows.map((r) => r[yi] as number);
  const [xFar, yFar] = [farFromZero(xs), farFromZero(ys)];
  const axis = (name: string, far: boolean, fmt: ReturnType<typeof numberFormatter>, x: boolean) => ({
    type: 'value' as const,
    ...axisCommon(c),
    // Bars and lines start at zero. A scatter does too, unless every dot is far from it.
    scale: far,
    name,
    nameLocation: x ? ('middle' as const) : ('end' as const),
    nameGap: x ? 26 : 8,
    nameTextStyle: { color: c.text2, fontSize: 11, align: x ? ('center' as const) : ('left' as const) },
    axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => fmt.short(v) },
    splitNumber: 4,
  });

  // Reference lines: a level up the side (a line across) or a value along the bottom (an upright
  // line), each read from the dots on screen. A median with no axis draws both, as it always did.
  const levels = resolveLevels(tile.lines, true, (axis) => (axis === 'x' ? xs : ys));
  const levelsOf = (axis: 'x' | 'y'): Level[] => levels.filter((l) => l.axis === axis);
  const shortOf = (axis: 'x' | 'y', n: number) => (axis === 'x' ? fx : fy).short(n);
  const top = multi ? 56 : 30;
  const [plotW, plotH] = [width - 60, height - top - 40];
  const across = levelMarkLine(levelsOf('x'), 'xAxis', axisSpan(xs, levelsOf('x').map((l) => l.value), !xFar, plotW), c, shortOf);
  const up = levelMarkLine(levelsOf('y'), 'yAxis', axisSpan(ys, levelsOf('y').map((l) => l.value), !yFar, plotH), c, shortOf);
  const markLine = across || up ? { ...(up ?? across!), data: [...(up?.data ?? []), ...(across?.data ?? [])] } : null;
  // The trend line is a straight line through the dots (least squares on across and up).
  const fit = wantsTrend(tile.lines) ? trendFit(ys, xs) : null;
  const trendTip = fit ? trendSentence(fit, fy.short(Math.abs(fit.slope)), `for each 1 more of ${xName}`) : '';
  const [x0, x1] = [Math.min(...xs), Math.max(...xs)];
  // Unseen series carry the lines. Their points keep both axes wide enough for every line.
  const reach: number[][] = [
    ...levelsOf('y').map((l) => [xs[0] ?? 0, l.value]),
    ...levelsOf('x').map((l) => [l.value, ys[0] ?? 0]),
  ];
  const carrier = (extra: object) => ({ type: 'scatter', silent: true, symbolSize: 0, itemStyle: { opacity: 0 }, tooltip: { show: false }, ...extra });
  const guide = [
    ...(markLine ? [carrier({ data: reach, markLine })] : []),
    ...(fit
      ? [
          carrier({
            data: [],
            silent: false,
            markLine: {
              symbol: 'none',
              animation: false,
              lineStyle: { color: c.text2, width: 1.5, type: [5, 4] },
              emphasis: { lineStyle: { width: 2 } },
              tooltip: { trigger: 'item', formatter: () => esc(trendTip) },
              data: [
                [
                  { coord: [x0, fitAt(fit, x0)], name: 'Trend' },
                  { coord: [x1, fitAt(fit, x1)], name: 'Trend', label: { show: true, formatter: 'Trend', position: 'end', color: c.text2, fontSize: 11, backgroundColor: c.surface, padding: [1, 4], borderRadius: 3 } },
                ],
              ],
            },
          }),
        ]
      : []),
  ];

  const option = {
    animationDuration: 350,
    grid: { left: 4, right: 16, top, bottom: 22, containLabel: true },
    legend: multi
      ? {
          type: 'scroll',
          top: 0,
          left: 0,
          icon: 'circle',
          itemWidth: 10,
          itemHeight: 10,
          textStyle: { color: c.text, fontSize: 12 },
          pageTextStyle: { color: c.text2 },
        }
      : undefined,
    tooltip: {
      ...tooltipBase(c),
      trigger: 'item',
      formatter: (p: { data: Dot }) =>
        `<b>${esc(p.data.name)}</b>${p.data.group ? `<br/>${esc(p.data.group)}` : ''}` +
        `<br/>${esc(xName)}: ${esc(fx.tip(p.data.value[0]))}<br/>${esc(yName)}: ${esc(fy.tip(p.data.value[1]))}` +
        (sized ? `<br/>${esc(sizeName)}: ${p.data.size === null ? '–' : esc(fs.tip(p.data.size))}` : ''),
    },
    xAxis: axis(xName, xFar, fx, true),
    yAxis: axis(yName, yFar, fy, false),
    series: [
      ...groups.map((g, i) => ({
        type: 'scatter',
        name: g.name,
        symbol: 'circle',
        symbolSize: 10,
        // ECharts draws scatter dots at 80 % by default; the palette is meant to show at full strength.
        itemStyle: { color: g.color, borderColor: c.surface, borderWidth: 2, opacity: 1 },
        // "All" writes the height of every dot, where there is room for it.
        label: { show: tile.labels === 'all', position: 'top', color: c.text2, fontSize: 11, formatter: (p: { value: number[] }) => fy.short(p.value[1]) },
        labelLayout: { hideOverlap: true },
        emphasis: { focus: 'none', scale: 1.2 },
        data: data[i],
      })),
      ...guide,
    ],
  };

  const notes = [];
  if (xFar || yFar) notes.push('Axes do not start at zero.');
  if (res.rows.length >= DOT_LIMIT) notes.push(`Only the first ${DOT_LIMIT.toLocaleString('en-US')} dots are shown.`);
  const samples = bubbleSamples(sizeMax);
  return {
    option,
    categories: [],
    pick: { by: 'item', value: (series, index) => dots[series]?.[index] },
    note: notes.join(' ') || null,
    sizeLegend: samples.length ? { title: sizeName, items: samples.map((v) => ({ size: bubbleSize(v, sizeMax), label: fs.short(v) })) } : undefined,
  };
}
