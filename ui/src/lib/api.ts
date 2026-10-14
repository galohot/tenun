// Typed calls to the Tenun server. Every change sends X-Tenun, which the server
// requires so that another web page cannot post to it.

import type { ExportPayload } from './export';

export type DType = 'number' | 'date' | 'datetime' | 'text' | 'bool';

export interface Column {
  name: string;
  dtype: DType;
  raw_type: string;
  /** The name shown in the app (from the column override). Queries use `name`. */
  label?: string;
  /** Left out of the field pickers. Still there for relationships. */
  hidden?: boolean;
  /** A tooltip in the field pickers. */
  description?: string;
  /** The share of blank cells, 0–1. Not set when nothing is blank or the data is read live. */
  blank?: number;
  /** The file had text and numbers (or dates) mixed in this column. */
  mixed?: boolean;
  /** After a type override: how many cells could not be read as that type and became blank. */
  failed?: number;
  /** A calculated column: a formula of the table's setup, not a column of the file (V4-D3). */
  calc?: boolean;
  /** A calculated column that could not be computed: the reason, in words. Its values are blank. */
  error?: string;
  /** A calculated column that uses TODAY() or NOW(): the day its view was built, `2026-10-11`. */
  as_of?: string;
}

export interface Table {
  id: string;
  source_id: string;
  name: string;
  label: string;
  columns: Column[];
  rows: number | null;
  /** What was detected and fixed when the sheet was read: "Header on row 4", "Filled down Region". */
  notes?: string[];
  /** Excel: the sheet row that holds the column names. */
  header_row?: number;
  /** The data is local, so its columns can be re-typed and turned into rows. */
  prep?: boolean;
}

export type DateFormat = 'auto' | 'dmy' | 'mdy';

/** What the user changed about one column of a table. */
export interface ColumnSetup {
  label?: string;
  dtype?: DType;
  /** How dates written as text are read. Only with `dtype` date or datetime. */
  date_format?: DateFormat;
  hidden?: boolean;
  description?: string;
}

export type Transform = { kind: 'unpivot'; keep: string[]; name: string; value: string };

/** A column that is computed from the others, in the view of its table. */
export interface CalcColumn {
  /** The name of the column. Tiles keep it, so it does not change after the column is made. */
  name: string;
  /** A formula, or raw DuckDB SQL when `sql` is set (Advanced). */
  formula: string;
  sql?: boolean;
  description?: string;
}

/** How one table is prepared. Stored in the workspace by table id. */
export interface TableSetup {
  /** Excel: the sheet row that holds the column names. Overrides detection. */
  header_row?: number;
  columns?: Record<string, ColumnSetup>;
  transforms?: Transform[];
  /** Calculated columns, in order. A column may use an earlier one, never a later one. */
  calc?: CalcColumn[];
  /** The date column a measure uses to line this table up with others over time. Not set: its only date column. */
  mainDate?: string;
  /** The table only holds links between other tables (programme, funder): a query may cross it from its one side, which makes many to many work. */
  linkTable?: boolean;
  /** The person said the table is not a link table: the Model view stops suggesting the mark. A marked table never has it. */
  notLinkTable?: boolean;
}

/** The place of a problem in a formula, in UTF-16 units: `formula.slice(at, at + len)` is the culprit. */
export interface FormulaError {
  message: string;
  at: number;
  len: number;
}

/** The answer of `POST /api/formula/check`. A wrong formula is a normal answer (`ok: false`). */
export interface FormulaCheck {
  ok: boolean;
  dtype?: DType;
  error?: FormulaError;
  /** The first 5 values of a column. */
  preview?: Cell[];
  /** The ids of the tables the formula reads. */
  uses?: string[];
}

export interface FormulaCheckRequest {
  formula: string;
  kind: 'column' | 'measure' | 'group';
  table?: string;
  /** The calculated column being edited: the columns of the table from it on are not there. */
  before?: string;
  /** The formula is raw DuckDB SQL (Advanced). */
  sql?: boolean;
}

/** A calculated column that cannot be saved. Nothing was saved. */
export class SetupRefused extends Error {
  /** The index of the column in the list that was sent. */
  calc: number;
  column: string;
  error: FormulaError;
  constructor(r: { calc: number; name: string; error: FormulaError }) {
    super(r.error.message);
    this.calc = r.calc;
    this.column = r.name;
    this.error = r.error;
  }
}

/** A wide sheet: months, quarters or years across the top. */
export interface UnpivotPlan {
  keep: string[];
  columns: string[];
  name: string;
  value: string;
  /** Every header has a year, so the new name column reads as a date. */
  dates: boolean;
}

export interface Profile {
  rows: number;
  /** The share of blank cells, 0–1. */
  blank: number;
  distinct: number;
  min?: Cell;
  max?: Cell;
  /** Text and yes/no columns: the 5 most common values. */
  top?: { value: string; count: number }[];
  /** Number and date columns: counts in 12 equal bins from min to max. */
  histogram?: number[];
  mixed: boolean;
}

import type { Calendar } from './locale';
export type { Calendar };

export interface FieldRef {
  table: string;
  column: string;
}

/** Fields from the top down (Region › Store), 2 to 6 of them, for drill-down in a chart (v5 build 01). */
export interface Hierarchy {
  /** `h<n>`; empty for a new one. */
  id: string;
  name: string;
  levels: FieldRef[];
}

/** One step down a drill: `value` keeps only that value of its level; no `value` ("Next level") shows every one. */
export interface DrillStep {
  value?: Cell;
}

/** Where the reader drilled to: one step for each level they went down. */
export interface DrillState {
  path: DrillStep[];
}

/** What a tile that drills says about it: the names of its levels, their date grains, the level shown and the path. */
export interface DrillInfo {
  levels: string[];
  grains: (Grain | null)[];
  at: number;
  path: DrillStep[];
  canDown: boolean;
}

export interface Relationship {
  id: string;
  /** The many side. */
  from: FieldRef;
  /** The one side. */
  to: FieldRef;
  /** A name for the link ("Shipped from"). Two links that join the same two tables each have one. */
  name?: string;
}

