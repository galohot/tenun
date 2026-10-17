// KPI cards (build 08, V4-D10, GRANDPLAN §4.1): what a number tile shows, in which order, how big, on which ground,
// and the rules that colour its value. A card style is saved in the workspace and applied by reference; a card's own
// fields override the style's, field by field. A KPI tile without `card` is drawn by KpiView as it always was. Pure,
// with type-only imports and t(), so the node tests and the contrast test load it.

import type { CardPart, CardRule, CardSpec, CardStyle, Tile } from './api';
import { t } from './i18n.ts';

export type CardGround = NonNullable<CardSpec['ground']>;
export type CardSize = NonNullable<CardSpec['size']>;
export type CardAlign = NonNullable<CardSpec['align']>;
export type CardTone = CardRule['tone'];
export type CardKind = CardPart['kind'];

/** A card with every field set: what is drawn. */
export interface ResolvedCard {
  parts: CardPart[];
  align: CardAlign;
  size: CardSize;
  ground: CardGround;
  rules: CardRule[];
}

/** The fields a style gives and a card may override, one by one. */
export const CARD_FIELDS = ['parts', 'align', 'size', 'ground', 'rules'] as const;
export type CardField = (typeof CARD_FIELDS)[number];

/** The parts a number tile has always shown: what "Classic" is, and where a new card starts. */
export function classicParts(withTarget: boolean): CardPart[] {
  return [
    { kind: 'title' },
    { kind: 'value' },
    { kind: 'period' },
    { kind: 'compare' },
    ...(withTarget ? [{ kind: 'target' } as CardPart] : []),
    { kind: 'spark', style: 'bars' },
  ];
}

/** The style a card names, or undefined when it names none or the style is gone. */
export function styleOf(card: CardSpec | undefined, styles: readonly CardStyle[] | undefined): CardStyle | undefined {
  return card?.style ? styles?.find((s) => s.id === card.style) : undefined;
}

/** The card as drawn: each field from the card, else from its style, else the default. */
export function resolveCard(card: CardSpec | undefined, styles: readonly CardStyle[] | undefined, withTarget: boolean): ResolvedCard {
  const style = styleOf(card, styles)?.card;
  return {
    parts: card?.parts ?? style?.parts ?? classicParts(withTarget),
    align: card?.align ?? style?.align ?? 'start',
    size: card?.size ?? style?.size ?? 'm',
    ground: card?.ground ?? style?.ground ?? 'plain',
    rules: card?.rules ?? style?.rules ?? [],
  };
}

/** The fields a styled card sets itself (and so does not take from its style). None for a card with no style. */
export function overrides(card: CardSpec | undefined, styles: readonly CardStyle[] | undefined): CardField[] {
  if (!styleOf(card, styles)) return [];
  return CARD_FIELDS.filter((f) => card?.[f] !== undefined);
}

/** A copy with no link to the original: a style's fields go into cards and other styles. */
const copy = <T>(x: T): T => JSON.parse(JSON.stringify(x));

/**
 * A card that stops using its style and keeps its look: the style's fields, with the card's own over them, and no style.
 * A deleted style is taken out of its cards this way, and an exported file carries its cards this way (`snapshot.rs`
 * does the same, so a viewer needs no styles).
 */
export function withoutStyle(card: CardSpec, style: CardStyle | undefined): CardSpec {
  const out: Record<string, unknown> = style ? copy(style.card) : {};
  for (const f of CARD_FIELDS) if (card[f] !== undefined) out[f] = copy(card[f]);
  return out as CardSpec;
}

/**
 * A tile as it is drawn: a number tile's card with its style merged in and no style left (`withoutStyle`). The app, a
 * picture and the exported file all draw this, so the drawing never looks a style up. Any other tile is returned as it is.
 */
export function drawnTile(tile: Tile, styles: readonly CardStyle[] | undefined): Tile {
  if (tile.type !== 'kpi' || !tile.card?.style) return tile;
  return { ...tile, card: withoutStyle(tile.card, styleOf(tile.card, styles)) };
}

/** The ground token of a drawn tile with a card, or null: a card fills its whole tile and draws its own title. */
export function cardGround(tile: Tile): string | null {
  return tile.type === 'kpi' && tile.card ? GROUND_TOKEN[tile.card.ground ?? 'plain'] : null;
}

/** A card's own fields as a style keeps them: everything that is drawn, nothing about the tile. */
export function asStyle(card: ResolvedCard): Omit<CardSpec, 'style'> {
  return copy({ parts: card.parts, align: card.align, size: card.size, ground: card.ground, rules: card.rules });
}

