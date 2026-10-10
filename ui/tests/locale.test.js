// Numbers and dates in Indonesian and English (V3-D9).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shortNumber, longNumber, tipNumber, formatDate, partName, periodName, dayMonth, guessLocale } from '../src/lib/locale.ts';

const short = (n, loc, f) => shortNumber(n, loc, f);

test('short numbers, English: unchanged from v1', () => {
  assert.equal(short(0, 'en'), '0');
  assert.equal(short(1284, 'en'), '1,284');
  assert.equal(short(9999, 'en'), '9,999');
  assert.equal(short(12900, 'en'), '12.9K');
  assert.equal(short(4_200_000, 'en'), '4.2M');
  assert.equal(short(1_300_000_000, 'en'), '1.3B');
  assert.equal(short(2_500_000_000_000, 'en'), '2.5T');
  assert.equal(short(12.5, 'en'), '12.5');
  assert.equal(short(-12900, 'en'), '−12.9K');
  assert.equal(short(NaN, 'en'), '–');
});

test('short numbers, Indonesian: rb, jt, M, T and a decimal comma', () => {
  assert.equal(short(1284, 'id'), '1.284');
  assert.equal(short(9999, 'id'), '9.999');
  assert.equal(short(12900, 'id'), '12,9 rb');
  assert.equal(short(1_200_000, 'id'), '1,2 jt');
  assert.equal(short(1_284_000, 'id'), '1,28 jt');
  assert.equal(short(1_200_000_000, 'id'), '1,2 M');
  assert.equal(short(2_500_000_000_000, 'id'), '2,5 T');
  assert.equal(short(12.5, 'id'), '12,5');
  assert.equal(short(-1_200_000, 'id'), '−1,2 jt');
  assert.equal(short(999_999, 'id'), '1 jt');
});

test('full numbers in both locales', () => {
  assert.equal(longNumber(1234567, 'en'), '1,234,567');
  assert.equal(longNumber(1234567, 'id'), '1.234.567');
  assert.equal(longNumber(1234.5, 'en'), '1,234.5');
  assert.equal(longNumber(1234.5, 'id'), '1.234,5');
  assert.equal(longNumber(0.256, 'id'), '0,26');
  assert.equal(longNumber(0.256, 'en'), '0.26');
  assert.equal(longNumber(-1234, 'en'), '−1,234');
});

test('tooltips show the full number, and in Indonesian its unit word', () => {
  assert.equal(tipNumber(1234567, 'id'), '1.234.567 (1,23 jt)');
  assert.equal(tipNumber(1234567, 'en'), '1,234,567');
  assert.equal(tipNumber(1234, 'id'), '1.234');
  assert.equal(tipNumber(1234567, 'id', { currency: 'IDR' }), 'Rp 1.234.567 (1,23 jt)');
});

test('a format picks the currency, the decimals and the form', () => {
  assert.equal(short(1_200_000, 'id', { currency: 'IDR' }), 'Rp 1,2 jt');
  assert.equal(short(1_200_000_000, 'id', { currency: 'IDR' }), 'Rp 1,2 M');
  assert.equal(short(1_200_000, 'en', { currency: 'USD' }), '$1.2M');
  assert.equal(short(1_200_000, 'id', { currency: 'MZN' }), '1,2 jt MT');
  assert.equal(short(1_200_000, 'en', { currency: 'MZN' }), 'MT 1.2M');
  assert.equal(short(-1_200_000, 'en', { currency: 'USD' }), '−$1.2M');
  assert.equal(short(1_200_000, 'en', { currency: 'EUR' }), '€1.2M');
  assert.equal(short(1_200_000, 'en', { currency: 'SGD' }), 'S$1.2M');
  assert.equal(short(1_234_567, 'id', { decimals: 1 }), '1,2 jt');
  assert.equal(short(1_234_567, 'en', { decimals: 3 }), '1.235M');
  assert.equal(short(1_234_567, 'en', { decimals: 0 }), '1M');
  assert.equal(short(999_999, 'en', { decimals: 0 }), '1M');
  assert.equal(short(1_234_567, 'en', { compact: false }), '1,234,567');
  assert.equal(short(1_234_567, 'id', { compact: false, currency: 'IDR' }), 'Rp 1.234.567');
  assert.equal(longNumber(1_234_567, 'en', { compact: true }), '1.23M');
  assert.equal(longNumber(1234.5, 'id', { decimals: 2 }), '1.234,50');
  assert.equal(longNumber(1234.5, 'en', { decimals: 0 }), '1,235');
});

