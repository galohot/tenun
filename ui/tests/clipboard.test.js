// Copy and paste (build 09, V4-D11): what a copy carries, how a paste reads it, the guesses of Match fields, the
// tiles moved to the matched fields, formulas renamed, names that clash, and where pasted tiles land.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  buildClip,
  clipText,
  dashboardCopy,
  fieldsOf,
  formulaRefs,
  freeName,
  guessMatches,
  joinMeasures,
  joinStyles,
  landTiles,
  MARKER,
  measureIds,
  placeBelow,
  readClip,
  renamesFor,
  remapTile,
  rewriteFormula,
  sameWorkspace,
  unmatched,
} from '../src/lib/clipboard.ts';

const col = (name, dtype = 'number') => ({ name, dtype, nullable: true });
const SALES = { id: 's1/Sales', source_id: 's1', name: 'Sales', label: 'Sales', rows: 3, columns: [col('date', 'date'), col('region', 'text'), col('amount'), col('cups'), col('note', 'text')] };
const STORES = { id: 's2/stores', source_id: 's2', name: 'stores', label: 'Stores', rows: 2, columns: [col('store_id', 'text'), col('city', 'text'), col('area')] };
const OTHER = { id: 's3/unused', source_id: 's3', name: 'unused', label: 'Unused', rows: 1, columns: [col('x')] };
const tile = (id, extra) => ({ id, type: 'bar', title: '', x: 0, y: 0, w: 4, h: 3, measures: [], dims: [], ...extra });

const kpi = tile('k', {
  type: 'kpi',
  measures: [{ table: 's1/Sales', column: 'amount', agg: 'sum', per: { table: 's2/stores', column: 'area', agg: 'sum' } }],
  dims: [{ table: 's1/Sales', column: 'date', grain: 'month', via: 'r9' }],
  card: { style: 'cs-a' },
  y: 2,
});
const chart = tile('c', { measures: [{ table: 's1/Sales', column: null, agg: 'count', named: 'm2' }], dims: [{ table: 's1/Sales', column: 'region' }], x: 4, y: 5 });
const text = tile('t', { type: 'text', text: '# Notes about [Sales].[amount]', y: 8 });
const ws = {
  measures: [
    { id: 'm1', name: 'Unused', formula: 'SUM([Unused].[x])' },
    { id: 'm2', name: 'Cups per order', formula: 'SUM([Sales].[cups]) / COUNT([Sales].[amount])' },
  ],
  cardStyles: [
    { id: 'cs-a', name: 'Headline', card: { size: 'l', ground: 'accent' } },
    { id: 'cs-b', name: 'Other', card: { size: 's' } },
  ],
};
const measureTables = { m1: ['s3/unused'], m2: ['s1/Sales'] };

test('a copy carries the tiles and only the measures, styles, tables and columns they use', () => {
  const clip = buildClip([kpi, chart, text], ws, [SALES, STORES, OTHER], measureTables);
  assert.deepEqual(clip.tiles.map((t) => t.id), ['k', 'c', 't']);
  assert.deepEqual(clip.measures.map((m) => m.id), ['m2']);
  assert.deepEqual(clip.cardStyles.map((s) => s.id), ['cs-a']);
  // Each table with the columns read: by the tiles (date, region, amount; area) and by the measure's formula (cups,
  // amount). Not note: nothing reads it. In the order of the table.
  assert.deepEqual(clip.tables, [
    { id: 's1/Sales', label: 'Sales', columns: [{ name: 'date', dtype: 'date' }, { name: 'region', dtype: 'text' }, { name: 'amount', dtype: 'number' }, { name: 'cups', dtype: 'number' }] },
    { id: 's2/stores', label: 'Stores', columns: [{ name: 'area', dtype: 'number' }] },
  ]);
  assert.deepEqual(formulaRefs('SUM([Sales].[cups]) / COUNT([sales].[a]]b]) & "[X].[y]"'), [{ table: 'Sales', column: 'cups' }, { table: 'sales', column: 'a]b' }]);
  // The fields of a tile: the value, its "divide by", the date.
  assert.deepEqual(fieldsOf(kpi), [
    { table: 's1/Sales', column: 'amount' },
    { table: 's2/stores', column: 'area' },
    { table: 's1/Sales', column: 'date' },
  ]);
  // The copy is a copy: changing it does not change the tile.
  clip.tiles[0].title = 'changed';
  assert.equal(kpi.title, '');
});

