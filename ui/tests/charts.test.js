// The arithmetic behind stack modes, the 100 % view and the waterfall (build 04).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { median } from '../src/lib/charts/lines.ts';
import { assignSlots, farFromZero, labelledSteps, normalizeShares, stackMode, stackable, unitOf, waterfallSteps } from '../src/lib/charts/math.ts';

const m = (agg) => ({ table: 't', column: 'c', agg });
const shape = (steps) => steps.map((s) => [s.kind, s.from, s.to, s.value]);

test('100 %: every category adds up to 1', () => {
  const { shares, totals } = normalizeShares([
    [10, 0, 50],
    [30, 0, 50],
  ]);
  assert.deepEqual(totals, [40, 0, 100]);
  assert.deepEqual(shares[0], [0.25, null, 0.5]);
  assert.deepEqual(shares[1], [0.75, null, 0.5]);
  for (let i = 0; i < 3; i++) {
    if (totals[i]) assert.equal(shares.reduce((s, row) => s + (row[i] ?? 0), 0), 1);
  }
});

test('100 %: a gap stays a gap and negatives count by their size', () => {
  const { shares } = normalizeShares([[20, null], [-20, 5]]);
  assert.deepEqual(shares[0], [0.5, null]);
  assert.deepEqual(shares[1], [-0.5, 1]);
});

test('stack default: stacked for sums and counts, side by side for the rest', () => {
  assert.deepEqual(['sum', 'count'].map((a) => stackable([a])), [true, true]);
  assert.deepEqual(['avg', 'min', 'max', 'median', 'count_distinct'].map((a) => stackable([a])), [false, false, false, false, false]);
  assert.equal(stackMode({ measures: [m('sum')] }, false), 'stacked');
  assert.equal(stackMode({ measures: [m('count')] }, false), 'stacked');
  assert.equal(stackMode({ measures: [m('avg')] }, false), 'side');
  assert.equal(stackMode({ measures: [m('sum')], stack: 'percent' }, false), 'percent');
  // An average cannot be stacked or shown as 100 %, whatever was saved.
  assert.equal(stackMode({ measures: [m('avg')], stack: 'percent' }, false), 'side');
  // Several values: every one of them has to add up. A split looks at the first value only.
  assert.equal(stackMode({ measures: [m('sum'), m('avg')] }, true), 'side');
  assert.equal(stackMode({ measures: [m('sum'), m('avg')] }, false), 'stacked');
});

test('stack default for several values: side by side unless they share one unit', () => {
  const col = (column, agg = 'sum') => ({ table: 't', column, agg });
  // Amount (money) and Quantity (units): different columns, so side by side.
  assert.equal(stackMode({ measures: [col('amount'), col('quantity')] }, true), 'side');
  // The same currency on both: they add up.
  const idr = { currency: 'IDR' };
  assert.equal(stackMode({ measures: [col('amount'), col('cost')], formats: [idr, idr] }, true), 'stacked');
  // Money and a plain number never share a unit; two currencies do not either.
  assert.equal(stackMode({ measures: [col('amount'), col('cost')], formats: [idr, null] }, true), 'side');
  assert.equal(stackMode({ measures: [col('amount'), col('cost')], formats: [idr, { currency: 'USD' }] }, true), 'side');
  // Counts of rows in one table share a unit; a count and a sum of a column do not.
  assert.equal(stackMode({ measures: [col(null, 'count'), col('id', 'count')] }, true), 'stacked');
  assert.equal(stackMode({ measures: [col('amount'), col(null, 'count')] }, true), 'side');
  assert.equal(unitOf(col('amount')), unitOf(col('amount', 'count_distinct')));
  // A split on one value stays stacked, and a saved choice wins for several values.
  assert.equal(stackMode({ measures: [col('amount')] }, false), 'stacked');
  assert.equal(stackMode({ measures: [col('amount'), col('quantity')], stack: 'stacked' }, true), 'stacked');
  assert.equal(stackMode({ measures: [col('amount'), col('quantity')], stack: 'percent' }, true), 'percent');
  // An average still cannot be stacked, whatever was saved.
  assert.equal(stackMode({ measures: [col('amount'), col('quantity', 'avg')], stack: 'stacked' }, true), 'side');
});

