// Named measures (build 03): what the app knows about them, and the places that used to add
// already-added numbers on the client (V4-D85d). Pure functions, no browser.
import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  addsUp,
  addsUpHere,
  additive,
  baseFormat,
  formatOf,
  formulaProblem,
  MAX_FORMULA,
  measuresOf,
  nameOf,
  nameProblem,
  namedMeasure,
  ownFormat,
  paceElapsed,
  repeatsAt,
  setKnownFromFile,
  setKnownMeasures,
  stepsByDate,
  targetUnit,
  tileProblem,
  tilesWith,
  whyNotAdded,
  withNamedFormats,
} from '../src/lib/measures.ts';
import { foldable, splitKeep, stackableTile, stackMode, unitOf, sameUnit } from '../src/lib/charts/math.ts';
import { foldSeries, splitKept } from '../src/lib/charts/fold.ts';
import { lineSource, pointsOf } from '../src/lib/charts/lines.ts';
import { pictureKind } from '../src/lib/picture.ts';
import { effectiveShowAs, showAsOptions } from '../src/lib/showas.ts';
import { suggestTypes } from '../src/lib/suggest.ts';
import { suggest } from '../src/lib/formula.ts';
import { targetInfo } from '../src/lib/kpi.ts';

const ratio = { id: 'm1', name: 'Achievement', formula: 'SUM([Sales].[amount]) / SUM([Targets].[target])' };
const revenue = { id: 'm2', name: 'Revenue', formula: 'SUM([Sales].[amount])', format: { currency: 'IDR', compact: true } };
const info = { m1: { additive: false, tables: ['s1/sales', 's2/targets'], dtype: 'number' }, m2: { additive: true, tables: ['s1/sales'], dtype: 'number' } };

beforeEach(() => setKnownMeasures([ratio, revenue], info));

const plain = (agg, per) => ({ table: 's1/sales', column: 'amount', agg, ...(per ? { per: { table: 's1/sales', column: null, agg: 'count' } } : {}) });
const tile = (type, measures, extra = {}) => ({ id: 't', type, title: '', x: 0, y: 0, w: 6, h: 4, measures, dims: [], ...extra });

test('a named measure is read by its name and added up only when the engine says so', () => {
  assert.equal(nameOf('m1'), 'Achievement');
  assert.equal(nameOf('gone'), 'A deleted measure');
  assert.equal(additive(namedMeasure('m1')), false);
  assert.equal(additive(namedMeasure('m2')), true);
  // A measure the app has not heard of is not trusted.
  assert.equal(additive(namedMeasure('gone')), false);
  // A plain measure is read from how it is added up.
  assert.equal(additive(plain('sum')), true);
  assert.equal(additive(plain('count')), true);
  for (const agg of ['avg', 'min', 'max', 'median', 'p90', 'count_distinct', 'stdev', 'variance']) assert.equal(additive(plain(agg)), false, agg);
  assert.equal(additive(plain('sum', true)), false, 'a ratio');
  // The tile adds up when its first value does, or when it has none and counts rows.
  assert.equal(addsUp(tile('bar', [namedMeasure('m2')])), true);
  assert.equal(addsUp(tile('bar', [namedMeasure('m1')])), false);
  assert.equal(addsUp(tile('bar', [])), true);
});

test('a viewer file carries the name, the format and whether the values add up', () => {
  setKnownFromFile([{ id: 'm9', name: 'Margin', format: { percent: true }, additive: false }]);
  assert.equal(nameOf('m9'), 'Margin');
  assert.equal(additive(namedMeasure('m9')), false);
  assert.deepEqual(formatOf(tile('kpi', [namedMeasure('m9')]), 0), { percent: true });
  setKnownFromFile(undefined);
  assert.equal(nameOf('m9'), 'A deleted measure');
});

