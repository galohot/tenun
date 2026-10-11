// Links between tables (build 01b): which tables a tile can reach, the field list with one group for each
// link that leads to a table, the rule for a link's name, and the words that say which link a field means.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  cleanName,
  defaultName,
  fieldGroups,
  fieldWithLink,
  firstLinks,
  groupOf,
  kindWords,
  linkLabel,
  nameProblem,
  pairs,
  proposeNames,
  reachableFrom,
  search,
  setLinkLabels,
  siblings,
  unnamedPairs,
} from '../src/lib/links.ts';
import { colFilter, isColFilter, markFilter, sameField, sameFilter, stageOf, tileFilter } from '../src/lib/filters.ts';

const table = (id, label, cols = []) => ({ id, label, name: label, source_id: id.split('/')[0], columns: cols.map((c) => ({ name: c, dtype: 'text', raw_type: '' })) });
const tables = [
  table('s1/transfers', 'Transfers', ['from_store', 'to_store', 'qty']),
  table('s2/stores', 'Stores', ['id', 'region', 'name']),
  table('s3/regions', 'Regions', ['code', 'manager']),
  table('s4/cards', 'Cards', ['store_id', 'card']),
];
const rel = (id, from, to, name) => ({ id, from: { table: from[0], column: from[1] }, to: { table: to[0], column: to[1] }, ...(name ? { name } : {}) });
const SHIP = rel('r1', ['s1/transfers', 'from_store'], ['s2/stores', 'id']);
const SOLD = rel('r2', ['s1/transfers', 'to_store'], ['s2/stores', 'id']);
const AREA = rel('r3', ['s2/stores', 'region'], ['s3/regions', 'code']);
const health = (kind, extra = {}) => ({ oneToOne: kind === 'one-one', duplicates: 0, sample: [], fromUnique: kind === 'one-one' || kind === 'many-one', kind, ...extra });

test('reach: a link goes from the many side to the one side, and the other way only when the data allows', () => {
  const rels = [SHIP, AREA];
  assert.deepEqual([...reachableFrom('s1/transfers', rels)].sort(), ['s1/transfers', 's2/stores', 's3/regions']);
  assert.deepEqual([...reachableFrom('s3/regions', rels)], ['s3/regions']);
  // One to one: the stores reach the transfers as well. A link nobody checked reads as stored.
  const h = { r1: health('one-one') };
  assert.ok(reachableFrom('s2/stores', rels, h).has('s1/transfers'));
  assert.ok(!reachableFrom('s2/stores', rels, {}).has('s1/transfers'));
  // The many side repeats a key again: one way. Many to many: not used. Read the other way round: the reverse works.
  assert.ok(!reachableFrom('s2/stores', rels, { r1: health('one-many') }).has('s1/transfers'));
  assert.ok(!reachableFrom('s1/transfers', rels, { r1: health('many-many') }).has('s2/stores'));
  const flipped = reachableFrom('s2/stores', rels, { r1: health('many-one') });
  assert.ok(flipped.has('s1/transfers'));
  assert.ok(!reachableFrom('s1/transfers', rels, { r1: health('many-one') }).has('s2/stores'));
});

test('first links: one way is clear, two ways need a name', () => {
  assert.deepEqual(firstLinks('s1/transfers', 's2/stores', [SHIP]), ['r1']);
  assert.deepEqual(firstLinks('s1/transfers', 's2/stores', [SHIP, SOLD]), ['r1', 'r2']);
  // Two ways to the stores are two ways to the regions.
  assert.deepEqual(firstLinks('s1/transfers', 's3/regions', [SHIP, SOLD, AREA]), ['r1', 'r2']);
  assert.deepEqual(firstLinks('s1/transfers', 's1/transfers', [SHIP, SOLD]), []);
  assert.deepEqual(firstLinks('s3/regions', 's1/transfers', [SHIP, SOLD, AREA]), []);
  // A longer way does not count: only the shortest ways compete.
  const direct = rel('r9', ['s1/transfers', 'qty'], ['s3/regions', 'code']);
  assert.deepEqual(firstLinks('s1/transfers', 's3/regions', [SHIP, AREA, direct]), ['r9']);
});

