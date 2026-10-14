// More in the tooltip (v5 build 10): the group key the engine uses, the lines and the small trend.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { tipHtml, tipKey, trendSvg } from '../src/lib/tips.ts';

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;');

test('a group is keyed as the engine keys it', () => {
  assert.equal(tipKey(['A']), 'A');
  assert.equal(tipKey(['A', 2]), 'A\u001f2');
  assert.equal(tipKey([null]), '\u0000null');
});

test('the lines name each value, and the trend is a line with its last point marked', () => {
  const html = tipHtml({ extra: [31, null], trend: [1, 2, null, 4] }, ['Orders', 'Average <ticket>'], [(v) => `${v}`], 'Last 12 weeks', '#123', '#999', esc);
  assert.match(html, /Orders<\/span> <b>31<\/b>/);
  assert.match(html, /Average &lt;ticket>/);
  assert.match(html, /<b>–<\/b>/);
  assert.match(html, /Last 12 weeks/);
  // Two pieces: the gap breaks the line.
  assert.equal((trendSvg([1, 2, null, 4], '#123').match(/M/g) ?? []).length, 2);
  assert.match(trendSvg([1, 2, 3], '#123'), /<circle/);
  assert.equal(trendSvg([5], '#123'), '');
  assert.equal(tipHtml(undefined, [], [], '', '', '', esc), '');
});
