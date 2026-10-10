// Tiles are computed by the Rust engine — the same code that builds exported
// files, published dashboards and the headless agent's snapshots.

import { api, type Grain, type QueryResult, type Tile } from './api';
import type { DashFilter } from './tiles';

export async function runTile(tile: Tile, filters: DashFilter[]): Promise<QueryResult> {
  return api.tile(tile, filters);
}

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const SHORT = MONTHS.map((m) => m.slice(0, 3));

function parse(s: string): Date {
  const [y, m, d] = s.slice(0, 10).split('-').map(Number);
  return new Date(Date.UTC(y, m - 1, d));
}

function periodName(start: Date, g: Grain): string {
  const y = start.getUTCFullYear();
  const m = start.getUTCMonth();
  switch (g) {
    case 'year':
      return String(y);
    case 'quarter':
      return `Q${Math.floor(m / 3) + 1} ${y}`;
    case 'month':
      return `${MONTHS[m]} ${y}`;
    case 'week':
      return `Week of ${SHORT[m]} ${start.getUTCDate()}, ${y}`;
    default:
      return `${SHORT[m]} ${start.getUTCDate()}, ${y}`;
  }
}

/** "* October 2026 has data up to Oct 14." for a chart whose last period is unfinished. */
export function partialNote(res: QueryResult, grain: Grain | null | undefined): string | null {
  if (!res.partial || !grain) return null;
  const until = parse(res.partial.until);
  return `* ${periodName(parse(res.partial.key), grain)} has data up to ${SHORT[until.getUTCMonth()]} ${until.getUTCDate()}.`;
}
