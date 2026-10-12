// KPI cards (build 08, V4-D10): how a card takes its style, which parts give way on a short tile, which rule matches,
// the six presets, and the trend's geometry. Pure functions, no browser.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  barBoxes,
  CARD_FIELDS,
  cardGround,
  changeTone,
  classicParts,
  drawable,
  drawnTile,
  fitParts,
  linePoints,
  overrides,
  PRESETS,
  presetOf,
  resolveCard,
  ruleFor,
  ruleWords,
  sparkBeside,
  valuePx,
  withoutStyle,
} from '../src/lib/card.ts';

const style = {
  id: 'cs-a',
  name: 'Quarterly',
  card: { parts: [{ kind: 'value' }, { kind: 'title' }], align: 'center', size: 'l', ground: 'accent', rules: [{ op: '<', value: 10, tone: 'bad' }] },
};

test('a card takes each field from itself, else its style, else the number tile as it always was', () => {
  // No card: the parts a number tile has always shown, with the target line only when there is a target.
  assert.deepEqual(resolveCard(undefined, [], false), { parts: classicParts(false), align: 'start', size: 'm', ground: 'plain', rules: [] });
  assert.deepEqual(
    resolveCard(undefined, [], true).parts.map((p) => p.kind),
    ['title', 'value', 'period', 'compare', 'target', 'spark'],
  );
  // A style gives every field; the card's own size wins over it, the rest still come from the style.
  const r = resolveCard({ style: 'cs-a', size: 's' }, [style], false);
  assert.deepEqual(r, { ...style.card, size: 's' });
  assert.deepEqual(overrides({ style: 'cs-a', size: 's' }, [style]), ['size']);
  // A card with no style overrides nothing; a style that is gone leaves the card's own fields and the defaults.
  assert.deepEqual(overrides({ size: 's' }, [style]), []);
  assert.deepEqual(resolveCard({ style: 'gone', ground: 'tint' }, [style], false), { parts: classicParts(false), align: 'start', size: 'm', ground: 'tint', rules: [] });
  assert.deepEqual(overrides({ style: 'gone', ground: 'tint' }, [style]), []);
  assert.deepEqual([...CARD_FIELDS], ['parts', 'align', 'size', 'ground', 'rules']);
});

test('a card that leaves its style keeps its look, and shares nothing with the style', () => {
  const out = withoutStyle({ style: 'cs-a', size: 's' }, style);
  assert.deepEqual(out, { ...style.card, size: 's' });
  assert.equal('style' in out, false);
  out.parts.push({ kind: 'note', text: 'x' });
  out.rules[0].value = 99;
  assert.equal(style.card.parts.length, 2);
  assert.equal(style.card.rules[0].value, 10);
  // The card's own fields are copied too.
  const mine = { style: 'cs-a', parts: [{ kind: 'value' }] };
  withoutStyle(mine, style).parts.push({ kind: 'title' });
  assert.equal(mine.parts.length, 1);
  // A style that is gone: the card keeps what it has.
  assert.deepEqual(withoutStyle({ style: 'gone', align: 'center' }, undefined), { align: 'center' });
});

test('a tile is drawn with its style merged in, and a card fills its tile with its ground', () => {
  const kpi = { id: 'k', type: 'kpi', title: '', x: 0, y: 0, w: 3, h: 2, measures: [], dims: [] };
  const styled = { ...kpi, card: { style: 'cs-a', align: 'start' } };
  const drawn = drawnTile(styled, [style]);
  assert.deepEqual(drawn.card, { ...style.card, align: 'start' });
  assert.equal(styled.card.style, 'cs-a');
  assert.equal(cardGround(drawn), 'accent');
  // No card, a card with no style, and another kind of tile are drawn as they are.
  assert.equal(drawnTile(kpi, [style]), kpi);
  const own = { ...kpi, card: { size: 's' } };
  assert.equal(drawnTile(own, [style]), own);
  assert.equal(cardGround(own), 'surface');
  assert.equal(cardGround(kpi), null);
  const bar = { ...kpi, type: 'bar', card: { style: 'cs-a' } };
  assert.equal(drawnTile(bar, [style]), bar);
  assert.equal(cardGround(bar), null);
});

test('parts that do not fit drop from the bottom: notes, then the trend, then extra values, then the rest; never the value', () => {
  const parts = [{ kind: 'title' }, { kind: 'value' }, { kind: 'period' }, { kind: 'compare' }, { kind: 'extra', measure: 1 }, { kind: 'spark', style: 'bars' }, { kind: 'note', text: 'n' }];
  // 300 px wide, size m: the value is 34 px (+ 8). Heights: title 20, value 42, period 17, compare 18, extra 20,
  // trend 32, note 17. In all 166.
  assert.equal(valuePx('m', 300), 34);
  const kinds = (h) => fitParts(parts, h, 'm', 300).shown.map((p) => p.kind).join(' ');
  assert.equal(kinds(166), 'title value period compare extra spark note');
  assert.equal(fitParts(parts, 166, 'm', 300).dropped, 0);
  assert.equal(kinds(165), 'title value period compare extra spark');
  assert.equal(kinds(140), 'title value period compare extra');
  assert.equal(kinds(100), 'title value period compare');
  // 97 is still too much for 60: compare, period and title go, from the bottom.
  assert.equal(kinds(60), 'value');
  assert.equal(fitParts(parts, 60, 'm', 300).dropped, 6);
  assert.equal(kinds(1), 'value');
  // A hidden part is not drawn and is not counted as dropped.
  assert.deepEqual(fitParts([{ kind: 'value' }, { kind: 'note', text: 'n', hidden: true }], 100, 'm', 300), { shown: [{ kind: 'value' }], dropped: 0 });
});

