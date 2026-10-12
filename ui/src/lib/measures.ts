// Named measures (build 03): the helpers the app and the viewer share. Pure functions and one small
// registry, so the node tests can load this file. The engine decides what a measure means and whether
// its values add up (`Snapshot.measure_info`); this file only remembers the answer and asks it.

import type { Dashboard, Measure, MeasureInfo, NamedMeasure, NumFormat, Tile } from './api';

/** A workspace uses this many measures. Same as the engine. */
export const MAX_MEASURES = 200;
/** The longest formula of a measure, in characters. Same as the engine. */
export const MAX_FORMULA = 10_000;
export const MAX_MEASURE_NAME = 60;

// ---- what is known about the measures ----------------------------------------------------------

/**
 * The measures of the open workspace (or of the exported file), remembered so the plain functions that
 * write titles, labels and formats can use their names. Set whenever the snapshot is, or from the
 * `measures` of a viewer file, like the names of the links.
 */
let known: { id: string; name: string; format?: NumFormat; additive?: boolean }[] = [];

export function setKnownMeasures(list: readonly NamedMeasure[] | undefined, info?: Record<string, MeasureInfo>): void {
  known = (list ?? []).map((m) => ({ id: m.id, name: m.name, format: m.format, additive: info?.[m.id]?.additive }));
}

/** The viewer file carries the name, the format and whether the values add up, never the formula. */
export function setKnownFromFile(list: readonly { id: string; name: string; format?: NumFormat | null; additive?: boolean }[] | undefined): void {
  known = (list ?? []).map((m) => ({ id: m.id, name: m.name, format: m.format ?? undefined, additive: m.additive }));
}

export const isNamed = (m: Pick<Measure, 'named'> | undefined | null): boolean => !!m?.named;

/** The measure a tile stores to use a named measure. The other fields mean nothing then. */
export const namedMeasure = (id: string): Measure => ({ named: id, table: '', column: null, agg: 'sum' });

/** The name of a named measure. One that is gone reads as "A deleted measure", so a title never breaks. */
export function nameOf(id: string): string {
  return known.find((m) => m.id === id)?.name ?? 'A deleted measure';
}

/**
 * The values of this measure add up to the whole: a sum or a count of one table, or a sum of them. A share
 * of the whole, a running total and a pile of bars are right only for these. A plain measure is read from
 * how it is added up (a ratio, an average, a middle value and a distinct count are not). A named measure is
 * what the engine says; one the app has not heard of is not trusted.
 */
export function additive(m: Pick<Measure, 'agg' | 'per' | 'named'>): boolean {
  if (m.named) return known.find((x) => x.id === m.named)?.additive === true;
  return !m.per && (m.agg === 'sum' || m.agg === 'count');
}

// ---- the gate: does a value add up on this tile? (V4-D85d, V4-D99) --------------------------------------

/** What the engine says a measure could not follow (`QueryResult.parts`). Only `repeats` and `overlap` matter here. */
export type Parts = readonly { measure: number; repeats?: boolean; overlap?: boolean }[] | null | undefined;

/** A table of value `i` has the same number for every group of the tile (the engine says so in `parts`). */
export const repeatsAt = (parts: Parts, i: number): boolean => !!parts?.some((p) => p.measure === i && p.repeats);

/** A group of the tile sits behind a link table, so one row is under several groups (the engine says so in `parts`, V4-D112). */
export const overlapsAt = (parts: Parts, i: number): boolean => !!parts?.some((p) => p.measure === i && p.overlap);

/**
 * THE gate. Value `i` of the tile adds up here when its measure does, no table of it is repeated across the
 * groups of the tile, and no group sits behind a link table (V4-D112). A share of the whole, a running total,
 * folding the small bars into "Other", a waterfall, stacking, a pace and a share chart need it. The screen,
 * every picture and every export ask this one function.
 */
export function addsUpHere(tile: Pick<Tile, 'measures'>, i: number, parts?: Parts): boolean {
  const m = tile.measures[i];
  if (!m) return true;
  return additive(m) && !repeatsAt(parts, i) && !overlapsAt(parts, i);
}

/** The first value of the tile adds up here (or the tile has none, and counts rows). */
export const addsUp = (tile: Pick<Tile, 'measures'>, parts?: Parts): boolean => addsUpHere(tile, 0, parts);

const KIND_WORDS: Record<string, string> = {
  avg: 'Averages',
  min: 'Lowest values',
  max: 'Highest values',
  median: 'Medians',
  p90: '90th percentiles',
  count_distinct: 'Unique counts',
  stdev: 'Spreads',
  variance: 'Variances',
};