test('field list: the one-side table appears once for each link that leads to it, headed by the link name', () => {
  const rels = [{ ...SHIP, name: 'Shipped from' }, { ...SOLD, name: 'Sold at' }, AREA];
  const groups = fieldGroups(tables, rels, undefined, 's1/transfers');
  assert.deepEqual(
    groups.map((g) => [g.title, g.table.id, g.via ?? null, g.sub ?? null]),
    [
      ['Transfers', 's1/transfers', null, null],
      ['Shipped from', 's2/stores', 'r1', 'Stores'],
      ['Sold at', 's2/stores', 'r2', 'Stores'],
      ['Shipped from', 's3/regions', 'r1', 'Regions'],
      ['Sold at', 's3/regions', 'r2', 'Regions'],
      ['Cards', 's4/cards', null, null],
    ],
  );
  assert.equal(new Set(groups.map((g) => g.key)).size, groups.length);
  // A link with no name is headed by the proposal.
  const plain = fieldGroups(tables, [SHIP, SOLD], undefined, 's1/transfers').filter((g) => g.via);
  assert.deepEqual(plain.map((g) => g.title), ['Stores (from_store)', 'Stores (to_store)']);
  // One way, or no value picked yet: each table once.
  assert.equal(fieldGroups(tables, [SHIP], undefined, 's1/transfers').filter((g) => g.table.id === 's2/stores').length, 1);
  assert.equal(fieldGroups(tables, [SHIP, SOLD], undefined, null).filter((g) => g.table.id === 's2/stores').length, 1);
  // The field a tile has picked finds its group; a field saved before names (no link) finds the first.
  assert.equal(groupOf(groups, 's2/stores', 'r2')?.title, 'Sold at');
  assert.equal(groupOf(groups, 's2/stores', undefined)?.title, 'Shipped from');
  assert.equal(groupOf(groups, 's4/cards', undefined)?.title, 'Cards');
  assert.equal(groupOf(groups, null, undefined), undefined);
});

test('names: the proposal, the label, and the rule a name must pass', () => {
  assert.equal(defaultName(SHIP, tables), 'Stores (from_store)');
  assert.equal(linkLabel(SHIP, tables), 'Stores (from_store)');
  assert.equal(linkLabel({ ...SHIP, name: ' Shipped from ' }, tables), 'Shipped from');
  assert.equal(cleanName('  Shipped   from '), 'Shipped from');
  const rels = [{ ...SHIP, name: 'Shipped from' }, SOLD, AREA];
  assert.equal(nameProblem('Sold at', SOLD, rels, tables), null);
  // A second link between the same tables needs a name; a link alone may go without.
  assert.equal(nameProblem('  ', SOLD, rels, tables), 'Give this link a name. Two links join these tables.');
  assert.equal(nameProblem('', AREA, rels, tables), null);
  assert.equal(nameProblem('stores', SOLD, rels, tables), 'A table is called stores. Give the link another name.');
  assert.equal(nameProblem('shipped FROM', SOLD, rels, tables), 'Another link is called shipped FROM. Give this one another name.');
  assert.equal(nameProblem('x'.repeat(61), SOLD, rels, tables), 'Keep the name to 60 letters or fewer.');
  // A name typed for another link in the same sheet counts.
  assert.equal(nameProblem('Sold at', SHIP, [SHIP, SOLD], tables, { r2: 'sold at' }), 'Another link is called Sold at. Give this one another name.');
  assert.equal(nameProblem('Shipped from', SHIP, [SHIP, SOLD], tables, { r1: 'Shipped from', r2: 'Sold at' }), null);
});

test('naming a pair: names are proposed from the many-side column and never repeat', () => {
  assert.deepEqual(proposeNames([SHIP, SOLD], tables), { r1: 'Stores (from_store)', r2: 'Stores (to_store)' });
  assert.deepEqual(proposeNames([{ ...SHIP, name: 'Shipped from' }, SOLD], tables), { r1: 'Shipped from', r2: 'Stores (to_store)' });
  const same = rel('r5', ['s1/transfers', 'from_store'], ['s2/stores', 'id']);
  assert.deepEqual(proposeNames([SHIP, same], tables), { r1: 'Stores (from_store)', r5: 'Stores (from_store) 2' });
  assert.deepEqual(siblings(SHIP, [SHIP, SOLD, AREA]).map((r) => r.id), ['r1', 'r2']);
  assert.deepEqual(pairs([SHIP, SOLD, AREA]).map((g) => g.map((r) => r.id)), [['r1', 'r2']]);
  assert.equal(unnamedPairs([SHIP, SOLD, AREA]).length, 1);
  assert.equal(unnamedPairs([{ ...SHIP, name: 'a' }, { ...SOLD, name: 'b' }, AREA]).length, 0);
});

test('the line between two tables says what the data says', () => {
  assert.equal(kindWords(undefined), '1 – many');
  assert.equal(kindWords(health('one-one')), '1 – 1');
  assert.equal(kindWords(health('many-one')), 'many – 1');
  assert.equal(kindWords(health('many-many')), 'many – many');
});

