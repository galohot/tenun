// Link tables (build 03b, V4-D112): which table looks like one, which tables a tile reaches through one,
// and the one gate that says a value does not add up when a group sits behind one. Pure functions, no browser.
import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { andList, suggestLinkTables } from '../src/lib/linktables.ts';
import { behindFrom, fieldGroups, firstLinks, linkTablesOf, reachableFrom, search } from '../src/lib/links.ts';
import { addsUp, addsUpHere, overlapsAt, setKnownMeasures, tileProblem, whyNotAdded } from '../src/lib/measures.ts';
import { foldable, stackMode, stackableTile } from '../src/lib/charts/math.ts';
import { foldSeries, splitKept } from '../src/lib/charts/fold.ts';
import { pictureKind } from '../src/lib/picture.ts';
import { effectiveShowAs, showAsOptions } from '../src/lib/showas.ts';
import { suggestTypes } from '../src/lib/suggest.ts';
import { isEmptySetup, withColumn, withLinkTable } from '../src/lib/prep.ts';

const table = (id, label, cols) => ({ id, label, name: label, source_id: id.split('/')[0], columns: cols.map((c) => ({ name: c, dtype: 'text', raw_type: '' })) });
const spending = table('s1/spending', 'spending', ['id', 'programme_id', 'date', 'amount']);
const programmes = table('s2/programmes', 'programmes', ['id', 'name', 'budget']);
const funders = table('s3/funders', 'funders', ['id', 'name', 'kind']);
const pf = table('s4/programme_funders', 'programme_funders', ['programme_id', 'funder_id', 'share']);
const tables = [spending, programmes, funders, pf];
const rel = (id, from, to) => ({ id, from: { table: from[0], column: from[1] }, to: { table: to[0], column: to[1] } });
const R1 = rel('r1', ['s1/spending', 'programme_id'], ['s2/programmes', 'id']);
const R2 = rel('r2', ['s4/programme_funders', 'programme_id'], ['s2/programmes', 'id']);
const R3 = rel('r3', ['s4/programme_funders', 'funder_id'], ['s3/funders', 'id']);
const rels = [R1, R2, R3];
const health = (fromUnique) => ({ oneToOne: false, duplicates: 0, sample: [], fromUnique, kind: 'one-many' });
const repeating = { r1: health(false), r2: health(false), r3: health(false) };

// ---- the suggestion --------------------------------------------------------------------------------

test('a table with two links to two different tables, mostly links, that nothing points at looks like a link table', () => {
  const found = suggestLinkTables(tables, rels, repeating);
  assert.equal(found.length, 1);
  assert.equal(found[0].table, 's4/programme_funders');
  assert.deepEqual(found[0].between, ['programmes', 'funders']);
  assert.match(found[0].why, /^programme_funders looks like a link table between programmes and funders\./);
  // Marked, or set aside: not suggested again.
  assert.deepEqual(suggestLinkTables(tables, rels, repeating, new Set(['s4/programme_funders'])), []);
  assert.deepEqual(suggestLinkTables(tables, rels, repeating, new Set(), ['s4/programme_funders']), []);
});

test('a fact table, a table two links lead to, and a table that points at one table twice are not link tables', () => {
  // Spending has one link and four columns.
  assert.deepEqual(suggestLinkTables([spending, programmes], [R1], repeating), []);
  // Transfers between stores: two links, both to the stores (that is two named links, not a link table).
  const stores = table('s5/stores', 'stores', ['id', 'name']);
  const transfers = table('s6/transfers', 'transfers', ['from_store', 'to_store']);
  const ship = rel('r5', ['s6/transfers', 'from_store'], ['s5/stores', 'id']);
  const sold = rel('r6', ['s6/transfers', 'to_store'], ['s5/stores', 'id']);
  assert.deepEqual(suggestLinkTables([stores, transfers], [ship, sold], {}), []);
  // A table something points at is a table of things, however many links it has.
  const tags = rel('r7', ['s7/tags', 'x'], ['s4/programme_funders', 'programme_id']);
  assert.deepEqual(suggestLinkTables(tables, [...rels, tags], repeating), []);
  // Links are a small part of a wide table: an order with a store, a customer and ten columns of its own.
  const wide = table('s8/orders', 'orders', ['store_id', 'customer_id', 'a', 'b', 'c', 'd', 'e']);
  const l1 = rel('r8', ['s8/orders', 'store_id'], ['s5/stores', 'id']);
  const l2 = rel('r9', ['s8/orders', 'customer_id'], ['s3/funders', 'id']);
  assert.deepEqual(suggestLinkTables([wide, stores, funders], [l1, l2], {}), []);
  // A column that holds one value per row is a one to one link, which already works.
  assert.deepEqual(suggestLinkTables(tables, rels, { ...repeating, r2: health(true) }), []);
  // A link nobody has read yet counts as repeating.
  assert.equal(suggestLinkTables(tables, rels, {}).length, 1);
  assert.equal(suggestLinkTables(tables, rels, undefined).length, 1);
});

