// Calculated columns (build 02): what the formula field suggests while you type, what a choice writes,
// where the underline goes, and which tiles a column feeds. Pure functions, no browser.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  FUNCTIONS,
  apply,
  bracketed,
  cut,
  kindWords,
  listWords,
  nameProblem,
  signature,
  suggest,
  tableNamed,
  tilesUsing,
  useWords,
  visibleColumns,
} from '../src/lib/formula.ts';
import { firstLinks, reachableFrom } from '../src/lib/links.ts';

const col = (name, dtype = 'number', more = {}) => ({ name, dtype, raw_type: '', ...more });
const table = (id, label, columns) => ({ id, source_id: id.split('/')[0], name: label, label, columns, rows: 10 });

const orders = table('s1/orders', 'orders', [
  col('id'),
  col('store_id'),
  col('ship_id'),
  col('amount', 'number', { label: 'Sales' }),
  col('secret', 'text', { hidden: true }),
  col('Net', 'number', { calc: true }),
  col('Net rounded', 'number', { calc: true }),
]);
const stores = table('s2/stores', 'stores', [col('id'), col('region', 'text'), col('rent')]);
const regions = table('s3/regions', 'regions', [col('code', 'text'), col('name', 'text')]);
const tables = [orders, stores, regions];
const rel = (id, from, to, name) => ({ id, from, to, ...(name ? { name } : {}) });
const f = (t, c) => ({ table: t, column: c });

/** The scope the field builds, with the same search the engine is tested against. */
function scopeOf(rels, kind = 'column', editing, base = orders) {
  return {
    table: base,
    tables,
    rels,
    kind,
    editing,
    reach: kind === 'column' ? reachableFrom(base.id, rels) : null,
    firsts: (t) => firstLinks(base.id, t, rels),
  };
}

const oneLink = [rel('r1', f('s1/orders', 'store_id'), f('s2/stores', 'id')), rel('r2', f('s2/stores', 'id'), f('s3/regions', 'code'))];
const twoLinks = [
  rel('r1', f('s1/orders', 'store_id'), f('s2/stores', 'id'), 'Sold at'),
  rel('r2', f('s1/orders', 'ship_id'), f('s2/stores', 'id'), 'Shipped from'),
];

/** Types `text` with the caret at the end, or at the `|` mark. */
function at(text) {
  const caret = text.indexOf('|');
  return caret < 0 ? [text, text.length] : [text.replace('|', ''), caret];
}

test('the function list has every function once, with a signature', () => {
  assert.equal(FUNCTIONS.length, 64);
  assert.equal(new Set(FUNCTIONS.map((x) => x.name)).size, 64);
  assert.equal(signature(FUNCTIONS.find((x) => x.name === 'IF')), 'IF(test, a, [b])');
  assert.equal(signature(FUNCTIONS.find((x) => x.name === 'TODAY')), 'TODAY()');
});

test('after [ the columns of the table are offered, the name shown first, and a choice closes the bracket', () => {
  const [text, caret] = at('[am');
  const c = suggest(text, caret, scopeOf(oneLink));
  assert.deepEqual(
    c.items.map((i) => i.label),
    ['Sales'],
  );
  assert.equal(c.items[0].detail, 'Number');
  const done = apply(text, c, c.items[0]);
  assert.equal(done.text, '[Sales]');
  assert.equal(done.caret, 7);
  // The real name works too, and finds the same column.
  const real = suggest('[amou', 5, scopeOf(oneLink));
  assert.equal(real.items[0].label, 'Sales');
});

test('a closing bracket that is already there is replaced, not doubled', () => {
  const [text, caret] = at('[Sa|] * 2');
  const c = suggest(text, caret, scopeOf(oneLink));
  const done = apply(text, c, c.items[0]);
  assert.equal(done.text, '[Sales] * 2');
  assert.equal(done.caret, 7);
});

test('hidden columns are not offered, and a calculated column sees only the ones above it', () => {
  const all = suggest('[', 1, scopeOf(oneLink)).items.map((i) => i.label);
  assert.ok(!all.includes('secret'));
  assert.ok(all.includes('Net') && all.includes('Net rounded'));
  // Editing "Net rounded": "Net" is above, "Net rounded" and the rest are not.
  const above = suggest('[', 1, scopeOf(oneLink, 'column', 'Net rounded')).items.map((i) => i.label);
  assert.ok(above.includes('Net') && !above.includes('Net rounded'));
  assert.equal(suggest('[Ne', 3, scopeOf(oneLink, 'column', 'Net')), null);
  assert.deepEqual(
    visibleColumns(orders, 'Net').map((c) => c.name),
    ['id', 'store_id', 'ship_id', 'amount'],
  );
});

