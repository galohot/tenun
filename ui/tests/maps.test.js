// The map tile (build 06): TopoJSON decoding, the colour steps, the words of the legend and the tooltip, the
// click filter, the suggestion and the phone height. Pure functions, no browser.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { bounds, features } from '../src/lib/topo.ts';
import { ordinal, padded, pointGroups, rampOf, stepOf, STEPS, withAlpha } from '../src/lib/charts/map.ts';
import { placeLevelOf } from '../src/lib/places.ts';
import { suggestTypes } from '../src/lib/suggest.ts';
import { markFilter } from '../src/lib/filters.ts';
import { mapHeight } from '../src/lib/layout.ts';
import { tileQuery, resultKey } from '../src/lib/tiles.ts';

// Two squares side by side that share one border, quantized: x and y in steps of 0.5, from (100, -10).
//   arc 0: the shared border, (1,0) up to (1,1)        arc 1: the left square's other three sides
//   arc 2: the right square's other three sides
const topo = {
  type: 'Topology',
  transform: { scale: [0.5, 0.5], translate: [100, -10] },
  arcs: [
    [[2, 0], [0, 2]],
    [[2, 2], [-2, 0], [0, -2], [2, 0]],
    [[2, 0], [2, 0], [0, 2], [-2, 0]],
  ],
  objects: {
    places: {
      type: 'GeometryCollection',
      geometries: [
        { type: 'Polygon', arcs: [[0, 1]], properties: { code: 'A', name: 'Left' } },
        { type: 'Polygon', arcs: [[2, -1]], properties: { code: 'B', name: 'Right' } },
        { type: null, properties: { code: 'C' } },
      ],
    },
  },
};

test('TopoJSON decodes to the shapes it holds, a shared border once in the file and in both shapes', () => {
  const fc = features(topo);
  assert.equal(fc.features.length, 3);
  const [a, b, c] = fc.features;
  assert.deepEqual(a.properties, { code: 'A', name: 'Left' });
  // The left square: up the shared border, then round the other three sides, back where it began.
  assert.deepEqual(a.geometry.coordinates, [[[101, -10], [101, -9], [100, -9], [100, -10], [101, -10]]]);
  // The right square runs the shared border backwards.
  assert.deepEqual(b.geometry.coordinates, [[[101, -10], [102, -10], [102, -9], [101, -9], [101, -10]]]);
  assert.equal(c.geometry, null);
  assert.deepEqual(bounds([a]), [[100, -9], [101, -10]]);
  assert.deepEqual(bounds(fc.features), [[100, -9], [102, -10]]);
  assert.equal(bounds([c]), null);
});

test('the shipped maps decode with every place inside the right box', () => {
  const read = (f) => JSON.parse(readFileSync(new URL(`../public/geo/${f}.topo.json`, import.meta.url), 'utf8'));
  const id2 = features(read('ID-2')).features;
  assert.equal(id2.length, 514);
  const [[w, n], [e, s]] = bounds(id2);
  // Indonesia: about 95° to 141° east, 6° north to 11° south.
  assert.ok(w > 94 && w < 96 && e > 140 && e < 142 && n > 5 && n < 7 && s < -10 && s > -12, JSON.stringify([w, n, e, s]));
  assert.ok(id2.every((f) => /^\d\d\.\d\d$/.test(f.properties.code) && /^ID-[A-Z]{2}$/.test(f.properties.parent)));
  assert.equal(features(read('ID-1')).features.length, 38);
  assert.equal(features(read('world')).features.length, 237);
});

test('a ramp has seven equal steps from the lowest value to the highest; one crossing zero is centred on it', () => {
  const r = rampOf([10, 80, null, 45]);
  assert.equal(r.kind, 'sequential');
  assert.equal(r.edges.length, STEPS + 1);
  assert.equal(r.edges[0], 10);
  assert.equal(r.edges[7], 80);
  assert.equal(stepOf(r, 10), 0);
  assert.equal(stepOf(r, 80), 6);
  assert.equal(stepOf(r, 19.9), 0);
  assert.equal(stepOf(r, 20), 1);
  assert.equal(stepOf(r, 45), 3);
  assert.equal(stepOf(r, null), null);
  // A change: blue below zero, grey near it, orange above. -30 is the far end of a ramp from -40 to 40.
  const d = rampOf([-30, 40, 5]);
  assert.equal(d.kind, 'diverging');
  assert.equal(d.edges[0], -40);
  assert.equal(d.edges[7], 40);
  assert.equal(stepOf(d, 0), 3);
  assert.equal(stepOf(d, 40), 6);
  assert.equal(stepOf(d, -30), 0);
  // One value alone is the middle step; no value has no ramp.
  assert.equal(stepOf(rampOf([5, 5]), 5), 3);
  assert.equal(rampOf([null]), null);
});

