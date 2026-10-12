// Getting things out of the app: a tile as a picture, its numbers as CSV or Excel, the whole
// dashboard as a picture or a PDF. The app uses this; the exported viewer does not.

import { mount, unmount } from 'svelte';
import { toCanvas } from 'html-to-image';
import { api, type QueryResult, type Tile } from './api';
import { buildOption, CHART_TYPES, readColors, type Colors } from './chart';
import { echarts } from './echarts';
import { columnHeads, measureLabel, type Named } from './format';
import { saveBlob, saveBytes } from './download';
import { motion } from './motion';
import { A4, buildPdf, buildPdfPages, pageSize, type PdfPage } from './pdf';
import { pageCuts, type Box } from './flow';
import { pictureKind, pictureNote } from './picture';
import { partialNote } from './run';
import { fileName, tileTable, toCsv } from './tabular';
import { withNamedFormats } from './measures';
import type { DashFilter } from './tiles';
import Capture from '../components/dash/Capture.svelte';

const SCALE = 2;
/** Browsers refuse canvases much bigger than this on a side. */
const MAX_SIDE = 15000;

const frame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));
const pause = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

const toBlob = (c: HTMLCanvasElement, type: string, quality?: number) =>
  new Promise<Blob>((ok, fail) => c.toBlob((b) => (b ? ok(b) : fail(new Error('The picture could not be made.'))), type, quality));

const loadImage = (src: string) =>
  new Promise<HTMLImageElement>((ok, fail) => {
    const i = new Image();
    i.onload = () => ok(i);
    i.onerror = () => fail(new Error('The picture could not be made.'));
    i.src = src;
  });

/** The numbers a tile shows, with the names the tile shows. */
function plainOf(tile: Tile, res: QueryResult, tables: Named[]) {
  const heads =
    tile.type === 'kpi'
      ? ['Period', tile.measures[0] ? measureLabel(tile.measures[0], tables) : 'Value']
      : columnHeads(res, tile, tables);
  return tileTable(tile, res, heads);
}

export function downloadCsv(tile: Tile, res: QueryResult, tables: Named[], title: string): void {
  const csv = toCsv(plainOf(tile, res, tables));
  saveBlob(new Blob([csv], { type: 'text/csv;charset=utf-8' }), fileName(title, 'csv'));
}