test('related tables are offered, and after a table its columns', () => {
  const scope = scopeOf(oneLink);
  const c = suggest('[st', 3, scope);
  // The table of the file's columns first, then tables.
  const store = c.items.find((i) => i.label === 'stores');
  assert.equal(store.kind, 'table');
  assert.equal(store.insert, '[stores].[');
  const done = apply('[st', c, store);
  assert.equal(done.text, '[stores].[');
  // The next suggestion lists the columns of stores.
  const next = suggest(done.text, done.caret, scope);
  assert.deepEqual(
    next.items.map((i) => i.label),
    ['id', 'region', 'rent'],
  );
  assert.equal(apply(done.text, next, next.items[1]).text, '[stores].[region]');
  // A table two steps away is reachable too; the one that points at us is not.
  assert.ok(suggest('[reg', 4, scope).items.some((i) => i.label === 'regions'));
  assert.equal(suggest('[stores].[re', 12, scope).items.map((i) => i.label).join(), 'region,rent');
  const fromStores = suggest('[ord', 4, scopeOf(oneLink, 'column', undefined, stores));
  assert.ok(!fromStores || !fromStores.items.some((i) => i.label === 'orders'));
});

test('two links to one table are offered one for each name', () => {
  const scope = scopeOf(twoLinks);
  const c = suggest('[S', 2, scope);
  const links = c.items.filter((i) => i.kind === 'link');
  assert.deepEqual(
    links.map((i) => i.label),
    ['Sold at', 'Shipped from'],
  );
  assert.ok(!c.items.some((i) => i.kind === 'table' && i.label === 'stores'));
  assert.equal(links[1].insert, '[Shipped from].[');
  // After a link's name, the columns of the table on its one side.
  const next = suggest('[Shipped from].[', 16, scope);
  assert.deepEqual(
    next.items.map((i) => i.label),
    ['id', 'region', 'rent'],
  );
  assert.equal(tableNamed(scope, 'Shipped from').id, 's2/stores');
  // Without names the table is offered once, with the reason.
  const unnamed = suggest('[stor', 5, scopeOf(twoLinks.map(({ name, ...r }) => r)));
  const shop = unnamed.items.find((i) => i.label === 'stores');
  assert.equal(shop.kind, 'table');
  assert.match(shop.detail, /Name them in Model/);
});

test('a measure may total any table, a column only the ones it reaches', () => {
  const none = [];
  assert.equal(suggest('[regi', 5, scopeOf(none)), null);
  assert.ok(suggest('[regi', 5, scopeOf(none, 'measure')).items.some((i) => i.label === 'regions'));
});

test('a word outside brackets offers the functions that start with it', () => {
  const scope = scopeOf(oneLink);
  const c = suggest('ROU', 3, scope);
  assert.deepEqual(
    c.items.map((i) => i.label),
    ['ROUND', 'ROUNDUP', 'ROUNDDOWN'],
  );
  assert.equal(c.items[0].detail, 'ROUND(x, [n])');
  assert.equal(c.items[0].hint, 'Rounds to n places.');
  const inside = suggest('IF(1, ROU', 9, scope);
  assert.equal(apply('IF(1, ROU', inside, inside.items[0]).text, 'IF(1, ROUND(');
  // A function with no arguments gets both brackets and the caret after them.
  const t = suggest('TOD', 3, scope);
  const d = apply('TOD', t, t.items[0]);
  assert.deepEqual(d, { text: 'TODAY()', caret: 7 });
  // Totals belong to measures.
  assert.equal(suggest('SUM', 3, scope), null);
  assert.ok(suggest('SUM', 3, scopeOf(oneLink, 'measure')).items.some((i) => i.label === 'SUMIF'));
  // Case does not matter, and what is typed in full with its bracket needs no help.
  assert.equal(suggest('rou', 3, scope).items[0].label, 'ROUND');
  assert.equal(suggest('ROUND(1)', 5, scope), null);
});

test('nothing is offered inside a text, after a number, or between the brackets of a closed name', () => {
  const scope = scopeOf(oneLink);
  assert.equal(suggest('"am', 3, scope), null);
  assert.equal(suggest('"say ""hi"" [am', 15, scope), null);
  assert.equal(suggest('12', 2, scope), null);
  assert.equal(suggest('[amount] ', 9, scope), null);
  // After a text that closed, the formula goes on.
  assert.ok(suggest('"a" & [am', 9, scope));
});