test('V4-D106: a tile writes a value in the format of its measure, with its own fields over it', () => {
  const t = tile('bar', [namedMeasure('m2'), plain('sum')]);
  assert.deepEqual(formatOf(t, 0), { currency: 'IDR', compact: true });
  assert.equal(formatOf(t, 1), null);
  assert.deepEqual(withNamedFormats(t).formats, [{ currency: 'IDR', compact: true }, null]);
  // What the tile sets wins, field by field: the decimals are its own, the currency is the measure's.
  const own = tile('bar', [namedMeasure('m2')], { formats: [{ decimals: 1 }] });
  assert.deepEqual(formatOf(own, 0), { currency: 'IDR', compact: true, decimals: 1 });
  assert.deepEqual(withNamedFormats(own).formats, [{ currency: 'IDR', compact: true, decimals: 1 }]);
  // No named measure, or one with no format: the same tile.
  const none = tile('bar', [plain('sum')]);
  assert.equal(withNamedFormats(none), none);
  const bare = tile('bar', [namedMeasure('m1')]);
  assert.equal(withNamedFormats(bare), bare);
  // The format of the measure is read on its own too.
  assert.deepEqual(baseFormat(t, 0), { currency: 'IDR', compact: true });
  assert.equal(baseFormat(t, 1), null);
});

test('V4-D106: a tile stores only what it changed, so swapping the measure leaves no copy of the old format', () => {
  const base = { currency: 'IDR', compact: true, decimals: 2 };
  // Touching one control stores that field only.
  assert.deepEqual(ownFormat(base, null, { decimals: 0 }), { decimals: 0 });
  // Setting a field to what the measure says stores nothing.
  assert.equal(ownFormat(base, null, { currency: 'IDR' }), null);
  assert.deepEqual(ownFormat(base, { decimals: 0 }, { decimals: 2 }), null);
  // Saying no where the measure says yes is stored as a no.
  assert.deepEqual(ownFormat(base, null, { currency: '' }), { currency: '' });
  assert.deepEqual(ownFormat({ percent: true }, null, { percent: false }), { percent: false });
  // Automatic decimals cannot undo the decimals of the measure: it goes back to them.
  assert.equal(ownFormat(base, { decimals: 0 }, { decimals: undefined }), null);
  // A measure with no format: the field is stored, and clearing it removes it.
  assert.deepEqual(ownFormat(null, null, { currency: 'USD' }), { currency: 'USD' });
  assert.equal(ownFormat(null, { currency: 'USD' }, { currency: undefined }), null);
  assert.deepEqual(ownFormat(null, null, { compact: false }), { compact: false }, 'an explicit full size is a choice');
  // The sequence of the bug: pick a measure with a format, change nothing but the decimals, swap it.
  const t1 = tile('bar', [namedMeasure('m2')]);
  const own = ownFormat(baseFormat(t1, 0), null, { decimals: 0 });
  const swapped = tile('bar', [namedMeasure('m1')], { formats: [own] });
  assert.deepEqual(formatOf(swapped, 0), { decimals: 0 }, 'no currency and no short size from the first measure');
});

test('V4-D110: a target of a value written as a percent is typed as 95 and stored as 0.95', () => {
  setKnownMeasures([{ ...ratio, format: { percent: true } }, revenue], info);
  assert.deepEqual(targetUnit(tile('kpi', [namedMeasure('m1')]), 0), { scale: 100, unit: '%' });
  assert.deepEqual(targetUnit(tile('kpi', [namedMeasure('m2')]), 0), { scale: 1, unit: '' });
  assert.deepEqual(targetUnit(tile('kpi', [plain('sum')], { formats: [{ percent: true }] }), 0), { scale: 100, unit: '%' });
  assert.deepEqual(targetUnit(tile('bar', [plain('sum')], { showAs: ['pct_total'] }), 0), { scale: 100, unit: '%' });
  assert.deepEqual(targetUnit(tile('bar', [plain('sum')], { showAs: ['rank'] }), 0), { scale: 1, unit: '' });
});

test('stacking, a bar and line combo and the units read a named measure as what it is', () => {
  assert.equal(stackableTile(tile('bar', [namedMeasure('m2')]), true), true);
  assert.equal(stackableTile(tile('bar', [namedMeasure('m1')]), true), false);
  assert.equal(stackMode(tile('bar', [namedMeasure('m1'), namedMeasure('m2')]), true), 'side');
  assert.equal(unitOf(namedMeasure('m1')), 'named:m1');
  assert.notEqual(unitOf(namedMeasure('m1')), unitOf(namedMeasure('m2')));
  assert.equal(sameUnit(tile('bar', [namedMeasure('m1'), namedMeasure('m2')])), false);
  assert.equal(sameUnit(tile('bar', [namedMeasure('m1'), namedMeasure('m1')])), true);
});

