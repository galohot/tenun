// Filter reach and every-page filters (v5 build 02): which tiles a filter reaches, the filters a tile takes, the
// filters a filter tile makes, and the every-page filters a page takes from the other pages' filter tiles.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { everyId, everyPageFilters, everySource, isEvery, reachOf, reaches, tileFilter } from '../src/lib/filters.ts';
import { appliedFilters } from '../src/lib/tiles.ts';

const f = (fromTile, to) => ({ table: 's', column: 'region', grain: null, op: 'in', values: ['Jawa'], fromTile, ...(to ? { to } : {}) });

test('a reach is none, a list, or every tile', () => {
  assert.deepEqual(reachOf({}), undefined);
  assert.deepEqual(reachOf({ reach: 'none' }), []);
  assert.deepEqual(reachOf({ reach: ['a', 'b'] }), ['a', 'b']);
  assert.ok(reaches(f('r'), 'x'));
  assert.ok(!reaches(f('r'), 'r'), 'never its own tile');
  assert.ok(reaches(f('r', ['x']), 'x'));
  assert.ok(!reaches(f('r', ['x']), 'y'));
  assert.ok(!reaches(f('r', []), 'x'));
});

test('a tile takes the filters that reach it, in the order the engine counts them', () => {
  const filters = [f('r', ['s']), f('k'), f('p', [])];
  assert.deepEqual(appliedFilters({ id: 's' }, filters).map((x) => x.fromTile), ['r', 'k']);
  assert.deepEqual(appliedFilters({ id: 'z' }, filters).map((x) => x.fromTile), ['k']);
});

test("a filter tile's selection carries its reach", () => {
  const tile = { id: 'f', type: 'filter', dims: [{ table: 's', column: 'region' }], filter: { style: 'list', values: ['Jawa'] } };
  assert.equal(tileFilter(tile, 'text').to, undefined);
  assert.deepEqual(tileFilter({ ...tile, reach: ['k'] }, 'text').to, ['k']);
  assert.deepEqual(tileFilter({ ...tile, reach: 'none' }, 'text').to, []);
});

test('a page takes the every-page selections of the other pages, with their source', () => {
  const dashboards = [
    { id: 'd1', tiles: [{ id: 'f', type: 'filter', reach: ['x'], dims: [{ table: 's', column: 'region' }], filter: { style: 'list', values: ['Jawa'], everyPage: true } }] },
    { id: 'd2', tiles: [{ id: 'g', type: 'filter', dims: [{ table: 's', column: 'store' }], filter: { style: 'list', values: ['A'] } }] },
    { id: 'd3', tiles: [{ id: 'h', type: 'filter', dims: [{ table: 's', column: 'store' }], filter: { style: 'list', everyPage: true } }] },
  ];
  const got = everyPageFilters(dashboards, 'd2', () => 'text');
  assert.equal(got.length, 1, 'only a filter tile marked for every page, with something selected');
  assert.equal(got[0].fromTile, everyId('d1', 'f'));
  assert.equal(got[0].to, undefined, 'on other pages it reaches every tile');
  assert.deepEqual(got[0].values, ['Jawa']);
  assert.deepEqual(everyPageFilters(dashboards, 'd1', () => 'text'), [], 'not on its own page');
  assert.ok(isEvery(got[0].fromTile));
  assert.deepEqual(everySource(got[0].fromTile), { dashboard: 'd1', tile: 'f' });
  assert.equal(everySource('k'), null);
});

test('a copied tile keeps the reach to the tiles that came along, and leaves the others out', async () => {
  const { withReach, dashboardCopy } = await import('../src/lib/clipboard.ts');
  const ids = new Map([['a', 'x'], ['b', 'y']]);
  assert.deepEqual(withReach({ id: 'x', reach: ['b', 'c'] }, ids).reach, ['y']);
  assert.ok(!('reach' in withReach({ id: 'x', reach: ['c'] }, ids)), 'a list left empty filters every tile again');
  assert.equal(withReach({ id: 'x', reach: 'none' }, ids).reach, 'none');
  assert.ok(!('reach' in withReach({ id: 'x', reach: ['b'] }, new Map())), 'on another page');
  let n = 0;
  const copy = dashboardCopy(
    { id: 'd1', title: 'One', refresh: 0, tiles: [{ id: 'a', type: 'bar', reach: ['b'], dims: [], measures: [] }, { id: 'b', type: 'bar', dims: [], measures: [] }] },
    'One copy',
    (p) => `${p}${++n}`,
  );
  assert.deepEqual(copy.tiles[0].reach, [copy.tiles[1].id]);
  assert.notEqual(copy.tiles[1].id, 'b');
});
