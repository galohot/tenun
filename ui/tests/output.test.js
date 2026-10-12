// Output and layout (build 11): the PDF file, the phone layout, the chart sentence and the CSV.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { A4, buildPdf, buildPdfPages, pageSize, PAGE_MAX, PAGE_WIDTH } from '../src/lib/pdf.ts';
import { phoneLayout, PHONE_CHART_HEIGHT, readingOrder } from '../src/lib/layout.ts';
import { chartSummary } from '../src/lib/summary.ts';
import { csvCell, fileName, kpiRows, tileTable, toCsv } from '../src/lib/tabular.ts';

const latin1 = (bytes) => Buffer.from(bytes).toString('latin1');

test('pdf: header, trailer and every xref offset points at its object', () => {
  const jpeg = Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0xff, 0xd9]);
  const pdf = buildPdf(jpeg, { w: 2400, h: 1600 }, pageSize(2400, 1600), 'Sales (Q3) \\ ünï');
  const text = latin1(pdf);
  assert.ok(text.startsWith('%PDF-1.4\n'));
  assert.ok(text.endsWith('%%EOF\n'));
  // startxref points at the word "xref".
  const start = Number(/startxref\n(\d+)\n%%EOF\n$/.exec(text)[1]);
  assert.equal(text.slice(start, start + 5), 'xref\n');
  const table = text.slice(start).split('\n');
  const [first, count] = table[1].split(' ').map(Number);
  assert.equal(first, 0);
  assert.equal(count, 7); // the free entry and six objects
  assert.equal(table[2], '0000000000 65535 f ');
  for (let n = 1; n < count; n++) {
    const [offset, gen, flag] = table[2 + n].split(' ');
    assert.equal(offset.length, 10);
    assert.equal(gen, '00000');
    assert.equal(flag, 'n');
    assert.equal(text.slice(Number(offset), Number(offset) + `${n} 0 obj`.length), `${n} 0 obj`);
  }
  assert.match(text, new RegExp(`/Size ${count} /Root 1 0 R`));
  // Every xref line is 20 bytes.
  assert.ok(table.slice(2, 2 + count).every((l) => l.length + 1 === 20));
});

test('pdf: a printed story has A4 pages, each with its own picture drawn from the top', () => {
  const jpeg = (k) => Uint8Array.from([0xff, 0xd8, k, k, k, 0xff, 0xd9]);
  // Three slices of a 1128 px wide picture: two full pages and a short last one.
  const pages = [1460, 1460, 600].map((h, k) => ({ jpeg: jpeg(k + 1), px: { w: 1128, h }, page: A4, draw: { w: A4.w, h: (A4.w * h) / 1128 } }));
  const text = latin1(buildPdfPages(pages, 'Story'));
  assert.match(text, /\/Kids \[3 0 R 6 0 R 9 0 R\] \/Count 3/);
  assert.equal((text.match(/\/MediaBox \[0 0 595.28 841.89\]/g) ?? []).length, 3);
  // The last picture is 600 px high: 595.28 × 600 / 1128 = 316.64 pt, drawn from the top: 841.89 − 316.64 = 525.25.
  assert.match(text, /q 595.28 0 0 316.64 0 525.25 cm \/Im2 Do Q/);
  // Every xref offset points at its object, and each picture is its own JPEG.
  const start = Number(/startxref\n(\d+)\n%%EOF\n$/.exec(text)[1]);
  const table = text.slice(start).split('\n');
  const count = Number(table[1].split(' ')[1]);
  assert.equal(count, 13); // the free entry, catalog, tree, three objects for each of three pages, the info
  for (let n = 1; n < count; n++) {
    const offset = Number(table[2 + n].split(' ')[0]);
    assert.equal(text.slice(offset, offset + `${n} 0 obj`.length), `${n} 0 obj`);
  }
  assert.match(text, /\/Info 12 0 R/);
  for (let k = 0; k < 3; k++) assert.ok(text.includes(latin1(jpeg(k + 1))));
});

