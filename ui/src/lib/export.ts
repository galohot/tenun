// The snapshot a dashboard is exported or published as. Built by the Rust
// engine (`tenun-core::snapshot`); the viewer reads it.

import type { Dashboard, Grain, NumFormat, QueryResult, Table } from './api';
import type { DashFilter } from './tiles';

export interface ExportPayload {
  version?: number;
  title: string;
  workspace: string;
  /** How the author's numbers and dates read. Older files have none and read as English. */
  locale?: 'id' | 'en';
  generated: string;
  tables: Pick<Table, 'id' | 'name' | 'label' | 'columns'>[];
  tiles: Dashboard['tiles'];
  /** v1 files only; newer files let the viewer name the tiles. */
  titles?: Record<string, string>;
  base: Record<string, QueryResult>;
  /** The map shapes its map tiles draw (build 06): TopoJSON by file name, downloaded GeoJSON by `<country>-<level>`. */
  geo?: Record<string, unknown>;
  /** cross[fromTile][valueKey][tile] = result with that value picked. */
  cross: Record<string, Record<string, Record<string, QueryResult>>>;
  /** The marks that were picked when the file was made. */
  filters: DashFilter[];
  /** Filters the dashboard keeps ("Keep only", "Exclude"). Every number in the file already has them applied. */
  saved?: DashFilter[];
  /** The one grain the dashboard set for every chart over time, if it set one. */
  grain?: Grain | null;
  /** Link id → what the link is called, for the links the tiles name. Not set when none does. */
  links?: Record<string, string>;
  /** The named measures the tiles use: their name and format, the type of the value and whether the values add up. Never the formula. */
  measures?: { id: string; name: string; format?: NumFormat | null; dtype?: string | null; additive?: boolean }[];
}