test('a name with a closing bracket is written twice', () => {
  assert.equal(bracketed('a]b'), '[a]]b]');
  const weird = table('s1/w', 'w', [col('a]b', 'text')]);
  const scope = { ...scopeOf([]), table: weird, tables: [weird] };
  const c = suggest('[a]]', 4, scope);
  assert.equal(c.items[0].insert, '[a]]b]');
  assert.equal(c.query, 'a]');
});

test('the underline cuts the text where the engine says, in UTF-16 units', () => {
  assert.deepEqual(cut('[amount] * [nope]', { at: 11, len: 6 }), ['[amount] * ', '[nope]', '']);
  // An emoji takes two units, and so does the engine's count.
  const text = '"😀" & [nope]';
  assert.deepEqual(cut(text, { at: 7, len: 6 }), ['"😀" & ', '[nope]', '']);
  // A length of 0 is a place: the end of the formula marks its last character.
  assert.deepEqual(cut('IF(1,', { at: 5, len: 0 }), ['IF(1', ',', '']);
  assert.deepEqual(cut('abc', null), ['abc', '', '']);
  // A place past the end does not break.
  assert.deepEqual(cut('abc', { at: 40, len: 5 }), ['ab', 'c', '']);
});

test('a name is refused for the reasons the engine gives', () => {
  assert.equal(nameProblem('Revenue incl. tax', orders), null);
  assert.match(nameProblem('', orders), /Give the column a name/);
  assert.match(nameProblem('amount', orders), /already/);
  assert.match(nameProblem('sales', orders), /already/);
  assert.match(nameProblem('a[b]', orders), /square brackets/);
  assert.match(nameProblem('__x', orders), /underscores/);
  assert.match(nameProblem('x'.repeat(101), orders), /100 letters/);
  // The column being edited may keep its own name.
  assert.equal(nameProblem('Net', orders, 'Net'), null);
});

test('the tiles and filters that use a column are found', () => {
  const dashboards = [
    {
      title: 'Overview',
      tiles: [
        { id: 't1', type: 'kpi', measures: [{ table: 's1/orders', column: 'Net', agg: 'sum' }], dims: [] },
        { id: 't2', measures: [{ table: 's1/orders', column: 'amount', agg: 'sum', per: { table: 's1/orders', column: 'Net', agg: 'sum' } }], dims: [] },
        { id: 't3', type: 'filter', measures: [], dims: [{ table: 's1/orders', column: 'Net' }] },
        { id: 't4', measures: [{ table: 's1/orders', column: 'amount', agg: 'sum' }], dims: [{ table: 's1/orders', column: 'id' }] },
        { id: 't5', measures: [], dims: [], cols: { table: 's1/orders', column: 'Net' } },
      ],
      filters: [{ table: 's1/orders', column: 'Net', fromTile: '' }],
    },
  ];
  const uses = tilesUsing(dashboards, 's1/orders', 'Net', (t) => `tile ${t.id}`);
  assert.deepEqual(
    uses.map((u) => u.tile),
    ['tile t1', 'tile t2', 'tile t3', 'tile t5', ''],
  );
  assert.equal(uses[0].dashboard, 'Overview');
  assert.deepEqual(
    uses.map(useWords),
    ['tile t1 (number tile) on Overview', 'tile t2 (tile) on Overview', 'tile t3 (filter) on Overview', 'tile t5 (tile) on Overview', 'A saved filter on Overview'],
  );
  assert.equal(useWords({ dashboard: 'D', tile: '', kind: kindWords('bar') }), 'A bar chart on D');
  assert.deepEqual(tilesUsing(dashboards, 's2/stores', 'Net', () => ''), []);
});

test('a list of names reads in one line', () => {
  assert.equal(listWords(['a', 'b']), 'a, b');
  assert.equal(listWords(['a', 'b', 'c', 'd', 'e', 'f', 'g'], 3), 'a, b, c and 4 more');
});

test('the function list file is the one the engine tests against', () => {
  const raw = JSON.parse(readFileSync(new URL('../src/lib/functions.json', import.meta.url), 'utf8'));
  assert.deepEqual(
    raw.map((x) => x.name),
    FUNCTIONS.map((x) => x.name),
  );
});
