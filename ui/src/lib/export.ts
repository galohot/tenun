// The snapshot a dashboard is exported or published as. Built by the Rust
// engine (`tenun-core::snapshot`); the viewer reads it.

import type { Dashboard, QueryResult, Table } from './api';
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
  /** cross[fromTile][valueKey][tile] = result with that value picked. */
  cross: Record<string, Record<string, Record<string, QueryResult>>>;
  filters: DashFilter[];
}
