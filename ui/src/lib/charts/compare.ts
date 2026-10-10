// Compare two values (Advanced). Two values of each group are a dumbbell: one row for each group, two
// dots joined by a hairline. One value over two periods is a slope chart: one line for each group from
// the first period to the second. The engine sends rows `[group, first, second]` already ordered by the
// size of the gap; this only draws them.

import type { Cell } from '../api';
import { cell, getLocale, measureLabel, numberFormatter } from '../format';
import { axisCommon, esc, key, makeLabel, tooltipBase, type BuildArgs, type Built } from './common';
import { farFromZero } from './math';
import { niceUp } from './small';
import { partialNote } from '../run';
import { isGood, moveOf, moveWords, relative } from './pairs';

type Api = {
  value: (i: number) => number;
  coord: (p: number[]) => number[];
  size: (p: number[]) => number[];
};

const num = (v: Cell | undefined): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);

/** What a period is called: a date by the grain of its field, anything else as it is. */
function periodName(v: Cell | undefined, grain: string | null | undefined): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'string' && /^\d{4}-\d{2}-\d{2}/.test(v)) return cell(v, 'date', grain as never);
  return cell(v, typeof v === 'number' ? 'number' : 'text', grain as never);
}

/** The pixel height each row needs in a dumbbell chart. The tile scrolls when the rows need more. */
export const ROW_PX = 26;

export function buildCompare(args: BuildArgs): Built {
  return args.res.pair ? buildSlope(args) : buildDumbbell(args);
}

