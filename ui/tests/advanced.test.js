// The arithmetic behind the Advanced charts (build 12): heat map steps, the funnel, small multiples
// and the combo rule.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { heatRange, heatStep, inkFor, contrast } from '../src/lib/charts/heat.ts';
import { funnelShape } from '../src/lib/charts/funnel.ts';
import { chartColumns, niceUp, sharedScale, splitCharts } from '../src/lib/charts/small.ts';
import { sameUnit } from '../src/lib/charts/math.ts';

test('heat map: steps are equal slices of the range', () => {
  assert.deepEqual(heatRange([3, null, 10, undefined, 7]), { lo: 3, hi: 10 });
  assert.equal(heatRange([null, null]), null);
  // Seven steps from 0 to 70: each step is ten wide, and the top value stays on the last step.
  assert.equal(heatStep(0, 0, 70, 7), 0);
  assert.equal(heatStep(9.9, 0, 70, 7), 0);
  assert.equal(heatStep(10, 0, 70, 7), 1);
  assert.equal(heatStep(69, 0, 70, 7), 6);
  assert.equal(heatStep(70, 0, 70, 7), 6);
  // Every cell the same: the middle step, not the lightest.
  assert.equal(heatStep(5, 5, 5, 7), 3);
});

test('heat map: ink is the one with the higher contrast', () => {
  assert.equal(inkFor('#eaf2fc'), '#1c1c1e');
  assert.equal(inkFor('#17488a'), '#ffffff');
  assert.ok(contrast('#ffffff', '#17488a') > 8);
});

test('funnel: widths, steps between stages, and the overall share', () => {
  const f = funnelShape([1000, 400, 100, 50]);
  assert.deepEqual(f.widths, [1, 0.4, 0.1, 0.05]);
  assert.deepEqual(f.steps, [null, 0.4, 0.25, 0.5]);
  assert.equal(f.overall, 0.05);
});

test('funnel: an empty stage, gaps and a single stage', () => {
  const f = funnelShape([200, 0, 50, null]);
  // A stage after an empty one has no step; a gap counts as zero.
  assert.deepEqual(f.steps, [null, 0, null, 0]);
  assert.equal(f.overall, 0);
  assert.equal(funnelShape([0, 0]).overall, null);
  assert.equal(funnelShape([7]).overall, null);
  assert.deepEqual(funnelShape([]).widths, []);
});

const result = (rows, keys) => ({
  columns: [{ name: 'p', role: 'dimension', dtype: 'text' }, { name: 'r', role: 'dimension', dtype: 'text' }, { name: 'v', role: 'measure', dtype: 'number' }],
  rows,
  ignored_filters: [],
  ms: 0,
  repeat: { keys, more: 3 },
});

test('small multiples: one result for each chart, with the repeat column dropped', () => {
  const res = result(
    [['a', 'N', 5], ['b', 'N', 7], ['a', 'S', 2], ['\u0000other', 'S', 1]],
    ['S', 'N'],
  );
  const parts = splitCharts(res);
  assert.deepEqual(parts.map((p) => p.key), ['S', 'N']);
  assert.deepEqual(parts[0].res.rows, [['a', 2], ['\u0000other', 1]]);
  assert.deepEqual(parts[1].res.rows, [['a', 5], ['b', 7]]);
  assert.equal(parts[0].res.columns.length, 2);
  assert.equal(parts[0].res.other, true);
  assert.equal(parts[1].res.other, undefined);
  assert.equal(parts[0].res.repeat, undefined);
  assert.deepEqual(splitCharts({ ...res, repeat: undefined }), []);
});

test('small multiples: the charts share a round scale from zero', () => {
  assert.equal(niceUp(0.9), 1);
  assert.equal(niceUp(7), 10);
  assert.equal(niceUp(2.2), 2.5);
  assert.equal(niceUp(240), 250);
  const parts = splitCharts(result([['a', 'N', 5], ['b', 'N', 7], ['a', 'S', 2]], ['N', 'S']));
  // The biggest value is 7: four steps of 2 reach 8.
  assert.deepEqual(sharedScale(parts), { min: 0, max: 8, interval: 2 });
  assert.equal(sharedScale([{ res: { rows: [['a', null]] } }]), null);
  // A negative value lowers the floor to a round number too.
  assert.deepEqual(sharedScale([{ res: { rows: [['a', -30], ['b', 80]] } }]), { min: -50, max: 100, interval: 50 });
  // 94 thousand: four steps of 25 thousand.
  assert.deepEqual(sharedScale([{ res: { rows: [['a', 94000]] } }]), { min: 0, max: 100000, interval: 25000 });
});

test('small multiples: columns follow the width and never exceed the charts', () => {
  assert.equal(chartColumns(1000, 12), 4);
  assert.equal(chartColumns(500, 12), 2);
  assert.equal(chartColumns(500, 1), 1);
  assert.equal(chartColumns(100, 6), 1);
});

test('combo: only values with one unit and one kind of aggregate may share an axis', () => {
  const m = (column, agg = 'sum') => ({ table: 't', column, agg });
  const idr = { currency: 'IDR' };
  // Revenue and cost, both in rupiah.
  assert.equal(sameUnit({ measures: [m('revenue'), m('cost')], formats: [idr, idr] }), true);
  // Money and units.
  assert.equal(sameUnit({ measures: [m('revenue'), m('quantity')], formats: [idr, null] }), false);
  // A sum and an average of the same column are different kinds of number.
  assert.equal(sameUnit({ measures: [m('revenue'), m('revenue', 'avg')] }), false);
  // Rows counted in one table share a unit; a count next to a sum does not.
  assert.equal(sameUnit({ measures: [m('id', 'count'), m(null, 'count')] }), true);
  assert.equal(sameUnit({ measures: [m(null, 'count'), m('revenue')] }), false);
  assert.equal(sameUnit({ measures: [m('revenue')] }), true);
});
