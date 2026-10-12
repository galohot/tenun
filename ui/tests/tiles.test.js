// KPI pace, the pivot layout and the "Also try" rules (build 05).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { targetInfo, targetWords } from '../src/lib/kpi.ts';
import { MAX_PIVOT_COLS, pivotModel } from '../src/lib/pivot.ts';
import { imageKind, normalisePicture } from '../src/lib/image.ts';
import { suggestTypes } from '../src/lib/suggest.ts';

test('target: a finished period compares the value itself', () => {
  const t = targetInfo(820, 1000, 1);
  assert.equal(t.share, 0.82);
  assert.equal(t.projected, null);
  assert.equal(t.onTrack, false);
  assert.equal(targetInfo(1000, 1000, 1).onTrack, true);
  assert.equal(targetInfo(500, 1000, undefined).projected, null);
});

test('target: an unfinished period is judged by pace', () => {
  // 14 of 31 days gone, 500 so far: heading for 1,107. Ahead of a 1,000 target though only 50 % there.
  const a = targetInfo(500, 1000, 14 / 31);
  assert.equal(a.share, 0.5);
  assert.ok(Math.abs(a.projected - 1107.14) < 0.01);
  assert.equal(a.onTrack, true);
  // 300 so far heads for 664: behind.
  assert.equal(targetInfo(300, 1000, 14 / 31).onTrack, false);
});

test('target: lower is better flips the verdict', () => {
  assert.equal(targetInfo(500, 1000, 14 / 31, 'down').onTrack, false); // heading for 1,107 over a limit of 1,000
  assert.equal(targetInfo(300, 1000, 14 / 31, 'down').onTrack, true);
  assert.equal(targetInfo(900, 1000, 1, 'down').onTrack, true);
});

test('target words: lower is better never says "Behind"', () => {
  // Higher is better.
  assert.deepEqual(targetWords(targetInfo(500, 1000, 14 / 31)), { chip: 'On track', pace: 'on pace' });
  assert.deepEqual(targetWords(targetInfo(300, 1000, 14 / 31)), { chip: 'Behind', pace: 'behind pace' });
  assert.deepEqual(targetWords(targetInfo(820, 1000, 1)), { chip: 'Behind', pace: '' });
  // Lower is better: over target, and heading over it while the period is not over.
  assert.deepEqual(targetWords(targetInfo(300, 1000, 14 / 31, 'down'), 'down'), { chip: 'On track', pace: 'on pace' });
  assert.deepEqual(targetWords(targetInfo(500, 1000, 14 / 31, 'down'), 'down'), { chip: 'Over target', pace: 'heading over target' });
  assert.deepEqual(targetWords(targetInfo(1200, 1000, 1, 'down'), 'down'), { chip: 'Over target', pace: '' });
  assert.deepEqual(targetWords(targetInfo(900, 1000, 1, 'down'), 'down'), { chip: 'On track', pace: '' });
});

test('target: nothing to say without a value or a sensible target', () => {
  assert.equal(targetInfo(null, 1000, 1), null);
  assert.equal(targetInfo(5, 0, 1), null);
  assert.equal(targetInfo(5, -3, 1), null);
  assert.equal(targetInfo(5, undefined, 1), null);
  assert.equal(targetInfo(5, NaN, 1), null);
});

const col = (name, role, dtype = 'text') => ({ name, role, dtype });

test('pivot: arranges rows, columns and totals as the engine sent them', () => {
  // Revenue by category (rows) and channel (columns).
  const res = {
    columns: [col('cat', 'dimension'), col('channel', 'dimension'), col('amount', 'measure', 'number')],
    rows: [
      ['Coffee', 'Online', 200],
      ['Coffee', 'Shop', 140],
      ['Tea', 'Online', 50],
      ['Tea', 'Shop', 30],
      ['Snack', 'Online', 10],
    ],
    ignored_filters: [],
    ms: 1,
    totals: {
      grand: [430],
      rows: [['Coffee', 340], ['Tea', 80], ['Snack', 10]],
      cols: [['Online', 260], ['Shop', 170]],
    },
  };
  const m = pivotModel(res, { cols: { table: 't', column: 'channel' } });
  assert.deepEqual(m.cols.map((c) => c.raw), ['Online', 'Shop']);
  assert.deepEqual(m.rows.map((r) => r.raw[0]), ['Coffee', 'Tea', 'Snack']);
  assert.deepEqual(m.rows[2].cells, [[10], null]); // Snack sold nothing in the shop
  assert.deepEqual(m.rows.map((r) => r.total[0]), [340, 80, 10]);
  assert.deepEqual(m.colTotals.map((c) => c[0]), [260, 170]);
  assert.deepEqual(m.grand, [430]);
  assert.equal(m.cut, false);
  // The cells add up to the totals the engine sent.
  const cellSum = m.rows.reduce((s, r) => s + r.cells.reduce((a, c) => a + (c?.[0] ?? 0), 0), 0);
  assert.equal(cellSum, m.grand[0]);
});

