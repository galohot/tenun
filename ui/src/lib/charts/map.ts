// The map tile (build 06): a filled map of places coloured by a value, or points sized by one. The geometry
// comes in as GeoJSON (topo.ts decodes Tenun's TopoJSON files); this file only turns a result into an ECharts
// option. Rules (GRANDPLAN §6): one sequential ramp of seven steps; a diverging ramp only when the values cross
// zero (a change); a shape with no data is the inset colour with a hatch, never the lowest step; the legend gives
// the range; colour by entity, never by rank, for points.

import type { EChartsCoreOption } from 'echarts/core';
import type { Cell, QueryResult } from '../api';
import type { Colors } from './common';
import type { Feature } from '../topo';
import { bounds } from '../topo.ts';
import { getLang, t } from '../i18n.ts';

/** How many steps a ramp has. */
export const STEPS = 7;

export interface Ramp {
  kind: 'sequential' | 'diverging';
  /** The edges of the steps: `STEPS + 1` numbers, low to high. */
  edges: number[];
  min: number;
  max: number;
}

/**
 * The steps of a map's colours. Values that cross zero get a diverging ramp centred on zero (a change); the
 * others a sequential one from the lowest to the highest value. Equal intervals, so a step is a range a person
 * can read off the legend.
 */
export function rampOf(values: readonly (number | null)[]): Ramp | null {
  const vs = values.filter((v): v is number => typeof v === 'number' && Number.isFinite(v));
  if (!vs.length) return null;
  const min = Math.min(...vs);
  const max = Math.max(...vs);
  if (min < 0 && max > 0) {
    const m = Math.max(-min, max);
    return { kind: 'diverging', min, max, edges: Array.from({ length: STEPS + 1 }, (_, i) => -m + (2 * m * i) / STEPS) };
  }
  const span = max - min;
  return { kind: 'sequential', min, max, edges: Array.from({ length: STEPS + 1 }, (_, i) => (span ? min + (span * i) / STEPS : min)) };
}

/** The step (0 to STEPS − 1) of a value, or null for no value. One value alone is the middle step. */
export function stepOf(ramp: Ramp, v: number | null): number | null {
  if (v === null || !Number.isFinite(v)) return null;
  if (ramp.max === ramp.min) return Math.floor(STEPS / 2);
  const { edges } = ramp;
  for (let i = STEPS - 1; i >= 0; i--) if (v >= edges[i]) return i;
  return 0;
}

/** "1st", "2nd", "3rd", "4th", "11th", "22nd"; in Indonesian "ke-1". */
export function ordinal(n: number): string {
  if (getLang() === 'id') return `ke-${n}`;
  const tens = n % 100;
  if (tens >= 11 && tens <= 13) return `${n}th`;
  return `${n}${['th', 'st', 'nd', 'rd'][n % 10] ?? 'th'}`;
}

export interface MapArgs {
  res: QueryResult;
  colors: Colors;
  /** The places of the level: the shapes, with `code` and `name` in their properties. */
  shapes: Feature[];
  /** The name the shapes are registered under (`echarts.registerMap`). */
  mapName: string;
  /** Kabupaten maps: the provinces, drawn over them as stronger lines. */
  borders?: { mapName: string } | null;
  /** The codes picked by a click. */
  selected: string[] | null;
  /** Writes a value the way the tile's format says. */
  format: (v: number) => string;
}

/** `#rrggbb` with an opacity, as `rgba()`; any other colour as it is. */
export function withAlpha(color: string, alpha: number): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color.trim());
  if (!m) return color;
  const [r, g, b] = m.slice(1).map((h) => parseInt(h, 16));
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** A shape with no data: the inset colour with a hatch in the axis text colour, faint, so it never reads as the lowest step. */
const noData = (c: Colors) => ({
  areaColor: c.inset,
  decal: { symbol: 'rect', symbolSize: 1, color: withAlpha(c.text2, 0.3), dashArrayX: [1, 0], dashArrayY: [1, 4], rotation: Math.PI / 4 },
});

