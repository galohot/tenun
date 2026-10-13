// The workspace calendar (v5 build 04): the names of fiscal years and quarters, and nothing else changes.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fiscalName, formatDate, getCalendar, setCalendar, weekStartSet } from '../src/lib/locale.ts';

test('a year that starts in April names its years by the year they end', () => {
  setCalendar({ yearStart: 4 });
  assert.equal(formatDate('2026-04-01', 'en', 'year'), 'FY 2027');
  assert.equal(formatDate('2025-04-01', 'id', 'year'), 'FY 2026');
  assert.equal(formatDate('2026-04-01', 'en', 'quarter'), 'Q1 FY 2027');
  assert.equal(formatDate('2027-01-01', 'en', 'quarter'), 'Q4 FY 2027');
  assert.equal(formatDate('2026-10-01', 'en', 'quarter'), 'Q3 FY 2027');
  // Months and days keep their names.
  assert.equal(formatDate('2026-04-01', 'en', 'month'), formatDate('2026-04-01', 'en', 'month'));
  assert.equal(fiscalName(new Date(Date.UTC(2026, 3, 1)), 'month'), null);
  assert.equal(weekStartSet(), null, 'no week start was set');
  setCalendar({ yearStart: 1, weekStart: 7 });
  assert.equal(formatDate('2026-01-01', 'en', 'year'), '2026');
  assert.equal(formatDate('2026-04-01', 'en', 'quarter'), 'Q2 2026');
  assert.equal(weekStartSet(), 7);
  setCalendar({ yearStart: 13, weekStart: 3 });
  assert.deepEqual(getCalendar(), { yearStart: 1, weekStart: 1 }, 'out of range: the default');
  setCalendar(null);
});
