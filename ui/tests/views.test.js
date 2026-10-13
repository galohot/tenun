// Saved views (v5 build 03): the state a view keeps, when the dashboard still reads as it, and what opening one does.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { currentState, newViewId, sameState, tilesWithView, viewClicks, viewDrills, viewOf } from '../src/lib/views.ts';

const tiles = [
  { id: 'f', type: 'filter', dims: [{ table: 's', column: 'region' }], filter: { style: 'list', values: ['Jawa'], single: true } },
  { id: 'd', type: 'filter', dims: [{ table: 'o', column: 'date' }], filter: { style: 'dates', preset: 'this_quarter' } },
  { id: 'b', type: 'bar', dims: [{ table: 'o', column: 'date', grain: 'year' }], drill: {} },
  { id: 'c', type: 'bar', dims: [{ table: 's', column: 'store' }] },
];
const click = { table: 's', column: 'store', grain: null, op: 'in', values: ['A'], fromTile: 'c' };

test('a view keeps the clicks, the selections, the grain and the drill paths', () => {
  const s = currentState({ tiles, grain: 'month' }, [click], { b: { path: [{ value: '2026-01-01' }] }, gone: { path: [{}] } });
  assert.deepEqual(s, {
    filters: [click],
    selections: { f: { values: ['Jawa'] }, d: { preset: 'this_quarter' } },
    grain: 'month',
    drill: { b: [{ value: '2026-01-01' }] },
  });
  const v = viewOf('v1', '  Java  ', s);
  assert.equal(v.name, 'Java');
  assert.equal(viewOf('v2', ' ', s).name, 'View');
  assert.equal(newViewId([]), 'v1');
  assert.equal(newViewId([{ id: 'v1' }, { id: 'v7' }, { id: 'x' }]), 'v8');
});

test('the dashboard reads as a view whatever the order, and not after a change', () => {
  const a = currentState({ tiles, grain: null }, [click, { ...click, fromTile: 'b' }], {});
  const b = { ...a, filters: [...a.filters].reverse() };
  assert.ok(sameState(a, b));
  assert.ok(!sameState(a, { ...a, grain: 'year' }));
  assert.ok(!sameState(a, { ...a, filters: [click] }));
  assert.ok(!sameState(a, { ...a, drill: { b: [{}] } }));
  // An empty drill path or an empty selection is the same as none.
  assert.ok(sameState({ ...a, drill: { b: [] } }, a));
});

test('opening a view sets the selections, and keeps what the view does not know out', () => {
  const view = viewOf('v1', 'Bali', { filters: [click, { ...click, fromTile: 'gone' }], selections: { f: { values: ['Bali'] } }, grain: null, drill: { b: [{}], c: [{}], gone: [{}] } });
  const next = tilesWithView(tiles, view);
  assert.deepEqual(next[0].filter, { style: 'list', single: true, values: ['Bali'] });
  assert.deepEqual(next[1].filter, { style: 'dates' }, 'a filter tile the view keeps nothing for is cleared');
  assert.equal(next[2], tiles[2]);
  assert.deepEqual(viewClicks(tiles, view), [click], 'a click from a tile that is gone is dropped');
  assert.deepEqual(viewDrills(tiles, view), { b: { path: [{}] } }, 'only tiles that drill');
});

test('a copied dashboard keeps its views, pointed at the copied tiles', async () => {
  const { viewForCopy } = await import('../src/lib/clipboard.ts');
  const ids = new Map([['f', 'f2'], ['c', 'c2'], ['b', 'b2']]);
  const v = { id: 'v1', name: 'Java', filters: [{ ...click, to: ['b'] }], selections: { f: { values: ['Jawa'] } }, grain: null, drill: { b: [{}] } };
  const got = viewForCopy(v, ids);
  assert.equal(got.filters[0].fromTile, 'c2');
  assert.deepEqual(got.filters[0].to, ['b2']);
  assert.deepEqual(got.selections, { f2: { values: ['Jawa'] } });
  assert.deepEqual(got.drill, { b2: [{}] });
});