/** The six presets of the card editor. Each is a whole card, so picking one replaces every field. */
export const PRESETS: { id: string; name: string; card: (withTarget: boolean) => Omit<CardSpec, 'style'> }[] = [
  { id: 'classic', get name() { return t('Classic'); }, card: (withTarget) => ({ parts: classicParts(withTarget), align: 'start', size: 'm', ground: 'plain', rules: [] }) },
  { id: 'big', get name() { return t('Big number'); }, card: () => ({ parts: [{ kind: 'value' }, { kind: 'title' }], align: 'center', size: 'l', ground: 'plain', rules: [] }) },
  {
    id: 'target',
    get name() {
      return t('With target');
    },
    card: () => ({ parts: [{ kind: 'title' }, { kind: 'value' }, { kind: 'target' }, { kind: 'compare' }], align: 'start', size: 'm', ground: 'tint', rules: [] }),
  },
  { id: 'compact', get name() { return t('Compact'); }, card: () => ({ parts: [{ kind: 'title' }, { kind: 'value' }, { kind: 'compare' }], align: 'start', size: 's', ground: 'plain', rules: [] }) },
  {
    id: 'trend',
    get name() {
      return t('Trend');
    },
    card: () => ({ parts: [{ kind: 'title' }, { kind: 'value' }, { kind: 'compare' }, { kind: 'spark', style: 'line' }], align: 'start', size: 'm', ground: 'plain', rules: [] }),
  },
  {
    id: 'split',
    get name() {
      return t('Split');
    },
    card: () => ({
      parts: [{ kind: 'title' }, { kind: 'value' }, { kind: 'divider' }, { kind: 'extra', measure: 1 }, { kind: 'extra', measure: 2 }],
      align: 'start',
      size: 'm',
      ground: 'plain',
      rules: [],
    }),
  },
];

/** The preset a card is now, or null when it was changed after it was picked. */
export function presetOf(card: ResolvedCard, withTarget: boolean): string | null {
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  return PRESETS.find((p) => same(asStyle(card), asStyle({ ...p.card(withTarget), rules: [] } as ResolvedCard)))?.id ?? null;
}

// ---- size and fit ---------------------------------------------------------------------------------------------------

/** The big value's three steps, in px. A narrow tile makes it smaller, never larger. */
export const VALUE_PX: Record<CardSize, number> = { s: 24, m: 34, l: 48 };

/**
 * The size of the value: its step, or smaller so its text fits the width. A figure is about 0.62 of the size wide; a
 * text of under four characters counts as four, so "7" is not drawn huge.
 */
export function valuePx(size: CardSize, width: number, chars = 6): number {
  return Math.max(18, Math.min(VALUE_PX[size], Math.floor(width / (Math.max(4, chars) * 0.62))));
}

/** The height each part needs, in px, with its space above. A trend needs at least this much and takes what is left. */
export function partHeight(p: CardPart, size: CardSize, width: number, chars = 6): number {
  switch (p.kind) {
    case 'value':
      return valuePx(size, width, chars) + 8;
    case 'title':
      return 20;
    case 'period':
    case 'note':
      return 17;
    case 'compare':
      return 18;
    case 'extra':
      return 20;
    case 'target':
      return 26;
    case 'spark':
      return 32;
    case 'divider':
      return 11;
  }
}

/** What a tile has to show, for `drawable`. */
export interface CardData {
  /** The tile has a target. */
  target: boolean;
  /** The value is compared with an earlier period. */
  versus: boolean;
  /** Periods in the trend. */
  bars: number;
  /** Values the tile has: the main one and the extras. */
  measures: number;
}

/**
 * The parts that have something to draw: no target line without a target, no change without an earlier period, no
 * trend under three periods, no extra value whose value is gone from the tile. A part the tile cannot fill takes no room.
 */
export function drawable(parts: readonly CardPart[], d: CardData): CardPart[] {
  return parts.filter((p) => {
    if (p.hidden) return false;
    if (p.kind === 'target') return d.target;
    if (p.kind === 'compare') return d.versus;
    if (p.kind === 'spark') return d.bars > 2;
    if (p.kind === 'extra') return p.measure >= 1 && p.measure < d.measures;
    if (p.kind === 'note') return p.text.trim() !== '';
    return true;
  });
}

/** A trend that is the last part sits beside the words when the tile is this wide, as the number tile always had it. */
export const SPARK_BESIDE_AT = 360;

/** Whether the trend sits beside the other parts: it is the last part shown and the tile is wide. */
export function sparkBeside(parts: readonly CardPart[], width: number): boolean {
  return width >= SPARK_BESIDE_AT && parts.length > 1 && parts[parts.length - 1].kind === 'spark';
}

/** The order parts give way when the tile is too short: notes first, then the trend, then extra values. */
const GIVES_WAY: CardKind[] = ['note', 'spark', 'extra'];

/**
 * The parts that fit a height, in their order. Parts drop from the bottom: notes first, then the trend, then the extra
 * values, then the rest from the bottom up. The value never drops, and neither does a trend beside the words, which
 * takes no height. `dropped` says how many did, for the "…".
 */
export function fitParts(parts: readonly CardPart[], height: number, size: CardSize, width: number, chars = 6): { shown: CardPart[]; dropped: number } {
  let shown = parts.filter((p) => !p.hidden);
  const before = shown.length;
  const beside = (i: number) => i === shown.length - 1 && sparkBeside(shown, width);
  const need = () => shown.reduce((t, p, i) => t + (beside(i) ? 0 : partHeight(p, size, width, chars)), 0);
  const dropLast = (pred: (p: CardPart) => boolean) => {
    for (let i = shown.length - 1; i >= 0; i--) {
      if (pred(shown[i]) && !beside(i)) {
        shown = shown.filter((_, j) => j !== i);
        return true;
      }
    }
    return false;
  };
  while (need() > height) {
    if (GIVES_WAY.some((k) => dropLast((p) => p.kind === k))) continue;
    if (!dropLast((p) => p.kind !== 'value')) break;
  }
  return { shown, dropped: before - shown.length };
}

