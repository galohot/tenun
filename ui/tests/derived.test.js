// New tables from other tables (build 04, V4-D6): what the sheet works out before it asks the engine.
// Pure functions, no browser.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { appendColumns, fromWords, guessKey, inputsOf, OPS, pickable, specOf, specProblem, tableNameProblem } from '../src/lib/derived.ts';

const col = (name, dtype = 'text') => ({ name, dtype });
const table = (id, label, cols) => ({ id, label, columns: cols.map((c) => (typeof c === 'string' ? col(c) : c)) });
const jan = table('s1/jan', 'jan', ['id', col('amount', 'number'), 'note']);
const feb = table('s2/feb', 'feb', ['id', col('amount', 'text'), 'extra']);
const orders = table('s3/orders', 'orders', ['id', 'store_id', 'region', 'amount']);
const stores = table('s4/stores', 'stores', ['id', 'name', 'region']);

test('an append lists each column once, in order, with the tables that lack it and the kinds it mixes', () => {
  assert.deepEqual(appendColumns([jan, feb]), [
    { name: 'id', missing: [], types: ['text'] },
    { name: 'amount', missing: [], types: ['number', 'text'] },
    { name: 'note', missing: ['feb'], types: ['text'] },
    { name: 'extra', missing: ['jan'], types: ['text'] },
  ]);
  assert.deepEqual(appendColumns([]), []);
});

test('the sheet says what is missing before it asks the engine', () => {
  assert.equal(specProblem({ op: 'append', tables: ['s1/jan'] }), 'Pick two or more tables to append.');
  assert.equal(specProblem({ op: 'append', tables: ['s1/jan', 's2/feb'] }), null);
  const merge = (left, right, on) => ({ op: 'merge', left, right, on, keep: 'left' });
  assert.equal(specProblem(merge('', 's4/stores', [['a', 'b']])), 'Pick the two tables.');
  assert.equal(specProblem(merge('s3/orders', 's3/orders', [['id', 'id']])), 'Pick two different tables to merge.');
  assert.equal(specProblem(merge('s3/orders', 's4/stores', [['store_id', '']])), 'Pick the columns that match: one from each table.');
  assert.equal(specProblem(merge('s3/orders', 's4/stores', [])), 'Pick the columns that match: one from each table.');
  assert.equal(specProblem(merge('s3/orders', 's4/stores', [['store_id', 'id']])), null);
  const group = (table, by, values) => ({ op: 'group', table, by, values });
  assert.equal(specProblem(group('', ['region'], [])), 'Pick a table.');
  assert.equal(specProblem(group('s3/orders', [], [])), 'Pick a column to group by, or add a value.');
  assert.equal(specProblem(group('s3/orders', ['region'], [{ name: 'Total', formula: ' ' }])), 'Give each value a name and a formula.');
  assert.equal(specProblem(group('s3/orders', ['region'], [{ name: 'Total', formula: 'SUM([amount])' }])), null);
  assert.equal(specProblem(group('s3/orders', ['region'], [])), null);
});

test('the name follows the rules of the engine', () => {
  assert.equal(tableNameProblem(' '), 'Give the new table a name.');
  assert.equal(tableNameProblem('__x'), 'A name cannot start with two underscores.');
  assert.match(tableNameProblem('a'.repeat(65)), /^A name has 64 characters at most\. This one has 65\.$/);
  assert.equal(tableNameProblem('Sales by province'), null);
  // The engine's words are the same.
  const rust = readFileSync(new URL('../../crates/tenun-core/src/derived.rs', import.meta.url), 'utf8');
  for (const words of ['Give the new table a name.', 'A name cannot start with two underscores.', 'Pick two or more tables to append.', 'Pick two different tables to merge.', 'Pick the columns that match: one from each table.']) {
    assert.ok(rust.includes(words), words);
  }
});

test('a new table says what it reads, and a table that is gone keeps its name', () => {
  const spec = { op: 'merge', left: 's3/orders', right: 's4/stores', on: [['store_id', 'id']], keep: 'inner' };
  assert.deepEqual(inputsOf(spec), ['s3/orders', 's4/stores']);
  assert.equal(fromWords(spec, [orders, stores]), 'From: orders, stores');
  assert.equal(fromWords(spec, [orders]), 'From: orders, stores');
  assert.equal(fromWords({ op: 'append', tables: ['s1/jan', 's1/jan'] }, [jan]), 'From: jan');
  assert.equal(specOf({ kind: 'derived', spec }), spec);
  assert.equal(specOf({ kind: 'csv' }), null);
  assert.deepEqual(OPS.map((o) => o.op), ['append', 'merge', 'group']);
});

test('a new table that is being changed cannot pick itself or a table that reads it', () => {
  const sources = [
    { id: 's1', kind: 'csv' },
    { id: 's5', kind: 'derived', spec: { op: 'append', tables: ['s1/jan', 's2/feb'] } },
    { id: 's6', kind: 'derived', spec: { op: 'group', table: 's5/Months', by: [], values: [] } },
    { id: 's7', kind: 'derived', spec: { op: 'group', table: 's3/orders', by: [], values: [] } },
  ];
  const all = [jan, feb, orders, table('s5/Months', 'Months', ['id']), table('s6/G', 'G', ['n']), table('s7/H', 'H', ['n'])];
  assert.deepEqual(pickable(all, sources, 's5').map((t) => t.id), ['s1/jan', 's2/feb', 's3/orders', 's7/H']);
  assert.equal(pickable(all, sources).length, all.length);
});

test('a merge starts from a key the two tables name alike', () => {
  assert.deepEqual(guessKey(orders, stores), ['store_id', 'id']);
  const lines = table('s9/lines', 'lines', ['order_id', 'id', 'qty']);
  assert.deepEqual(guessKey(lines, orders), ['order_id', 'id']);
  const both = table('s10/a', 'a', ['code', 'x']);
  assert.deepEqual(guessKey(both, table('s11/b', 'b', ['code', 'y'])), ['code', 'code']);
  assert.deepEqual(guessKey(jan, stores), ['', '']);
  assert.deepEqual(guessKey(undefined, stores), ['', '']);
});