test('V4-D85d: the small bars fold into Other only when their values add up', () => {
  const p = { categories: 10, max: 5, series: 1 };
  assert.equal(foldable({ ...p, addsUp: true }), true);
  assert.equal(foldable({ ...p, addsUp: false }), false, 'a ratio is not the sum of ratios');
  assert.equal(foldable({ ...p, addsUp: true, categories: 5 }), false, 'they fit');
  assert.equal(foldable({ ...p, addsUp: true, series: 2 }), false);
  // Wired to the tile: Achievement does not fold, Revenue does.
  assert.equal(foldable({ ...p, addsUp: addsUp(tile('bar', [namedMeasure('m1')])) }), false);
  assert.equal(foldable({ ...p, addsUp: addsUp(tile('bar', [namedMeasure('m2')])) }), true);
});

test('V4-D85d: a split chart folds its small groups into Other only when their values add up', () => {
  assert.equal(splitKeep(12, 8, true), 7, 'seven groups and Other');
  assert.equal(splitKeep(12, 8, false), 12, 'every group stays');
  assert.equal(splitKeep(6, 8, true), 6);
});

test('V4-D109: lines on a split chart read the total of the groups, or every point when there is none', () => {
  const split = { perValue: false, multi: true, stacked: false };
  assert.equal(lineSource({ ...split, addsUp: true }), 'total');
  assert.equal(lineSource({ ...split, addsUp: false }), 'points', 'an average has no total of its groups');
  // One series, or one series for each value: nothing is added.
  assert.equal(lineSource({ perValue: false, multi: false, stacked: false, addsUp: false }), 'first');
  assert.equal(lineSource({ perValue: true, multi: true, stacked: false, addsUp: false }), 'first');
  assert.equal(lineSource({ perValue: true, multi: true, stacked: true, addsUp: true }), 'total');
  // Every point drawn, "Other" left out.
  const series = [{ values: [1, 2, null] }, { values: [10, 20, 30] }];
  assert.deepEqual(pointsOf(series, ['a', 'b', 'rest'], (c) => c === 'rest'), [1, 2, 10, 20]);
});

test('V4-D99: a value adds up on a tile when its measure does and no table of it repeats across the groups', () => {
  const t = tile('bar', [namedMeasure('m2'), plain('avg')]);
  const parts = [{ measure: 0, note: 'targets is not split by product.', repeats: true }, { measure: 1, note: 'x', repeats: false }];
  assert.equal(addsUpHere(t, 0), true);
  assert.equal(addsUpHere(t, 0, parts), false, 'the engine says a table of it is repeated');
  assert.equal(addsUpHere(t, 1, parts), false, 'an average never adds up');
  assert.equal(repeatsAt(parts, 0), true);
  assert.equal(repeatsAt(parts, 1), false, 'a note about a filter does not repeat a table across the groups');
  assert.equal(addsUp(t, parts), false);
  // The words for each case, as the start of a sentence.
  assert.equal(whyNotAdded(tile('bar', [plain('avg')]), 0), 'Averages are not added up');
  assert.equal(whyNotAdded(tile('bar', [plain('sum', true)]), 0), 'Ratios are not added up');
  assert.equal(whyNotAdded(tile('bar', [namedMeasure('m1')]), 0), 'Achievement is not added up');
  assert.match(whyNotAdded(t, 0, parts), /^Revenue is not added up here, because a table it reads is the same for every group$/);
  assert.equal(whyNotAdded(t, 0), null);
  // Stacking asks the same gate.
  assert.equal(stackableTile(t, false), true);
  assert.equal(stackableTile(t, false, parts), false);
  assert.equal(stackMode(t, false, parts), 'side');
  // So do the ways of showing a value that add values up, and what the name of the value says.
  const dims = [{ table: 's1/sales', column: 'region' }];
  const shown = tile('bar', [namedMeasure('m2')], { dims, showAs: ['pct_total'] });
  assert.equal(effectiveShowAs(shown, 0, 'other'), 'pct_total');
  assert.equal(effectiveShowAs(shown, 0, 'other', parts), null);
  assert.ok(showAsOptions(shown, 'other', 0).includes('pct_total'));
  assert.ok(!showAsOptions(shown, 'other', 0, parts).includes('pct_total'));
  assert.ok(showAsOptions(shown, 'other', 0, parts).includes('rank'), 'a rank adds nothing up');
});