/**
 * Why value `i` is not added up, as the start of a sentence ("Averages are not added up"), or null when it adds
 * up. The app says this in one line wherever it leaves something out for it (V4-D99c).
 */
export function whyNotAdded(tile: Pick<Tile, 'measures'>, i: number, parts?: Parts): string | null {
  const m = tile.measures[i];
  if (!m || addsUpHere(tile, i, parts)) return null;
  const name = m.named ? nameOf(m.named) : null;
  if (additive(m)) {
    if (overlapsAt(parts, i)) return `${name ?? 'This value'} is not added up here, because one row can be in two groups`;
    return `${name ?? 'This value'} is not added up here, because a table it reads is the same for every group`;
  }
  if (name) return `${name} is not added up`;
  return `${m.per ? 'Ratios' : (KIND_WORDS[m.agg] ?? 'These values')} are not added up`;
}

/** The ways of showing a value that add values up. */
export const ADDING_WAYS: readonly string[] = ['pct_total', 'pct_group', 'running', 'ytd'];

const DATE_PARTS = ['month_of_year', 'weekday', 'hour'];

/** A waterfall steps over time when its first field is a date, or a part of a date (a month of the year). */
export function stepsByDate(tile: Pick<Tile, 'dims'>, res: { columns: { dtype: string }[] } | null | undefined): boolean {
  const first = res?.columns[0]?.dtype;
  return first === 'date' || first === 'datetime' || DATE_PARTS.includes(tile.dims[0]?.grain ?? '');
}

/** The tile types that draw the parts of a whole. A pie (build 07) is one: it needs nothing but this gate. */
const SHARE_TYPES: readonly string[] = ['share', 'donut', 'pie'];

/**
 * Why a tile cannot be drawn from its values, in words, or null (V4-D99). A share chart is the parts of a whole,
 * and a waterfall by category adds one category to the next. `byDate` is true for a waterfall stepped over
 * time, where each step is a difference, which any value has. Called by the tile on the screen and by every
 * picture of it.
 */
export function tileProblem(tile: Pick<Tile, 'type' | 'measures'>, parts?: Parts, byDate = false): string | null {
  if (tile.type === 'waterfall' && !byDate) {
    const why = whyNotAdded(tile, 0, parts);
    return why ? `A waterfall adds one category to the next. ${why}, so there are no steps to draw.` : null;
  }
  if (SHARE_TYPES.includes(tile.type)) {
    // Shares of a whole: the parts must add up to it. Rows in two groups would pass 100 % (V4-D112).
    if (tile.measures[0] && additive(tile.measures[0]) && overlapsAt(parts, 0) && !repeatsAt(parts, 0)) {
      return 'These shares would add up to more than 100 %, because one row can be in two groups.';
    }
    const why = whyNotAdded(tile, 0, parts);
    return why ? `${why}, so it has no shares. A share chart needs a sum or a count.` : null;
  }
  return null;
}

/**
 * The share of the period that has data, for the pace of a number tile. A pace is the value so far scaled up to
 * the whole period, which is right for a sum or a count; a ratio or an average so far is already the value of
 * the period, so it has no pace.
 */
export const paceElapsed = (tile: Pick<Tile, 'measures'>, elapsed: number | null | undefined, parts?: Parts): number | null | undefined =>
  addsUp(tile, parts) ? elapsed : null;

// ---- formats ---------------------------------------------------------------------------------------------

const ownOf = (tile: Pick<Tile, 'formats'>, i: number): NumFormat | null => tile.formats?.[i] ?? null;
/** The format of the named measure of value `i`, if it has one. */
export const baseFormat = (tile: Pick<Tile, 'measures'>, i: number): NumFormat | null => {
  const id = tile.measures[i]?.named;
  return (id && known.find((m) => m.id === id)?.format) || null;
};

/**
 * The format a tile writes a value in: the one of its named measure, with whatever the tile sets itself over it,
 * field by field (V4-D106). A tile that picks a currency keeps the decimals and the percent of the measure.
 */
export function formatOf(tile: Pick<Tile, 'formats' | 'measures'>, i: number): NumFormat | null {
  const own = ownOf(tile, i);
  const base = baseFormat(tile, i);
  if (!base) return own;
  return { ...base, ...(own ?? {}) };
}