/**
 * What the data says about a link, made when its tables load. `kind` reads from the one side:
 * `one-many` is the usual link, `one-one` works both ways, `many-one` means the stored way stopped
 * working so the link reads the other way round, and `many-many` is not used.
 */
export interface LinkHealth {
  oneToOne: boolean;
  /** Rows on the one side beyond the first of each key. */
  duplicates: number;
  /** Up to five of the keys that repeat, and how many rows each has. */
  sample: Cell[];
  sampleRows?: number[];
  /** How many keys repeat. */
  keys?: number;
  /** No key repeats on the many side, so the link also works from the one side. */
  fromUnique: boolean;
  kind: 'one-many' | 'one-one' | 'many-one' | 'many-many';
  unusable?: boolean;
  /** The warning in plain words. Not set when the link is healthy. */
  note?: string;
}

/** How often a web link is fetched again. */
export type UrlEvery = '15min' | 'hourly' | 'daily' | 'manual';

/** Where a link sends its key. */
export type KeyPlace = 'header' | 'query';

/** The place and name of a link's key. The value is kept in the secrets file and never comes back. */
export interface LinkKey {
  place: KeyPlace;
  name: string;
}

/** How a JSON link finds its next page. */
export type Paging =
  | { by: 'link' }
  | { by: 'field'; path: string }
  | { by: 'cursor'; path: string; param: string }
  | { by: 'page'; param: string };

/** What the Add sheet shows after a look at the first page of a link. */
export interface LinkLook {
  format: 'csv' | 'json';
  /** JSON: every list of records in the answer. `.` is the answer itself. */
  lists: { path: string; rows: number }[];
  /** JSON: the list that was read. */
  records: string;
  columns: string[];
  /** The first 5 rows. */
  rows: string[][];
  /** Rows on the first page. */
  total: number;
  /** `link`, `field:<path>`, `cursor:<path>`, `page`, or empty. */
  pages_hint: string;
}

export interface PastePreview {
  columns: string[];
  rows: string[][];
  /** Rows of data, not counting the header. */
  total: number;
}

export interface Source {
  id: string;
  name: string;
  kind: 'excel' | 'csv' | 'parquet' | 'sqlite' | 'duckdb' | 'folder' | 'url' | 'postgres' | 'mysql' | 'derived';
  /** A new table (build 04): how it is made from other tables. Its one table is called `name`. */
  spec?: DerivedSpec;
  path?: string;
  /** Folder: the file name patterns, separated by `;`. */
  pattern?: string;
  /** Web link. */
  url?: string;
  every?: UrlEvery;
  /** JSON link: where the list of records sits. Empty: Tenun picks. */
  records?: string;
  /** JSON link: how the next page is found. Empty: one page. */
  pages?: Paging;
  /** JSON link: where the key goes and what it is called. */
  key?: LinkKey;
  host?: string;
  database?: string;
}

/** How a new table is made from other tables (build 04, V4-D6). Table ids name the inputs. */
export type DerivedSpec =
  | { op: 'append'; tables: string[] }
  | { op: 'merge'; left: string; right: string; on: [string, string][]; keep: 'left' | 'inner' }
  | { op: 'group'; table: string; by: string[]; values: GroupValue[] };

/** One value of a grouped table: a column name and a formula of totals, such as SUM([amount]). */
export interface GroupValue {
  name: string;
  formula: string;
}

/** A look at a new table before it is made. */
export interface DerivedPreview {
  columns: Column[];
  rows: Cell[][];
  /** How many rows the table has. */
  total: number;
  notes?: string[];
}

/** `median` and `p90` (the 90th percentile) take numbers only. `stdev` and `variance` are Advanced (see tiers.ts). */
export type Agg = 'sum' | 'avg' | 'min' | 'max' | 'count' | 'count_distinct' | 'median' | 'p90' | 'stdev' | 'variance';
export type Grain = 'year' | 'quarter' | 'month' | 'week' | 'day' | 'month_of_year' | 'weekday' | 'hour';
/** Date parts come back as numbers: months 1–12, weekdays 1–7 (Monday = 1), hours 0–23. */
export const DATE_PARTS: readonly Grain[] = ['month_of_year', 'weekday', 'hour'];
export const isPart = (g: Grain | null | undefined): boolean => !!g && DATE_PARTS.includes(g);

export interface Measure {
  /** A named measure has no table: its formula says what it reads. */
  table: string;
  column: string | null;
  agg: Agg;
  /** Makes a ratio: agg(column) divided by agg(per). A zero below gives a gap. */
  per?: { table: string; column: string | null; agg: Agg; via?: string };
  /** The link (a relationship id) the path to `table` crosses, when two links lead to it. Not set: the first. */
  via?: string;
  /**
   * A measure of the workspace (`NamedMeasure.id`). Then `table`, `column` and `agg` mean nothing: the
   * formula says what is added up. A tile stores `{ named, table: '', column: null, agg: 'sum' }`.
   */
  named?: string;
}

/** A formula of totals with a name, which any tile can use. It may add up tables from different sources. */
export interface NamedMeasure {
  /** `m<n>`. A tile keeps this id, so the name can change. A new measure is sent with an empty id. */
  id: string;
  name: string;
  formula: string;
  /** How the value is written. A tile's own format wins. */
  format?: NumFormat;
  description?: string;
}

/** What the engine knows about a measure: its type, whether its values add up, and why it does not work. */
export interface MeasureInfo {
  dtype?: DType;
  /** The values of the groups add up to the whole (a sum, a count, a difference of them). A share or a running total needs it. */
  additive: boolean;
  /** The ids of the tables the formula adds up. */
  tables: string[];
  error?: string;
}

export interface Dim {
  table: string;
  column: string;
  grain?: Grain | null;
  /** The link the path to `table` crosses (see `Measure.via`). */
  via?: string;
}

export interface Filter {
  table: string;
  column: string;
  grain?: Grain | null;
  /** The link the path to `table` crosses. A click on a mark carries the link of its field. */
  via?: string;
  op: 'in' | 'not_in' | 'between' | 'gte' | 'lte' | 'range' | 'contains' | 'starts_with' | 'blank' | 'not_blank' | 'relative';
  values: (string | number | boolean | null)[];
}

