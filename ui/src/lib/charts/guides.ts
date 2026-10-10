// Reference lines on a chart (build 07): a target you type, or the average, median, lowest or highest
// of the values shown. Drawn as dashed ink lines with the name at the right end, never in a series
// colour. This file turns the tile's `lines` into levels and into an ECharts `markLine`; the maths
// (what the average is, where a name fits) is in `lines.ts`.

import type { TileLine } from '../api';
import type { Colors } from './common';
import { MAX_LINES, placeLevelLabels, placeUprightLabels, statOf, type Place, type StatKind } from './lines';

export type Axis = 'x' | 'y';

/** One line to draw: where it is, and what it is called ("Average", "Target", or the typed label). */
export interface Level {
  axis: Axis;
  value: number;
  name: string;
}

const WORDS: Record<StatKind | 'target', string> = {
  average: 'Average',
  median: 'Median',
  min: 'Minimum',
  max: 'Maximum',
  target: 'Target',
};

/** The lines of a tile that sit at a level: everything but the trend, and the Advanced band. */
export const levelLines = (lines: TileLine[] | undefined): TileLine[] =>
  (lines ?? []).slice(0, MAX_LINES).filter((l) => l.kind !== 'trend' && l.kind !== 'band');

/** Does the tile ask for a trend line? (It counts towards the limit of 4 lines.) */
export const wantsTrend = (lines: TileLine[] | undefined): boolean => (lines ?? []).slice(0, MAX_LINES).some((l) => l.kind === 'trend');

/**
 * The levels to draw. `read(axis)` gives the numbers a statistic is read from. On a scatter a line
 * has an axis (a median without one means both axes, as it did before this menu); on every other
 * chart the value axis is `y`. A target with no usable number is skipped.
 */
export function resolveLevels(lines: TileLine[] | undefined, scatter: boolean, read: (axis: Axis) => number[]): Level[] {
  const out: Level[] = [];
  for (const l of levelLines(lines)) {
    const name = l.label?.trim() || WORDS[l.kind as StatKind | 'target'];
    const axes: Axis[] = scatter ? (l.axis ? [l.axis] : l.kind === 'median' ? ['x', 'y'] : ['y']) : ['y'];
    for (const axis of axes) {
      const value = l.kind === 'target' ? (typeof l.value === 'number' && Number.isFinite(l.value) ? l.value : null) : statOf(l.kind as StatKind, read(axis));
      if (value !== null) out.push({ axis, value, name });
    }
  }
  return out;
}

/** About how wide a label of 11 px text is. */
const textWidth = (t: string) => 8 + t.length * 6.3;

/** The width in px of the widest name, as it is written on the chart. */
export const widestName = (levels: Level[], short: (axis: Axis, n: number) => string): number =>
  Math.max(0, ...levels.map((l) => textWidth(`${l.name} ${short(l.axis, l.value)}`)));

/** The height of one row of names above the plot, in px. */
export const NAME_ROW = 17;

/** Where each name goes: level lines (right end, above or below) or upright ones (first or second row, or left out). */
function namePlaces(levels: Level[], key: 'xAxis' | 'yAxis', span: { lo: number; hi: number; length: number; overhang?: number }, short: (axis: Axis, n: number) => string): Place[] {
  const values = levels.map((l) => l.value);
  return key === 'yAxis'
    ? placeLevelLabels(values, span.lo, span.hi, span.length)
    : placeUprightLabels(values, span.lo, span.hi, span.length, (i) => textWidth(`${levels[i].name} ${short(levels[i].axis, levels[i].value)}`), span.overhang);
}

/** Do the names of these upright lines need a second row above the plot? */
export function needsSecondRow(levels: Level[], key: 'xAxis' | 'yAxis', span: { lo: number; hi: number; length: number; overhang?: number }, short: (axis: Axis, n: number) => string): boolean {
  return key === 'xAxis' && namePlaces(levels, key, span, short).includes('below');
}

/**
 * The ECharts `markLine` for lines of one direction. `key` is the coordinate of the value: `yAxis`
 * draws level lines (the name sits at the right end, above or below the line), `xAxis` upright ones
 * (the name sits above the plot, in one row or two). `span` is the range of the axis and `length` its
 * length in px, which decide whether two names would touch; a name that would touch another is
 * left out (the line stays).
 */
export function levelMarkLine(
  levels: Level[],
  key: 'xAxis' | 'yAxis',
  span: { lo: number; hi: number; length: number; overhang?: number },
  c: Colors,
  short: (axis: Axis, n: number) => string,
  /** Upright lines: which end of the line carries the name. A flipped axis (horizontal bars) swaps the ends. */
  top: 'start' | 'end' = 'end',
  /** Level lines: the names sit outside the plot, to the right of the lines, where no bar can be under them. */
  outside = false,
) {
  if (!levels.length) return null;
  const texts = levels.map((l) => `${l.name} ${short(l.axis, l.value)}`);
  const places = namePlaces(levels, key, span, short);
  return {
    silent: true,
    symbol: 'none',
    animation: false,
    lineStyle: { color: c.text2, width: 1, type: [5, 4] as number[] },
    data: levels.map((l, i) => ({
      [key]: l.value,
      label: {
        show: places[i] !== 'hide',
        formatter: texts[i],
        color: c.text2,
        fontSize: 11,
        // The name has the colour of the surface behind it, so a line of data under it stays readable.
        backgroundColor: c.surface,
        padding: [1, 4],
        borderRadius: 3,
        rotate: 0,
        ...(key === 'yAxis'
          ? outside
            ? { position: 'end' as const, distance: 4, offset: [0, places[i] === 'below' ? 8 : -8] }
            : { position: places[i] === 'below' ? ('insideEndBottom' as const) : ('insideEndTop' as const), distance: 2 }
          : { position: top, distance: places[i] === 'below' ? 4 + NAME_ROW : 4 }),
      },
    })),
  };
}

/** The range an axis shows, for placing names: it takes in the lines and, for a chart from zero, zero. */
export function axisSpan(data: number[], levels: number[], fromZero: boolean, length: number, overhang = 0) {
  const all = [...data, ...levels];
  const lo = Math.min(...all, ...(fromZero ? [0] : []));
  // ECharts rounds the top of the axis up to a tidy number; a little room stands in for that.
  const hi = Math.max(...all, ...(fromZero ? [0] : []));
  return { lo, hi: hi + (hi - lo) * 0.06, length: Math.max(40, length), overhang };
}