function buildDumbbell({ res, tile, colors: c, selected, width, tables = [] }: BuildArgs): Built {
  const loc = getLocale();
  const fmt = numberFormatter(tile.formats?.[0], loc);
  const label = makeLabel(res, tile);
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  const groups = res.rows.map((r) => r[0]);
  const names = groups.map(label);
  const sides = [0, 1].map((i) => (tile.measures[i] ? measureLabel(tile.measures[i], tables) : (res.columns[i + 1]?.name ?? '')));
  const all = res.rows.flatMap((r) => [num(r[1]), num(r[2])]).filter((v): v is number => v !== null);
  const far = farFromZero(all);
  const dim = (v: Cell) => (isSel(v) ? 1 : 0.28);
  // The axis has a little room at both ends, so the labels of the outer dots stay inside the plot.
  // Without a far-off range it still starts at zero (or below, for a negative value).
  const [lo, hi] = [Math.min(0, ...all), Math.max(0, ...all)];
  const [dLo, dHi] = [Math.min(...all), Math.max(...all)];
  const span = (far ? dHi - dLo : hi - lo) || 1;
  const roomMin = far ? dLo - span * 0.08 : lo < 0 ? lo - span * 0.08 : 0;
  const roomMax = (far ? dHi : hi) + span * 0.08;
  // Both ends go out to a round tick, so the last two numbers of the axis never crowd each other.
  const step = niceUp((roomMax - roomMin) / 5);
  const axisMin = roomMin === 0 ? 0 : Math.floor(roomMin / step) * step;
  const axisMax = Math.ceil(roomMax / step) * step;
  const nearEdge = (v: number) => (v - axisMin) / (axisMax - axisMin) < 0.14;

  // The hairline between the two dots.
  const line = {
    type: 'custom',
    name: 'Gap',
    z: 2,
    silent: false,
    encode: { x: [1, 2], y: 0 },
    renderItem: (_: unknown, api: Api) => {
      const i = api.value(0);
      const a = num(res.rows[i]?.[1]);
      const b = num(res.rows[i]?.[2]);
      if (a === null || b === null) return { type: 'group', children: [] };
      const [p, q] = [api.coord([a, i]), api.coord([b, i])];
      return { type: 'line', shape: { x1: p[0], y1: p[1], x2: q[0], y2: q[1] }, style: { stroke: c.text2, lineWidth: 1.5, opacity: 0.55 * dim(groups[i]) } };
    },
    data: res.rows.map((r, i) => ({ value: [i, num(r[1]) ?? 0, num(r[2]) ?? 0] })),
  };
  // A dot at each end. The label sits outside the pair, on the side away from the other dot.
  const dot = (side: 0 | 1) => ({
    type: 'scatter',
    name: sides[side],
    z: 4,
    symbol: 'circle',
    symbolSize: 11,
    itemStyle: { color: c.series[side], borderColor: c.surface, borderWidth: 2, opacity: 1 },
    emphasis: { scale: 1.2 },
    labelLayout: { hideOverlap: true },
    data: res.rows
      .map((r, i) => ({ r, i }))
      .filter(({ r }) => num(r[1 + side]) !== null)
      .map(({ r, i }) => {
        const [mine, other] = [num(r[1 + side])!, num(r[2 - side])];
        const outward = other === null || mine <= other ? 'left' : 'right';
        // A dot close to the axis puts its number above itself, not over the names of the rows.
        const place = outward === 'left' && nearEdge(mine) ? 'top' : outward;
        return {
          value: [mine, i],
          group: i,
          itemStyle: { opacity: dim(r[0]) },
          label: { show: true, position: place, color: c.text2, fontSize: 11, formatter: () => fmt.short(mine), opacity: dim(r[0]) },
        };
      }),
  });

  const tipOf = (i: number) => {
    const r = res.rows[i];
    const [a, b] = [num(r[1]), num(r[2])];
    const lines = [`<b>${esc(names[i])}</b>`, `${esc(sides[0])}: <b>${a === null ? '–' : esc(fmt.tip(a))}</b>`, `${esc(sides[1])}: <b>${b === null ? '–' : esc(fmt.tip(b))}</b>`];
    if (a !== null && b !== null) {
      const mv = moveWords(moveOf(a, b));
      const rel = relative(a, b);
      lines.push(`Gap: ${esc(fmt.tip(Math.abs(b - a)))} ${mv.glyph} ${esc(sides[1])} ${mv.word}${rel === null ? '' : ` (${rel > 0 ? '+' : rel < 0 ? '−' : ''}${Math.abs(Math.round(rel * 100))} %)`}`);
    }
    return lines.join('<br/>');
  };

  const option = {
    animationDuration: 350,
    legend: { type: 'scroll', data: sides, top: 0, left: 0, icon: 'circle', itemWidth: 10, itemHeight: 10, textStyle: { color: c.text, fontSize: 12 }, pageTextStyle: { color: c.text2 } },
    grid: { left: 4, right: 36, top: 30, bottom: 4, containLabel: true },
    tooltip: { ...tooltipBase(c), trigger: 'item', formatter: (p: { dataIndex: number; data: { group?: number }; seriesType: string }) => tipOf(p.seriesType === 'scatter' ? (p.data.group ?? 0) : p.dataIndex) },
    xAxis: { type: 'value' as const, ...axisCommon(c), scale: far, min: axisMin, max: axisMax, interval: step, axisLabel: { ...axisCommon(c).axisLabel, formatter: (v: number) => fmt.short(v) } },
    yAxis: {
      type: 'category' as const,
      data: names,
      inverse: true,
      ...axisCommon(c),
      splitLine: { show: false },
      axisLabel: { ...axisCommon(c).axisLabel, width: Math.min(150, Math.max(60, width * 0.25)), overflow: 'truncate' as const, interval: 0 },
      axisPointer: { type: 'shadow' as const },
      boundaryGap: true,
    },
    series: [line, dot(0), dot(1)],
  };
  const cut = res.total_rows ? `Showing the ${res.rows.length} rows with the biggest gap, of ${res.total_rows.toLocaleString('en-US')}.` : '';
  return { option, categories: groups, horizontal: true, rows: res.rows.length, note: [far ? 'The axis does not start at zero.' : '', cut].filter(Boolean).join(' ') || null };
}

