// Which features are Advanced (V3-D1, V3-D2). One place decides, so the Inspector can hide
// controls and show a notice. Rendering never reads the setting: a tile that uses Advanced
// features draws, cross-filters and exports the same in Simple mode.

import type { Tile, TileType } from './api';
import { app } from './state.svelte';

/** While false, nobody can switch Advanced on and the switch is hidden. Build 12 turned it on. */
export const SHOW_ADVANCED_SWITCH: boolean = true;

export const ADVANCED_TYPES: readonly TileType[] = [
  'heatmap',
  'funnel',
  'box',
  'bullet',
  'compare',
  'sankey',
  'calendar',
  'cohort',
  'gantt',
  // A parameter's control (v5 build 05).
  'param',
];

/** The types in the second group of the picker, "More charts". */
export const MORE_CHARTS: readonly TileType[] = ['heatmap', 'funnel', 'box', 'bullet', 'compare', 'sankey', 'calendar', 'cohort', 'gantt'];

/** True when the tile needs an Advanced feature to look or behave as it does. */
export function usesAdvanced(tile: Tile): boolean {
  return (
    ADVANCED_TYPES.includes(tile.type) ||
    !!tile.repeat ||
    tile.layout === 'panels' ||
    tile.layout === 'indexed' ||
    !!tile.mix?.length ||
    tile.sort?.by === 'manual' ||
    tile.measures.some((m) => m.agg === 'stdev' || m.agg === 'variance' || m.agg === 'closing' || m.agg === 'opening') ||
    !!tile.lines?.some((l) => l.kind === 'band') ||
    // A third measure on a scatter sets the dot size.
    (tile.type === 'scatter' && tile.measures.length > 2) ||
    // A trend column in a table, and groups that open and close in a pivot.
    !!tile.spark ||
    !!tile.expand ||
    // A forecast (v5 build 08), unusual values (build 09).
    !!tile.forecast ||
    !!tile.unusual ||
    // A Top N, a target or a line that takes a parameter (v5 build 05).
    !!tile.top?.param ||
    !!tile.target?.param ||
    !!tile.lines?.some((l) => l.param)
  );
}

/** A Gantt chart needs a start and an end, so it is offered when some table has two date columns. */
export function ganttOffered(tables: readonly { columns: readonly { dtype: string }[] }[]): boolean {
  return tables.some((t) => t.columns.filter((c) => c.dtype === 'date' || c.dtype === 'datetime').length >= 2);
}

export function advancedOn(): boolean {
  return app.settings?.advanced === true;
}

/** For the Inspector: Advanced editing is locked for this tile. */
export function advancedLocked(tile: Tile): boolean {
  return !advancedOn() && usesAdvanced(tile);
}
