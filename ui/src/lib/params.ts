// Parameters and what-if (v5 build 05, V5-D7): a number or a choice that a measure reads with PARAM("name"), and that a
// Top N, a number tile's target or a chart's line can take. The engine reads the values for measures and Top N; the
// screen reads them for targets and lines, here. Pure, with type imports and t(), so the node tests load it.

import type { Cell, Param, Tile } from './api';
import { t } from './i18n.ts';

/** A choice offers at most this many values; an export holds the steps of a parameter that has at most `MAX_STEPS`. */
export const MAX_CHOICES = 50;
export const MAX_STEPS = 21;
/** Buttons for at most this many choices; a dropdown above that. */
export const MAX_BUTTONS = 5;

/** The value of each parameter by id: the saved ones, with `over` in their place. */
export function paramValues(list: readonly Param[], over: Readonly<Record<string, Cell>> = {}): Record<string, Cell> {
  const out: Record<string, Cell> = {};
  for (const p of list) out[p.id] = p.id in over ? over[p.id] : p.value;
  return out;
}

const num = (v: Cell | undefined): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);

/**
 * The tile as it is drawn with these values: a target or a line that takes a parameter has its value. A parameter
 * that is gone, or is not a number, leaves the tile's own number. The Top N is the engine's (it shapes the result).
 */
export function withParams(tile: Tile, values: Readonly<Record<string, Cell>>): Tile {
  const at = (id: string | undefined) => (id ? num(values[id]) : null);
  const target = tile.target?.param ? at(tile.target.param) : null;
  const lines = tile.lines?.some((l) => l.param && at(l.param) !== null);
  if (target === null && !lines) return tile;
  return {
    ...tile,
    ...(target !== null ? { target: { ...tile.target!, value: target } } : {}),
    ...(lines ? { lines: tile.lines!.map((l) => (l.param && at(l.param) !== null ? { ...l, value: at(l.param)! } : l)) } : {}),
  };
}

/** The parameters a tile reads on the screen side (its target and lines) and for its Top N. */
export function paramsOf(tile: Tile): string[] {
  const out = new Set<string>();
  if (tile.type === 'param' && tile.param?.id) out.add(tile.param.id);
  if (tile.top?.param) out.add(tile.top.param);
  if (tile.target?.param) out.add(tile.target.param);
  for (const l of tile.lines ?? []) if (l.param) out.add(l.param);
  return [...out];
}

/** Rounds away the float noise of `min + step * i`, as the engine does. */
const round = (x: number) => Math.round(x * 1e9) / 1e9;

/** Every value the control can take when there are at most `MAX_STEPS`: a number's steps, or the choices. */
export function stepsOf(p: Param): Cell[] | null {
  if (p.kind === 'choice') return (p.choices ?? []).length <= MAX_STEPS ? [...(p.choices ?? [])] : null;
  const { min, max, step } = p;
  if (min === undefined || max === undefined || !step || step <= 0 || max < min) return null;
  const n = Math.floor((max - min) / step + 1e-9) + 1;
  return n <= MAX_STEPS ? Array.from({ length: n }, (_, i) => round(min + step * i)) : null;
}

/** How many values a number parameter's control has. */
export const stepCount = (p: Pick<Param, 'min' | 'max' | 'step'>): number =>
  p.min === undefined || p.max === undefined || !p.step || p.step <= 0 || p.max < p.min ? 0 : Math.floor((p.max - p.min) / p.step + 1e-9) + 1;

/** The key of a value in an export's `steps` (`js_key` in the engine). */
export const stepKey = (v: Cell): string => (v === null || v === undefined ? '\u0000null' : String(v));

/** The control a parameter tile draws: a slider or a box for a number; buttons or a dropdown for a choice. */
export function controlOf(p: Param, style: string | undefined): 'slider' | 'input' | 'buttons' | 'dropdown' {
  if (p.kind !== 'choice') return style === 'input' ? 'input' : 'slider';
  return style !== 'dropdown' && (p.choices ?? []).length <= MAX_BUTTONS ? 'buttons' : 'dropdown';
}

/** A number kept on the control: inside its range, on a step from its minimum. */
export function snap(p: Param, v: number): number {
  const min = p.min ?? 0;
  const max = p.max ?? 100;
  const step = p.step && p.step > 0 ? p.step : 1;
  const k = Math.round((Math.min(max, Math.max(min, v)) - min) / step);
  return round(Math.min(max, min + k * step));
}

/** What is wrong with a parameter before it is saved, in words; null when nothing is. The engine checks again. */
export function paramProblem(p: Param, all: readonly Param[]): string | null {
  const name = p.name.trim();
  if (!name) return t('Give the parameter a name.');
  if (name.includes('"')) return t('A name cannot hold a double quote.');
  if (all.some((o) => o.id !== p.id && o.name.trim().toLowerCase() === name.toLowerCase())) return t('Another parameter is called {name}.', { name });
  if (p.kind === 'choice') {
    const n = (p.choices ?? []).length;
    if (n === 0) return t('Add at least one value to choose from.');
    if (n > MAX_CHOICES) return t('A choice can offer at most {n} values.', { n: MAX_CHOICES });
    return null;
  }
  if (p.min === undefined || p.max === undefined || !Number.isFinite(p.min) || !Number.isFinite(p.max)) return t('Give the smallest and the largest value.');
  if (p.max < p.min) return t('The largest value must not be below the smallest.');
  if (!p.step || !(p.step > 0)) return t('The step must be above 0.');
  if (stepCount(p) > 10_000) return t('That is more than 10,000 steps. Use a larger step.');
  return null;
}

/** The choices typed one per line: a line that reads as a number is a number; empty lines go; repeats go. */
export function choicesFrom(text: string): Cell[] {
  const out: Cell[] = [];
  for (const line of text.split('\n')) {
    const s = line.trim();
    if (!s) continue;
    const n = /^-?\d+(\.\d+)?$/.test(s) ? Number(s) : null;
    const v: Cell = n !== null && Number.isFinite(n) ? n : s;
    if (!out.some((o) => stepKey(o) === stepKey(v))) out.push(v);
  }
  return out;
}

/** A new parameter: a percent from 0 to 20 in steps of 1 %. */
export const newParam = (n: number): Param => ({
  id: '',
  name: t('Parameter {n}', { n }),
  kind: 'number',
  value: 0,
  min: 0,
  max: 0.2,
  step: 0.01,
  format: { percent: true },
});