test('percent multiplies by 100', () => {
  assert.equal(short(0.1234, 'en', { percent: true }), '12.3%');
  assert.equal(short(0.1234, 'id', { percent: true }), '12,3%');
  assert.equal(short(0.5, 'en', { percent: true }), '50%');
  assert.equal(short(0.1234, 'id', { percent: true, decimals: 2 }), '12,34%');
});

test('dates, English: unchanged from v1', () => {
  assert.equal(formatDate('2026-10-14', 'en', 'year'), '2026');
  assert.equal(formatDate('2026-10-14', 'en', 'quarter'), 'Q4 2026');
  assert.equal(formatDate('2026-10-01', 'en', 'month'), 'Oct 2026');
  assert.equal(formatDate('2026-09-07', 'en', 'week'), 'Week of Sep 7, 2026');
  assert.equal(formatDate('2026-10-14', 'en', 'day'), 'Oct 14, 2026');
  assert.equal(formatDate('2026-10-14', 'en', null, 'date'), 'Oct 14, 2026');
  assert.equal(formatDate('2026-10-14T09:30:00', 'en', null, 'datetime'), 'Oct 14, 2026 09:30');
  assert.equal(formatDate('not a date', 'en', 'day'), 'not a date');
});

test('dates, Indonesian: day first, short month names', () => {
  assert.equal(formatDate('2026-10-14', 'id', 'year'), '2026');
  assert.equal(formatDate('2026-10-14', 'id', 'quarter'), 'Q4 2026');
  assert.equal(formatDate('2026-10-01', 'id', 'month'), 'Okt 2026');
  assert.equal(formatDate('2026-08-01', 'id', 'month'), 'Agu 2026');
  assert.equal(formatDate('2026-05-01', 'id', 'month'), 'Mei 2026');
  assert.equal(formatDate('2026-09-07', 'id', 'week'), 'Minggu 7 Sep 2026');
  assert.equal(formatDate('2026-10-14', 'id', 'day'), '14 Okt 2026');
  assert.equal(formatDate('2026-12-03T09:30:00', 'id', null, 'datetime'), '3 Des 2026 09:30');
});

test('date parts have names in both locales', () => {
  const months = Array.from({ length: 12 }, (_, i) => partName(i + 1, 'month_of_year', 'id'));
  assert.deepEqual(months, ['Jan', 'Feb', 'Mar', 'Apr', 'Mei', 'Jun', 'Jul', 'Agu', 'Sep', 'Okt', 'Nov', 'Des']);
  const days = Array.from({ length: 7 }, (_, i) => partName(i + 1, 'weekday', 'id'));
  assert.deepEqual(days, ['Sen', 'Sel', 'Rab', 'Kam', 'Jum', 'Sab', 'Min']);
  const en = Array.from({ length: 7 }, (_, i) => partName(i + 1, 'weekday', 'en'));
  assert.deepEqual(en, ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']);
  assert.equal(partName(9, 'hour', 'id'), '09:00');
  assert.equal(partName(10, 'month_of_year', 'en'), 'Oct');
});

test('period names match the ones the engine writes', () => {
  const d = (s) => new Date(`${s}T00:00:00Z`);
  assert.equal(periodName(d('2026-09-01'), 'month', 'en'), 'September 2026');
  assert.equal(periodName(d('2026-10-01'), 'month', 'id'), 'Oktober 2026');
  assert.equal(periodName(d('2026-09-07'), 'week', 'id'), 'Minggu 7 Sep 2026');
  assert.equal(periodName(d('2026-09-07'), 'week', 'en'), 'Week of Sep 7, 2026');
  assert.equal(dayMonth(d('2026-10-14'), 'en'), 'Oct 14');
  assert.equal(dayMonth(d('2026-10-14'), 'id'), '14 Okt');
});

test('the locale follows the language tag', () => {
  assert.equal(guessLocale('id-ID'), 'id');
  assert.equal(guessLocale('id'), 'id');
  assert.equal(guessLocale('en-GB'), 'en');
  assert.equal(guessLocale(undefined), 'en');
});
