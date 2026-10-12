// The phone layout: below this width a dashboard is one column, in reading order. The app, the
// exported file and the hosted viewer share it. Pure code with no imports at run time.

import type { Tile } from './api';

/** Below this many pixels of width the dashboard is one column. */
export const PHONE_MAX = 700;
/** A phone chart is this high. */
export const PHONE_CHART_HEIGHT = 260;
const PHONE_PIVOT_HEIGHT = 320;
/** These read better a little taller on a phone. */
const WIDE_TYPES: string[] = ['pivot', 'heatmap', 'funnel', 'sankey', 'cohort', 'gantt', 'compare'];
/** A number tile is not made narrower than this to fit two in a row. */
const MIN_NUMBER_WIDTH = 150;

export interface Placed {
  t: Tile;
  x: number;
  y: number;
  w: number;
  h: number;
}

const rowsHeight = (rows: number, row: number, gap: number) => Math.max(1, rows) * row + (Math.max(1, rows) - 1) * gap;

/** A map keeps its shape on a phone: Indonesia and the world are about twice as wide as high. The legend and the credit line go under it. */
export const mapHeight = (width: number) => Math.round(width * 0.55) + 96;

/** Reading order: top to bottom, then left to right. */
export const readingOrder = (tiles: Tile[]): Tile[] => [...tiles].sort((a, b) => a.y - b.y || a.x - b.x);

/**
 * Number tiles first, two to a row when they fit, then everything else in reading order. A
 * chart is 260 px high. A table keeps that height and scrolls sideways inside it.
 */
export function phoneLayout(tiles: Tile[], width: number, row = 76, gap = 12): Placed[] {
  const ordered = readingOrder(tiles);
  const numbers = ordered.filter((t) => t.type === 'kpi');
  const rest = ordered.filter((t) => t.type !== 'kpi');
  const perRow = Math.max(1, Math.min(2, Math.floor((width + gap) / (MIN_NUMBER_WIDTH + gap))));
  const out: Placed[] = [];
  let y = 0;
  const cell = (width - gap * (perRow - 1)) / perRow;
  for (let i = 0; i < numbers.length; i += perRow) {
    const line = numbers.slice(i, i + perRow);
    // A lone number at the end of a row takes the whole width.
    const w = line.length === 1 ? width : cell;
    const h = Math.max(...line.map((t) => (t.h <= 1 ? row : rowsHeight(2, row, gap))));
    line.forEach((t, k) => out.push({ t, x: k * (cell + gap), y, w, h }));
    y += h + gap;
  }
  for (const t of rest) {
    const h =
      t.type === 'text' || t.type === 'image' || t.type === 'filter'
        ? rowsHeight(t.h, row, gap)
        : t.type === 'calendar'
          ? rowsHeight(3, row, gap)
          : t.type === 'map'
            ? mapHeight(width)
            : WIDE_TYPES.includes(t.type)
              ? PHONE_PIVOT_HEIGHT
              : PHONE_CHART_HEIGHT;
    out.push({ t, x: 0, y, w: width, h });
    y += h + gap;
  }
  return out;
}
