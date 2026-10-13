// Drill-down inside a chart (v5 build 01, V5-D3): which tiles drill and how, the steps of a drill, and the words of
// the drill bar. The engine does the drilling (`drill.rs`); this file only keeps the reader's place. Pure, with no
// imports at run time, so the node tests load it.

import type { Cell, DrillInfo, DrillState, Grain, Hierarchy, Tile } from './api';

/** The tile types that drill. A map is left out: a place stands for several names in the data (V5-D36). */
export const DRILL_TYPES: readonly Tile['type'][] = ['bar', 'line', 'area', 'table', 'pie', 'donut', 'pivot'];

export const TOP: DrillState = { path: [] };

const PERIODS: readonly Grain[] = ['year', 'quarter', 'month', 'week', 'day'];

/** What the Inspector offers for a tile: off, the date chain of its first field, or a hierarchy that holds that field. */
export function drillOptions(tile: Pick<Tile, 'type' | 'dims'>, hierarchies: readonly Hierarchy[]): { value: string; label: string }[] {
  const first = tile.dims[0];
  if (!DRILL_TYPES.includes(tile.type) || !first) return [];
  const out = [{ value: '', label: 'Off' }];
  if (first.grain && PERIODS.includes(first.grain) && first.grain !== 'day') {
    const chain = first.grain === 'week' ? ['week', 'day'] : ['year', 'quarter', 'month', 'day'].slice(['year', 'quarter', 'month'].indexOf(first.grain));
    out.push({ value: 'date', label: `By date: ${chain.join(' › ')}` });
  }
  for (const h of hierarchies) {
    const at = h.levels.findIndex((l) => l.table === first.table && l.column === first.column);
    if (at >= 0 && at < h.levels.length - 1) out.push({ value: h.id, label: h.name });
  }
  return out.length > 1 ? out : [];
}

/** The tile's drill setting from an option's value. */
export const drillSetting = (value: string): Tile['drill'] => (value === '' ? undefined : value === 'date' ? {} : { hierarchy: value });

/** The option a tile's drill setting is. */
export const drillSettingValue = (d: Tile['drill']): string => (d ? (d.hierarchy ?? 'date') : '');

/**
 * The tile as the chart draws it at the level shown: its first field is that level's field (a date at a smaller grain,
 * or a lower field of the hierarchy). A click on a mark filters by that field.
 */
export function shownTile<T extends Pick<Tile, 'dims' | 'drill'>>(tile: T, info: DrillInfo | undefined, hierarchies: readonly Hierarchy[]): T {
  const first = tile.dims[0];
  if (!info || info.at === 0 || !first || !tile.drill) return tile;
  const rest = tile.dims.slice(1);
  if (!tile.drill.hierarchy) return { ...tile, dims: [{ ...first, grain: info.grains[info.at] ?? first.grain }, ...rest] };
  const h = hierarchies.find((x) => x.id === tile.drill?.hierarchy);
  const start = h?.levels.findIndex((l) => l.table === first.table && l.column === first.column) ?? -1;
  const level = h && start >= 0 ? h.levels[start + info.at] : undefined;
  return level ? { ...tile, dims: [{ table: level.table, column: level.column, grain: null }, ...rest] } : tile;
}

/** One level down, keeping only `value`. */
export const down = (s: DrillState, value: Cell): DrillState => ({ path: [...s.path, { value }] });

/** One level down for every value at once ("Next level"). */
export const nextLevel = (s: DrillState): DrillState => ({ path: [...s.path, {}] });

/** One level up. */
export const up = (s: DrillState): DrillState => ({ path: s.path.slice(0, -1) });

/** Back to a level of the bar: 0 is the top. */
export const toDepth = (s: DrillState, depth: number): DrillState => ({ path: s.path.slice(0, depth) });

export const isTop = (s: DrillState): boolean => s.path.length === 0;

/**
 * The drill bar: "All years", then one part for each step ("2026", "Every quarter"), each with the depth a click on it
 * goes back to; then what the chart shows now ("by month"). `fmt` writes a value of a level with that level's grain.
 */
export function crumbs(info: DrillInfo, fmt: (v: Cell, grain: Grain | null) => string): { parts: { label: string; depth: number }[]; showing: string } {
  const plural = (s: string) => (/(s|x|ch|sh)$/i.test(s) ? `${s}es` : /[^aeiou]y$/i.test(s) ? `${s.slice(0, -1)}ies` : `${s}s`);
  const lower = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);
  const parts = [{ label: `All ${lower(plural(info.levels[0] ?? 'values'))}`, depth: 0 }];
  info.path.forEach((step, i) => {
    const label = 'value' in step ? fmt(step.value ?? null, info.grains[i] ?? null) : `Every ${lower(info.levels[i] ?? 'value')}`;
    parts.push({ label, depth: i + 1 });
  });
  return { parts, showing: `by ${lower(info.levels[info.at] ?? '')}` };
}