/** A box a little larger than `b`, so the shapes at its edge are not cut. */
export function padded(b: [[number, number], [number, number]] | null, by = 0.06): [[number, number], [number, number]] | null {
  if (!b) return null;
  const [[w, n], [e, s]] = b;
  const dx = Math.max((e - w) * by, 0.05);
  const dy = Math.max((n - s) * by, 0.05);
  return [[w - dx, Math.min(90, n + dy)], [e + dx, Math.max(-90, s - dy)]];
}

/** A filled map, with the ramp the legend draws. */
export function buildFilled(a: MapArgs): { option: EChartsCoreOption; ramp: Ramp | null } {
  const { res, colors: c } = a;
  const values = res.rows.map((r) => (typeof r[1] === 'number' ? r[1] : null));
  const ramp = rampOf(values);
  const steps = ramp?.kind === 'diverging' ? c.div : c.ramp;
  const codes = res.rows.map((r) => String(r[0]));
  const nameOf = new Map(a.shapes.map((f) => [String(f.properties.code), String(f.properties.name ?? f.properties.code)]));
  const picked = a.selected?.length ? new Set(a.selected) : null;
  const total = res.rows.length;
  const data: { name: string; value: number | null; rank: number; itemStyle: object }[] = res.rows.map((r, i) => {
    const v = values[i];
    const s = ramp ? stepOf(ramp, v) : null;
    const dim = picked && !picked.has(codes[i]);
    return {
      name: codes[i],
      value: v,
      rank: i + 1,
      itemStyle: s === null ? noData(c) : { areaColor: steps[s], opacity: dim ? 0.35 : 1 },
    };
  });
  // The shapes with no data are items of their own: the hatch is theirs alone, not inherited by the places
  // that have a value.
  const seen = new Set(codes);
  for (const f of a.shapes) {
    const code = String(f.properties.code);
    if (!seen.has(code)) data.push({ name: code, value: null, rank: 0, itemStyle: noData(c) });
  }
  // Fit the shapes that have data, so a map of one province is not a dot in the country.
  const fit = padded(bounds(a.shapes.filter((f) => seen.has(String(f.properties.code)))) ?? bounds(a.shapes), 0.04);
  const place = { left: 8, right: 8, top: 8, bottom: 8, boundingCoords: fit ?? undefined, roam: false };
  const series: Record<string, unknown>[] = [
    {
      type: 'map',
      map: a.mapName,
      nameProperty: 'code',
      ...place,
      selectedMode: false,
      data,
      itemStyle: { areaColor: c.inset, borderColor: c.surface, borderWidth: 0.6 },
      emphasis: { label: { show: false }, itemStyle: { borderColor: c.text, borderWidth: 1.2 } },
      label: { show: false },
    },
  ];
  if (a.borders) {
    series.push({
      type: 'map',
      map: a.borders.mapName,
      nameProperty: 'code',
      ...place,
      silent: true,
      selectedMode: false,
      itemStyle: { areaColor: 'transparent', borderColor: c.text2, borderWidth: 1 },
      emphasis: { disabled: true },
      label: { show: false },
      tooltip: { show: false },
    });
  }
  return {
    ramp,
    option: {
      animation: false,
      tooltip: {
        trigger: 'item',
        confine: true,
        backgroundColor: c.raised,
        borderWidth: 0,
        textStyle: { color: c.text, fontSize: 12 },
        formatter: (p: { name: string; value: unknown; data?: { rank?: number } }) => {
          const name = escape(nameOf.get(p.name) ?? p.name);
          if (typeof p.value !== 'number' || !Number.isFinite(p.value)) return `${name}<br/><span style="color:${c.text2}">${t('No data')}</span>`;
          const rank = p.data?.rank ? `<br/><span style="color:${c.text2}">${t('{rank} of {total}', { rank: ordinal(p.data.rank), total })}</span>` : '';
          return `${name}<br/><b>${escape(a.format(p.value))}</b>${rank}`;
        },
      },
      series,
    },
  };
}

export interface PointArgs {
  res: QueryResult;
  colors: Colors;
  /** The outline under the points: the world, or the provinces of a country. */
  outline: Feature[];
  mapName: string;
  format: (v: number) => string;
  /** The name of the value, for the tooltip. */
  valueName: string;
  /** Points of more groups than the palette has: the rest is one grey "Other". */
  maxGroups?: number;
}

