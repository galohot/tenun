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

/** The first value of the tile adds up (or the tile has none, and counts rows). Shares, folding the small bars into "Other" and a pace need it. */
export const addsUp = (tile: Pick<Tile, 'measures'>): boolean => !tile.measures[0] || additive(tile.measures[0]);

/**
 * The share of the period that has data, for the pace of a number tile. A pace is the value so far scaled up to
 * the whole period, which is right for a sum or a count; a ratio or an average so far is already the value of
 * the period, so it has no pace (V4-D85d).
 */
export const paceElapsed = (tile: Pick<Tile, 'measures'>, elapsed: number | null | undefined): number | null | undefined => (addsUp(tile) ? elapsed : null);

/** The format a tile writes a value in: its own, else the one of the named measure. */
export function formatOf(tile: Pick<Tile, 'formats' | 'measures'>, i: number): NumFormat | null {
  const own = tile.formats?.[i];
  if (own) return own;
  const id = tile.measures[i]?.named;
  return (id && known.find((m) => m.id === id)?.format) || null;
}

/**
 * The tile as it is drawn: a value that has no format of its own takes the format of its named
 * measure. A tile with no named measure, or with nothing to add, is returned as it is.
 */
export function withNamedFormats<T extends Pick<Tile, 'formats' | 'measures'>>(tile: T): T {
  if (!tile.measures.some((m) => m.named)) return tile;
  const formats = tile.measures.map((_, i) => formatOf(tile, i));
  if (!formats.some(Boolean) || formats.every((f, i) => f === (tile.formats?.[i] ?? null))) return tile;
  return { ...tile, formats };
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