test('pdf: the image stream is the JPEG, byte for byte, and its length is right', () => {
  const jpeg = Uint8Array.from({ length: 300 }, (_, i) => (i * 7 + 3) & 0xff);
  jpeg[0] = 0xff;
  jpeg[1] = 0xd8;
  const pdf = buildPdf(jpeg, { w: 10, h: 5 }, { w: 842, h: 421 });
  const text = latin1(pdf);
  assert.match(text, /\/Filter \/DCTDecode \/Length 300 >>\nstream\n/);
  const at = text.indexOf('/DCTDecode');
  const from = text.indexOf('stream\n', at) + 7;
  assert.deepEqual([...pdf.slice(from, from + 300)], [...jpeg]);
  assert.equal(text.slice(from + 300, from + 300 + 10), '\nendstream');
  assert.match(text, /\/Width 10 \/Height 5 /);
  assert.match(text, /\/MediaBox \[0 0 842 421\]/);
});

test('pdf: the page keeps the shape of the picture and stays inside what viewers allow', () => {
  assert.deepEqual(pageSize(2400, 1200), { w: PAGE_WIDTH, h: 421 });
  const tall = pageSize(1000, 100000);
  assert.equal(tall.h, PAGE_MAX);
  assert.ok(tall.w < PAGE_WIDTH);
});

const tile = (id, type, x, y, w = 3, h = 2) => ({ id, type, x, y, w, h, title: '', measures: [], dims: [] });

test('phone layout: number tiles first, two to a row, then the rest in reading order', () => {
  const tiles = [tile('c', 'bar', 0, 4, 12, 4), tile('k2', 'kpi', 3, 0), tile('k1', 'kpi', 0, 0), tile('k3', 'kpi', 6, 0), tile('t', 'text', 0, 2, 4, 1)];
  const out = phoneLayout(tiles, 366, 76, 12);
  assert.deepEqual(out.map((p) => p.t.id), ['k1', 'k2', 'k3', 't', 'c']);
  const [k1, k2, k3, t, c] = out;
  assert.equal(k1.y, k2.y);
  assert.ok(k2.x > k1.x);
  assert.equal(k1.w, (366 - 12) / 2);
  // The third number is alone in its row, so it takes the whole width.
  assert.equal(k3.w, 366);
  assert.ok(k3.y > k1.y);
  assert.equal(t.h, 76);
  assert.equal(c.h, PHONE_CHART_HEIGHT);
  assert.equal(c.w, 366);
  // Nothing overlaps: each block starts below the one before.
  assert.ok(t.y >= k3.y + k3.h && c.y >= t.y + t.h);
});

test('phone layout: very narrow screens give each number the whole row', () => {
  const out = phoneLayout([tile('a', 'kpi', 0, 0), tile('b', 'kpi', 3, 0)], 200);
  assert.ok(out.every((p) => p.w === 200 && p.x === 0));
  assert.ok(out[1].y > out[0].y);
});

test('phone layout: a number one row high stays one row high', () => {
  const [p] = phoneLayout([tile('a', 'kpi', 0, 0, 3, 1)], 366);
  assert.equal(p.h, 76);
  assert.deepEqual(readingOrder([tile('b', 'bar', 0, 2), tile('a', 'bar', 6, 0), tile('c', 'bar', 0, 0)]).map((t) => t.id), ['c', 'a', 'b']);
});

const words = {
  label: (v) => String(v),
  value: (v) => `${v / 1000} rb`,
};
const col = (name, role, dtype = 'text') => ({ name, role, dtype });

test('chart sentence: kind, title, count, highest and lowest', () => {
  const res = {
    columns: [col('product', 'dimension'), col('revenue', 'measure', 'number')],
    rows: [['Kopi Susu', 1200], ['Latte', 800], ['Teh', 120]],
    ignored_filters: [],
    ms: 0,
  };
  assert.equal(
    chartSummary({ type: 'bar' }, res, 'Revenue by product', words),
    'Bar chart. Revenue by product. 3 bars. Highest: Kopi Susu, 1.2 rb. Lowest: Teh, 0.12 rb.',
  );
  assert.match(chartSummary({ type: 'line' }, { ...res, rows: [['a', 1]] }, 'One', words), /^Line chart\. One\. 1 point\. Value: a, 0\.001 rb\.$/);
  assert.equal(chartSummary({ type: 'bar' }, { ...res, rows: [] }, 'Empty', words), 'Bar chart. Empty. No data.');
});

