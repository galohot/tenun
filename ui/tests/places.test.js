// The place names a map reads (build 05, V4-D8): the search and the CSV of the Place names sheet.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PLACE_LISTS, placesTable, searchPlaces } from '../src/lib/places.ts';

// The gazetteer the engine matches with, read as the sheet gets it.
const tsv = readFileSync(new URL('../../crates/tenun-core/src/geo/places.tsv', import.meta.url), 'utf8').trim().split('\n').slice(1);
const rows = (level) =>
  tsv
    .map((l) => l.split('\t'))
    .filter((f) => f[0] === level)
    .map(([, , code, name, aliases, parent]) => ({ code, name, aliases: aliases ? aliases.split('|') : [], parent }));

test('the search finds a place by code, name or another form, word by word', () => {
  const kab = rows('2');
  assert.equal(kab.length, 514);
  assert.deepEqual(searchPlaces(kab, '3204').map((r) => r.name), ['Kabupaten Bandung']);
  assert.deepEqual(searchPlaces(kab, '32.04').map((r) => r.name), ['Kabupaten Bandung']);
  assert.deepEqual(searchPlaces(kab, 'kota bandung').map((r) => r.code), ['32.73']);
  // "Bandung" alone is Kabupaten Bandung, Kabupaten Bandung Barat and Kota Bandung.
  assert.equal(searchPlaces(kab, 'bandung').length, 3);
  assert.deepEqual(searchPlaces(kab, 'toba samosir').map((r) => r.name), ['Kabupaten Toba']);
  assert.equal(searchPlaces(kab, '  ').length, 514);
  assert.deepEqual(searchPlaces(kab, 'zzz'), []);
  const prov = rows('1');
  assert.deepEqual(searchPlaces(prov, 'jabar').map((r) => r.code), ['ID-JB']);
  // The Indonesian name of the US Virgin Islands holds "Amerika Serikat" too.
  assert.deepEqual(searchPlaces(rows('0'), 'amerika serikat').map((r) => r.code), ['US', 'VI']);
  assert.deepEqual(searchPlaces(rows('0'), 'amerika').map((r) => r.code), ['AS', 'US', 'VI']);
});

test('the CSV has a code, a name, the other forms and the place it is in', () => {
  const t = placesTable(searchPlaces(rows('2'), '3204'));
  assert.deepEqual(t.heads, ['code', 'name', 'also read as', 'in']);
  assert.deepEqual(t.rows, [['32.04', 'Kabupaten Bandung', '', 'ID-JB']]);
  assert.deepEqual(t.numeric, [false, false, false, false]);
  assert.deepEqual(PLACE_LISTS.map((l) => l.level), ['country', 'province', 'regency']);
});
