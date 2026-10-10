// Column labels, the words for what was detected, and edits of a table's setup (build 08).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  barHeights,
  blankNote,
  failedText,
  hasUnpivot,
  isEmptySetup,
  isShown,
  labelOf,
  withColumn,
  withHeaderRow,
  withoutUnpivot,
  withUnpivot,
} from '../src/lib/prep.ts';

const tables = [
  {
    id: 's1/Sales',
    columns: [
      { name: 'amt', dtype: 'number', raw_type: 'DOUBLE', label: 'Amount (Rp)' },
      { name: 'qty', dtype: 'number', raw_type: 'BIGINT', label: '  ' },
      { name: 'day', dtype: 'date', raw_type: 'DATE' },
    ],
  },
];

test('a label is the name shown; a blank one is not a label', () => {
  assert.equal(labelOf(tables, 's1/Sales', 'amt'), 'Amount (Rp)');
  assert.equal(labelOf(tables, 's1/Sales', 'qty'), undefined);
  assert.equal(labelOf(tables, 's1/Sales', 'day'), undefined);
  assert.equal(labelOf(tables, 's1/Nope', 'amt'), undefined);
  assert.equal(labelOf(undefined, 's1/Sales', 'amt'), undefined);
});

test('a hidden column stays in a picker only while it is in use', () => {
  const c = { name: 'a', dtype: 'text', raw_type: '', hidden: true };
  assert.equal(isShown(c), false);
  assert.equal(isShown(c, true), true);
  assert.equal(isShown({ ...c, hidden: false }), true);
});

test('the blank share shows from 10 % up', () => {
  assert.equal(blankNote({}), '');
  assert.equal(blankNote({ blank: 0.1 }), '');
  assert.equal(blankNote({ blank: 0.104 }), '10 % blank');
  assert.equal(blankNote({ blank: 0.234 }), '23 % blank');
  assert.equal(blankNote({ blank: 1 }), '100 % blank');
});

test('unreadable cells are counted in words', () => {
  assert.equal(failedText({ dtype: 'date' }), '');
  assert.equal(failedText({ dtype: 'date', failed: 12 }), '12 values could not be read as dates.');
  assert.equal(failedText({ dtype: 'date', failed: 1 }), '1 value could not be read as a date.');
  assert.equal(failedText({ dtype: 'number', failed: 3 }), '3 values could not be read as numbers.');
});

test('column overrides are added, changed and removed without leaving empty parts', () => {
  let s = withColumn(undefined, 'amt', { label: ' Amount ' });
  assert.deepEqual(s, { columns: { amt: { label: 'Amount' } } });
  s = withColumn(s, 'amt', { hidden: true, description: 'Before tax' });
  assert.deepEqual(s.columns.amt, { label: 'Amount', hidden: true, description: 'Before tax' });
  // Showing the column again drops the flag; an empty label drops the label.
  s = withColumn(s, 'amt', { hidden: false, label: '' });
  assert.deepEqual(s.columns.amt, { description: 'Before tax' });
  s = withColumn(s, 'amt', { description: undefined });
  assert.deepEqual(s, {});
  assert.equal(isEmptySetup(s), true);
  // A date format only goes with a date type, and "automatic" is not stored.
  s = withColumn(undefined, 'day', { dtype: 'date', date_format: 'dmy' });
  assert.deepEqual(s.columns.day, { dtype: 'date', date_format: 'dmy' });
  assert.deepEqual(withColumn(s, 'day', { dtype: 'number' }).columns.day, { dtype: 'number' });
  assert.deepEqual(withColumn(s, 'day', { date_format: 'auto' }).columns.day, { dtype: 'date' });
  assert.deepEqual(withColumn(s, 'day', null), {});
});

test('the header row is a whole number from 1, or back to detection', () => {
  assert.deepEqual(withHeaderRow(undefined, 4), { header_row: 4 });
  assert.deepEqual(withHeaderRow({ header_row: 4 }, undefined), {});
  assert.deepEqual(withHeaderRow({ header_row: 4 }, 0), {});
  assert.deepEqual(withHeaderRow(undefined, 3.7), { header_row: 3 });
});

test('turning columns into rows is one step that can be undone', () => {
  const plan = { keep: ['Region'] };
  let s = withUnpivot({ header_row: 2 }, plan, ' Month ', 'Budget');
  assert.deepEqual(s.transforms, [{ kind: 'unpivot', keep: ['Region'], name: 'Month', value: 'Budget' }]);
  assert.equal(hasUnpivot(s), true);
  // Doing it again replaces it.
  s = withUnpivot(s, plan, 'Period', 'Amount');
  assert.equal(s.transforms.length, 1);
  assert.equal(s.transforms[0].name, 'Period');
  s = withoutUnpivot(s);
  assert.deepEqual(s, { header_row: 2 });
  assert.equal(hasUnpivot(s), false);
});

test('histogram bars scale to the biggest bin', () => {
  assert.deepEqual(barHeights([1, 2, 4]), [0.25, 0.5, 1]);
  assert.deepEqual(barHeights([0, 0]), [0, 0]);
  assert.deepEqual(barHeights([]), []);
});
