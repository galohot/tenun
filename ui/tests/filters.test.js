// Filter tiles, saved filters and the one grain (build 10). The engine builds the same filters on its side.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { canExclude, colFilter, hasPeriod, isCleared, markFilter, nextDay, pinFilter, sameFilter, tileFilter, withGrain } from '../src/lib/filters.ts';

const dim = (column, grain) => ({ table: 't', column, ...(grain ? { grain } : {}) });
const filterTile = (filter, column = 'channel') => ({ id: 'f1', type: 'filter', title: '', measures: [], dims: [dim(column)], filter });

test('a list filter keeps the picked values, and nothing picked keeps everything', () => {
  assert.equal(tileFilter(filterTile({ style: 'list' }), 'text'), null);
  assert.equal(tileFilter(filterTile({ style: 'list', values: [] }), 'text'), null);
  const f = tileFilter(filterTile({ style: 'list', values: ['Online', 'Shop'] }), 'text');
  assert.deepEqual([f.op, f.values, f.fromTile, f.column], ['in', ['Online', 'Shop'], 'f1', 'channel']);
});

test('the blank value also means empty text', () => {
  assert.deepEqual(tileFilter(filterTile({ style: 'dropdown', values: [null] }), 'text').values, [null, '']);
  // A number column is blank only when NULL.
  assert.deepEqual(tileFilter(filterTile({ style: 'dropdown', values: [null] }), 'number').values, [null]);
});

test('a range keeps both ends, or one', () => {
  const r = (range) => tileFilter(filterTile({ style: 'range', range }, 'qty'), 'number');
  assert.deepEqual([r([3, 5]).op, r([3, 5]).values], ['between', [3, 5]]);
  assert.deepEqual([r([3, null]).op, r([3, null]).values], ['gte', [3]]);
  assert.deepEqual([r([null, 5]).op, r([null, 5]).values], ['lte', [5]]);
  assert.equal(r([null, null]), null);
});

test('dates: a preset, or a span of whole days that keeps its last day', () => {
  const d = (f) => tileFilter(filterTile(f, 'date'), 'date');
  assert.deepEqual([d({ style: 'dates', preset: 'last_12_months' }).op, d({ style: 'dates', preset: 'last_12_months' }).values], ['relative', ['last_12_months']]);
  const span = d({ style: 'dates', preset: 'custom', range: ['2026-03-10', '2026-03-15'] });
  assert.deepEqual([span.op, span.values], ['range', ['2026-03-10', '2026-03-16']]);
  assert.deepEqual(d({ style: 'dates', range: ['2026-03-10', null] }).values, ['2026-03-10']);
  assert.equal(d({ style: 'dates', preset: 'custom', range: [null, null] }), null);
  assert.equal(d({ style: 'dates' }), null);
  assert.equal(nextDay('2026-12-31'), '2027-01-01');
  assert.equal(nextDay('2028-02-28'), '2028-02-29');
});

test('a tile that is not a filter, or has no field, filters nothing', () => {
  assert.equal(tileFilter({ ...filterTile({ style: 'list', values: ['a'] }), type: 'bar' }, 'text'), null);
  assert.equal(tileFilter({ ...filterTile({ style: 'list', values: ['a'] }), dims: [] }, 'text'), null);
  assert.equal(isCleared({ style: 'list' }), true);
  assert.equal(isCleared({ style: 'list', values: ['a'] }), false);
  assert.equal(isCleared({ style: 'dates', preset: 'ytd' }), false);
});

test('one grain: only a date cut into periods follows it', () => {
  const t = (d) => ({ id: 'x', type: 'line', measures: [], dims: [d] });
  assert.equal(withGrain(t(dim('date', 'month')), 'year').dims[0].grain, 'year');
  // A date part, a text field and an unset grain keep what they had.
  const part = t(dim('date', 'weekday'));
  assert.equal(withGrain(part, 'year'), part);
  const text = t(dim('cat'));
  assert.equal(withGrain(text, 'year'), text);
  // "Auto" and a grain the tile already has change nothing, and keep the same object.
  const m = t(dim('date', 'month'));
  assert.equal(withGrain(m, null), m);
  assert.equal(withGrain(m, 'month'), m);
  // A date part is not a grain a dashboard can ask for.
  assert.equal(withGrain(m, 'weekday'), m);
  assert.equal(hasPeriod([text, part]), false);
  assert.equal(hasPeriod([text, m]), true);
});

test('a click makes the filter of its mark', () => {
  const bar = { type: 'bar', measures: [], dims: [dim('cat')] };
  const f = markFilter(bar, 'Tea');
  assert.deepEqual([f.op, f.values, f.column], ['in', ['Tea'], 'cat']);
  // "Other" is everything outside the top N.
  const other = markFilter(bar, '\u0000other', ['A', 'B']);
  assert.deepEqual([other.op, other.values, other.label], ['not_in', ['A', 'B'], 'Other']);
  // A histogram bin is a range of its number column.
  const hist = { type: 'histogram', measures: [{ table: 't', column: 'n', agg: 'count' }], dims: [] };
  assert.deepEqual(markFilter(hist, 0.1, undefined, { start: 0, width: 0.2 }).values, [0.1, 0.3]);
  assert.equal(markFilter(hist, 5), null);
  assert.equal(colFilter(undefined, 'x'), null);
  assert.equal(colFilter(dim('channel'), 'Shop').values[0], 'Shop');
});

test('a second click on the same mark is the same filter', () => {
  const a = { table: 't', column: 'c', op: 'in', values: ['x'] };
  assert.equal(sameFilter(a, { ...a }), true);
  assert.equal(sameFilter(a, { ...a, values: ['y'] }), false);
  assert.equal(sameFilter(a, { ...a, op: 'not_in' }), false);
  assert.equal(sameFilter({ ...a, values: [null] }, { ...a, values: [null] }), true);
});

test('Keep only and Exclude turn a click into a saved filter', () => {
  const click = { table: 't', column: 'c', op: 'in', values: ['x'], fromTile: 'bar1' };
  const keep = pinFilter(click, 'keep', 'pin_1');
  assert.deepEqual([keep.op, keep.values, keep.fromTile], ['in', ['x'], 'pin_1']);
  assert.deepEqual([pinFilter(click, 'exclude', 'pin_2').op, pinFilter(click, 'exclude', 'pin_2').fromTile], ['not_in', 'pin_2']);
  // Other: keeping it keeps the label; excluding it keeps the groups of the top N.
  const other = { table: 't', column: 'c', op: 'not_in', values: ['A', 'B'], label: 'Other', fromTile: 'bar1' };
  assert.equal(pinFilter(other, 'keep', 'p').label, 'Other');
  const ex = pinFilter(other, 'exclude', 'p');
  assert.deepEqual([ex.op, ex.values, ex.label], ['in', ['A', 'B'], 'Not Other']);
  // A bin of a histogram cannot be excluded.
  const bin = { table: 't', column: 'n', op: 'range', values: [0, 5], fromTile: 'h' };
  assert.equal(pinFilter(bin, 'exclude', 'p'), null);
  assert.equal(canExclude(bin), false);
  assert.equal(canExclude(click), true);
  assert.equal(pinFilter(bin, 'keep', 'p').op, 'range');
});