test('ranks read as words, and points keep eight colour groups and one Other', () => {
  assert.deepEqual([1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 38, 101, 111].map(ordinal), [
    '1st', '2nd', '3rd', '4th', '11th', '12th', '13th', '21st', '22nd', '23rd', '38th', '101st', '111th',
  ]);
  const rows = Array.from({ length: 10 }, (_, i) => [0, 0, `g${i}`, 10 - i]);
  const g = pointGroups(rows, 8);
  assert.deepEqual(g.names, ['g0', 'g1', 'g2', 'g3', 'g4', 'g5', 'g6', 'g7']);
  assert.equal(g.other, true);
  assert.equal(pointGroups(rows.slice(0, 3)).other, false);
});

test('a click on a place filters by every name of it, and the chip says its name', () => {
  const tile = { type: 'map', dims: [{ table: 's1/sales', column: 'place' }], measures: [] };
  const geo = { codes: ['ID-JB', 'ID-JK'], raw: [['Jawa Barat', 'Jabar', '32'], ['Jakarta']], names: ['Jawa Barat', 'DKI Jakarta'] };
  assert.deepEqual(markFilter(tile, 'ID-JB', undefined, undefined, undefined, geo), {
    table: 's1/sales', column: 'place', grain: null, op: 'in', values: ['Jawa Barat', 'Jabar', '32'], label: 'Jawa Barat',
  });
  // A code that is not on the map clicks nothing.
  assert.equal(markFilter(tile, 'ID-XX', undefined, undefined, undefined, geo), null);
});

test('a column named like places suggests a map, and the name decides the level', () => {
  assert.equal(placeLevelOf('province'), 'province');
  assert.equal(placeLevelOf('Provinsi'), 'province');
  assert.equal(placeLevelOf('kab_kota'), 'regency');
  assert.equal(placeLevelOf('Kabupaten/Kota'), 'regency');
  assert.equal(placeLevelOf('city'), 'regency');
  assert.equal(placeLevelOf('negara'), 'country');
  assert.equal(placeLevelOf('amount'), null);
  assert.equal(placeLevelOf('protein'), null);
  const tables = [{ id: 's1/sales', columns: [{ name: 'province', dtype: 'text' }, { name: 'amount', dtype: 'number' }] }];
  const bar = { id: 't', type: 'bar', title: '', x: 0, y: 0, w: 6, h: 4, measures: [{ table: 's1/sales', column: 'amount', agg: 'sum' }], dims: [{ table: 's1/sales', column: 'province' }] };
  const s = suggestTypes(bar, tables, 33);
  assert.equal(s[0].type, 'map');
  assert.deepEqual(s[0].variant.map, { level: 'province', country: 'ID' });
});

test('a map keeps its shape on a phone, and points with a latitude and a longitude ask a question', () => {
  assert.equal(mapHeight(390), Math.round(390 * 0.55) + 96);
  const lat = { table: 's1/stores', column: 'lat' };
  const lon = { table: 's1/stores', column: 'lon' };
  const points = { id: 'p', type: 'map', title: '', x: 0, y: 0, w: 8, h: 6, measures: [], dims: [], map: { level: 'country', style: 'points', lat, lon } };
  const q = tileQuery(points, []);
  assert.deepEqual(q.measures, [{ table: 's1/stores', column: null, agg: 'count' }]);
  assert.equal(tileQuery({ ...points, map: { level: 'country', style: 'points', lat } }, []), null);
  // Another level asks again.
  assert.notEqual(resultKey(points, []), resultKey({ ...points, map: { ...points.map, country: 'ID' } }, []));
});

test('the hatch of no data is a faint line of the axis colour, and a fit leaves a margin', () => {
  assert.equal(withAlpha('#69696e', 0.3), 'rgba(105, 105, 110, 0.3)');
  assert.equal(withAlpha('rgb(1, 2, 3)', 0.3), 'rgb(1, 2, 3)');
  assert.deepEqual(padded([[100, 0], [110, -10]], 0.1), [[99, 1], [111, -11]]);
  // Never past the poles.
  assert.deepEqual(padded([[0, 89], [10, -89]], 0.1)[0][1], 90);
  assert.equal(padded(null), null);
});