/**
 * A filter on a dashboard. `fromTile` is the tile it came from, which it does not filter: a click on
 * a mark, or a filter tile's selection. A filter the dashboard saves ("Keep only", "Exclude") names
 * no tile, so it filters them all. `label` is what a chip says when the values alone do not tell.
 */
export interface DashFilter extends Filter {
  fromTile: string;
  label?: string;
  /** The tiles it filters (v5 build 02): the reach of the tile it came from. `[]`: none. Not set: every tile. */
  to?: string[];
}

export interface SemanticQuery {
  measures: Measure[];
  dimensions: Dim[];
  filters?: Filter[];
  sort?: { by: number; desc: boolean }[];
  limit?: number;
}

/** What a value shown another way counts. A percent is a fraction: 0.25 reads as 25 %. */
export type Unit = 'percent' | 'rank';

export interface OutColumn {
  name: string;
  role: 'dimension' | 'measure';
  dtype: DType;
  /** Set by "Show as": the value is a share (`percent`) or a place in the ranking (`rank`). */
  unit?: Unit;
}

export type Cell = string | number | boolean | null;

export interface QueryResult {
  columns: OutColumn[];
  rows: Cell[][];
  ignored_filters: number[];
  ms: number;
  /** Number tiles only: the computed figure and its comparison. */
  kpi?: KpiInfo;
  /** Time charts: the last period is not over yet (data runs to `until`). */
  partial?: { key: string; until: string };
  /** The last group is "Other" (key `\u0000other`): everything outside the top N. */
  other?: boolean;
  /** The groups that are not in "Other", for a `not_in` filter. */
  otherOf?: Cell[];
  /** Histograms: bin `i` covers `[start + i·width, start + (i+1)·width)`. */
  bins?: Bins;
  /** Tables with a total row, and pivots. Computed by the engine from the same measures. */
  totals?: Totals;
  /** Time charts with `tile.compare`: the earlier series, its keys moved forward to line up with `rows`. */
  compare?: { label: string; rows: Cell[][] };
  /** Tables only, when the row limit cut the result: how many rows there are in all. */
  total_rows?: number;
  /** What the reader should know about the links this result crossed: a key that repeats on the one side, so one row of it was used. Names the keys and the files; stays in the app. */
  notes?: string[];
  /** The same notes with no key value and no file name, for a picture or a file that leaves the computer. */
  publicNotes?: string[];
  /** Small multiples: the repeat values that have a chart (drawing order) and how many more there are. Rows are `[group, repeat value, value]`. */
  repeat?: { keys: Cell[]; more: number };
  /** Indexed lines: the rows before they were divided by the first value, same shape as `rows`. */
  real?: Cell[][];
  /** Box plots: the outlier dots, `[group, value]`. The rows are `[group, min, q1, median, q3, max, lower whisker, upper whisker, count]`. */
  outliers?: Cell[][];
  /** Tables with a trend column: for each row key (`sparkKey`), the last 12 periods of the first value. */
  sparks?: Record<string, (number | null)[]>;
  /** Compare charts over two periods: the two periods, in the order of the two value columns. */
  pair?: { periods: Cell[] | null };
  /** Named measures: a group or a filter that one table of the formula cannot reach, so it is left out of it. The words are also in `notes`. */
  parts?: { measure: number; note: string; repeats?: boolean; overlap?: boolean; splitOnly?: boolean }[];
  /** Map tiles: the place of each row and the names not on the map (build 06). */
  geo?: GeoInfo;
  /** A tile that drills (v5 build 01): its levels, the one shown, the path to it. */
  drill?: DrillInfo;
  /** Text tiles: live numbers by `<measure id>`, `<id>|compare` and `<id>|period` (build 10). */
  values?: Record<string, Cell>;
  /** The parameters the tile read, by id (v5 build 05): an export holds its results for each of their values. */
  params?: string[];
}

/**
 * Every entry holds measure values in the order of the tile's measures. `rows` entries start with
 * the values of the row fields, `cols` entries with the column value. `cols` also lists the column
 * values in the engine's order.
 */
export interface Totals {
  grand: Cell[];
  /** A pivot with groups that open and close adds one entry for each group: fewer row values than a row has. */
  rows?: Cell[][];
  cols?: Cell[][];
  /** Expandable pivot: the cells of each group over each column. The group values, the column value, then the measures. */
  groups?: Cell[][];
}

export interface Bins {
  start: number;
  width: number;
}

export interface KpiInfo {
  value: number | null;
  /** What the value covers: "September 2026", "Oct 1–14, 2026", or "All data". */
  period: string;
  /** What it is compared with, or null when there is nothing to compare. */
  versus: string | null;
  prev: number | null;
  /** Relative change, e.g. 0.04 for +4 %. */
  delta: number | null;
  partial: boolean;
  /** The share of the period that has data, 0–1. 1 for a finished period; not set for "All data". */
  elapsed?: number;
  /** The last periods, oldest first, for the column chart. */
  bars: { label: string; value: number | null }[];
  /** The tile's other values (its measures after the first) for the same period: a card shows them (build 08). */
  extras?: (number | null)[];
}

export type TileType =
  | 'kpi' | 'bar' | 'line' | 'area' | 'share' | 'donut' | 'table'
  // Simple types added in v3.
  | 'scatter' | 'histogram' | 'waterfall' | 'pivot' | 'text' | 'image' | 'filter'
  // Advanced types. They always render; only editing them needs the Advanced switch (V3-D2).
  | 'heatmap' | 'funnel' | 'box' | 'bullet' | 'compare' | 'sankey' | 'calendar' | 'cohort' | 'gantt'
  // Simple types added in v4. `donut` draws a donut again (V4-D7); files of v1 to v3 drew it as `share`.
  | 'map' | 'pie'
  // Advanced, v5: a parameter's control (build 05).
  | 'param';

/** The level of a map: countries, the provinces of a country, or its second level (kabupaten and kota). */
export type MapLevel = 'country' | 'province' | 'regency';

