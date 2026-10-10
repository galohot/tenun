// Tiles are computed by the Rust engine — the same code that builds exported
// files, published dashboards and the headless agent's snapshots.

import { api, type Grain, type QueryResult, type Tile } from './api';
import { dayMonth, periodName } from './format';
import type { DashFilter } from './tiles';

export async function runTile(tile: Tile, filters: DashFilter[]): Promise<QueryResult> {
  return api.tile(tile, filters);
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