test('pivot: several values sit under each column, and an Other row stays last', () => {
  const res = {
    columns: [col('cat', 'dimension'), col('ch', 'dimension'), col('a', 'measure', 'number'), col('n', 'measure', 'number')],
    rows: [
      ['Coffee', 'Shop', 140, 2],
      ['\u0000other', 'Shop', 30, 1],
    ],
    other: true,
    ignored_filters: [],
    ms: 1,
    totals: { grand: [170, 3], rows: [['Coffee', 140, 2], ['\u0000other', 30, 1]], cols: [['Shop', 170, 3]] },
  };
  const m = pivotModel(res, { cols: { table: 't', column: 'ch' } });
  assert.equal(m.nValues, 2);
  assert.deepEqual(m.rows[0].cells[0], [140, 2]);
  assert.equal(m.rows.at(-1).raw[0], '\u0000other');
  assert.deepEqual(m.grand, [170, 3]);
});

test('pivot: without totals the columns are sorted, and more than 30 are cut', () => {
  const dates = Array.from({ length: 35 }, (_, i) => `2026-01-${String(i + 1).padStart(2, '0')}`).reverse();
  const res = {
    columns: [col('cat', 'dimension'), col('day', 'dimension', 'date'), col('v', 'measure', 'number')],
    rows: dates.map((d) => ['A', d, 1]),
    ignored_filters: [],
    ms: 1,
  };
  const m = pivotModel(res, { cols: { table: 't', column: 'day' } });
  assert.equal(m.cols.length, MAX_PIVOT_COLS);
  assert.equal(m.colCount, 35);
  assert.equal(m.cut, true);
  assert.equal(m.cols[0].raw, '2026-01-01');
  assert.equal(m.grand, null);
});

test('pivot: with no column field it is a list of the row fields', () => {
  const res = {
    columns: [col('cat', 'dimension'), col('v', 'measure', 'number')],
    rows: [['A', 1], ['B', 2]],
    ignored_filters: [],
    ms: 1,
    totals: { grand: [3] },
  };
  const m = pivotModel(res, {});
  assert.equal(m.hasCols, false);
  assert.equal(m.nRows, 1);
  assert.equal(m.cols.length, 1);
  assert.deepEqual(m.rows.map((r) => r.cells[0]), [[1], [2]]);
  assert.deepEqual(m.grand, [3]);
});

// ---- chart suggestions -------------------------------------------------------

const tables = [
  {
    id: 't',
    columns: [
      { name: 'date', dtype: 'date' },
      { name: 'cat', dtype: 'text' },
      { name: 'channel', dtype: 'text' },
      { name: 'amount', dtype: 'number' },
      { name: 'qty', dtype: 'number' },
    ],
  },
];
const sum = (column) => ({ table: 't', column, agg: 'sum' });
const dim = (column, grain) => ({ table: 't', column, ...(grain ? { grain } : {}) });
const tile = (type, measures, dims) => ({ id: 'x', type, title: '', x: 0, y: 0, w: 6, h: 4, measures, dims });
const types = (t, groups = null) => suggestTypes(t, tables, groups).map((s) => s.type);

test('suggestions: a date and a value', () => {
  assert.deepEqual(types(tile('bar', [sum('amount')], [dim('date', 'month')])), ['line', 'area']);
  assert.deepEqual(types(tile('kpi', [sum('amount')], [dim('date', 'month')])), ['line', 'area', 'bar']);
});

test('suggestions: a small category, but only when it is small', () => {
  assert.deepEqual(types(tile('bar', [sum('amount')], [dim('cat')]), 4), ['share', 'table']);
  assert.deepEqual(types(tile('line', [sum('amount')], [dim('cat')]), 12), ['bar', 'share', 'table']);
  assert.deepEqual(types(tile('bar', [sum('amount')], [dim('cat')]), 13), []);
  assert.deepEqual(types(tile('bar', [sum('amount')], [dim('cat')]), null), []);
});

