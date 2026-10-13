// Tiles are computed by the Rust engine — the same code that builds exported
// files, published dashboards and the headless agent's snapshots.

import { api, type DrillState, type Grain, type QueryResult, type Tile } from './api';
import { dayMonth, periodName } from './format';
import type { DashFilter } from './tiles';

/** One tile; `drill` is where the reader drilled to (v5 build 01). */
export async function runTile(tile: Tile, filters: DashFilter[], drill?: DrillState): Promise<QueryResult> {
  return api.tile(tile, filters, drill);
}

function parse(s: string): Date {
  const [y, m, d] = s.slice(0, 10).split('-').map(Number);
  return new Date(Date.UTC(y, m - 1, d));
}

/** "* October 2026 has data up to Oct 14." for a chart whose last period is unfinished. */
export function partialNote(res: QueryResult, grain: Grain | null | undefined): string | null {
  if (!res.partial || !grain) return null;
  return `* ${periodName(parse(res.partial.key), grain)} has data up to ${dayMonth(parse(res.partial.until))}.`;
}