test('V4-D99: a share chart and a waterfall by category are refused with words by one gate, for the screen and every picture', () => {
  const ratio = namedMeasure('m1');
  const sum = namedMeasure('m2');
  const parts = [{ measure: 0, note: 'n', repeats: true }];
  assert.match(tileProblem(tile('waterfall', [ratio]), undefined, false), /^A waterfall adds one category to the next\. Achievement is not added up/);
  assert.equal(tileProblem(tile('waterfall', [ratio]), undefined, true), null, 'steps over time are differences');
  assert.equal(tileProblem(tile('waterfall', [sum]), undefined, false), null);
  assert.ok(tileProblem(tile('waterfall', [sum]), parts, false), 'a table repeated across the categories');
  assert.match(tileProblem(tile('share', [plain('avg')])), /^Averages are not added up, so it has no shares/);
  // A donut is a donut again (V4-D7): its own words.
  assert.equal(tileProblem(tile('donut', [ratio])), 'A donut needs parts that add up to a whole. Achievement does not. Try a bar chart.');
  assert.equal(tileProblem(tile('bar', [ratio])), null);
  // A waterfall steps over time when its first field is a date or a part of a date.
  assert.equal(stepsByDate({ dims: [{ grain: 'month_of_year' }] }, { columns: [{ dtype: 'number' }] }), true);
  assert.equal(stepsByDate({ dims: [{}] }, { columns: [{ dtype: 'date' }] }), true);
  assert.equal(stepsByDate({ dims: [{}] }, { columns: [{ dtype: 'text' }] }), false);
  // A picture asks the same gate: a refused tile is copied from the screen, never drawn again as a chart.
  const rows = [['a', 1]];
  const res = { rows, columns: [{ dtype: 'text' }, { dtype: 'number' }] };
  const charts = ['bar', 'line', 'waterfall'];
  assert.equal(pictureKind(tile('waterfall', [ratio]), res, charts), 'screen');
  assert.equal(pictureKind(tile('waterfall', [sum]), res, charts), 'chart');
  assert.equal(pictureKind(tile('waterfall', [sum]), { ...res, parts }, charts), 'screen');
  assert.equal(pictureKind(tile('waterfall', [ratio]), { ...res, columns: [{ dtype: 'date' }, { dtype: 'number' }] }, charts), 'chart');
  assert.equal(pictureKind(tile('bar', [ratio]), res, charts), 'chart');
  assert.equal(pictureKind(tile('bar', [ratio]), { ...res, rows: [] }, charts), 'screen');
  assert.equal(pictureKind(tile('bar', [ratio]), null, charts), 'screen');
});

test('V4-D85d: a pace scales the value so far up to the period, which a ratio does not have', () => {
  assert.equal(paceElapsed(tile('kpi', [namedMeasure('m2')]), 0.5), 0.5);
  assert.equal(paceElapsed(tile('kpi', [namedMeasure('m1')]), 0.5), null);
  assert.equal(paceElapsed(tile('kpi', [plain('avg')]), 0.5), null);
  assert.equal(paceElapsed(tile('kpi', [namedMeasure('m2')]), 0.5, [{ measure: 0, repeats: true }]), null);
  // With no pace the value itself is compared with the target.
  const sum = targetInfo(40, 100, paceElapsed(tile('kpi', [namedMeasure('m2')]), 0.5));
  assert.equal(sum.projected, 80);
  const rat = targetInfo(0.8, 1, paceElapsed(tile('kpi', [namedMeasure('m1')]), 0.5));
  assert.equal(rat.projected, null);
  assert.equal(rat.onTrack, false);
});

test('V4-D85d: a measure that does not add up gets no share, running total or year to date', () => {
  const t = (id, how) => tile('bar', [namedMeasure(id)], { dims: [{ table: 's1/sales', column: 'date', grain: 'month' }], showAs: [how] });
  for (const how of ['pct_total', 'running', 'ytd']) {
    assert.equal(effectiveShowAs(t('m1', how), 0, 'date'), null, how);
    assert.equal(effectiveShowAs(t('m2', how), 0, 'date'), how, how);
  }
  // A change or a rank is a difference or an order: any value has it.
  assert.equal(effectiveShowAs(t('m1', 'change'), 0, 'date'), 'change');
  assert.equal(effectiveShowAs(t('m1', 'rank'), 0, 'date'), 'rank');
});

