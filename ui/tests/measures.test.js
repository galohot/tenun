// Named measures (build 03): what the app knows about them, and the places that used to add
// already-added numbers on the client (V4-D85d). Pure functions, no browser.
import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import {
  addsUp,
  additive,
  formatOf,
  formulaProblem,
  MAX_FORMULA,
  measuresOf,
  nameOf,
  nameProblem,
  namedMeasure,
  paceElapsed,
  setKnownFromFile,
  setKnownMeasures,
  tilesWith,
  withNamedFormats,
} from '../src/lib/measures.ts';
import { foldable, splitKeep, stackableTile, stackMode, unitOf, sameUnit, waterfallProblem } from '../src/lib/charts/math.ts';
import { linesAllowed } from '../src/lib/charts/lines.ts';
import { effectiveShowAs } from '../src/lib/showas.ts';
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

test('a value with no format of its own is written the way its measure is', () => {
  const t = tile('bar', [namedMeasure('m2'), plain('sum')]);
  assert.deepEqual(formatOf(t, 0), { currency: 'IDR', compact: true });
  assert.equal(formatOf(t, 1), null);
  assert.deepEqual(withNamedFormats(t).formats, [{ currency: 'IDR', compact: true }, null]);
  // The tile's own format wins.
  const own = tile('bar', [namedMeasure('m2')], { formats: [{ decimals: 1 }] });
  assert.deepEqual(withNamedFormats(own).formats, [{ decimals: 1 }]);
  assert.equal(withNamedFormats(own), own, 'nothing to add: the same tile');
  // No named measure, or one with no format: the same tile.
  const none = tile('bar', [plain('sum')]);
  assert.equal(withNamedFormats(none), none);
  const bare = tile('bar', [namedMeasure('m1')]);
  assert.equal(withNamedFormats(bare), bare);
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

test('V4-D85d: reference lines on a split chart read a total that a ratio does not have', () => {
  assert.equal(linesAllowed({ percent: false, perValue: false, multi: true, addsUp: true }), true);
  assert.equal(linesAllowed({ percent: false, perValue: false, multi: true, addsUp: false }), false);
  // One series, or one series for each value: nothing is added.
  assert.equal(linesAllowed({ percent: false, perValue: false, multi: false, addsUp: false }), true);
  assert.equal(linesAllowed({ percent: false, perValue: true, multi: true, addsUp: false }), true);
  assert.equal(linesAllowed({ percent: true, perValue: false, multi: false, addsUp: true }), false);
});

test('V4-D85d: a waterfall by category adds the steps up, so it needs values that add up', () => {
  assert.equal(waterfallProblem(namedMeasure('m1'), false) !== null, true);
  assert.equal(waterfallProblem(namedMeasure('m2'), false), null);
  assert.equal(waterfallProblem(plain('avg'), false) !== null, true);
  // By date each step is a difference, which any value has.
  assert.equal(waterfallProblem(namedMeasure('m1'), true), null);
  assert.equal(waterfallProblem(undefined, false), null);
});

test('V4-D85d: a pace scales the value so far up to the period, which a ratio does not have', () => {
  assert.equal(paceElapsed(tile('kpi', [namedMeasure('m2')]), 0.5), 0.5);
  assert.equal(paceElapsed(tile('kpi', [namedMeasure('m1')]), 0.5), null);
  assert.equal(paceElapsed(tile('kpi', [plain('avg')]), 0.5), null);
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
  // A column has no totals, so it is offered the row functions only.
  const colScope = { ...scope, table: tables[0], kind: 'column' };
  assert.ok(!suggest('SU', 2, colScope).items.some((i) => i.label === 'SUM'));
});