test('colour slots: a group keeps its colour when others leave', () => {
  const memory = new Map();
  // The unfiltered render fills the slots in order.
  assert.deepEqual(assignSlots(memory, ['Cafe', 'Online', 'Shop', 'Tea'], 8), [0, 1, 2, 3]);
  // A filter takes Online and Shop away: Cafe and Tea keep slots 0 and 3.
  assert.deepEqual(assignSlots(memory, ['Cafe', 'Tea'], 8), [0, 3]);
  // The filter is lifted: everyone is back where they were.
  assert.deepEqual(assignSlots(memory, ['Cafe', 'Online', 'Shop', 'Tea'], 8), [0, 1, 2, 3]);
});

test('colour slots: a group never seen before takes the lowest free slot', () => {
  const memory = new Map();
  assignSlots(memory, ['a', 'b', 'c'], 8);
  // "d" appears while "b" is filtered out: slot 1 still belongs to "b", so "d" takes 3.
  assert.deepEqual(assignSlots(memory, ['a', 'c', 'd'], 8), [0, 2, 3]);
  assert.deepEqual(assignSlots(memory, ['a', 'b', 'c', 'd'], 8), [0, 1, 2, 3]);
});

test('colour slots: when every slot is held, a new group takes one that no shown group uses', () => {
  const memory = new Map();
  assignSlots(memory, ['a', 'b', 'c'], 3);
  // Only "x" and "a" are on screen. "x" is new and every slot is held; "b" (slot 1) is not shown.
  const [sa, sx] = assignSlots(memory, ['a', 'x'], 3);
  assert.equal(sa, 0);
  assert.equal(sx, 1);
  assert.equal(memory.has('b'), false);
  // More groups than slots wrap round without breaking the ones that are remembered.
  const wrapped = assignSlots(new Map(), ['a', 'b', 'c', 'd'], 3);
  assert.deepEqual(wrapped, [0, 1, 2, 0]);
});

test('waterfall by category: steps from 0 and a closing total', () => {
  const steps = waterfallSteps([100, 30, -20, 0], false);
  assert.deepEqual(shape(steps), [
    ['up', 0, 100, 100],
    ['up', 100, 130, 30],
    ['down', 130, 110, -20],
    ['flat', 110, 110, 0],
    ['total', 0, 110, 110],
  ]);
  assert.deepEqual(steps.map((s) => s.row), [0, 1, 2, 3, null]);
});

test('waterfall by category: a negative value goes below zero and the total follows', () => {
  const steps = waterfallSteps([-40, 10], false);
  assert.deepEqual(shape(steps), [
    ['down', 0, -40, -40],
    ['up', -40, -30, 10],
    ['total', 0, -30, -30],
  ]);
});

test('waterfall by date: the first period is the start, then the change, then the end', () => {
  const steps = waterfallSteps([100, 130, 110, 110], true);
  assert.deepEqual(shape(steps), [
    ['start', 0, 100, 100],
    ['up', 100, 130, 30],
    ['down', 130, 110, -20],
    ['flat', 110, 110, 0],
    ['end', 0, 110, 110],
  ]);
  assert.deepEqual(steps.map((s) => s.row), [0, 1, 2, 3, null]);
});

test('waterfall by date: a period with no value keeps the level', () => {
  const steps = waterfallSteps([50, null, 80], true);
  assert.deepEqual(shape(steps), [
    ['start', 0, 50, 50],
    ['flat', 50, 50, 0],
    ['up', 50, 80, 30],
    ['end', 0, 80, 80],
  ]);
});

test('waterfall: no rows, no bars', () => {
  assert.deepEqual(waterfallSteps([], true), []);
  assert.deepEqual(waterfallSteps([], false), []);
});

test('waterfall labels: the ends and the three biggest steps', () => {
  const steps = waterfallSteps([100, 5, -40, 20, -1, 60], false);
  const shown = [...labelledSteps(steps)].sort((a, b) => a - b);
  // Steps 0 (100), 2 (-40) and 5 (60) are the biggest; the total is the last bar.
  assert.deepEqual(shown, [0, 2, 5, 6]);
  const dated = waterfallSteps([100, 105, 65, 85, 84, 144], true);
  // The start and the end, and the changes of -40, +60 and +20.
  assert.deepEqual([...labelledSteps(dated)].sort((a, b) => a - b), [0, 2, 3, 5, 6]);
});

test('scatter helpers: median and axes that start at zero', () => {
  assert.equal(median([]), null);
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 1, 3, 2]), 2.5);
  assert.equal(farFromZero([10, 40, 90]), false);
  assert.equal(farFromZero([80, 90, 100]), true);
  assert.equal(farFromZero([-100, -90, -80]), true);
  assert.equal(farFromZero([-5, 20]), false);
  assert.equal(farFromZero([]), false);
});