test('a trend that is the last part sits beside the words in a wide tile, and then takes no height', () => {
  const parts = [{ kind: 'title' }, { kind: 'value' }, { kind: 'period' }, { kind: 'compare' }, { kind: 'spark', style: 'bars' }];
  assert.equal(sparkBeside(parts, 360), true);
  assert.equal(sparkBeside(parts, 359), false);
  assert.equal(sparkBeside([...parts, { kind: 'note', text: 'n' }], 400), false);
  // 400 px wide: value 34 + 8; title 20, period 17, compare 18: 97. The trend beside costs nothing.
  assert.equal(fitParts(parts, 97, 'm', 400).dropped, 0);
  // One pixel short: the trend beside stays (it frees nothing); compare goes.
  assert.deepEqual(fitParts(parts, 96, 'm', 400).shown.map((p) => p.kind), ['title', 'value', 'period', 'spark']);
  // A big value shrinks so its text fits the width, never grows. A figure is 0.62 of the size wide; six characters
  // unless told: 150 / (6 × 0.62) = 40; "18.4K" is five: 150 / 3.1 = 48; one character counts as four.
  assert.equal(valuePx('l', 600), 48);
  assert.equal(valuePx('l', 150), 40);
  assert.equal(valuePx('l', 150, 5), 48);
  assert.equal(valuePx('l', 100, 1), 40);
  assert.equal(valuePx('s', 60), 18);
});

test('only the parts a tile can fill take room', () => {
  const all = [
    { kind: 'value' },
    { kind: 'target' },
    { kind: 'compare' },
    { kind: 'spark' },
    { kind: 'extra', measure: 1 },
    { kind: 'extra', measure: 3 },
    { kind: 'note', text: '  ' },
    { kind: 'divider', hidden: true },
  ];
  const kinds = (d) => drawable(all, d).map((p) => p.kind + (p.measure ?? '')).join(' ');
  assert.equal(kinds({ target: true, versus: true, bars: 3, measures: 4 }), 'value target compare spark extra1 extra3');
  assert.equal(kinds({ target: false, versus: false, bars: 2, measures: 2 }), 'value extra1');
});

test('the first rule the value meets colours it; its words name the target when the rule is at it', () => {
  const rules = [
    { op: '<', value: 100, tone: 'bad' },
    { op: 'between', value: 150, to: 100, tone: 'warn' },
    { op: '>', value: 120, tone: 'good' },
  ];
  assert.equal(ruleFor(rules, 99.9).tone, 'bad');
  // Between counts both ends, in either order; it comes before "> 120", so 130 is a warning.
  assert.equal(ruleFor(rules, 100).tone, 'warn');
  assert.equal(ruleFor(rules, 130).tone, 'warn');
  assert.equal(ruleFor(rules, 150.1).tone, 'good');
  assert.equal(ruleFor(rules, null), null);
  assert.equal(ruleFor(rules, NaN), null);
  assert.equal(ruleFor([{ op: 'between', value: 1, tone: 'bad' }], 1), null);
  assert.equal(ruleFor(undefined, 5), null);
  const f = (v) => v.toLocaleString('en-US');
  assert.equal(ruleWords(rules[0], f, 100), 'Below target');
  assert.equal(ruleWords(rules[0], f, 90), 'Below 100');
  assert.equal(ruleWords({ op: '>', value: 1200, tone: 'good' }, f, null), 'Above 1,200');
  assert.equal(ruleWords(rules[1], f, 100), 'Between 100 and 150');
});

test('six presets, each a whole card that reads back as itself', () => {
  assert.deepEqual(PRESETS.map((p) => p.name), ['Classic', 'Big number', 'With target', 'Compact', 'Trend', 'Split']);
  for (const p of PRESETS) assert.equal(presetOf(resolveCard(p.card(true), [], true), true), p.id, p.id);
  const by = (id) => PRESETS.find((p) => p.id === id).card(false);
  assert.deepEqual(by('classic').parts, classicParts(false));
  assert.deepEqual(by('trend').parts.at(-1), { kind: 'spark', style: 'line' });
  assert.deepEqual(by('split').parts.filter((p) => p.kind === 'extra').map((p) => p.measure), [1, 2]);
  assert.equal(by('big').size, 'l');
  assert.equal(by('compact').size, 's');
  // A changed preset is no preset.
  assert.equal(presetOf({ ...resolveCard(by('compact'), [], false), size: 'l' }, false), null);
});

test('a change under half a percent is the same; the trend draws as the number tile always did', () => {
  assert.equal(changeTone(0.004), 'flat');
  assert.equal(changeTone(-0.004), 'flat');
  assert.equal(changeTone(0.1), 'good');
  assert.equal(changeTone(0.1, 'down'), 'bad');
  assert.equal(changeTone(-0.1, 'down'), 'good');
  assert.equal(changeTone(null), 'flat');
  // Two columns, 0 and 10, in the 100 × 40 box: each 49.2 wide with a gap of 1.6. A zero is a 0.8 sliver at the floor.
  const round = (o) => Object.fromEntries(Object.entries(o).map(([k, v]) => [k, Math.round(v * 1e9) / 1e9]));
  const [a, b] = barBoxes([0, 10]).map(round);
  assert.deepEqual(a, { x: 0, y: 40, w: 49.2, h: 0.8 });
  assert.deepEqual(b, { x: 50.8, y: 0, w: 49.2, h: 40 });
  // A line through 1, a gap and 3, in 100 × 40 px with a 3 px inset.
  const pts = linePoints([1, null, 3], 100, 40);
  assert.deepEqual(pts.map((p) => p.x), [3, 50, 97]);
  assert.deepEqual([pts[0].y, pts[2].y], [37, 3]);
  assert.ok(Number.isNaN(pts[1].y));
});