/** A map tile's map (build 06). */
export interface MapSpec {
  level: MapLevel;
  /** ISO 3166-1 alpha-2 of the country, for a province or kabupaten map. Not set: Indonesia. */
  country?: string;
  style?: 'filled' | 'points';
  /** Points: the latitude and the longitude fields. */
  lat?: Dim;
  lon?: Dim;
}

/** What a map result says about its places. */
export interface GeoInfo {
  level?: MapLevel;
  country?: string;
  /** The code of each row's place, index-aligned with the rows. */
  codes?: (string | null)[];
  /** The names in the data of each row's place: what a click filters by. */
  raw?: Cell[][];
  /** The standard name of each row's place. */
  names?: string[];
  /** Names that are not on the map, with a hint when a name is two places. */
  unmatched?: { name: string; hint?: string }[];
  unmatchedCount?: number;
  /** Points whose latitude or longitude is not on the earth. */
  dropped?: number;
  /** Another country's boundaries are not on this computer yet. */
  download?: boolean;
}

/** How one value is written. Index-aligned with `Tile.measures`; every field is optional. */
export interface NumFormat {
  /** 0–4. Unset: the number as it is, up to 2 decimals. */
  decimals?: number;
  /** IDR, USD, EUR, SGD or MZN. */
  currency?: string;
  /** Short (1,2 jt) or full. Unset: short on charts and number tiles, full in tables. */
  compact?: boolean;
  /** Multiply by 100 and add %. */
  percent?: boolean;
}

/** How a value is shown next to the others. The engine computes it; a total cell is blank for any but `value`. */
export type ShowAs =
  | 'value' | 'pct_total' | 'pct_group' | 'running' | 'moving_3' | 'moving_7' | 'moving_12'
  | 'change' | 'pct_change' | 'rank' | 'ytd';

/**
 * A filter tile: its field is `dims[0]`. `values` (list, dropdown), `range` (range: lowest and highest
 * number; dates: first and last day, either may be null) and `preset` (dates) hold the selection. A
 * filter with none of them keeps everything.
 */
export interface TileFilter {
  style: 'list' | 'dropdown' | 'range' | 'dates';
  /** List and dropdown: pick one value at most. */
  single?: boolean;
  values?: Cell[];
  range?: [Cell, Cell];
  /** A relative date preset, or `custom` while the two days are being picked. */
  preset?: string;
  /** The selection filters every page of the workspace (v5 build 02, Advanced). */
  everyPage?: boolean;
}

/** A picture on a dashboard. */
export interface TileImage {
  /** A data URL: PNG, JPEG or WebP, at most 1 MB. */
  data: string;
  alt: string;
  fit?: 'contain' | 'cover';
}

/** A reference line on a chart. `band` (value to `to`) is Advanced. */
export interface TileLine {
  kind: 'target' | 'average' | 'median' | 'min' | 'max' | 'trend' | 'band';
  value?: number;
  to?: number;
  label?: string;
  /** Scatter only: `y` is a level line (a value up the side), `x` an upright one. Unset: both, for a median. */
  axis?: 'x' | 'y';
  /** A target line at a number parameter's value (v5 build 05). `value` is the value it had when it was set. */
  param?: string;
}

/** How the cells of a value look in a table or pivot. Index-aligned with `Tile.measures`. */
export interface CondFormat {
  /** `bars`: a thin bar behind the number. `goodbad`: the number is coloured, with a ▲ or ▼. */
  kind: 'bars' | 'goodbad';
  /** Good-bad: the value to compare with (0 when unset, and for a change). */
  threshold?: number;
  /** Good-bad: which direction is good. Default up. */
  better?: 'up' | 'down';
}

/** A part of a number card (build 08), drawn in order, top to bottom. `hidden` keeps a part's settings while it is not drawn. */
export type CardPart = (
  | { kind: 'value' }
  | { kind: 'title' }
  | { kind: 'period' }
  | { kind: 'compare' }
  | { kind: 'target' }
  | { kind: 'spark'; style?: 'bars' | 'line' }
  | { kind: 'divider' }
  /** Another value of the tile: an index into `Tile.measures`, 1 or more. */
  | { kind: 'extra'; measure: number; label?: string }
  | { kind: 'note'; text: string }
) & { hidden?: boolean };

/** When the value meets it, the card colours the value and adds words ("Below target"). The first that matches counts. */
export interface CardRule {
  op: '>' | '<' | 'between';
  value: number;
  to?: number;
  tone: 'good' | 'bad' | 'warn';
}

/**
 * A number card (GRANDPLAN §4.1). With a `style`, each field the card does not set comes from the style; so `parts` is
 * optional here, where the plan has it required.
 */
export interface CardSpec {
  /** A card style's id. */
  style?: string;
  parts?: CardPart[];
  align?: 'start' | 'center';
  /** The big value's size step. */
  size?: 's' | 'm' | 'l';
  ground?: 'plain' | 'tint' | 'accent';
  rules?: CardRule[];
}

/** A card look saved in the workspace and applied by its id. Editing it changes every card that uses it. */
export interface CardStyle {
  id: string;
  name: string;
  card: Omit<CardSpec, 'style'>;
}