test('V4-D85d: a share chart is not offered for a value that does not add up', () => {
  const tables = [{ id: 's1/sales', columns: [{ name: 'region', dtype: 'text' }, { name: 'amount', dtype: 'number' }] }];
  const dims = [{ table: 's1/sales', column: 'region' }];
  const kinds = (m) => suggestTypes(tile('bar', [m], { dims }), tables, 5).map((s) => s.type);
  assert.ok(kinds(namedMeasure('m2')).includes('share'));
  assert.ok(!kinds(namedMeasure('m1')).includes('share'));
  assert.ok(!kinds(plain('avg')).includes('share'));
});

test('names: new, unique, and not too long', () => {
  const all = [ratio, revenue];
  assert.equal(nameProblem('Margin', all), null);
  assert.match(nameProblem('', all), /Give the measure a name/);
  assert.match(nameProblem('  achievement ', all), /Another measure is called Achievement/);
  assert.equal(nameProblem('Achievement', all, 'm1'), null, 'the measure being edited keeps its name');
  assert.match(nameProblem('x'.repeat(61), all), /60 letters/);
  assert.equal(nameProblem('x'.repeat(60), all), null);
});

test('formulas: a hostile length is refused before the engine is asked', () => {
  assert.match(formulaProblem('  '), /Write a formula/);
  assert.equal(formulaProblem('SUM([Sales].[amount])'), null);
  assert.equal(formulaProblem('1'.repeat(MAX_FORMULA)), null);
  assert.match(formulaProblem('1'.repeat(MAX_FORMULA + 1)), /10,000 characters/);
});

test('tiles that use a measure are listed in the order of the dashboards', () => {
  const dashboards = [
    { id: 'd1', title: 'Main', tiles: [tile('kpi', [namedMeasure('m1')], { id: 'a', title: 'Goal' }), tile('bar', [plain('sum')], { id: 'b' })] },
    { id: 'd2', title: 'Second', tiles: [tile('bar', [plain('sum'), namedMeasure('m1')], { id: 'c' })] },
  ];
  assert.deepEqual(
    tilesWith(dashboards, 'm1').map((u) => [u.dashboardTitle, u.tile, u.title]),
    [['Main', 'a', 'Goal'], ['Second', 'c', '']],
  );
  assert.equal(tilesWith(dashboards, 'm2').length, 0);
  assert.deepEqual(measuresOf(tile('bar', [namedMeasure('m1'), plain('sum'), namedMeasure('m1'), namedMeasure('m2')])), ['m1', 'm2']);
});

test('the formula field offers the tables of a measure, which has no table of its own', () => {
  const col = (name, dtype = 'number') => ({ name, dtype, raw_type: '' });
  const tables = [
    { id: 's1/sales', source_id: 's1', name: 'sales', label: 'sales', columns: [col('amount'), col('date', 'date')], rows: 1 },
    { id: 's2/targets', source_id: 's2', name: 'targets', label: 'Store targets', columns: [col('target')], rows: 1 },
  ];
  const scope = { table: null, tables, rels: [], kind: 'measure', reach: null };
  // A bracket starts a table: a measure has no column of its own.
  const open = suggest('SUM([', 5, scope);
  assert.deepEqual(open.items.map((i) => i.label), ['sales', 'Store targets']);
  assert.equal(open.items[0].insert, '[sales].[');
  assert.equal(open.items[0].detail, 'Table');
  // After the table, its columns.
  const cols = suggest('SUM([Store targets].[', 21, scope);
  assert.deepEqual(cols.items.map((i) => i.label), ['target']);
  // A word offers the totals, and a column function too (a total can wrap arithmetic).
  const fns = suggest('SU', 2, scope);
  assert.ok(fns.items.some((i) => i.label === 'SUM'));
  // The totals come first in a measure: SU offers SUM, then SUBSTITUTE.
  assert.deepEqual(fns.items.map((i) => i.label).slice(0, 3), ['SUM', 'SUMIF', 'SUBSTITUTE']);
  // A column offers the row functions first; its totals add up a related table (v5 build 06).
  const colScope = { ...scope, table: tables[0], kind: 'column' };
  assert.deepEqual(suggest('SU', 2, colScope).items.map((i) => i.label), ['SUBSTITUTE', 'SUM', 'SUMIF']);
});