test('chart sentence: a split chart counts its categories, not every segment', () => {
  const res = {
    columns: [col('m', 'dimension'), col('r', 'dimension'), col('v', 'measure', 'number')],
    rows: [['Jan', 'A', 5], ['Jan', 'B', 7], ['Feb', 'A', 9], ['Feb', 'B', 1]],
    ignored_filters: [],
    ms: 0,
  };
  assert.match(chartSummary({ type: 'bar' }, res, 'T', words), /^Bar chart\. T\. 2 bars\. Highest: Feb, A, /);
});

test('chart sentence: "Other" is not the highest, a scatter has no extremes', () => {
  const res = {
    columns: [col('p', 'dimension'), col('v', 'measure', 'number')],
    rows: [['a', 10], ['b', 5], ['\u0000other', 99]],
    other: true,
    ignored_filters: [],
    ms: 0,
  };
  assert.match(chartSummary({ type: 'bar' }, res, '', words), /Highest: a, 0\.01 rb\. Lowest: b, 0\.005 rb\.$/);
  const sc = { columns: [col('a', 'dimension'), col('x', 'measure', 'number'), col('y', 'measure', 'number')], rows: [['p', 1, 2], ['q', 3, 4]], ignored_filters: [], ms: 0 };
  assert.equal(chartSummary({ type: 'scatter' }, sc, 'X vs Y', words), 'Scatter chart. X vs Y. 2 dots.');
});

test('csv: BOM, CRLF, dot decimals, ISO dates, Other, quotes and formula guard', () => {
  const t = {
    heads: ['Date', 'Category', 'Amount'],
    rows: [['2026-10-01', 'Coffee, hot', 1234.5], ['2026-10-02', '=SUM(A1)', -3], ['2026-10-03', '\u0000other', null]],
    total: ['Total', null, 1231.5],
    numeric: [false, false, true],
  };
  const csv = toCsv(t);
  assert.ok(csv.startsWith('﻿Date,Category,Amount\r\n'));
  const lines = csv.slice(1).split('\r\n');
  assert.equal(lines[1], '2026-10-01,"Coffee, hot",1234.5');
  assert.equal(lines[2], "2026-10-02,'=SUM(A1),-3");
  assert.equal(lines[3], '2026-10-03,Other,');
  assert.equal(lines[4], 'Total,,1231.5');
  assert.equal(csvCell('say "hi"', true), '"say ""hi"""');
  assert.equal(csvCell(-5, false), '-5');
});

test('table: a total row only for a table with totals, blank for a value shown another way', () => {
  const res = {
    columns: [col('cat', 'dimension'), col('a', 'measure', 'number'), col('b', 'measure', 'number')],
    rows: [['x', 1, 2], ['y', 3, 4]],
    totals: { grand: [4, 6] },
    ignored_filters: [],
    ms: 0,
  };
  const heads = ['Category', 'A', 'B'];
  const on = tileTable({ type: 'table', totals: true, showAs: [null, 'pct_total'] }, res, heads);
  assert.deepEqual(on.total, ['Total', 4, null]);
  assert.equal(tileTable({ type: 'table' }, res, heads).total, null);
  assert.equal(tileTable({ type: 'bar', totals: true }, res, heads).total, null);
  // The earlier series of a comparison is one more column.
  const cmp = tileTable({ type: 'line' }, { ...res, compare: { label: 'Last year', rows: [['x', 7], ['y', 8]] } }, heads);
  assert.deepEqual(cmp.heads, ['Category', 'A', 'B', 'Last year']);
  assert.deepEqual(cmp.rows[1], ['y', 3, 4, 8]);
});

test('table: a number tile is its history', () => {
  const res = { columns: [], rows: [], kpi: { bars: [{ label: 'Sep', value: 5 }, { label: 'Oct', value: null }] }, ignored_filters: [], ms: 0 };
  assert.deepEqual(kpiRows({ measures: [{ column: 'amount' }] }, res).rows, [['Sep', 5], ['Oct', null]]);
  const t = tileTable({ type: 'kpi', measures: [{}] }, res, ['Period', 'Amount']);
  assert.deepEqual(t.heads, ['Period', 'Amount']);
  assert.deepEqual(t.rows, [['Sep', 5], ['Oct', null]]);
});

test('file names lose the characters a file system refuses', () => {
  assert.equal(fileName('Sales: Q3/Q4 <draft>', 'png'), 'Sales_ Q3_Q4 _draft_.png');
  assert.equal(fileName('', 'csv', 'rows'), 'rows.csv');
});
