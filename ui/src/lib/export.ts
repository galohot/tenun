// The snapshot a dashboard is exported or published as. Built by the Rust
// engine (`tenun-core::snapshot`); the viewer reads it.

import type { Dashboard, Grain, Hierarchy, NumFormat, Param, QueryResult, SiteSettings, Table } from './api';
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
  /** A story's layout and order (build 11). Not set: a grid page. */
  layout?: 'grid' | 'flow';
  order?: string[];
  /** Link id → what the link is called, for the links the tiles name. Not set when none does. */
  links?: Record<string, string>;
  /** The named measures the tiles use: their name and format, the type of the value and whether the values add up. Never the formula. */
  measures?: { id: string; name: string; format?: NumFormat | null; dtype?: string | null; additive?: boolean }[];
  /**
   * Drill-down offline (v5 build 01, V5-D5): `down[tile][valueKey]` is the level below the top for that value, and
   * `down[tile]["\u0000all"]` the level below for every value. One level only.
   */
  down?: Record<string, Record<string, QueryResult>>;
  /** The hierarchies the drilled tiles use. */
  hierarchies?: Hierarchy[];
  /** Filters from other pages' filter tiles that apply to every page (v5 build 02). The numbers already have them. */
  every?: DashFilter[];
  /** Saved views (v5 build 03): each one as the dashboard reads in it, and the one it opens on. */
  views?: { id: string; name: string; payload: ExportPayload }[];
  defaultView?: string;
  /** Parameters (v5 build 05): with the values this page shows. Never their descriptions. */
  params?: Param[];
  /** `steps[param][valueKey][tile]`: the tiles that read a parameter, at each of its other values (V5-D5, at most 21). */
  steps?: Record<string, Record<string, Record<string, QueryResult>>>;
}

/**
 * A whole site in one file (build 13, V4-D15): every page as an exported page is, the site's navigation, and the pages
 * each drill-through opens, kept to a clicked value, for the first 50 values (V4-D16). A site saved as a folder holds
 * one page in each file (`page`) and names the files (`files`).
 */
export interface SitePayload {
  version?: number;
  kind: 'site';
  title: string;
  workspace: string;
  locale?: 'id' | 'en';
  generated: string;
  site: SiteSettings;
  /** Every page, in the order of the dashboards; hidden ones too, since links reach them. */
  order: string[];
  titles: Record<string, string>;
  hidden?: string[];
  pages: Record<string, ExportPayload>;
  /** By `<page id>/<tile id>`: the page a click opens, that page for each clicked value's key, and whether there are more values than the file holds. */
  drill: Record<string, { page: string; values: Record<string, ExportPayload>; more: boolean }>;
  /**
   * Every-page filters offline (v5 build 02): by `<page id>/<filter tile id>`, for each value's key the results of every
   * other page's tiles with that value; `more` when the field has more values than the file holds (50).
   */
  every?: Record<string, { page: string; tile: string; more: boolean; values: Record<string, Record<string, Record<string, QueryResult>>> }>;
  geo?: ExportPayload['geo'];
  /** A folder: the file of each page, and the page this file holds. */
  files?: Record<string, string>;
  page?: string;
}