export interface Tile {
  id: string;
  type: TileType;
  /** Empty = generated from the fields. */
  title: string;
  x: number;
  y: number;
  w: number;
  h: number;
  measures: Measure[];
  /** [group by, split by] for charts; any number for tables. */
  dims: Dim[];
  horizontal?: boolean;
  /** Number tiles: which direction of change is good. Default up. */
  better?: 'up' | 'down';
  /** `manual` is Advanced. */
  sort?: { by: 'value' | 'label' | 'manual'; desc?: boolean; order?: string[] };
  /** Keep the biggest N groups; the rest become one "Other" bar. `other` defaults to true. `param`: N is a number parameter's value (v5 build 05). */
  top?: { n: number; other?: boolean; param?: string };
  lines?: TileLine[];
  /** How each value is written, index-aligned with `measures`. */
  formats?: (NumFormat | null)[];
  /** Values written on the chart: Auto (the usual few), every bar or point, or none. */
  labels?: 'auto' | 'all' | 'none';
  /** Table and pivot: how the cells of each value look, index-aligned with `measures`. */
  cond?: (CondFormat | null)[];
  /** Table: the width in px of each column, from a drag on a heading. The engine ignores it. */
  widths?: number[];
  /** Small multiples (Advanced): one chart for each value of this field. */
  repeat?: Dim;
  /** Small multiples: every chart has its own scale. Not set: they share one. */
  ownScale?: boolean;
  /** Several measures: `panels` and `indexed` are Advanced. */
  layout?: 'together' | 'panels' | 'indexed';
  /** Bars plus a line, index-aligned with measures (Advanced). */
  mix?: ('bar' | 'line')[];
  /** Compare two periods: the two values of the second field to compare. Not set: the latest two. */
  periods?: Cell[];
  /** Table (Advanced): a trend column. The field gives the periods. */
  spark?: Dim;
  /** Pivot (Advanced): the groups of the row fields open and close, with a subtotal for each. */
  expand?: boolean;
  /** Bars and areas with a split or several values: pile them up, put them side by side, or fill the height. */
  stack?: 'stacked' | 'side' | 'percent';
  /** Histogram: the width of a bin. Not set: the engine picks one. */
  bins?: { width?: number };
  /** Number tiles: the goal for the period. `param`: the goal is a number parameter's value (v5 build 05). */
  target?: { value: number; param?: string };
  /** Number tiles: the card's parts, size, ground and rules, or a card style it uses (build 08). Not set: the number tile as it always was. */
  card?: CardSpec;
  /** Text tiles: the period the live numbers cover, set from the dashboard's grain (build 10). Not set: by month. */
  grain?: Grain | null;
  /** Text tiles: a style for the whole text, and its alignment (build 10). Not set: body, left. */
  textStyle?: 'body' | 'lead' | 'note';
  textAlign?: 'start' | 'center';
  /** A click on a mark opens this page, keeping only the clicked value (drill-through, build 12). Not set: it filters this page. */
  action?: { page: string };
  /** In a story (build 11): full or half width, and a chart's height. The grid place stays, for a switch back. */
  flow?: { width?: 'full' | 'half'; height?: 's' | 'm' | 'l' };
  /** Table: add a total row. Pivot: totals are on unless this is false. */
  totals?: boolean;
  /** Pivot: the field whose values become the columns. The rows are `dims`. */
  cols?: Dim;
  /** How each value is shown, index-aligned with `measures`. */
  showAs?: (ShowAs | null)[];
  /** Line, area and bar over time: also draw the same question one year (or one period) earlier. */
  compare?: 'prev_year' | 'prev_period' | null;
  /** Filter tile: its style and what it has selected. */
  filter?: TileFilter;
  /** Text tile: a small Markdown subset (see markdown.ts). */
  text?: string;
  /** Image tile. */
  image?: TileImage;
  /** Map tile: the level, the country, filled or points (build 06). */
  map?: MapSpec;
  /** Drill-down (v5 build 01, Advanced): a hierarchy, or `{}` for the date of the first field (year › quarter › month › day). */
  drill?: { hierarchy?: string };
  /** The tiles its clicks and selections filter (v5 build 02, Advanced): none, or a list. Not set: every tile. */
  reach?: 'none' | string[];
  /** Parameter tiles (v5 build 05): the parameter it moves, and its control. Not set: a slider or buttons. */
  param?: { id: string; style?: 'slider' | 'input' | 'buttons' | 'dropdown' };
}

/** Settings of this computer (not of the workspace). */
export interface Settings {
  /** Show the editing controls for Advanced features. */
  advanced: boolean;
}

export interface PublishSettings {
  /** Set by the publisher: the cloud dashboard (or site), its link, the last success and the last error. */
  id?: string;
  url?: string;
  last?: string;
  error?: string;
  access: 'link' | 'invited';
  invites: string[];
  every: 'manual' | 'change' | 'hour' | 'day';
  /** Overrides this computer's name on the hosted page. */
  source: string;
}

export interface Dashboard {
  id: string;
  title: string;
  /** Seconds between refreshes of database data; 0 = off. */
  refresh: number;
  tiles: Tile[];
  publish?: PublishSettings;
  /** One grain for every time tile. Not set: each tile keeps its own ("Auto"). */
  grain?: Grain | null;
  /** Filters the dashboard keeps ("Keep only", "Exclude"). They apply to every tile. */
  filters?: DashFilter[];
  /** A grid of tiles, or a story that reads like a document (build 11). Not set: grid. */
  layout?: 'grid' | 'flow';
  /** A story's order of tiles, by id (build 11). Tiles it does not name follow in reading order. */
  order?: string[];
  /** Left out of a site's navigation (build 12). A link still reaches it. */
  hidden?: boolean;
  /** Saved views (v5 build 03): how the dashboard was read, by name. At most 10. */
  views?: SavedView[];
  /** The view the dashboard opens on. Not set: as it is saved. */
  defaultView?: string;
}

/** A saved view (v5 build 03): the clicks, the filter tiles' selections, the grain, the drill paths, the parameters. */
export interface SavedView {
  /** `v<n>`. */
  id: string;
  name: string;
  filters: DashFilter[];
  selections?: Record<string, Pick<TileFilter, 'values' | 'range' | 'preset'>>;
  grain?: Grain | null;
  drill?: Record<string, DrillStep[]>;
  params?: Record<string, Cell>;
}

/** The workspace as a site (build 12, GRANDPLAN §4.1). Off by default (V4-D14). */
export interface SiteSettings {
  on: boolean;
  title: string;
  nav: 'top' | 'side';
  /** The page the site opens on. Not set: the first page in the navigation. */
  home?: string;
  groups?: { name: string; pages: string[] }[];
  /** One line of plain text under every page. */
  footer?: string;
  /** How the whole site is published to tenun.cloud (build 14), as a dashboard's are. */
  publish?: PublishSettings;
}

/** The raw rows behind a tile or one of its marks. */
export interface RowsBehind {
  columns: { label: string; dtype: DType }[];
  rows: Cell[][];
  /** How many rows match. `rows` holds the first 500. */
  total: number;
}

export interface CloudStatus {
  base: string;
  connected: boolean;
  email: string | null;
  source: string;
  /** Free-plan usage; null when tenun.cloud could not be reached. */
  usage: { dashboards: number; limit: number } | null;
}