test('V4-D85d: the small bars of a bar chart are summed into Other only when their values add up', () => {
  const cats = ['a', 'b', 'c', 'd', 'e', 'f'];
  const vals = [10, 8, 6, 4, 2, 1];
  const out = foldSeries(cats, vals, 1, 4, true, 'OTHER');
  assert.deepEqual(out.categories, ['a', 'b', 'c', 'OTHER']);
  assert.deepEqual(out.values, [10, 8, 6, 7], 'Other is 4 + 2 + 1');
  // A value that does not add up keeps every bar: 7 of averages would be a sum of averages.
  assert.equal(foldSeries(cats, vals, 1, 4, false, 'OTHER'), null);
  assert.equal(foldSeries(cats, vals, 2, 4, true, 'OTHER'), null, 'more than one series');
  assert.equal(foldSeries(cats, vals, 1, 6, true, 'OTHER'), null, 'they fit');
  // A gap counts as nothing.
  assert.deepEqual(foldSeries(cats, [1, 2, 3, null, 5, 6], 1, 4, true, 'O').values, [1, 2, 3, 11]);
  assert.deepEqual(splitKept(['g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7', 'g8', 'g9', 'g10'], 8, true), {
    kept: ['g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7'],
    other: true,
  });
  const every = splitKept(['g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7', 'g8', 'g9', 'g10'], 8, false);
  assert.equal(every.kept.length, 10);
  assert.equal(every.other, false);
  assert.deepEqual(splitKept(['g1', 'g2'], 8, true), { kept: ['g1', 'g2'], other: false });
});

// The files that draw the chart and the sheet import Svelte and ECharts, which node cannot load. What they
// must call is pinned here by reading them, so a change that drops the gate from one fails a test.
const source = (f) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8');

test('the code that draws asks the gate (read from the files that node cannot load)', () => {
  const common = source('lib/charts/common.ts');
  assert.match(common, /foldSeries\(p\.categories, p\.series\[0\]\?\.values \?\? \[\], p\.series\.length, max, addsUp, FOLDED\)/);
  assert.match(common, /splitKept\(ranked, MAX_SERIES, tileAddsUp\(tile, res\.parts\)\)/);
  const cartesian = source('lib/charts/cartesian.ts');
  assert.match(cartesian, /tileAddsUp\(tile, res\.parts\)/);
  assert.match(cartesian, /fold\(whole, Math\.max\(4, room\), addsUp\)/);
  assert.match(cartesian, /lineSource\(\{ perValue, multi, stacked, addsUp \}\)/);
  assert.match(cartesian, /stackMode\(tile, perValue, res\.parts\)/);
  assert.match(cartesian, /source !== 'points'/, 'no trend over a split of values that do not add up');
  const body = source('components/dash/TileBody.svelte');
  assert.match(body, /tileProblem\(tile, res\.parts, stepsByDate\(tile, res\), res\.rows\)/);
  const output = source('lib/output.ts');
  assert.match(output, /pictureKind\(tile, res, CHART_TYPES\) === 'chart'/);
  assert.match(output, /pictureNote\(res, built\.note, built\.gateNote,/);
  assert.match(output, /withNamedFormats\(tile\)/);
  const kpi = source('components/dash/KpiView.svelte');
  assert.match(kpi, /paceElapsed\(tile, kpi\.elapsed, parts\)/);
  const viewer = source('viewer/Viewer.svelte');
  assert.match(viewer, /setKnownFromFile\(data\.measures\)/);
  // A text or a picture tile has no result: an exported file with one drew nothing at all when this read `res.parts`.
  assert.doesNotMatch(viewer, /res\.parts/);
  const sheet = source('components/data/MeasureSheet.svelte');
  assert.match(sheet, /formulaProblem\(formula\)/);
  assert.match(sheet, /nameProblem\(name, all, editing\)/);
  assert.match(sheet, /onclose=\{close\}/, 'the sheet asks before it closes with typing');
  const state = source('lib/state.svelte.ts');
  assert.match(state, /setKnownMeasures\(v\?\.workspace\.measures, v\?\.measure_info\)/);
});