function buildSlope({ res, tile, colors: c, selected, width, tables = [] }: BuildArgs): Built {
  const loc = getLocale();
  const fmt = numberFormatter(tile.formats?.[0], loc);
  const label = makeLabel(res, tile);
  const isSel = (v: Cell) => !selected || selected.some((s) => key(s) === key(v));
  const periods = res.pair?.periods ?? [];
  const grain = tile.dims[1]?.grain;
  // An unfinished second period is marked, and its change has no colour: the fall is not real.
  const unfinished = res.partial !== undefined && key(periods[1]) === res.partial.key;
  const ends = [0, 1].map((i) => periodName(periods[i], grain) + (i === 1 && unfinished ? '*' : ''));
  const measure = tile.measures[0] ? measureLabel(tile.measures[0], tables) : '';
  const groups = res.rows.map((r) => r[0]);
  const names = groups.map(label);
  const all = res.rows.flatMap((r) => [num(r[1]), num(r[2])]).filter((v): v is number => v !== null);
  const far = farFromZero(all);

  const colour = (i: number) => {
    if (unfinished) return c.text2;
    const good = isGood(moveOf(res.rows[i][1], res.rows[i][2]), tile.better);
    return good === null ? c.text2 : good ? c.good : c.bad;
  };
  const room = Math.min(190, Math.max(70, width * 0.26));
  // A long name is cut by itself, so the number beside it always stays. A letter is about 6 px wide.
  const clip = (name: string, value: string) => {
    const fit = Math.floor((room - 14) / 6.2) - value.length - 2;
    return name.length <= fit ? name : `${name.slice(0, Math.max(3, fit - 1)).trimEnd()}…`;
  };
  const series = res.rows.map((r, i) => {
    const [a, b] = [num(r[1]), num(r[2])];
    const mv = moveWords(moveOf(a, b));
    const opacity = isSel(r[0]) ? 1 : 0.25;
    const end = (side: 0 | 1, v: number | null) => ({
      value: v,
      label: {
        show: v !== null,
        position: side === 0 ? ('left' as const) : ('right' as const),
        color: c.text2,
        fontSize: 11,
        // The right end carries the sign, so the colour of the line is never the only sign.
        formatter: () => (side === 0 ? `${clip(names[i], fmt.short(v ?? 0))}  ${fmt.short(v ?? 0)}` : `${fmt.short(v ?? 0)} ${mv.glyph}`),
        width: room - 14,
        overflow: 'truncate' as const,
        opacity,
      },
    });
    return {
      type: 'line',
      name: names[i],
      symbol: 'circle',
      symbolSize: 8,
      showSymbol: true,
      lineStyle: { color: colour(i), width: 2, opacity },
      itemStyle: { color: colour(i), borderColor: c.surface, borderWidth: 2, opacity },
      emphasis: { focus: 'series', lineStyle: { width: 3 } },
      labelLayout: { hideOverlap: true },
      connectNulls: false,
      data: [end(0, a), end(1, b)],
    };
  });

  const tipOf = (i: number) => {
    const r = res.rows[i];
    const [a, b] = [num(r[1]), num(r[2])];
    const lines = [`<b>${esc(names[i])}</b>`, `${esc(ends[0])}: <b>${a === null ? '–' : esc(fmt.tip(a))}</b>`, `${esc(ends[1])}: <b>${b === null ? '–' : esc(fmt.tip(b))}</b>`];
    const mv = moveWords(moveOf(a, b));
    if (a !== null && b !== null) {
      const rel = relative(a, b);
      lines.push(`${mv.glyph} ${esc(measure || 'The value')} ${mv.word} by ${esc(fmt.tip(Math.abs(b - a)))}${rel === null ? '' : ` (${Math.abs(Math.round(rel * 100))} %)`}`.trim());
    } else {
      lines.push(`The value ${mv.word}.`);
    }
    return lines.join('<br/>');
  };

  const option = {
    animationDuration: 350,
    grid: { left: room, right: room, top: 28, bottom: 6, containLabel: false },
    tooltip: { ...tooltipBase(c), trigger: 'item', formatter: (p: { seriesIndex: number }) => tipOf(p.seriesIndex) },
    xAxis: {
      type: 'category' as const,
      data: ends,
      position: 'top' as const,
      boundaryGap: false,
      ...axisCommon(c),
      splitLine: { show: false },
      axisLabel: { ...axisCommon(c).axisLabel, color: c.text, fontSize: 12, fontWeight: 600, hideOverlap: false },
    },
    yAxis: { type: 'value' as const, show: false, scale: far },
    series,
  };
  const note = [far ? 'The scale does not start at zero.' : '', unfinished ? partialNote(res, grain) : ''].filter(Boolean).join(' ') || null;
  return {
    option,
    categories: groups,
    // A click on a line names its group.
    pick: { by: 'item', value: (series) => groups[series] },
    note,
  };
}