// ---- rules ----------------------------------------------------------------------------------------------------------

/** The first rule the value meets, or null. A value of nothing meets none. */
export function ruleFor(rules: readonly CardRule[] | undefined, value: number | null | undefined): CardRule | null {
  if (value === null || value === undefined || !Number.isFinite(value)) return null;
  for (const r of rules ?? []) {
    if (r.op === '>' && value > r.value) return r;
    if (r.op === '<' && value < r.value) return r;
    if (r.op === 'between' && r.to !== undefined && value >= Math.min(r.value, r.to) && value <= Math.max(r.value, r.to)) return r;
  }
  return null;
}

/** The words a rule adds beside the value. A rule at the tile's target says "target" instead of the number. */
export function ruleWords(rule: CardRule, format: (v: number) => string, target?: number | null): string {
  const atTarget = target !== null && target !== undefined && rule.op !== 'between' && rule.value === target;
  if (rule.op === '<') return atTarget ? t('Below target') : t('Below {value}', { value: format(rule.value) });
  if (rule.op === '>') return atTarget ? t('Above target') : t('Above {value}', { value: format(rule.value) });
  return t('Between {from} and {to}', { from: format(Math.min(rule.value, rule.to ?? rule.value)), to: format(Math.max(rule.value, rule.to ?? rule.value)) });
}

// ---- colour ---------------------------------------------------------------------------------------------------------

/** The token of each ground (theme.css). */
export const GROUND_TOKEN: Record<CardGround, string> = { plain: 'surface', tint: 'tint', accent: 'accent' };

export interface Inks {
  /** Words. */
  text: string;
  muted: string;
  good: string;
  bad: string;
  warn: string;
  /** The trend's last period (a mark). */
  mark: string;
  /** The trend's other periods and the divider: no meaning of their own. */
  soft: string;
}

/**
 * The token each part uses on each ground. On the accent ground every word is the on-accent colour: a change and a rule
 * keep their arrow and their words, and the colour that backs them would not read on the accent. `tests/contrast.test.js`
 * measures every pair in both themes.
 */
export const INKS: Record<CardGround, Inks> = {
  plain: { text: 'text', muted: 'text-2', good: 'success', bad: 'danger', warn: 'warn', mark: 's1', soft: 'grid-line' },
  tint: { text: 'text', muted: 'text-2', good: 'success', bad: 'danger', warn: 'warn', mark: 's1', soft: 'sep' },
  accent: { text: 'on-accent', muted: 'on-accent', good: 'on-accent', bad: 'on-accent', warn: 'on-accent', mark: 'on-accent', soft: 'on-accent-soft' },
};

/** The ink of a tone on a ground. */
export const toneInk = (ground: CardGround, tone: CardTone): string => INKS[ground][tone];

/** A change under half a percent is "about the same", not a trend. */
export function changeTone(delta: number | null, better: 'up' | 'down' = 'up'): 'good' | 'bad' | 'flat' {
  if (delta === null || Math.abs(delta) < 0.005) return 'flat';
  return delta > 0 === (better === 'up') ? 'good' : 'bad';
}

// ---- the trend ------------------------------------------------------------------------------------------------------

/** Columns in a 0..100 × 0..40 box: the number tile's history, the same as KpiView draws it. */
export function barBoxes(values: readonly (number | null)[]): { x: number; y: number; w: number; h: number }[] {
  const vals = values.map((v) => v ?? 0);
  const hi = Math.max(0, ...vals);
  const lo = Math.min(0, ...vals);
  const range = hi - lo || 1;
  const n = vals.length;
  const gap = 1.6;
  const bw = (100 - gap * (n - 1)) / n;
  return vals.map((v, i) => {
    const top = 40 - ((Math.max(v, 0) - lo) / range) * 40;
    const bottom = 40 - ((Math.min(v, 0) - lo) / range) * 40;
    return { x: i * (bw + gap), y: top, w: bw, h: Math.max(bottom - top, 0.8) };
  });
}

/** A line through the values in a box of `w` × `h` px, inset by `pad` so the last dot is whole. A gap breaks the line. */
export function linePoints(values: readonly (number | null)[], w: number, h: number, pad = 3): { x: number; y: number; v: number | null }[] {
  const nums = values.filter((v): v is number => v !== null);
  const hi = Math.max(...nums);
  const lo = Math.min(...nums);
  const range = hi - lo || 1;
  const n = values.length;
  return values.map((v, i) => ({
    x: n === 1 ? w / 2 : pad + (i / (n - 1)) * (w - 2 * pad),
    y: v === null ? NaN : hi === lo ? h / 2 : pad + (1 - (v - lo) / range) * (h - 2 * pad),
    v,
  }));
}
