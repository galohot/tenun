// Pie and donut (build 07, V4-D7, GRANDPLAN §6), the parts that need no app state: the slices, their labels, the
// centre of a donut and the ECharts option. At most six slices and "Other", largest first from twelve o'clock,
// clockwise; colours by entity; a 1 px separator in the surface colour; the share on the slice when it fits, outside
// on a short leader line when it is small. Pure, with only pure imports at run time, so the node tests load it.
// `pie.ts` feeds it from a result.

import type { EChartsCoreOption } from 'echarts/core';
import type { Cell } from '../api';
import { pctShareIn, type Locale } from '../locale.ts';

/** The key `"Other"` has in a result (`shape::OTHER_KEY`). */
const OTHER_KEY = '\u0000other';
/** The key a value is compared by, as `common.ts` writes it. */
const keyOf = (v: Cell) => (v === null ? '\u0000null' : String(v));

/**
 * A slice smaller than this share of the whole has its label outside, on a short leader line. The build file said 8 %;
 * a name and a share need more room than a share alone, so a slice of 12 % lost its label (V4-D136).
 */
export const INSIDE_AT = 0.15;
/** On a phone (the window narrower than this), or in a tile narrower than `LIST_TILE`, the labels are a list under the chart. */
export const LIST_BELOW = 700;
export const LIST_TILE = 260;
/** The hole of a donut, as a share of its radius. */
export const DONUT_HOLE = 0.62;
/**
 * A name on a slice is cut to this many characters. ECharts would cut it to a width instead, but a label with a
 * width has a box that wide, and the pie's overlap check then hid a neighbour that the text did not touch.
 */
export const INSIDE_CHARS = 14;
/** The app's own font for text ECharts draws on the canvas (a CSS variable does not reach it). */
const FONT = "-apple-system, BlinkMacSystemFont, 'Inter Variable', 'Segoe UI Variable', system-ui, sans-serif";

export interface Slice {
  raw: Cell;
  label: string;
  value: number;
  share: number;
  other: boolean;
  /** The palette slot; null for "Other", which is grey. */
  slot: number | null;
}

/**
 * The slices of a result: positive values, largest first, the engine's "Other" last. `slots` gives each name its
 * palette slot; it is called with the names in A to Z order, so a name keeps its slot across filters when the
 * caller keeps a memory (`common.ts slotsFor`).
 */
export function slicesOf(rows: readonly Cell[][], label: (v: Cell) => string, slots: (keys: string[]) => number[]): Slice[] {
  const all = rows
    .map((r) => ({ raw: r[0], value: typeof r[r.length - 1] === 'number' ? (r[r.length - 1] as number) : 0 }))
    .filter((r) => r.value > 0);
  const other = all.find((r) => r.raw === OTHER_KEY);
  const rest = all.filter((r) => r !== other).sort((a, b) => b.value - a.value);
  const total = all.reduce((s, r) => s + r.value, 0);
  const names = rest.map((r) => keyOf(r.raw)).sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  const given = slots(names);
  const slotOf = new Map(names.map((k, i) => [k, given[i]]));
  const out: Slice[] = rest.map((r) => ({ raw: r.raw, label: label(r.raw), value: r.value, share: total ? r.value / total : 0, other: false, slot: slotOf.get(keyOf(r.raw)) ?? 0 }));
  if (other) out.push({ raw: OTHER_KEY, label: 'Other', value: other.value, share: total ? other.value / total : 0, other: true, slot: null });
  return out;
}

/** The words on a slice: its share, one decimal under 10 %. */
export const shareText = (share: number, loc: Locale = 'en') => pctShareIn(share, loc);

/** A name cut to fit on a slice, with an ellipsis; counted in characters, not code units. */
export function shortName(s: string, n = INSIDE_CHARS): string {
  const chars = Array.from(s);
  return chars.length > n ? `${chars.slice(0, n - 1).join('').trimEnd()}…` : s;
}

/** The two inks a label on a slice can have. */
const LIGHT_INK = '#fff';
const DARK_INK = '#111';

