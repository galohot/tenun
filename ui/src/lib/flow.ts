// Story pages (build 11, V4-D13): a dashboard that reads like a document. Its tiles run down a centred column in the
// page's order, a text as tall as its words, a chart in three heights; two half-width tiles share a row on a wide
// screen and stack on a phone. The tiles keep their places on the grid, so a page switches back unchanged. Pure, with
// type-only imports and t(), so the node tests load it.

import type { Dashboard, Tile } from './api';
import { t } from './i18n.ts';

/** The column of a story: text reads best up to 760 px; a full-width chart may go to 1080. */
export const FLOW_TEXT = 760;
export const FLOW_WIDE = 1080;
/** The gap between tiles, across and down. */
export const FLOW_GAP = 16;
/** Narrower than this, two half tiles stack. */
export const FLOW_PAIR_FROM = 640;

export type FlowHeight = 's' | 'm' | 'l';
/** The three heights of a chart, in px. A number tile is shorter: it holds a figure, not a plot. */
export const CHART_PX: Record<FlowHeight, number> = { s: 240, m: 340, l: 460 };
export const NUMBER_PX: Record<FlowHeight, number> = { s: 140, m: 200, l: 260 };

export const isFlow = (d: Pick<Dashboard, 'layout'> | null | undefined): boolean => d?.layout === 'flow';

/** Text grows with its words; a picture keeps its shape in its box like a chart. */
const sized = (t: Tile) => t.type !== 'text';

/** A tile's width in a story: full, unless it says half. */
export const flowWidth = (t: Tile): 'full' | 'half' => t.flow?.width ?? 'full';

/** A tile's height in a story, in px, or null for a text, which is as tall as its words. */
export function flowHeight(t: Tile): number | null {
  if (!sized(t)) return null;
  const step = t.flow?.height ?? (t.type === 'kpi' ? 's' : 'm');
  return t.type === 'kpi' ? NUMBER_PX[step] : CHART_PX[step];
}

/**
 * The tiles in the page's order: the order the story keeps, then any tile it does not name yet in reading order (top to
 * bottom, then left to right on the grid).
 */
export function flowOrder(d: Pick<Dashboard, 'tiles' | 'order'>): Tile[] {
  const byId = new Map(d.tiles.map((t) => [t.id, t]));
  const named = (d.order ?? []).filter((id) => byId.has(id));
  const seen = new Set(named);
  const rest = [...d.tiles].filter((t) => !seen.has(t.id)).sort((a, b) => a.y - b.y || a.x - b.x);
  return [...named.map((id) => byId.get(id)!), ...rest];
}

/**
 * The rows of a story: one tile each, except two half tiles in a row, which share it when the page is wide. A half tile
 * with no half beside it keeps its width, on the left.
 */
export function flowRows(tiles: readonly Tile[], wide: boolean): Tile[][] {
  const rows: Tile[][] = [];
  for (let i = 0; i < tiles.length; i++) {
    const t = tiles[i];
    const next = tiles[i + 1];
    if (wide && flowWidth(t) === 'half' && next && flowWidth(next) === 'half') {
      rows.push([t, next]);
      i++;
    } else rows.push([t]);
  }
  return rows;
}

/** How wide a row's column is: text alone keeps to the reading width; anything else may use the wide one. */
export const rowWidth = (row: readonly Tile[]): number => (row.every((t) => t.type === 'text') ? FLOW_TEXT : FLOW_WIDE);

/** The order with one tile moved to another place. */
export function moveInOrder(order: readonly Tile[], from: number, to: number): string[] {
  const ids = order.map((t) => t.id);
  if (from === to || to < 0 || to >= ids.length) return ids;
  const [id] = ids.splice(from, 1);
  ids.splice(to, 0, id);
  return ids;
}

/** The order with a new tile put in at a place. */
export function insertInOrder(order: readonly Tile[], at: number, id: string): string[] {
  const ids = order.map((t) => t.id);
  ids.splice(Math.max(0, Math.min(at, ids.length)), 0, id);
  return ids;
}

// ---- pages of a printed story -----------------------------------------------------------------------------------------

/** A tile's place on the drawn page, top to bottom, in px; `split` when it may be cut across (a text). */
export interface Box {
  top: number;
  bottom: number;
  split: boolean;
}

/**
 * Where a tall picture of a story is cut into pages, as [start, end) in px. A cut falls between tiles, never inside a
 * chart: the page ends above the first tile that would not fit. Only a text taller than a page is cut inside, and so is
 * a tile taller than a page, which could not fit anywhere.
 */
export function pageCuts(boxes: readonly Box[], total: number, page: number): [number, number][] {
  const cuts: [number, number][] = [];
  let start = 0;
  while (total - start > page) {
    const limit = start + page;
    // The tiles that cross the limit: a cut there would go through them.
    const crossing = boxes.filter((b) => b.top < limit && b.bottom > limit && b.top >= start);
    let end = limit;
    const hard = crossing.filter((b) => !b.split);
    if (hard.length) {
      const top = Math.min(...hard.map((b) => b.top));
      // Above the tile, unless the tile starts the page (it is taller than a page): then it is cut.
      end = top > start ? top : limit;
    }
    cuts.push([start, end]);
    start = end;
  }
  cuts.push([start, total]);
  return cuts;
}

/** A new story: a heading and a lead paragraph to write over, and one chart to fill in. */
export function newStory(id: string, n: number, tileId: (p: string) => string): Dashboard {
  return {
    id,
    title: n ? t('Story {n}', { n: n + 1 }) : t('Story'),
    refresh: 60,
    layout: 'flow',
    tiles: [
      { id: tileId('t'), type: 'text', title: '', x: 0, y: 0, w: 12, h: 2, measures: [], dims: [], text: t('# A title\n\nWhat this page tells, in a sentence or two.'), textStyle: 'lead' },
      { id: tileId('t'), type: 'bar', title: '', x: 0, y: 2, w: 12, h: 5, measures: [], dims: [] },
    ],
  } as Dashboard;
}