test('suggestions: two values and a category offer scatter', () => {
  const s = suggestTypes(tile('bar', [sum('amount'), sum('qty')], [dim('cat')]), tables, 30);
  assert.deepEqual(s.map((x) => x.type), ['scatter']);
  assert.equal(s[0].variant.measures.length, 2);
});

test('suggestions: one number column alone', () => {
  assert.deepEqual(types(tile('table', [sum('amount')], [])), ['histogram', 'kpi']);
  assert.deepEqual(types(tile('histogram', [sum('amount')], [])), ['kpi']);
  // A text column cannot be binned.
  assert.deepEqual(types(tile('table', [{ table: 't', column: 'cat', agg: 'count' }], [])), []);
});

test('suggestions: a category and a date offer a pivot, category down the side', () => {
  const s = suggestTypes(tile('bar', [sum('amount')], [dim('date', 'month'), dim('cat')]), tables, null);
  const pivot = s.find((x) => x.type === 'pivot');
  assert.equal(pivot.variant.dims[0].column, 'cat');
  assert.equal(pivot.variant.cols.column, 'date');
});

test('suggestions: never more than three, never the current type, nothing for notes', () => {
  const t = tile('bar', [sum('amount')], [dim('date', 'month'), dim('cat')]);
  const all = types(t, null);
  assert.ok(all.length <= 3);
  assert.ok(!all.includes('bar'));
  assert.deepEqual(types(tile('text', [], []), 5), []);
  assert.deepEqual(types(tile('image', [], []), 5), []);
  assert.deepEqual(types(tile('bar', [], []), 5), []);
  // A share strip is not offered on a share strip. A donut is a donut again (V4-D7), so a strip may be offered on it.
  assert.ok(!types(tile('share', [sum('amount')], [dim('cat')]), 5).includes('share'));
});

test('pivot: with two row fields, rows of the same first value sit together', () => {
  const res = {
    columns: [col('cat', 'dimension'), col('region', 'dimension'), col('ch', 'dimension'), col('v', 'measure', 'number')],
    // The engine ranks by size, so the two categories alternate.
    rows: [['A', 'N', 'x', 9], ['B', 'N', 'x', 8], ['A', 'S', 'x', 7], ['B', 'S', 'x', 6]],
    ignored_filters: [],
    ms: 1,
    totals: { grand: [30], rows: [['A', 'N', 9], ['B', 'N', 8], ['A', 'S', 7], ['B', 'S', 6]], cols: [['x', 30]] },
  };
  const m = pivotModel(res, { cols: { table: 't', column: 'ch' } });
  assert.deepEqual(m.rows.map((r) => r.raw.join('/')), ['A/N', 'A/S', 'B/N', 'B/S']);
  assert.deepEqual(m.rows.map((r) => r.total[0]), [9, 7, 8, 6]);
});

test('pivot: counts every column the engine knows, and a pivot with no row field has its total on the one row', () => {
  const res = {
    columns: [col('day', 'dimension', 'date'), col('v', 'measure', 'number')],
    rows: [['a', 1], ['b', 2]],
    ignored_filters: [],
    ms: 1,
    totals: { grand: [10], cols: [['a', 1], ['b', 2], ['c', 3], ['d', 4]] },
  };
  const m = pivotModel(res, { cols: { table: 't', column: 'day' } });
  assert.equal(m.nRows, 0);
  assert.equal(m.colCount, 4);
  assert.deepEqual(m.rows[0].total, [10]);
});

test('image: the first bytes decide the kind, not the name', () => {
  const b64 = (s) => Buffer.from(s, 'latin1').toString('base64');
  const png = `data:image/png;base64,${b64('\x89PNG\r\n\x1a\n\0\0\0\rIHDR')}`;
  assert.equal(imageKind(png), 'png');
  const svg = `data:image/png;base64,${b64('<svg xmlns="http://www.w3.org/2000/svg"/>')}`;
  assert.equal(imageKind(svg), null);
  assert.equal(normalisePicture(svg), null);
  const jpeg = `data:image/png;base64,${b64('\xff\xd8\xff\xe0\0\x10JFIF')}`;
  assert.ok(normalisePicture(jpeg).startsWith('data:image/jpeg;base64,'));
  assert.equal(imageKind(`data:image/webp;base64,${b64('RIFF\0\0\0\0WEBPVP8 ')}`), 'webp');
});
