// A workspace as a site (build 12): its settings cleaned up, the navigation and its groups, and the filter a
// drill-through opens another page with, for each kind of tile.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DRILL, drillOf, moveGroup, navOf, normaliseSite, setGroup, visiblePages } from '../src/lib/site.ts';

const dash = (id, title, extra = {}) => ({ id, title, refresh: 60, tiles: [], ...extra });
const DASHES = [dash('d1', 'Coffee overview'), dash('d2', 'More views'), dash('d3', 'Stores', { hidden: true }), dash('d4', '  ')];

test('site settings are cleaned up: off by default, pages that exist, one group each, a visible home', () => {
  assert.deepEqual(normaliseSite(undefined, DASHES, 'Coffee'), { on: false, title: 'Coffee', nav: 'top', home: 'd1', groups: [], footer: '' });
  const s = normaliseSite(
    { on: true, title: '  ', nav: 'side', home: 'd3', footer: '  Made by\n  the team ', groups: [{ name: 'Sales', pages: ['d2', 'gone', 'd1'] }, { name: ' ', pages: ['d1', 'd3'] }] },
    DASHES,
    'Coffee',
  );
  // The hidden page cannot be home; d1 is in the first group only; a group with no name gets one.
  assert.deepEqual(s, { on: true, title: 'Coffee', nav: 'side', home: 'd1', groups: [{ name: 'Sales', pages: ['d2', 'd1'] }, { name: 'Group', pages: ['d3'] }], footer: 'Made by the team' });
  assert.equal(normaliseSite({ on: true, home: 'd2' }, DASHES).home, 'd2');
  // No visible page: no home.
  assert.equal('home' in normaliseSite({}, [dash('x', 'X', { hidden: true })]), false);
  assert.deepEqual(visiblePages(DASHES).map((d) => d.id), ['d1', 'd2', 'd4']);
});

test('the navigation lists pages in no group first, then each group; hidden pages are left out', () => {
  const s = normaliseSite({ on: true, groups: [{ name: 'Sales', pages: ['d2', 'd3'] }, { name: 'Empty', pages: [] }] }, DASHES);
  assert.deepEqual(navOf(s, DASHES), {
    pages: [{ id: 'd1', title: 'Coffee overview' }, { id: 'd4', title: 'Untitled' }],
    groups: [{ name: 'Sales', pages: [{ id: 'd2', title: 'More views' }] }, { name: 'Empty', pages: [] }],
  });
  assert.deepEqual(moveGroup(s, 1, 0).groups.map((g) => g.name), ['Empty', 'Sales']);
  assert.equal(moveGroup(s, 0, 5), s);
  // A page goes into one group and leaves any other.
  assert.deepEqual(setGroup(s, 'd2', 1).groups.map((g) => g.pages), [['d3'], ['d2']]);
  assert.deepEqual(setGroup(s, 'd2', null).groups.map((g) => g.pages), [['d3'], []]);
});

test('a drill-through opens its page with only the clicked value, for every kind of mark', () => {
  const from = { id: 'd1', title: 'Coffee overview' };
  const store = { table: 's1/Orders', column: 'store_id' };
  const to = { action: { page: 'd3' } };
  // A bar, a line point, a slice, a table row: the first field, the value.
  for (const type of ['bar', 'line', 'pie', 'donut', 'table']) {
    const d = drillOf({ type, dims: [store], measures: [], ...to }, from, DASHES, 6);
    assert.deepEqual(d, { page: 'd3', filter: { table: 's1/Orders', column: 'store_id', grain: null, op: 'in', values: [6], fromTile: DRILL }, from }, type);
  }
  // A place on a map: every name of it, with its standard name.
  const geo = { codes: ['ID-JB'], raw: [['Jawa Barat', 'Jabar']], names: ['Jawa Barat'] };
  const m = drillOf({ type: 'map', dims: [{ table: 's1/Sales', column: 'place' }], measures: [], ...to }, from, DASHES, 'ID-JB', undefined, undefined, undefined, geo);
  assert.deepEqual(m.filter.values, ['Jawa Barat', 'Jabar']);
  assert.equal(m.filter.label, 'Jawa Barat');
  // Other: everything but the named groups.
  const o = drillOf({ type: 'bar', dims: [store], measures: [], ...to }, from, DASHES, '\u0000other', [1, 2]);
  assert.deepEqual([o.filter.op, o.filter.values], ['not_in', [1, 2]]);
  // No action, a page that is gone, this page: no drill.
  assert.equal(drillOf({ type: 'bar', dims: [store], measures: [] }, from, DASHES, 6), null);
  assert.equal(drillOf({ type: 'bar', dims: [store], measures: [], action: { page: 'gone' } }, from, DASHES, 6), null);
  assert.equal(drillOf({ type: 'bar', dims: [store], measures: [], action: { page: 'd1' } }, from, DASHES, 6), null);
});