/** The groups of a points map, the biggest first: up to `max`, then "Other". */
export function pointGroups(rows: readonly Cell[][], max = 8): { names: string[]; other: boolean } {
  const totals = new Map<string, number>();
  for (const r of rows) {
    if (r.length < 4) continue;
    const g = r[2] === null ? '' : String(r[2]);
    totals.set(g, (totals.get(g) ?? 0) + (typeof r[3] === 'number' ? Math.abs(r[3]) : 0));
  }
  const names = [...totals.keys()].sort((x, y) => totals.get(y)! - totals.get(x)! || x.localeCompare(y));
  return names.length > max ? { names: names.slice(0, max), other: true } : { names, other: false };
}

/** A points map: one dot for each place, its area by the value (square root, so twice the value is twice the ink). */
export function buildPoints(a: PointArgs): EChartsCoreOption {
  const { res, colors: c } = a;
  const grouped = res.rows.some((r) => r.length >= 4);
  const valueAt = grouped ? 3 : 2;
  const vals = res.rows.map((r) => (typeof r[valueAt] === 'number' ? Math.abs(r[valueAt] as number) : 0));
  const top = Math.max(1, ...vals);
  const size = (v: number) => 4 + 20 * Math.sqrt(v / top);
  const { names, other } = grouped ? pointGroups(res.rows, a.maxGroups ?? 8) : { names: [''], other: false };
  const groupOf = (r: Cell[]) => {
    if (!grouped) return '';
    const g = r[2] === null ? '' : String(r[2]);
    return names.includes(g) ? g : '\u0000other';
  };
  const order = [...names, ...(other ? ['\u0000other'] : [])];
  const series = order.map((g, i) => ({
    type: 'scatter',
    coordinateSystem: 'geo',
    name: g === '\u0000other' ? t('Other') : g || t('Blank'),
    data: res.rows
      .filter((r) => groupOf(r) === g)
      .map((r) => ({ value: [r[1], r[0], r[valueAt]], symbolSize: size(Math.abs(Number(r[valueAt]) || 0)) })),
    itemStyle: { color: g === '\u0000other' ? c.text2 : c.series[i % c.series.length], opacity: 0.8, borderColor: c.surface, borderWidth: 0.5 },
    emphasis: { scale: 1.2 },
  }));
  // The box of the points, a little wider, or the outline.
  const xs = res.rows.map((r) => Number(r[1]));
  const ys = res.rows.map((r) => Number(r[0]));
  const pad = (lo: number, hi: number) => Math.max(1, (hi - lo) * 0.15);
  const box =
    res.rows.length > 0
      ? [
          [Math.min(...xs) - pad(Math.min(...xs), Math.max(...xs)), Math.max(...ys) + pad(Math.min(...ys), Math.max(...ys))],
          [Math.max(...xs) + pad(Math.min(...xs), Math.max(...xs)), Math.min(...ys) - pad(Math.min(...ys), Math.max(...ys))],
        ]
      : bounds(a.outline);
  return {
    animation: false,
    legend: grouped && order.length > 1 ? { bottom: 0, icon: 'circle', itemWidth: 8, itemHeight: 8, textStyle: { color: c.text2, fontSize: 11 } } : undefined,
    geo: {
      map: a.mapName,
      roam: false,
      silent: true,
      left: 8,
      right: 8,
      top: 8,
      bottom: grouped && order.length > 1 ? 28 : 8,
      boundingCoords: box ?? undefined,
      itemStyle: { areaColor: c.inset, borderColor: c.grid, borderWidth: 0.6 },
      emphasis: { disabled: true },
      label: { show: false },
    },
    tooltip: {
      trigger: 'item',
      confine: true,
      backgroundColor: c.raised,
      borderWidth: 0,
      textStyle: { color: c.text, fontSize: 12 },
      formatter: (p: { seriesName: string; value: unknown[] }) => {
        const [lon, lat, v] = p.value as number[];
        const head = grouped ? `${escape(p.seriesName)}<br/>` : '';
        return `${head}<b>${escape(a.valueName)}: ${escape(a.format(v))}</b><br/><span style="color:${c.text2}">${lat.toFixed(3)}, ${lon.toFixed(3)}</span>`;
      },
    },
    series,
  };
}

const escape = (s: string) => s.replace(/[&<>"]/g, (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[ch]!);