test('the words list tables the way a person does', () => {
  assert.equal(andList(['a']), 'a');
  assert.equal(andList(['a', 'b']), 'a and b');
  assert.equal(andList(['a', 'b', 'c']), 'a, b and c');
});

// ---- which tables a tile reaches ----------------------------------------------------------------------

test('a marked table is crossed from the one side of its link, once, and an unmarked one never', () => {
  const marked = new Set(['s4/programme_funders']);
  assert.deepEqual([...reachableFrom('s1/spending', rels, repeating)].sort(), ['s1/spending', 's2/programmes']);
  const through = reachableFrom('s1/spending', rels, repeating, marked);
  assert.deepEqual([...through].sort(), ['s1/spending', 's2/programmes', 's3/funders', 's4/programme_funders']);
  // The funders and the link table are behind it; the programmes are not.
  assert.deepEqual([...behindFrom('s1/spending', rels, repeating, marked)].sort(), ['s3/funders', 's4/programme_funders']);
  assert.deepEqual([...behindFrom('s1/spending', rels, repeating)], []);
  // The funders reach the programmes the other way round.
  const back = search('s3/funders', rels, repeating, marked);
  assert.equal(back.edge.get('s2/programmes'), 'r2');
  assert.deepEqual([...back.behind].sort(), ['s2/programmes', 's4/programme_funders']);
  // A table that has no link table in the way is never behind one.
  assert.deepEqual([...behindFrom('s2/programmes', [R1], repeating, marked)], []);
});

test('the marks come from the setups of the workspace', () => {
  assert.deepEqual([...linkTablesOf({ 's4/programme_funders': { linkTable: true }, 's2/programmes': { mainDate: 'x' } })], ['s4/programme_funders']);
  assert.deepEqual([...linkTablesOf(undefined)], []);
});

test('the field list offers a table behind a link table, and a table two ways away once for each way', () => {
  const marked = new Set(['s4/programme_funders']);
  const plain = fieldGroups(tables, rels, repeating, 's1/spending').map((g) => g.table.id);
  assert.deepEqual(plain.sort(), tables.map((t) => t.id).sort(), 'every table is listed, related or not');
  // The way to the funders starts with the link to the programmes: one way, so one entry.
  assert.deepEqual(firstLinks('s1/spending', 's3/funders', rels, repeating, marked), ['r1']);
  assert.equal(fieldGroups(tables, rels, repeating, 's1/spending', false, marked).filter((g) => g.table.id === 's3/funders').length, 1);
});

// ---- the one gate ------------------------------------------------------------------------------------------

const sum = { table: 's1/spending', column: 'amount', agg: 'sum' };
const tile = (type, measures, extra = {}) => ({ id: 't', type, title: '', x: 0, y: 0, w: 6, h: 4, measures, dims: [], ...extra });
const overlap = [{ measure: 0, note: 'A row of spending with two funders is under both. The total counts it once.', overlap: true }];

beforeEach(() => setKnownMeasures([], {}));

test('V4-D112: a group behind a link table makes a sum not add up on that tile, through the gate every use asks', () => {
  const t = tile('bar', [sum]);
  assert.equal(addsUpHere(t, 0), true);
  assert.equal(addsUpHere(t, 0, overlap), false);
  assert.equal(addsUp(t, overlap), false);
  assert.equal(overlapsAt(overlap, 0), true);
  assert.equal(overlapsAt(overlap, 1), false);
  assert.equal(whyNotAdded(t, 0, overlap), 'This value is not added up here, because one row can be in two groups');
  assert.equal(whyNotAdded(t, 0), null);
  // The small bars do not fold into an Other bar the client adds up, and a split keeps all its groups.
  assert.equal(foldable({ addsUp: addsUp(t, overlap), categories: 5, max: 2, series: 1 }), false);
  assert.equal(foldable({ addsUp: addsUp(t), categories: 5, max: 2, series: 1 }), true);
  assert.deepEqual(splitKept(['a', 'b', 'c'], 2, addsUp(t, overlap)), { kept: ['a', 'b', 'c'], other: false });
  assert.deepEqual(splitKept(['a', 'b', 'c'], 2, addsUp(t)), { kept: ['a'], other: true });
  assert.equal(foldSeries(['a', 'b', 'c'], [3, 2, 1], 1, 2, addsUp(t, overlap), 'Other'), null);
  assert.deepEqual(foldSeries(['a', 'b', 'c'], [3, 2, 1], 1, 2, addsUp(t), 'Other'), { categories: ['a', 'Other'], values: [3, 3] });
  // Stacking puts the series side by side, not on top of each other.
  assert.equal(stackableTile(t, false), true);
  assert.equal(stackableTile(t, false, overlap), false);
  assert.equal(stackMode(t, false, overlap), 'side');
  // A share, a running total and a year to date are not offered, and a saved one is not applied.
  const shown = tile('bar', [sum], { dims: [{ table: 's3/funders', column: 'name' }], showAs: ['pct_total'] });
  assert.equal(effectiveShowAs(shown, 0, 'other'), 'pct_total');
  assert.equal(effectiveShowAs(shown, 0, 'other', overlap), null);
  assert.ok(!showAsOptions(shown, 'other', 0, overlap).includes('pct_total'));
  assert.ok(showAsOptions(shown, 'other', 0, overlap).includes('rank'));
  // A share chart is not offered as an "Also try" tile.
  const tablesList = [{ id: 's3/funders', columns: [{ name: 'name', dtype: 'text' }] }];
  const bar = tile('bar', [sum], { dims: [{ table: 's3/funders', column: 'name' }] });
  assert.ok(suggestTypes(bar, tablesList, 5).some((s) => s.type === 'share'));
  assert.ok(!suggestTypes(bar, tablesList, 5, overlap).some((s) => s.type === 'share'));
});

