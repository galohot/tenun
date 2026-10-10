// Which features are Advanced (V3-D1, V3-D2). One place decides, so the Inspector can hide
// controls and show a notice. Rendering never reads the setting: a tile that uses Advanced
// features draws, cross-filters and exports the same in Simple mode.

import type { Tile, TileType } from './api';
import { app } from './state.svelte';

/** Build 12 turns this on. While false, nobody can switch Advanced on and the switch is hidden. */
export const SHOW_ADVANCED_SWITCH: boolean = false;

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
];

/** True when the tile needs an Advanced feature to look or behave as it does. */
export function usesAdvanced(tile: Tile): boolean {
  return (
    ADVANCED_TYPES.includes(tile.type) ||
    !!tile.repeat ||
    tile.layout === 'panels' ||
    tile.layout === 'indexed' ||
    !!tile.mix?.length ||
    tile.sort?.by === 'manual' ||
    tile.measures.some((m) => m.agg === 'stdev' || m.agg === 'variance') ||
    !!tile.lines?.some((l) => l.kind === 'band') ||
    // A third measure on a scatter sets the dot size (build 13).
    (tile.type === 'scatter' && tile.measures.length > 2)
  );
}

export function advancedOn(): boolean {
  return app.settings?.advanced === true;
}

/** For the Inspector: Advanced editing is locked for this tile. */
export function advancedLocked(tile: Tile): boolean {
  return !advancedOn() && usesAdvanced(tile);
}