/**
 * The format as the tile stores it when the user changes one field: only what differs from its measure.
 * A field the measure sets and the user clears is stored as a neutral value (no currency, not a percent), so
 * the tile can say no where the measure says yes; a field the user sets to what the measure says is dropped.
 * Swapping the measure then never leaves a copy of the old measure's format behind.
 */
export function ownFormat(base: NumFormat | null, own: NumFormat | null, patch: Partial<NumFormat>): NumFormat | null {
  const next: NumFormat = { ...(own ?? {}) };
  if ('currency' in patch) {
    const v = patch.currency || '';
    if (v && v !== base?.currency) next.currency = v;
    else if (!v && base?.currency) next.currency = '';
    else delete next.currency;
  }
  if ('decimals' in patch) {
    // Automatic cannot undo the decimals of the measure; it goes back to them.
    if (patch.decimals !== undefined && patch.decimals !== base?.decimals) next.decimals = patch.decimals;
    else delete next.decimals;
  }
  if ('compact' in patch) {
    if (patch.compact !== undefined && patch.compact !== base?.compact) next.compact = patch.compact;
    else delete next.compact;
  }
  if ('percent' in patch) {
    if (patch.percent && !base?.percent) next.percent = true;
    else if (!patch.percent && base?.percent) next.percent = false;
    else delete next.percent;
  }
  return Object.keys(next).length ? next : null;
}

/**
 * The tile as it is drawn: a value takes the format of its named measure, with its own over it. A tile with no
 * named measure, or with nothing to add, is returned as it is.
 */
export function withNamedFormats<T extends Pick<Tile, 'formats' | 'measures'>>(tile: T): T {
  if (!tile.measures.some((m) => m.named)) return tile;
  const formats = tile.measures.map((_, i) => formatOf(tile, i));
  if (!formats.some(Boolean) || formats.every((f, i) => JSON.stringify(f) === JSON.stringify(ownOf(tile, i)))) return tile;
  return { ...tile, formats };
}

/**
 * The number a value is typed in: a measure written as a percent is read as 95 for 95 %, and stored as 0.95.
 * `scale` is 100 when the value shows as a percent (its format, or a share) and 1 otherwise; `unit` is the
 * sign to show beside the field (V4-D110).
 */
export function targetUnit(tile: Pick<Tile, 'formats' | 'measures' | 'showAs'>, i: number): { scale: number; unit: string } {
  const how = tile.showAs?.[i];
  const shared = how === 'pct_total' || how === 'pct_group' || how === 'pct_change';
  return formatOf(tile, i)?.percent || shared ? { scale: 100, unit: '%' } : { scale: 1, unit: '' };
}

// ---- names -----------------------------------------------------------------------------------------

const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/** Why a name cannot be used for a measure, in words, or null. The same rules as the engine. `except` is the measure being edited. */
export function nameProblem(name: string, all: readonly NamedMeasure[], except?: string): string | null {
  const n = name.trim().replace(/\s+/g, ' ');
  if (!n) return 'Give the measure a name.';
  if ([...n].length > MAX_MEASURE_NAME) return `Keep the name to ${MAX_MEASURE_NAME} letters or fewer.`;
  const clash = all.find((m) => m.id !== except && same(m.name, n));
  if (clash) return `Another measure is called ${clash.name}. Choose another name.`;
  return null;
}

/** Why a formula cannot be saved before the engine is asked, or null. */
export function formulaProblem(formula: string): string | null {
  if (!formula.trim()) return 'Write a formula, such as SUM([Sales].[amount]).';
  if ([...formula].length > MAX_FORMULA) return `Keep a formula to ${MAX_FORMULA.toLocaleString('en-US')} characters or fewer.`;
  return null;
}

// ---- who uses a measure -----------------------------------------------------------------------------

export interface MeasureUse {
  dashboard: string;
  dashboardTitle: string;
  tile: string;
  title: string;
  kind: string;
}

/** The tiles that use a measure, in the order of the dashboards. */
export function tilesWith(dashboards: readonly Dashboard[], id: string): MeasureUse[] {
  const out: MeasureUse[] = [];
  for (const d of dashboards) {
    for (const t of d.tiles ?? []) {
      if ((t.measures ?? []).some((m) => m.named === id)) {
        out.push({ dashboard: d.id, dashboardTitle: d.title, tile: t.id, title: t.title ?? '', kind: t.type });
      }
    }
  }
  return out;
}

/** The ids of the named measures a tile uses. */
export const measuresOf = (tile: Pick<Tile, 'measures'>): string[] => [...new Set(tile.measures.map((m) => m.named).filter((x): x is string => !!x))];
