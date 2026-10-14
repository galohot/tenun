// More in the tooltip (v5 build 10, V5-D12): up to three more values for each group of a chart, and a small trend of
// its first value. The engine works them out; this writes them into the tooltip. Pure, with only type imports, so the
// node tests load it.

import type { Cell, QueryResult } from './api';

export type Tip = NonNullable<QueryResult['tips']>[string];

/** A tooltip shows at most this many more values. */
export const MAX_TIP_VALUES = 3;

/** The key of a group, as the engine writes it (`advanced::spark_key`): its values joined by U+001F. */
export const tipKey = (groups: readonly Cell[]): string =>
  groups.map((v) => (v === null || v === undefined ? '\u0000null' : String(v))).join('\u001f');

/** A 120 × 32 line of a trend with its last point marked, as SVG. Gaps break the line. Empty with fewer than 2 points. */
export function trendSvg(values: readonly (number | null)[], color: string, w = 120, h = 32): string {
  const nums = values.filter((v): v is number => v !== null && Number.isFinite(v));
  if (nums.length < 2) return '';
  const lo = Math.min(...nums);
  const hi = Math.max(...nums);
  const pad = 3;
  const x = (i: number) => pad + (i * (w - 2 * pad)) / Math.max(1, values.length - 1);
  const y = (v: number) => (hi === lo ? h / 2 : pad + ((hi - v) * (h - 2 * pad)) / (hi - lo));
  let d = '';
  let pen = false;
  values.forEach((v, i) => {
    if (v === null || !Number.isFinite(v)) {
      pen = false;
      return;
    }
    d += `${pen ? 'L' : 'M'}${x(i).toFixed(1)} ${y(v).toFixed(1)}`;
    pen = true;
  });
  const last = values.length - 1;
  const end = values[last];
  const dot = end === null || !Number.isFinite(end) ? '' : `<circle cx="${x(last).toFixed(1)}" cy="${y(end).toFixed(1)}" r="2.5" fill="${color}"/>`;
  return `<svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" style="display:block;margin-top:4px"><path d="${d}" fill="none" stroke="${color}" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round"/>${dot}</svg>`;
}

/**
 * The tooltip's lines for one group: each extra value with its name, then the trend with its caption. `esc` escapes
 * text for HTML; `muted` is the colour of the names.
 */
export function tipHtml(
  tip: Tip | undefined,
  names: readonly string[],
  formats: readonly ((v: number) => string)[],
  trendCaption: string,
  color: string,
  muted: string,
  esc: (s: string) => string,
): string {
  if (!tip) return '';
  let out = '';
  (tip.extra ?? []).forEach((v, i) => {
    const shown = typeof v === 'number' ? (formats[i] ?? String)(v) : v === null ? '–' : String(v);
    out += `<br/><span style="color:${muted}">${esc(names[i] ?? '')}</span> <b>${esc(shown)}</b>`;
  });
  const svg = tip.trend ? trendSvg(tip.trend, color) : '';
  if (svg) out += `<br/><span style="color:${muted}">${esc(trendCaption)}</span>${svg}`;
  return out;
}