export interface Workspace {
  version: number;
  name: string;
  sources: Source[];
  relationships: Relationship[];
  dashboards: Dashboard[];
  layout: { model?: Record<string, { x: number; y: number }> } | null;
  /** How numbers and dates read. A file without it reads as English. */
  locale?: 'id' | 'en';
  /** How each table is prepared, by table id. */
  tables?: Record<string, TableSetup>;
  /** Named measures, in the order they were made. */
  measures?: NamedMeasure[];
  /** Card styles of number tiles (build 08). */
  cardStyles?: CardStyle[];
  /** The workspace as a site (build 12). Not set: not a site. */
  site?: SiteSettings;
  /** Hierarchies for drill-down (v5 build 01). */
  hierarchies?: Hierarchy[];
  /** The month the year starts in and the day the week starts on (v5 build 04). Not set: January and Monday. */
  calendar?: Calendar;
  /** Parameters (v5 build 05): numbers and choices that measures read with PARAM("…"), with their values now. */
  params?: Param[];
}

/**
 * A parameter (v5 build 05, V5-D7): a number from `min` to `max` in steps of `step`, or a choice from `choices`. A
 * measure reads it with PARAM("name"); a Top N, a number tile's target and a chart's line can take it. A new one is
 * sent with an empty id.
 */
export interface Param {
  id: string;
  name: string;
  kind: 'number' | 'choice';
  value: Cell;
  min?: number;
  max?: number;
  step?: number;
  choices?: Cell[];
  format?: NumFormat;
  description?: string;
}

export interface Snapshot {
  workspace: Workspace;
  path: string | null;
  tables: Table[];
  /** Source id → why it did not load. */
  errors: Record<string, string>;
  /** Table id → the calculated columns that could not be computed, in words. Their values are blank. */
  table_errors?: Record<string, string>;
  /** Source id → when it last loaded without error (ISO 8601, UTC). */
  loaded_at?: Record<string, string>;
  /** Ids of the web links whose key has a value on this computer. */
  link_keys?: string[];
  /** Relationship id → what the data says about the link. Not set for a link that was not checked. */
  health?: Record<string, LinkHealth>;
  /** Link id → why its name no longer works: a table now has that name, or another link does. */
  link_clashes?: Record<string, string>;
  /** Measure id → its type, whether its values add up, the tables it adds up and why it does not work. */
  measure_info?: Record<string, MeasureInfo>;
  dirty: boolean;
}

export interface Suggestion {
  from: FieldRef;
  to: FieldRef;
  score: number;
  reason: string;
}

export interface Health {
  ok: boolean;
  version: string;
  duckdb: string;
  native: boolean;
}

export interface FsListing {
  path: string;
  parent: string | null;
  entries: { name: string; path: string; dir: boolean; size: number; supported: boolean }[];
  places: { name: string; path: string }[];
}

/** A key as the sheet sends it. */
export interface KeyIn extends LinkKey {
  /** Empty: use the value that is stored already, if there is one. */
  value: string;
}

/** The optional settings of a web link when it is added. */
export interface LinkSettings {
  records?: string;
  pages?: Paging | null;
  key?: KeyIn | null;
}

/** A measure that cannot be saved. Nothing was saved. */
export class MeasureRefused extends Error {
  /** The index of the measure in the list that was sent. */
  measure: number;
  name: string;
  /** What the message is about: the `name`, the `formula`, or the `list` (too many measures, a repeated id). */
  field: 'name' | 'formula' | 'list';
  error: FormulaError;
  constructor(r: { measure: number; name: string; field?: 'name' | 'formula' | 'list'; error: FormulaError }) {
    super(r.error.message);
    this.measure = r.measure;
    this.name = r.name;
    this.field = r.field ?? 'formula';
    this.error = r.error;
  }
}

/** A tile that uses a measure. */
export interface TileRef {
  dashboard: string;
  dashboardTitle: string;
  tile: string;
  title: string;
  kind: string;
}

/** A measure that tiles use, and the tiles. The save waits for a yes. */
export interface MeasureUse {
  measure: string;
  name: string;
  tiles: TileRef[];
}

export class ApiError extends Error {}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: body === undefined ? { 'X-Tenun': '1' } : { 'Content-Type': 'application/json', 'X-Tenun': '1' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    throw new ApiError(text || res.statusText);
  }
  if (!res.ok) {
    throw new ApiError((data as { error?: string })?.error ?? res.statusText);
  }
  normalizeSnapshot(data);
  return data as T;
}

/**
 * A ratio in an older or hand-edited file may leave out `per.column`; it means a count of rows, like null.
 * The tiles are kept as JSON in the file, so the engine does not fill it in.
 */
export function normalizeSnapshot(data: unknown): void {
  const dashboards = (data as { workspace?: { dashboards?: Dashboard[] } } | null)?.workspace?.dashboards;
  for (const d of dashboards ?? []) {
    for (const t of d.tiles ?? []) {
      for (const m of t.measures ?? []) {
        if (m.per && m.per.column === undefined) m.per.column = null;
      }
    }
  }
}