test('a paste reads its own text back, and nothing else', () => {
  const clip = buildClip([chart], ws, [SALES], measureTables, { title: 'Sales', grain: 'month' });
  const text = clipText(clip);
  assert.ok(text.startsWith(`${MARKER}\n{`));
  assert.deepEqual(readClip(text), clip);
  assert.deepEqual(readClip(text.replace('\n', '\r\n')), clip);
  assert.equal(readClip('region\tamount\nJava\t10'), null);
  // The marker must be the first line: a note that mentions it is not a copy.
  assert.equal(readClip(`A note about ${text}`), null);
  assert.equal(readClip(`${MARKER}\nnot json`), null);
  assert.equal(readClip(`${MARKER}\n{"tenun":"tiles","version":2,"tiles":[]}`), null);
  assert.equal(readClip(`${MARKER}\n{"tenun":"tiles","version":1,"tiles":[{"id":"x"}]}`), null);
  // A copy without the extras still reads: nothing is missing.
  assert.deepEqual(readClip(`${MARKER}\n{"tenun":"tiles","version":1,"tiles":[]}`), { tenun: 'tiles', version: 1, tiles: [], measures: [], cardStyles: [], tables: [] });
});

test('the same workspace is the one with every table and column; another gets guesses by label, columns, then type', () => {
  const clip = buildClip([kpi, chart], ws, [SALES, STORES], measureTables);
  assert.equal(sameWorkspace(clip, [SALES, STORES]), true);
  assert.equal(sameWorkspace(clip, [SALES]), false);
  // The other workspace: "sales" by its label (any case); stores has a new name, found by a column it shares.
  const there = [
    { ...SALES, id: 'a/sales', label: 'sales', columns: [col('Date', 'date'), col('Region', 'text'), col('total'), col('cups'), col('memo', 'text')] },
    { ...STORES, id: 'b/shops', label: 'Shops', columns: [col('shop', 'text'), col('area'), col('floor')] },
    { ...OTHER, id: 'c/x', label: 'X' },
  ];
  const m = guessMatches(clip, there);
  assert.deepEqual(m['s1/Sales'], { to: 'a/sales', columns: { date: 'Date', region: 'Region', amount: 'total', cups: 'cups' } });
  assert.deepEqual(m['s2/stores'], { to: 'b/shops', columns: { area: 'area' } });
  assert.deepEqual(unmatched(clip, m), []);
  // Nothing alike: no guess, and the fields are listed.
  const none = guessMatches(clip, [{ ...OTHER, columns: [col('when', 'date')] }]);
  assert.equal(none['s2/stores'].to, null);
  assert.deepEqual(unmatched(clip, none).map((u) => `${u.table}.${u.column}`), ['Sales.null', 'Stores.null']);
});

test('a pasted tile reads the matched fields; a link of the other workspace is dropped, one here is kept', () => {
  const m = { 's1/Sales': { to: 'a/sales', columns: { amount: 'total', date: 'Date' } }, 's2/stores': { to: null, columns: { area: null } } };
  const t = remapTile(kpi, m, new Set());
  assert.deepEqual(t.measures[0], { table: 'a/sales', column: 'total', agg: 'sum', per: { table: 's2/stores', column: 'area', agg: 'sum' } });
  assert.deepEqual(t.dims[0], { table: 'a/sales', column: 'Date', grain: 'month' });
  assert.equal(remapTile(kpi, m, new Set(['r9'])).dims[0].via, 'r9');
  // The tile it came from is not changed; a named measure's own fields are not touched.
  assert.equal(kpi.measures[0].table, 's1/Sales');
  assert.equal(remapTile(chart, m, new Set()).measures[0].table, 's1/Sales');
});

test('a formula takes the matched names; text in quotes and other tables stay', () => {
  const r = [{ table: 'Sales', to: 'Orders', columns: { amount: 'total', cups: 'cups' } }];
  assert.equal(rewriteFormula('SUM([Sales].[amount]) / COUNT([sales].[cups])', r), 'SUM([Orders].[total]) / COUNT([Orders].[cups])');
  assert.equal(rewriteFormula('IF([Sales].[note] = "[Sales].[amount]", 1, 0)', r), 'IF([Orders].[note] = "[Sales].[amount]", 1, 0)');
  assert.equal(rewriteFormula('SUM([Stores].[area]) + [amount]', r), 'SUM([Stores].[area]) + [amount]');
  // A bracket inside a name is written twice, and stays so.
  assert.equal(rewriteFormula('SUM([Sales].[a]]b])', [{ table: 'Sales', to: 'Q]1', columns: { 'a]b': 'c' } }]), 'SUM([Q]]1].[c])');
  assert.equal(rewriteFormula('"say ""hi"" [Sales].[amount]"', r), '"say ""hi"" [Sales].[amount]"');
});