/** The luminance of a colour written `#rgb`, `#rrggbb` or `rgb(r, g, b)` (WCAG 2), or null for any other form. */
function luminance(color: string): number | null {
  const c = color.trim();
  let rgb: number[] | null = null;
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(c);
  if (hex) {
    const h = hex[1].length === 3 ? [...hex[1]].map((x) => x + x).join('') : hex[1];
    rgb = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
  } else {
    const m = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/i.exec(c);
    if (m) rgb = [m[1], m[2], m[3]].map(Number);
  }
  if (!rgb) return null;
  const [r, g, b] = rgb.map((v) => {
    const x = v / 255;
    return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The ink of a label on a slice: white or near-black, whichever has more contrast with the slice. White when unsure. */
export function inkOn(color: string): string {
  const l = luminance(color);
  if (l === null) return LIGHT_INK;
  const dark = luminance(DARK_INK) as number;
  return 1.05 / (l + 0.05) >= (l + 0.05) / (dark + 0.05) ? LIGHT_INK : DARK_INK;
}

/** The label of a slice goes inside when the slice is big enough, outside on a leader line when it is not. */
export const labelInside = (share: number) => share >= INSIDE_AT;

/** What the centre of a donut says: the slice under the pointer, or the total. */
export function centre(slices: readonly Slice[], hovered: number | null, format: (v: number) => string, measure: string, loc: Locale = 'en'): { value: string; caption: string } {
  const s = hovered === null ? undefined : slices[hovered];
  if (s) return { value: shareText(s.share, loc), caption: s.label };
  return { value: format(slices.reduce((t, x) => t + x.value, 0)), caption: measure };
}

/** The part of an ECharts instance `hiddenLabels` reads. Not public API: `tests/pie.test.js` pins it on the bundled version. */
interface Drawn {
  getModel(): {
    getSeriesByIndex(i: number): { getData(): { count(): number; getItemGraphicEl(i: number): { getTextContent?(): { ignore?: boolean } | undefined } | undefined } } | undefined;
  };
}

/** The slices whose label ECharts hid after drawing, because it would have touched another label. */
export function hiddenLabels(chart: unknown): number[] {
  try {
    const data = (chart as Drawn).getModel().getSeriesByIndex(0)?.getData();
    if (!data) return [];
    const out: number[] = [];
    for (let i = 0; i < data.count(); i++) if (data.getItemGraphicEl(i)?.getTextContent?.()?.ignore) out.push(i);
    return out;
  } catch {
    return [];
  }
}

export interface PieColors {
  surface: string;
  raised: string;
  text: string;
  text2: string;
  series: string[];
}

export interface PieOptionArgs {
  slices: Slice[];
  colors: PieColors;
  donut: boolean;
  /** Labels under the chart instead of on it (a phone). */
  listed: boolean;
  /** The keys picked by a click: the other slices are dimmed. */
  picked: Set<string> | null;
  format: (v: number) => string;
  /** The name of the value, under a donut's total. */
  measure: string;
  hovered: number | null;
  loc: Locale;
  /** The smaller side of the chart in pixels: the centre of a donut is written to fit it. */
  size?: number;
  /** Escapes a value for the tooltip. */
  esc: (s: string) => string;
}

export function pieOption(a: PieOptionArgs): EChartsCoreOption {
  const { slices, colors: c, donut, listed, loc } = a;
  // Labels outside need room at the sides: a donut, or a pie with a small slice, is drawn smaller.
  const outside = !listed && (donut || slices.some((s) => !labelInside(s.share)));
  const radius = listed ? 82 : outside ? 52 : 70;
  const data = slices.map((s) => {
    const inside = labelInside(s.share);
    const share = shareText(s.share, loc);
    const color = s.slot === null ? c.text2 : c.series[s.slot % c.series.length];
    return {
      name: s.label,
      value: s.value,
      itemStyle: { color, opacity: a.picked && !a.picked.has(keyOf(s.raw)) ? 0.35 : 1 },
      label: listed
        ? { show: false }
        : inside && !donut
          ? { show: true, position: 'inside', formatter: `${shortName(s.label)}\n${share}`, color: inkOn(color), fontSize: 11, fontWeight: 600, lineHeight: 15 }
          : { show: true, position: 'outside', formatter: `${s.label}\n${share}`, color: c.text, fontSize: 11, lineHeight: 15, alignTo: 'edge', edgeDistance: 8, overflow: 'break' },
      labelLine: { show: !listed && (donut || !inside), length: 8, length2: 8, lineStyle: { color: c.text2 } },
    };
  });
  const mid = centre(slices, a.hovered, a.format, a.measure, loc);
  // The big number fits the hole: 22 px on a desktop tile, smaller on a phone.
  const big = Math.round(Math.max(14, Math.min(22, (a.size ?? 320) * 0.065)));
  return {
    animation: false,
    tooltip: {
      trigger: 'item',
      confine: true,
      backgroundColor: c.raised,
      borderWidth: 0,
      textStyle: { color: c.text, fontSize: 12 },
      formatter: (p: { dataIndex: number }) => {
        const s = slices[p.dataIndex];
        return s ? `${a.esc(s.label)}<br/><b>${a.esc(a.format(s.value))}</b> <span style="color:${c.text2}">${shareText(s.share, loc)}</span>` : '';
      },
    },
    series: [
      {
        type: 'pie',
        // Largest first from twelve o'clock, clockwise. The slices come in that order; ECharts keeps it.
        startAngle: 90,
        clockwise: true,
        avoidLabelOverlap: true,
        radius: donut ? [`${Math.round(radius * DONUT_HOLE)}%`, `${radius}%`] : ['0%', `${radius}%`],
        center: ['50%', '50%'],
        itemStyle: { borderColor: c.surface, borderWidth: 1, borderRadius: 0 },
        emphasis: { scale: false, label: { show: true } },
        // Two labels that would still touch: one is hidden, and the tooltip and the list keep its value.
        labelLayout: { hideOverlap: true },
        data,
      },
    ],
    graphic: donut
      ? [
          {
            type: 'group',
            left: 'center',
            top: 'middle',
            silent: true,
            children: [
              { type: 'text', style: { text: mid.value, fill: c.text, fontSize: big, fontWeight: 600, fontFamily: FONT, textAlign: 'center' }, left: 'center', top: 0 },
              { type: 'text', style: { text: mid.caption, fill: c.text2, fontSize: 11, fontFamily: FONT, textAlign: 'center', width: 120, overflow: 'truncate' }, left: 'center', top: big + 8 },
            ],
          },
        ]
      : undefined,
  };
}
