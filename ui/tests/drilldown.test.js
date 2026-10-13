// Drill-down inside a chart (v5 build 01): what the Inspector offers, the steps down and up, the words of the drill
// bar, and the tile as it is drawn at a level (its first field is that level's field, so a click filters by it).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { crumbs, down, drillOptions, drillSetting, drillSettingValue, isTop, nextLevel, shownTile, toDepth, TOP, up } from '../src/lib/drilldown.ts';

const H = [
  { id: 'h1', name: 'Region › Store', levels: [{ table: 's/stores', column: 'region' }, { table: 's/stores', column: 'store' }] },
  { id: 'h2', name: 'Category › Product', levels: [{ table: 'p/products', column: 'category' }, { table: 'p/products', column: 'product' }] },
];

test('a date by month drills to days; a field in a hierarchy offers it; a map and a number do not drill', () => {
  const byMonth = { type: 'bar', dims: [{ table: 'o/orders', column: 'date', grain: 'month' }] };
  assert.deepEqual(
    drillOptions(byMonth, H).map((o) => o.label),
    ['Off', 'By date: month › day'],
  );
  const byYear = { type: 'line', dims: [{ table: 'o/orders', column: 'date', grain: 'year' }] };
  assert.equal(drillOptions(byYear, H)[1].label, 'By date: year › quarter › month › day');
  const byRegion = { type: 'pie', dims: [{ table: 's/stores', column: 'region' }] };
  assert.deepEqual(drillOptions(byRegion, H).map((o) => o.value), ['', 'h1']);
  // The lowest field of a hierarchy has nothing below it.
  assert.deepEqual(drillOptions({ type: 'bar', dims: [{ table: 's/stores', column: 'store' }] }, H), []);
  assert.deepEqual(drillOptions({ type: 'map', dims: [{ table: 's/stores', column: 'region' }] }, H), []);
  assert.deepEqual(drillOptions({ type: 'kpi', dims: [{ table: 'o/orders', column: 'date', grain: 'year' }] }, H), []);
  // A day has no smaller grain.
  assert.deepEqual(drillOptions({ type: 'bar', dims: [{ table: 'o/orders', column: 'date', grain: 'day' }] }, H), []);
  assert.deepEqual(drillSetting(''), undefined);
  assert.deepEqual(drillSetting('date'), {});
  assert.deepEqual(drillSetting('h1'), { hierarchy: 'h1' });
  assert.equal(drillSettingValue(undefined), '');
  assert.equal(drillSettingValue({}), 'date');
  assert.equal(drillSettingValue({ hierarchy: 'h2' }), 'h2');
});

test('steps go down with a value or for every value, and back up', () => {
  let s = down(TOP, '2026-01-01');
  assert.deepEqual(s, { path: [{ value: '2026-01-01' }] });
  s = nextLevel(s);
  assert.deepEqual(s, { path: [{ value: '2026-01-01' }, {}] });
  // A blank value is a value; it is not "every value".
  assert.deepEqual(down(TOP, null), { path: [{ value: null }] });
  assert.ok('value' in down(TOP, null).path[0]);
  assert.ok(!('value' in nextLevel(TOP).path[0]));
  assert.deepEqual(up(s), { path: [{ value: '2026-01-01' }] });
  assert.deepEqual(toDepth(s, 0), TOP);
  assert.ok(isTop(toDepth(s, 0)) && !isTop(s));
});

test('the drill bar names the path and what is shown', () => {
  const fmt = (v, g) => (g === 'year' ? String(v).slice(0, 4) : g === 'quarter' ? `Q${(Number(String(v).slice(5, 7)) + 2) / 3} ${String(v).slice(0, 4)}` : String(v));
  const info = {
    levels: ['Year', 'Quarter', 'Month', 'Day'],
    grains: ['year', 'quarter', 'month', 'day'],
    at: 2,
    path: [{ value: '2026-01-01' }, { value: '2026-04-01' }],
    canDown: true,
  };
  assert.deepEqual(crumbs(info, fmt), {
    parts: [
      { label: 'All years', depth: 0 },
      { label: '2026', depth: 1 },
      { label: 'Q2 2026', depth: 2 },
    ],
    showing: 'by month',
  });
  const every = { levels: ['Region', 'Store'], grains: [null, null], at: 1, path: [{}], canDown: false };
  assert.deepEqual(crumbs(every, fmt).parts.map((p) => p.label), ['All regions', 'Every region']);
  const category = { levels: ['Category', 'Product'], grains: [null, null], at: 0, path: [], canDown: true };
  assert.equal(crumbs(category, fmt).parts[0].label, 'All categories');
});

test('a drilled tile is drawn with the field of its level', () => {
  const tile = { dims: [{ table: 'o/orders', column: 'date', grain: 'year' }, { table: 's/stores', column: 'region' }], drill: {} };
  const info = { levels: ['Year', 'Quarter'], grains: ['year', 'quarter'], at: 1, path: [{ value: '2026-01-01' }], canDown: true };
  assert.deepEqual(shownTile(tile, info, H).dims, [{ table: 'o/orders', column: 'date', grain: 'quarter' }, { table: 's/stores', column: 'region' }]);
  const byRegion = { dims: [{ table: 's/stores', column: 'region' }], drill: { hierarchy: 'h1' } };
  const at1 = { levels: ['Region', 'Store'], grains: [null, null], at: 1, path: [{ value: 'Jawa' }], canDown: false };
  assert.deepEqual(shownTile(byRegion, at1, H).dims, [{ table: 's/stores', column: 'store', grain: null }]);
  // At the top, or with no drill, the tile is as it is.
  assert.equal(shownTile(byRegion, { ...at1, at: 0 }, H), byRegion);
  assert.equal(shownTile({ dims: byRegion.dims }, at1, H).dims, byRegion.dims);
});