test('names that clash get a number; identical measures and styles are used as they are', () => {
  assert.equal(freeName('Revenue', ['revenue', 'Revenue (2)']), 'Revenue (3)');
  assert.equal(freeName('Margin', ['Revenue']), 'Margin');
  const here = [{ id: 'm2', name: 'Cups per order', formula: 'SUM([Sales].[cups]) / COUNT([Sales].[amount])' }];
  // The same measure: the same id. A different formula under the same name: added as "Cups per order (2)", no id yet.
  assert.deepEqual(joinMeasures(ws.measures.slice(1), here), { added: [], ids: { m2: 'm2' } });
  const changed = joinMeasures([{ id: 'm2', name: 'Cups per order', formula: 'SUM([Sales].[cups])' }], here);
  assert.deepEqual(changed, { added: [{ id: '', name: 'Cups per order (2)', formula: 'SUM([Sales].[cups])' }], ids: { m2: { added: 0 } } });
  // A rewrite runs before the comparison.
  const moved = joinMeasures(ws.measures.slice(1), [], (f) => f.replace(/Sales/g, 'Orders'));
  assert.equal(moved.added[0].formula, 'SUM([Orders].[cups]) / COUNT([Orders].[amount])');
  let n = 0;
  const styles = joinStyles(ws.cardStyles, [{ id: 'cs-a', name: 'Headline', card: { size: 'm' } }], () => `cs-new${++n}`);
  assert.deepEqual(styles.ids, { 'cs-a': 'cs-new1', 'cs-b': 'cs-b' });
  assert.deepEqual(styles.list.map((s) => s.name), ['Headline', 'Headline (2)', 'Other']);
  assert.deepEqual(joinStyles(ws.cardStyles.slice(0, 1), ws.cardStyles, () => 'x').ids, { 'cs-a': 'cs-a' });
});

test('pasted tiles land below the lowest tile and keep their places to each other', () => {
  const pasted = placeBelow([tile('a', { x: 4, y: 3, h: 2 }), tile('b', { x: 0, y: 5, h: 4 })], [tile('x', { y: 0, h: 3 }), tile('y', { y: 6, h: 2 })]);
  assert.deepEqual(pasted.map((t) => [t.x, t.y]), [[4, 8], [0, 10]]);
  assert.deepEqual(placeBelow([tile('a', { y: 7 })], []).map((t) => t.y), [0]);
});

test('a paste from another workspace: renames for the formulas, ids of the saved measures, the tiles as they land', () => {
  const clip = buildClip([kpi, chart], ws, [SALES, STORES], measureTables);
  const there = [{ ...SALES, id: 'a/orders', label: 'Orders' }];
  const m = { 's1/Sales': { to: 'a/orders', columns: { amount: 'total', cups: 'cups', note: null } }, 's2/stores': { to: null, columns: { area: null } } };
  // Only matched columns are renamed; an unmatched table renames nothing.
  assert.deepEqual(renamesFor(clip, m, there), [{ table: 'Sales', to: 'Orders', columns: { amount: 'total', cups: 'cups' } }]);
  // Two new measures were sent after the 3 here; the engine refused the first, so the second is the 4th saved.
  const added = [{ id: '', name: 'A', formula: 'x' }, { id: '', name: 'B', formula: 'y' }];
  const saved = [{ id: 'm1' }, { id: 'm2' }, { id: 'm3' }, { id: 'm7' }];
  assert.deepEqual(measureIds({ old1: 'm2', oldA: { added: 0 }, oldB: { added: 1 } }, added, [added[1]], saved, 3), { old1: 'm2', oldB: 'm7' });
  let n = 0;
  const landed = landTiles(clip, m, { measures: { m2: 'm7' }, styles: { 'cs-a': 'cs-z' } }, new Set(), () => `new${++n}`);
  assert.deepEqual(landed.map((t) => t.id), ['new1', 'new2']);
  assert.equal(landed[0].measures[0].table, 'a/orders');
  assert.equal(landed[0].card.style, 'cs-z');
  assert.equal(landed[1].measures[0].named, 'm7');
  // From this workspace: the fields stay.
  const same = landTiles(clip, null, { measures: {}, styles: {} }, new Set(), () => 'n');
  assert.deepEqual(same[0].measures, kpi.measures);
  assert.deepEqual(same[0].dims, kpi.dims);
});

test('a duplicated dashboard has new ids, its own title, its filters on the new tiles, and no publish link', () => {
  const d = { id: 'd1', title: 'Sales', refresh: 60, grain: 'month', tiles: [kpi, chart], publish: { id: 'p', url: 'u' }, filters: [{ table: 's1/Sales', column: 'region', op: 'in', values: ['Java'], fromTile: 'c' }, { table: 's1/Sales', column: 'region', op: 'in', values: [], fromTile: '' }] };
  let n = 0;
  const c = dashboardCopy(d, 'Sales copy', (p) => `${p}${++n}`);
  assert.equal(c.title, 'Sales copy');
  assert.equal(c.grain, 'month');
  assert.notEqual(c.id, 'd1');
  assert.deepEqual(c.tiles.map((t) => t.id), ['t1', 't2']);
  assert.deepEqual(c.filters.map((f) => f.fromTile), ['t2', '']);
  assert.equal(c.publish, undefined);
  assert.equal(d.tiles[0].id, 'k');
});