/** Excel: the server runs the tile again with the same filters and writes the file. */
export async function downloadXlsx(tile: Tile, res: QueryResult, filters: DashFilter[], tables: Named[], title: string, native: boolean): Promise<void> {
  // A value with no format of its own is written the way its named measure is.
  const body = { tile: withNamedFormats(tile), filters, headers: plainOf(tile, res, tables).heads, title };
  if (native) {
    const { paths } = await api.pick({ kind: 'save_xlsx', name: title || 'tenun' });
    if (!paths[0]) return;
    await api.exportXlsx(body, paths[0]);
    return;
  }
  saveBytes(await api.exportXlsx(body), fileName(title, 'xlsx'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet');
}

// ---- One tile as a picture ----------------------------------------------------------------

const HEADER = 36;
const PAD = 14;

/** A chart: ECharts draws it at 2x on a canvas that is not on the screen, then the title is added on top. */
async function chartPicture(tile: Tile, res: QueryResult, tables: Named[], title: string, colors: Colors, size: { w: number; h: number }) {
  const w = Math.max(320, size.w - PAD * 2);
  const h = Math.max(180, size.h - HEADER - 10);
  const holder = document.createElement('div');
  holder.style.cssText = `position:fixed;left:-10000px;top:0;width:${w}px;height:${h}px`;
  document.body.appendChild(holder);
  let url = '';
  let note = '';
  try {
    const built = buildOption({ res, tile: withNamedFormats(tile), colors, selected: null, width: w, height: h, tables });
    // A picture leaves the computer: it says it with no key value and no file name.
    note = pictureNote(res, built.note, built.gateNote, partialNote(res, tile.dims[0]?.grain));
    const chart = echarts.init(holder, null, { renderer: 'canvas', width: w, height: h });
    chart.setOption({ ...built.option, animation: false });
    url = chart.getDataURL({ pixelRatio: SCALE, backgroundColor: colors.surface });
    chart.dispose();
  } finally {
    holder.remove();
  }
  const img = await loadImage(url);
  await document.fonts?.ready;
  const font = getComputedStyle(document.body).fontFamily;
  const footer = note ? 22 : 10;
  const canvas = document.createElement('canvas');
  canvas.width = (w + PAD * 2) * SCALE;
  canvas.height = (HEADER + h + footer) * SCALE;
  const ctx = canvas.getContext('2d')!;
  ctx.scale(SCALE, SCALE);
  ctx.fillStyle = colors.surface;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = colors.text;
  ctx.font = `600 13px ${font}`;
  ctx.textBaseline = 'middle';
  ctx.fillText(fit(ctx, title, w), PAD, HEADER / 2 + 2);
  ctx.drawImage(img, PAD, HEADER, w, h);
  if (note) {
    ctx.fillStyle = colors.text2;
    ctx.font = `400 11px ${font}`;
    ctx.fillText(fit(ctx, note, w), PAD, HEADER + h + 10);
  }
  return canvas;
}

/** The text cut with an ellipsis so it fits the width. */
function fit(ctx: CanvasRenderingContext2D, text: string, width: number): string {
  if (ctx.measureText(text).width <= width) return text;
  let s = text;
  while (s.length > 1 && ctx.measureText(s + '…').width > width) s = s.slice(0, -1);
  return s + '…';
}

/**
 * The picture is made from a copy of the page, and the copy loses what a person ticked or moved.
 * The state goes into the attributes for the length of the capture, then it is put back.
 */
async function withControlState<T>(root: HTMLElement, draw: () => Promise<T>): Promise<T> {
  const undo: (() => void)[] = [];
  for (const input of root.querySelectorAll('input')) {
    if (input.type === 'checkbox' || input.type === 'radio') {
      const had = input.hasAttribute('checked');
      if (input.checked !== had) {
        input.toggleAttribute('checked', input.checked);
        undo.push(() => input.toggleAttribute('checked', had));
      }
    } else if (input.type === 'range') {
      const had = input.getAttribute('value');
      input.setAttribute('value', input.value);
      undo.push(() => (had === null ? input.removeAttribute('value') : input.setAttribute('value', had)));
    }
  }
  try {
    return await draw();
  } finally {
    for (const f of undo) f();
  }
}

/**
 * A tile that is not a chart (or a chart with no rows): what is on the screen is drawn to a canvas.
 * The footer of the tile holds the notes of the app, which name keys and may quote a formula, so it is
 * left out; the line under the picture is made from the public notes (V4-D52).
 */
async function domPicture(el: HTMLElement, colors: Colors, note: string) {
  await document.fonts?.ready;
  const canvas = await withControlState(el, () =>
    toCanvas(el, {
      pixelRatio: SCALE,
      backgroundColor: colors.surface,
      // The buttons that show on hover and the footer with the notes are not part of the picture.
      filter: (n) => !(n instanceof HTMLElement && n.matches('.edit, .resize, article > footer')),
    }),
  );
  return withNote(canvas, note, colors);
}

/** The canvas with a line of text under it, in the muted colour. */
function withNote(canvas: HTMLCanvasElement, note: string, colors: Colors): HTMLCanvasElement {
  if (!note) return canvas;
  const font = getComputedStyle(document.body).fontFamily;
  const out = document.createElement('canvas');
  out.width = canvas.width;
  out.height = canvas.height + 22 * SCALE;
  const ctx = out.getContext('2d')!;
  ctx.fillStyle = colors.surface;
  ctx.fillRect(0, 0, out.width, out.height);
  ctx.drawImage(canvas, 0, 0);
  ctx.scale(SCALE, SCALE);
  ctx.fillStyle = colors.text2;
  ctx.font = `400 11px ${font}`;
  ctx.textBaseline = 'middle';
  ctx.fillText(fit(ctx, note, canvas.width / SCALE - PAD * 2), PAD, canvas.height / SCALE + 11);
  return out;
}

/** The tile as a PNG file. `el` is the tile on the screen; charts do not need it for their pixels. */
export async function downloadTilePng(el: HTMLElement, tile: Tile, res: QueryResult | null, tables: Named[], title: string): Promise<void> {
  const colors = readColors();
  const canvas =
    res && pictureKind(tile, res, CHART_TYPES) === 'chart'
      ? await chartPicture(tile, res, tables, title, colors, { w: el.clientWidth, h: el.clientHeight })
      : await domPicture(el, colors, pictureNote(res, res && tile.type !== 'kpi' ? partialNote(res, tile.dims[0]?.grain) : null));
  saveBlob(await toBlob(canvas, 'image/png'), fileName(title, 'png'));
}

// ---- The whole dashboard ------------------------------------------------------------------

export interface DashboardPicture {
  title: string;
  /** A line under the title: the workspace and the day. */
  subtitle: string;
  /** What the dashboard is filtered by, one phrase each. */
  filters: string[];
  tiles: Tile[];
  results: Record<string, QueryResult | null | undefined>;
  tables: Named[];
  /** `light` for a document that may be printed. */
  theme: 'light' | 'dark';
  /** The width of the dashboard in pixels. */
  width?: number;
  /** A story draws as a column in its order, and prints on A4 pages cut between tiles (build 11). */
  layout?: 'grid' | 'flow';
  order?: string[];
}

/** Draws the dashboard off screen with the real tile components and returns it as one canvas at 2x. */
async function dashboardCanvas(p: DashboardPicture): Promise<HTMLCanvasElement> {
  return (await dashboardDrawing(p)).canvas;
}

/** The picture, and where each tile sits on it, top to bottom, in the canvas's pixels (for the pages of a story). */
async function dashboardDrawing(p: DashboardPicture): Promise<{ canvas: HTMLCanvasElement; boxes: Box[] }> {
  const root = document.documentElement;
  const before = root.dataset.theme;
  const switched = p.theme !== before;
  if (switched) {
    // The colours come from the theme on the page, so the page takes the other theme for a moment.
    root.dataset.theme = p.theme;
    await frame();
    await frame();
  }
  const holder = document.createElement('div');
  holder.style.cssText = 'position:fixed;left:-20000px;top:0;pointer-events:none';
  document.body.appendChild(holder);
  motion.still = true;
  const colors = readColors();
  const app = mount(Capture, { target: holder, props: { ...p, colors, width: p.width ?? 1200 } });
  try {
    await document.fonts?.ready;
    // Charts measure themselves, then draw. Give them a few frames.
    await frame();
    await pause(450);
    await frame();
    const node = holder.firstElementChild as HTMLElement;
    const side = Math.max(node.offsetWidth, node.offsetHeight);
    const ratio = Math.min(SCALE, MAX_SIDE / side);
    const top = node.getBoundingClientRect().top;
    const boxes: Box[] = [...node.querySelectorAll<HTMLElement>('[data-tile]')].map((el) => {
      const r = el.getBoundingClientRect();
      return { top: (r.top - top) * ratio, bottom: (r.bottom - top) * ratio, split: el.dataset.split === '1' };
    });
    const canvas = await withControlState(node, () => toCanvas(node, { pixelRatio: ratio, backgroundColor: colors.surface }));
    return { canvas, boxes };
  } finally {
    motion.still = false;
    unmount(app);
    holder.remove();
    if (switched) {
      if (before) root.dataset.theme = before;
      else delete root.dataset.theme;
    }
  }
}

export async function downloadDashboardPng(p: DashboardPicture): Promise<void> {
  const canvas = await dashboardCanvas(p);
  saveBlob(await toBlob(canvas, 'image/png'), fileName(p.title, 'png'));
}

export async function downloadDashboardPdf(p: DashboardPicture): Promise<void> {
  const { canvas, boxes } = await dashboardDrawing({ ...p, theme: 'light' });
  const jpegOf = async (c: HTMLCanvasElement) => new Uint8Array(await (await toBlob(c, 'image/jpeg', 0.92)).arrayBuffer());
  if (p.layout === 'flow') {
    // A story prints on A4 pages, cut between tiles: never inside a chart (build 11).
    const pageH = Math.floor((canvas.width * A4.h) / A4.w);
    const pages: PdfPage[] = [];
    for (const [start, end] of pageCuts(boxes, canvas.height, pageH)) {
      const slice = document.createElement('canvas');
      slice.width = canvas.width;
      slice.height = Math.max(1, Math.round(end - start));
      slice.getContext('2d')!.drawImage(canvas, 0, Math.round(start), canvas.width, slice.height, 0, 0, canvas.width, slice.height);
      pages.push({ jpeg: await jpegOf(slice), px: { w: slice.width, h: slice.height }, page: A4, draw: { w: A4.w, h: (A4.w * slice.height) / canvas.width } });
    }
    saveBytes(buildPdfPages(pages, p.title), fileName(p.title, 'pdf'), 'application/pdf');
    return;
  }
  const px = { w: canvas.width, h: canvas.height };
  saveBytes(buildPdf(await jpegOf(canvas), px, pageSize(px.w, px.h), p.title), fileName(p.title, 'pdf'), 'application/pdf');
}
