// Which "Show as" choices a tile offers, and how they combine with stacking (build 06).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dim0Kind, effectiveShowAs, groupCount, showAsOptions, withShowAs } from '../src/lib/showas.ts';
import { stackMode, unitOf } from '../src/lib/charts/math.ts';

const sum = { table: 't', column: 'amount', agg: 'sum' };
const dim = { table: 't', column: 'cat' };
const split = { table: 't', column: 'region' };

test('a category offers share and rank only', () => {
  const tile = { type: 'bar', dims: [dim], measures: [sum] };
  assert.deepEqual(showAsOptions(tile, 'other'), ['value', 'pct_total', 'rank']);
});

test('a split adds the share of the group', () => {
  const tile = { type: 'bar', dims: [dim, split], measures: [sum] };
  assert.deepEqual(showAsOptions(tile, 'other'), ['value', 'pct_total', 'pct_group', 'rank']);
  // Several values draw one series each and have no split.
  assert.equal(groupCount({ type: 'bar', dims: [dim, split], measures: [sum, sum] }), 1);
});

test('a date adds the ways that read in order, and a part of a date everything but year to date', () => {
  const tile = { type: 'line', dims: [dim], measures: [sum] };
  const date = showAsOptions(tile, 'date');
  for (const o of ['running', 'moving_3', 'moving_7', 'moving_12', 'change', 'pct_change', 'ytd']) assert.ok(date.includes(o), o);
  const part = showAsOptions(tile, 'part');
  assert.ok(part.includes('running') && !part.includes('ytd'));
});

test('a number tile, a share and a tile with no group offer nothing', () => {
  assert.deepEqual(showAsOptions({ type: 'kpi', dims: [dim], measures: [sum] }, 'date'), []);
  assert.deepEqual(showAsOptions({ type: 'share', dims: [dim], measures: [sum] }, 'other'), []);
  assert.deepEqual(showAsOptions({ type: 'table', dims: [], measures: [sum] }, 'none'), []);
});

test('table and pivot count the fields the way the engine does', () => {
  assert.equal(groupCount({ type: 'table', dims: [dim, split, dim], measures: [sum] }), 3);
  assert.equal(groupCount({ type: 'pivot', dims: [dim], measures: [sum], cols: split }), 2);
  assert.deepEqual(showAsOptions({ type: 'pivot', dims: [dim], measures: [sum], cols: split }, 'other'), ['value', 'pct_total', 'pct_group', 'rank']);
});

test('what the engine really does: year to date needs a date, a group share needs a split', () => {
  const one = { type: 'bar', dims: [dim], measures: [sum], showAs: ['ytd'] };
  assert.equal(effectiveShowAs(one, 0, 'other'), null);
  assert.equal(effectiveShowAs(one, 0, 'date'), 'ytd');
  const group = { type: 'bar', dims: [dim], measures: [sum], showAs: ['pct_group'] };
  assert.equal(effectiveShowAs(group, 0, 'other'), 'pct_total');
  assert.equal(effectiveShowAs({ ...group, dims: [dim, split] }, 0, 'other'), 'pct_group');
  assert.equal(effectiveShowAs({ ...group, type: 'kpi' }, 0, 'other'), null);
  assert.equal(effectiveShowAs({ ...group, showAs: ['value'] }, 0, 'other'), null);
});

test('the kind of the first group-by field', () => {
  assert.equal(dim0Kind('date', 'month'), 'date');
  assert.equal(dim0Kind('datetime', 'weekday'), 'part');
  assert.equal(dim0Kind('text', null), 'other');
  assert.equal(dim0Kind(undefined, null), 'other');
});

test('showAs stays in step with the values', () => {
  assert.deepEqual(withShowAs(undefined, 1, 'running', 3), [null, 'running']);
  assert.equal(withShowAs([null, 'running'], 1, 'value', 3), undefined);
  assert.deepEqual(withShowAs(['rank', 'running'], 1, null, 2), ['rank']);
});

test('ratios, ranks and percent changes cannot be stacked', () => {
  const ratio = { ...sum, per: { table: 't', column: 'qty', agg: 'sum' } };
  assert.equal(stackMode({ measures: [ratio] }, false), 'side');
  assert.equal(stackMode({ measures: [sum], showAs: ['rank'] }, false), 'side');
  assert.equal(stackMode({ measures: [sum], showAs: ['pct_change'] }, false), 'side');
  // A running total or a share of the total adds up.
  assert.equal(stackMode({ measures: [sum], showAs: ['running'] }, false), 'stacked');
  assert.equal(stackMode({ measures: [sum], showAs: ['pct_total'] }, false), 'stacked');
});

test('a share is a unit of its own', () => {
  assert.equal(unitOf(sum, { currency: 'IDR' }, 'pct_total'), 'percent');
  assert.equal(unitOf(sum, null, 'rank'), 'rank');
  assert.notEqual(unitOf(sum), unitOf({ ...sum, per: { table: 't', column: 'qty', agg: 'sum' } }));
});
