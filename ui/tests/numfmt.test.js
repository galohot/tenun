// Number formats (formats §2): every row of the shared fixture, which the engine runs too (numtext.rs).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { writeNumber, codeProblem, currencyParts, CURRENCY_CODES } from '../src/lib/numfmt.ts';
import { shortNumber, longNumber, tipNumber } from '../src/lib/locale.ts';

const { rows } = JSON.parse(readFileSync(new URL('./fixtures/numfmt.json', import.meta.url), 'utf8'));

test('every shared row: short, full and tone', () => {
  const bad = [];
  for (const r of rows) {
    const s = writeNumber(r.n, r.loc, r.f, r.digits, true);
    const l = writeNumber(r.n, r.loc, r.f, r.digits, false);
    const tone = r.tone ?? null;
    if (s.text !== r.short || l.text !== r.full || l.tone !== tone)
      bad.push(`${r.n} ${r.loc} ${r.digits ?? ''} ${JSON.stringify(r.f)}: short ${JSON.stringify(s.text)} (want ${JSON.stringify(r.short)}), full ${JSON.stringify(l.text)} (want ${JSON.stringify(r.full)}), tone ${l.tone} (want ${tone})`);
  }
  assert.deepEqual(bad, []);
});

test('the old names write the same as before for a format without new fields', () => {
  assert.equal(shortNumber(1234567, 'id', { currency: 'IDR' }), 'Rp 1,23 jt');
  assert.equal(longNumber(1234.5, 'en', { currency: 'USD' }), '$1,234.5');
  assert.equal(tipNumber(1234567, 'id', { currency: 'IDR' }), 'Rp 1.234.567 (1,23 jt)');
  assert.equal(tipNumber(1234567, 'id', { scale: 6 }), '1,2 jt');
});

test('a code that cannot be read says why', () => {
  assert.equal(codeProblem('#,##0.00'), null);
  assert.equal(codeProblem('[>=1000]0.0,"K";0'), null);
  assert.match(codeProblem('"abc'), /quote/);
  assert.match(codeProblem('[Blink]0'), /does not know/);
  assert.match(codeProblem('0;0;0;0;0'), /4 parts/);
  assert.match(codeProblem('0.0.0'), /decimal point/);
});

test('every ISO currency has a place', () => {
  assert.ok(CURRENCY_CODES.length > 150);
  for (const c of CURRENCY_CODES) {
    const [pre, post] = currencyParts(c, 'en');
    assert.ok(pre || post, c);
  }
});

// ---- column formats (formats §2.3) ----------------------------------------------------------------------------
import { setColumnFormats, columnFormatsOf, formatOf, ownFormat, withNamedFormats } from '../src/lib/measures.ts';
import { datePattern, patternProblem } from '../src/lib/locale.ts';

test('a sum, an average or a maximum of a column takes its format; a count and a ratio do not', () => {
  setColumnFormats(columnFormatsOf({ t1: { columns: { amount: { format: { currency: 'IDR' } }, day: { date_pattern: 'd MMM yyyy' } } } }));
  const m = (agg, extra = {}) => ({ table: 't1', column: 'amount', agg, ...extra });
  assert.deepEqual(formatOf({ measures: [m('sum')] }, 0), { currency: 'IDR' });
  assert.deepEqual(formatOf({ measures: [m('max')] }, 0), { currency: 'IDR' });
  assert.equal(formatOf({ measures: [m('count')] }, 0), null);
  assert.equal(formatOf({ measures: [m('sum', { per: { table: 't1', column: 'amount', agg: 'sum' } })] }, 0), null);
  // The tile's own format goes over the column's, field by field.
  assert.deepEqual(formatOf({ measures: [m('sum')], formats: [{ decimals: 0 }] }, 0), { currency: 'IDR', decimals: 0 });
  assert.deepEqual(withNamedFormats({ measures: [m('sum')] }).formats, [{ currency: 'IDR' }]);
  setColumnFormats(null);
  assert.equal(formatOf({ measures: [m('sum')] }, 0), null);
});

test('a tile turns off what its column or measure turns on, and only stores what differs', () => {
  const base = { currency: 'IDR', scale: 6, negative: 'parens', suffix: 'pcs' };
  assert.deepEqual(ownFormat(base, null, { currency: undefined }), { currency: '' });
  assert.deepEqual(ownFormat(base, null, { scale: undefined, compact: true }), { scale: 0, compact: true });
  assert.deepEqual(ownFormat(base, null, { negative: 'minus' }), { negative: 'minus' });
  assert.deepEqual(ownFormat(base, { suffix: 'kg' }, { suffix: 'pcs' }), null);
  assert.equal(ownFormat(null, null, { decimals: undefined }), null);
  // A neutral value writes as no setting: no currency, no fixed unit.
  assert.equal(writeNumber(1234567, 'en', { ...base, ...ownFormat(base, null, { currency: undefined, scale: undefined }) }, null, false).text, '1,234,567 pcs');
});

test('date patterns, in both languages', () => {
  assert.equal(datePattern('2026-10-15', 'EEE, d MMM yyyy', 'id'), 'Kam, 15 Okt 2026');
  assert.equal(datePattern('2026-10-15', 'EEEE d MMMM yyyy', 'en'), 'Thursday 15 October 2026');
  assert.equal(datePattern('2026-10-15', 'dd/MM/yy', 'en'), '15/10/26');
  assert.equal(datePattern('2026-10-15T14:05:09', 'yyyy-MM-dd HH:mm:ss', 'en'), '2026-10-15 14:05:09');
  assert.equal(datePattern('2026-10-15T14:05:00', "h:mm a 'on' d MMM", 'en'), '2:05 PM on 15 Oct');
  assert.equal(datePattern('not a date', 'd MMM yyyy', 'en'), 'not a date');
  assert.match(patternProblem("d 'MMM"), /quote/);
  assert.match(patternProblem("'just text'"), /no part/);
  assert.equal(patternProblem('d MMM yyyy'), null);
});