test('a field that crosses a named link says so, and its clicks keep the link', () => {
  setLinkLabels([{ ...SHIP, name: 'Shipped from' }, { ...SOLD, name: 'Sold at' }], tables);
  const to = { table: 's2/stores', column: 'region', via: 'r2' };
  assert.equal(fieldWithLink('region', 'r1'), 'Shipped from › region');
  assert.equal(fieldWithLink('region', undefined), 'region');
  const tile = (d) => ({ id: 't', type: 'bar', title: '', x: 0, y: 0, w: 6, h: 4, measures: [{ table: 's1/transfers', column: 'qty', agg: 'sum' }], dims: [d] });
  // A click on a mark filters by the same role, and a filter on the other role is another filter.
  const click = markFilter(tile(to), 'West');
  assert.equal(click.via, 'r2');
  assert.ok(!('via' in markFilter(tile({ table: 's2/stores', column: 'region' }), 'West')));
  assert.ok(sameFilter(click, markFilter(tile(to), 'West')));
  assert.ok(!sameFilter(click, markFilter(tile({ ...to, via: 'r1' }), 'West')));
  // A filter tile keeps the role of its field.
  const f = tileFilter({ ...tile(to), type: 'filter', filter: { style: 'list', values: ['West'] } }, 'text');
  assert.equal(f.via, 'r2');
  // A link that was removed is no longer a name.
  setLinkLabels([], tables);
  assert.equal(fieldWithLink('region', 'r1'), 'region');
});

test('search: the same answers as the engine, from the vectors both read', () => {
  const cases = JSON.parse(readFileSync(new URL('./fixtures/paths.json', import.meta.url), 'utf8'));
  assert.ok(cases.length >= 10);
  // The kinds the engine's test builds records for. A link with no record is unknown.
  const health = (kind) => ({ oneToOne: kind === 'one-one', duplicates: 0, sample: [], fromUnique: kind === 'one-one' || kind === 'many-one', kind });
  for (const c of cases) {
    const rels = c.rels.map((r) => ({ id: r.id, from: { table: r.from[0], column: r.from[1] }, to: { table: r.to[0], column: r.to[1] } }));
    const h = Object.fromEntries(Object.entries(c.health).map(([id, kind]) => [id, health(kind)]));
    const found = search(c.base, rels, h);
    assert.deepEqual(Object.fromEntries(found.edge), c.reach, `${c.name}: the way to each table`);
    assert.deepEqual(Object.fromEntries(found.firsts), c.first, `${c.name}: the first links`);
    for (const [table, first] of Object.entries(c.first)) assert.deepEqual(firstLinks(c.base, table, rels, h), first, c.name);
    assert.deepEqual([...reachableFrom(c.base, rels, h)].sort(), Object.keys(c.reach).sort(), c.name);
  }
});

test('field list for a filter: a table two links lead to from some table is listed once for each', () => {
  const rels = [{ ...SHIP, name: 'Shipped from' }, { ...SOLD, name: 'Sold at' }, AREA];
  const titles = (groups) => groups.map((g) => [g.title, g.via ?? null]);
  // No first value and no roles: each table once.
  assert.equal(fieldGroups(tables, rels, undefined, null).length, tables.length);
  // A filter tile has no first value: it can pick either link to the stores, and either way to the regions.
  const groups = fieldGroups(tables, rels, undefined, null, true);
  assert.deepEqual(
    titles(groups).filter(([t]) => ['Shipped from', 'Sold at'].includes(t)),
    [['Shipped from', 'r1'], ['Sold at', 'r2'], ['Shipped from', 'r1'], ['Sold at', 'r2']],
  );
  assert.deepEqual(
    groups.filter((g) => g.via).map((g) => g.table.id),
    ['s2/stores', 's2/stores', 's3/regions', 's3/regions'],
  );
  // A table nothing reaches two ways stays one entry, and the tables of other tiles are not touched.
  assert.equal(groups.filter((g) => g.table.id === 's4/cards').length, 1);
  assert.equal(groups.filter((g) => g.table.id === 's1/transfers').length, 1);
  assert.equal(fieldGroups(tables, [SHIP], undefined, null, true).length, tables.length);
});

test('a pivot with the same field on its rows and its columns through two links keeps them apart', () => {
  const rows = { table: 's2/stores', column: 'region', via: 'r1' };
  const cols = { table: 's2/stores', column: 'region', via: 'r2' };
  const tile = { id: 'p', type: 'pivot', title: '', x: 0, y: 0, w: 8, h: 5, measures: [{ table: 's1/transfers', column: 'qty', agg: 'sum' }], dims: [rows], cols };
  const onRow = markFilter(tile, 'West');
  const onCol = colFilter(tile.cols, 'West');
  assert.equal(onRow.via, 'r1');
  assert.equal(onCol.via, 'r2');
  // Only the click on the column header is a column filter; the same table and column on the rows is not.
  assert.ok(isColFilter(tile, onCol));
  assert.ok(!isColFilter(tile, onRow));
  assert.ok(!sameFilter(onRow, onCol));
  assert.ok(sameField(rows, onRow) && !sameField(rows, onCol));
  // A Sankey node reads the stage of its own link.
  const flow = { id: 's', type: 'sankey', dims: [{ table: 's2/stores', column: 'region', via: 'r1' }, { table: 's2/stores', column: 'region', via: 'r2' }] };
  assert.equal(stageOf(flow, { table: 's2/stores', column: 'region', via: 'r2', op: 'in', values: [] }), 1);
  assert.equal(stageOf(flow, { table: 's2/stores', column: 'region', via: 'r1', op: 'in', values: [] }), 0);
  assert.equal(stageOf(tile, onRow), 0);
});