export const api = {
  health: () => call<Health>('GET', '/health'),
  settings: () => call<Settings>('GET', '/settings'),
  saveSettings: (s: Settings) => call<Settings>('PUT', '/settings', s),
  workspace: () => call<Snapshot>('GET', '/workspace'),
  newWorkspace: () => call<Snapshot>('POST', '/workspace/new', { lang: navigator.language }),
  setLocale: (locale: 'id' | 'en') => call<Snapshot>('PUT', '/workspace/locale', { locale }),
  setCalendar: (calendar: Calendar) => call<Snapshot>('PUT', '/workspace/calendar', calendar),
  setParams: (list: Param[]) => call<Snapshot>('PUT', '/workspace/params', list),
  setParamValues: (values: Record<string, Cell>) => call<Snapshot>('PUT', '/workspace/params/values', values),
  open: (path: string) => call<Snapshot>('POST', '/workspace/open', { path }),
  save: (path?: string) => call<Snapshot>('POST', '/workspace/save', { path: path ?? null }),
  rename: (name: string) => call<Snapshot>('POST', '/workspace/rename', { name }),
  sample: () => call<Snapshot>('POST', '/sample'),
  addFile: (path: string) => call<Snapshot & { added: string }>('POST', '/sources', { kind: 'file', path }),
  addFolder: (path: string, pattern?: string) =>
    call<Snapshot & { added: string }>('POST', '/sources', { kind: 'folder', path, pattern: pattern || null }),
  addUrl: (url: string, every: UrlEvery, name?: string, link: LinkSettings = {}) =>
    call<Snapshot & { added: string }>('POST', '/sources', {
      kind: 'url',
      url,
      every,
      name: name || null,
      records: link.records || null,
      pages: link.pages ?? null,
      key: link.key ?? null,
    }),
  /** The first page of a web link: its format, the lists of records, the first rows. Saves nothing. */
  linkPreview: (req: { url: string; records?: string; key?: KeyIn | null }) =>
    call<LinkLook>('POST', '/link/preview', {
      url: req.url,
      records: req.records || null,
      key: req.key ?? null,
    }),
  /**
   * Changes the records, the pages and the key of a web link, and fetches it again. Leave `key` out
   * to keep the key as it is; `null` removes it. A key with no value keeps the value that is stored.
   */
  setLink: (id: string, body: { records?: string; pages?: Paging | null; key?: KeyIn | null }) =>
    call<Snapshot>('PUT', `/sources/${encodeURIComponent(id)}/link`, {
      records: body.records || null,
      pages: body.pages ?? null,
      ...(body.key === undefined ? {} : { key: body.key }),
    }),
  setUrlEvery: (id: string, every: UrlEvery) =>
    call<Snapshot>('PUT', `/sources/${encodeURIComponent(id)}/every`, { every }),
  /** The name, the first rows and the size of a pasted table. Fails when the text is not a table. */
  pastePreview: (text: string, header: boolean) => call<PastePreview>('POST', '/paste/preview', { text, header }),
  /** Writes a pasted table as a CSV file in the data folder and adds it as a source. */
  paste: (name: string, text: string, header: boolean) =>
    call<Snapshot & { added: string }>('POST', '/paste', { name, text, header }),
  addDatabase: (kind: 'postgres' | 'mysql', conn: Record<string, unknown>) =>
    call<Snapshot & { added: string }>('POST', '/sources', { kind, ...conn }),
  /** Makes a new table from other tables (build 04). */
  addDerived: (name: string, spec: DerivedSpec) =>
    call<Snapshot & { added: string }>('POST', '/sources', { kind: 'derived', name, spec }),
  /** Changes how a new table is made. Its name, links and tiles stay. */
  setDerived: (id: string, spec: DerivedSpec) => call<Snapshot>('PUT', `/sources/${encodeURIComponent(id)}/derived`, { spec }),
  /** The columns, first rows, size and notes of a new table before it is made. Saves nothing. */
  previewDerived: (spec: DerivedSpec) => call<DerivedPreview>('POST', '/derived/preview', { spec }),
  /** The standard names and codes of one level of one country (build 05). */
  places: (level: 'country' | 'province' | 'regency', country = '') =>
    call<{ level: string; country: string; places: { code: string; name: string; aliases: string[]; parent?: string }[] }>(
      'GET',
      `/geo/places?level=${level}&country=${encodeURIComponent(country)}`,
    ),
  removeSource: (id: string) => call<Snapshot>('DELETE', `/sources/${encodeURIComponent(id)}`),
  reloadSource: (id: string) => call<Snapshot>('POST', `/sources/${encodeURIComponent(id)}/reload`),
  preview: (id: string, limit = 200) =>
    call<{ columns: Column[]; rows: Cell[][] }>('GET', `/tables/preview?id=${encodeURIComponent(id)}&limit=${limit}`),
  /** Sets how a table is prepared (header row, column overrides, unpivot); null clears it. */
  setTableSetup: async (table: string, setup: TableSetup | null): Promise<Snapshot> => {
    const r = await call<Snapshot | { ok: false; calc: number; name: string; error: FormulaError }>('PUT', '/tables/setup', { table, setup });
    // A calculated column that cannot be saved is a normal answer, not an HTTP error (V4-D35).
    if ('ok' in r && r.ok === false) throw new SetupRefused(r);
    return r as Snapshot;
  },
  /**
   * Saves the whole list of named measures. A new measure has an empty id. A formula that cannot be saved
   * throws `MeasureRefused`. A save that would delete measures that tiles use answers `{ used }` and saves
   * nothing, until it is sent again with `confirm`.
   */
  setMeasures: async (measures: NamedMeasure[], confirm = false): Promise<{ saved: Snapshot } | { used: MeasureUse[] }> => {
    const r = await call<Snapshot | { ok: false; measure: number; name: string; field?: 'name' | 'formula' | 'list'; error: FormulaError } | { ok: false; used: MeasureUse[] }>('PUT', '/workspace/measures', { measures, confirm });
    if ('ok' in r && r.ok === false) {
      if ('used' in r) return { used: r.used };
      throw new MeasureRefused(r);
    }
    return { saved: r as Snapshot };
  },
  /** Checks a formula and gives its type and first values, or the place of the problem. */
  checkFormula: (req: FormulaCheckRequest) => call<FormulaCheck>('POST', '/formula/check', req),
  profile: (table: string, column: string) =>
    call<Profile>('GET', `/profile?table=${encodeURIComponent(table)}&column=${encodeURIComponent(column)}`),
  /** The columns of a wide sheet that could turn into rows, or null. */
  unpivotPlan: (table: string) => call<UnpivotPlan | null>('GET', `/prep/unpivot?table=${encodeURIComponent(table)}`),
  unpivotPreview: (table: string, keep: string[], name: string, value: string) =>
    call<{ before: { columns: Column[]; rows: Cell[][] }; after: { columns: Column[]; rows: Cell[][] } }>(
      'POST',
      '/prep/unpivot/preview',
      { table, keep, name, value },
    ),
  addRelationship: (from: FieldRef, to: FieldRef) => call<Snapshot>('POST', '/relationships', { from, to }),
  removeRelationship: (id: string) => call<Snapshot>('DELETE', `/relationships/${encodeURIComponent(id)}`),
  /** Names a link, or clears its name (an empty name). A name must be new and cannot be a table's. */
  renameRelationship: (id: string, name: string) =>
    call<Snapshot>('PATCH', `/relationships/${encodeURIComponent(id)}`, { name }),
  /** Names several links as one set, so two links can swap their names. */
  renameRelationships: (names: Record<string, string>) => call<Snapshot>('PUT', '/relationships/names', { names }),
  /** Reads the links of live database tables again. The server tells the windows if an answer changed. */
  checkLinks: () => call<{ changed: boolean }>('POST', '/relationships/check'),
  suggest: () => call<Suggestion[]>('GET', '/relationships/suggest'),
  putDashboards: (d: Dashboard[]) => call<{ ok: boolean }>('PUT', '/dashboards', d),
  putCardStyles: (list: CardStyle[]) => call<{ ok: boolean }>('PUT', '/workspace/card-styles', list),
  putHierarchies: (list: Hierarchy[]) => call<Snapshot>('PUT', '/workspace/hierarchies', list),
  putSite: (site: SiteSettings) => call<{ ok: boolean }>('PUT', '/workspace/site', site),
  /** What an exported page holds (the site preview draws it, build 12). */
  snapshot: (dashboard: string, filters: DashFilter[] = []) => call<ExportPayload>('POST', '/snapshot', { dashboard, filters }),
  putLayout: (l: unknown) => call<{ ok: boolean }>('PUT', '/layout', l),
  query: (q: SemanticQuery) => call<QueryResult>('POST', '/query', q),
  fs: (path?: string) => call<FsListing>('GET', `/fs${path ? `?path=${encodeURIComponent(path)}` : ''}`),
  pick: (
    req: { kind: 'data_files' } | { kind: 'folder' } | { kind: 'image' } | { kind: 'open_workspace' } | { kind: 'save_workspace' | 'save_html' | 'save_xlsx' | 'save_zip'; name: string },
  ) =>
    call<{ paths: string[] }>('POST', '/pick', req),
  /** The picture the native dialog chose, for an image tile. */
  readImage: async (path: string): Promise<Blob> => {
    const res = await fetch('/api/image/read', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Tenun': '1' },
      body: JSON.stringify({ path }),
    });
    if (!res.ok) throw new ApiError((await res.json().catch(() => null))?.error ?? res.statusText);
    return res.blob();
  },
  openBrowser: () => call<{ ok: boolean }>('POST', '/open-browser'),
  tile: (tile: Tile, filters: unknown[], drill?: DrillState) => call<QueryResult>('POST', '/tile', { tile, filters, ...(drill ? { drill } : {}) }),
  /** The raw rows behind a tile; `click` is the filter a click on the mark makes. */
  rows: (tile: Tile, filters: unknown[], click: Filter | null) => call<RowsBehind>('POST', '/rows', { tile, filters, click }),
  cloud: () => call<CloudStatus>('GET', '/cloud'),
  cloudConnect: (open: boolean) => call<{ user_code: string; url: string; expires_in: number }>('POST', '/cloud/connect', { open }),
  cloudDisconnect: () => call<{ ok: boolean }>('POST', '/cloud/disconnect', {}),
  publish: (dashboard: string) => call<{ ok: boolean; url: string; version: number; snapshot: Snapshot }>('POST', '/cloud/publish', { dashboard }),
  unpublish: (dashboard: string) => call<Snapshot>('POST', '/cloud/unpublish', { dashboard }),
  /** The whole site, every page in one upload (build 14). */
  publishSite: () => call<{ ok: boolean; url: string; version: number; snapshot: Snapshot }>('POST', '/cloud/publish-site', {}),
  unpublishSite: () => call<Snapshot>('POST', '/cloud/unpublish-site', {}),
  /** Writes the HTML file (desktop app) when `path` is given. */
  exportTo: (dashboard: string, filters: unknown[], path: string) =>
    call<{ ok: boolean }>('POST', '/export-html', { dashboard, filters, path }),
  /** The Excel file of one tile. With `path` (desktop app) the server writes it there and nothing comes back. */
  exportXlsx: async (body: { tile: Tile; filters: unknown[]; headers: string[]; title: string; drill?: DrillState }, path?: string): Promise<Uint8Array> => {
    const res = await fetch('/api/export-xlsx', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Tenun': '1' },
      body: JSON.stringify({ ...body, path }),
    });
    if (!res.ok) throw new ApiError((await res.json().catch(() => null))?.error ?? res.statusText);
    return path ? new Uint8Array() : new Uint8Array(await res.arrayBuffer());
  },
  /**
   * The whole site (build 13): one HTML file, or a ZIP of a folder. With a path (the desktop app) the file is written
   * there. Gives the bytes (browser mode) and each page's size in bytes, by its title.
   */
  exportSite: async (zip: boolean, path?: string): Promise<{ bytes: Uint8Array; sizes: Record<string, number> }> => {
    const res = await fetch('/api/export-site', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Tenun': '1' },
      body: JSON.stringify({ zip, ...(path ? { path } : {}) }),
    });
    if (!res.ok) throw new ApiError((await res.json().catch(() => null))?.error ?? res.statusText);
    if (path) {
      const r = (await res.json()) as { sizes?: Record<string, number> };
      return { bytes: new Uint8Array(), sizes: r.sizes ?? {} };
    }
    let sizes: Record<string, number> = {};
    try {
      sizes = JSON.parse(res.headers.get('x-tenun-sizes') ?? '{}');
    } catch {
      sizes = {};
    }
    return { bytes: new Uint8Array(await res.arrayBuffer()), sizes };
  },
  /** Returns the HTML file's text (browser mode). */
  exportHtml: async (dashboard: string, filters: unknown[]): Promise<string> => {
    const res = await fetch('/api/export-html', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Tenun': '1' },
      body: JSON.stringify({ dashboard, filters }),
    });
    if (!res.ok) throw new ApiError((await res.json().catch(() => null))?.error ?? res.statusText);
    return res.text();
  },
};