test('V4-D112: a share chart and a waterfall by category are refused with words, and every picture asks the same', () => {
  assert.equal(tileProblem(tile('share', [sum])), null);
  assert.equal(tileProblem(tile('share', [sum]), overlap), 'These shares would add up to more than 100 %, because one row can be in two groups.');
  assert.equal(tileProblem(tile('donut', [sum]), overlap), 'These shares would add up to more than 100 %, because one row can be in two groups.');
  assert.match(tileProblem(tile('waterfall', [sum]), overlap, false), /^A waterfall adds one category to the next\. This value is not added up here, because one row can be in two groups/);
  assert.equal(tileProblem(tile('waterfall', [sum]), overlap, true), null, 'steps over time are differences');
  // An average says it is an average, whatever else is true.
  assert.match(tileProblem(tile('share', [{ ...sum, agg: 'avg' }]), overlap), /^Averages are not added up, so it has no shares/);
  const res = { rows: [['a', 1]], columns: [{ dtype: 'text' }, { dtype: 'number' }] };
  assert.equal(pictureKind(tile('share', [sum]), res, ['share']), 'chart');
  assert.equal(pictureKind(tile('share', [sum]), { ...res, parts: overlap }, ['share']), 'screen');
});

// ---- the marks in the file, and the screens that read them ------------------------------------------------

test('the mark is kept by every other save of the setup, and an unmarked table leaves nothing behind', () => {
  const on = withLinkTable(undefined, true);
  assert.deepEqual(on, { linkTable: true });
  assert.equal(isEmptySetup(on), false);
  // Renaming a column of the table keeps the mark.
  assert.deepEqual(withColumn(on, 'share', { label: 'Share of the grant' }).linkTable, true);
  // Unmarking leaves an empty setup, which the app does not write.
  assert.equal(isEmptySetup(withLinkTable(on, false)), true);
  assert.deepEqual(withLinkTable({ mainDate: 'date', linkTable: true }, false), { mainDate: 'date' });
});

const source = (f) => readFileSync(new URL(`../src/${f}`, import.meta.url), 'utf8');

test('the screens that node cannot load read the marks and the gate (read from the files)', () => {
  assert.match(source('lib/state.svelte.ts'), /linkTables = \$derived\(linkTablesOf\(this\.snap\?\.workspace\.tables\)\)/);
  const picker = source('components/dash/FieldPicker.svelte');
  assert.match(picker, /fieldGroups\(app\.tables, app\.snap\?\.workspace\.relationships \?\? \[\], app\.linkHealth, base, roles, app\.linkTables\)/);
  assert.match(picker, /behindFrom\(base, /, 'a value cannot be read from behind a link table');
  const inspector = source('components/dash/Inspector.svelte');
  assert.match(inspector, /reachableFrom\(m\.table, rels, app\.linkHealth, app\.linkTables\)/);
  assert.match(inspector, /tileProblem\(tile, parts, /, 'the inspector says why a chart type is refused');
  assert.match(inspector, /title=\{chipWhy\(t\.id\) \?\? t\.label\}/, 'and the button of a type that would be refused says it too');
  const model = source('components/ModelView.svelte');
  assert.match(model, /suggestLinkTables\(app\.tables, ws\.relationships, app\.linkHealth, app\.linkTables/);
  assert.match(model, /withLinkTable\(ws\.tables\?\.\[id\], on\)/);
  assert.match(source('components/dash/Suggestions.svelte'), /suggestTypes\(\$state\.snapshot\(tile\) as Tile, app\.tables, groups, res\?\.parts\)/);
});
