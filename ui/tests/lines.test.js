// Reference lines, the trend fit and the look of table cells (build 07).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fitAt, fitWords, placeLevelLabels, placeUprightLabels, shownSeries, statOf, stepWord, trendFit, trendSentence, usableValues } from '../src/lib/charts/lines.ts';
import { barScale, barSpan, barStyle, goodBad } from '../src/lib/cells.ts';

const OTHER = '\u0000other';
const close = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) < eps, `${a} is not ${b}`);

test('trend fit: a straight rise has slope 2 and fits perfectly', () => {
  const fit = trendFit([1, 3, 5, 7, 9]);
  close(fit.slope, 2);
  close(fit.intercept, 1);
  close(fit.r2, 1);
  assert.equal(fit.n, 5);
  close(fitAt(fit, 5), 11);
});

test('trend fit: a hand-worked example', () => {
  // x = 0..4, y = 2, 1, 4, 3, 5: mean 3, Sxx = 10, Sxy = 8, Syy = 10 -> slope 0.8, R² = 64 / 100 = 0.64
  const fit = trendFit([2, 1, 4, 3, 5]);
  close(fit.slope, 0.8);
  close(fit.intercept, 3 - 0.8 * 2);
  close(fit.r2, 0.64);
  assert.equal(fitWords(fit.r2), 'fits loosely');
});

test('trend fit: gaps and the unfinished period are left out', () => {
  // The 5th value is the half month: with it the line bends down, without it the slope is 10.
  const ys = [10, 20, null, 40, 5];
  const withAll = trendFit(ys);
  const fit = trendFit(ys, undefined, new Set([4]));
  assert.equal(fit.n, 3);
  close(fit.slope, 10);
  close(fit.r2, 1);
  assert.ok(withAll.slope < fit.slope);
});

test('trend fit: scatter uses x and y, and too little data gives no line', () => {
  const fit = trendFit([3, 5, 9, 11], [1, 2, 4, 5]);
  close(fit.slope, 2);
  close(fit.intercept, 1);
  assert.equal(trendFit([1, 2]), null);
  assert.equal(trendFit([1, 2, 3], [4, 4, 4]), null);
  // Flat values: nothing to explain, so no clear trend.
  const flat = trendFit([5, 5, 5, 5]);
  assert.equal(flat.slope, 0);
  assert.equal(fitWords(flat.r2), 'no clear trend');
});

test('R² in words: above 0.7, 0.3 to 0.7, below 0.3', () => {
  assert.deepEqual([0.71, 0.7, 0.3, 0.29, 0].map(fitWords), ['fits well', 'fits loosely', 'fits loosely', 'no clear trend', 'no clear trend']);
});

test('hover text says rises or falls, per step, and how well it fits', () => {
  assert.equal(trendSentence({ slope: 1200000, intercept: 0, r2: 0.9, n: 8 }, '1,2 jt', 'per month'), 'Rises about 1,2 jt per month. It fits well.');
  assert.equal(trendSentence({ slope: -3, intercept: 0, r2: 0.5, n: 8 }, '3', 'per week'), 'Falls about 3 per week. It fits loosely.');
  assert.equal(trendSentence({ slope: 1, intercept: 0, r2: 0.1, n: 8 }, '1', 'per day'), 'Rises about 1 per day. There is no clear trend.');
  assert.deepEqual(['month', 'month_of_year', 'weekday', 'hour', 'year', undefined].map(stepWord), ['month', 'month', 'day', 'hour', 'year', 'step']);
});

test('average and median: the Other group never counts', () => {
  const categories = ['a', 'b', 'c', 'd', OTHER];
  const series = [{ values: [10, 20, 60, null, 500] }];
  const shown = usableValues(shownSeries(series, false), categories, (c) => c === OTHER);
  assert.deepEqual(shown, [10, 20, 60]);
  close(statOf('average', shown), 30);
  assert.equal(statOf('median', shown), 20);
  assert.equal(statOf('min', shown), 10);
  assert.equal(statOf('max', shown), 60);
  // With Other counted the average would be 147.5: the test fails if Other leaks in.
  close(statOf('average', usableValues(series[0].values, categories, () => false)), 147.5);
  assert.equal(statOf('average', []), null);
});

test('with a split the statistics read the total of each category', () => {
  const categories = ['jan', 'feb', 'mar'];
  const series = [{ values: [1, 2, null] }, { values: [3, null, 4] }, { values: [null, 6, null] }];
  const totals = shownSeries(series, true);
  assert.deepEqual(totals, [4, 8, 4]);
  assert.equal(statOf('median', usableValues(totals, categories, () => false)), 4);
  // Several values side by side read the first one only.
  assert.deepEqual(shownSeries(series, false), [1, 2, null]);
});

test('level names never sit on each other', () => {
  // Axis 0 to 100 over 200 px: a line is 2 px per unit. Names are 15 px tall.
  assert.deepEqual(placeLevelLabels([50], 0, 100, 200), ['above']);
  // Two lines 5 units (10 px) apart: the higher one goes above, the lower one below.
  assert.deepEqual(placeLevelLabels([50, 55], 0, 100, 200), ['below', 'above']);
  // Three lines close together: the third has no place and its name is left out.
  assert.deepEqual(placeLevelLabels([50, 52, 54], 0, 100, 200), ['hide', 'below', 'above']);
  // A line at the very top has no room above, so its name goes below.
  assert.deepEqual(placeLevelLabels([100], 0, 100, 200), ['below']);
});

test('upright level names keep apart along the top', () => {
  assert.deepEqual(placeUprightLabels([10, 90], 0, 100, 400, () => 80), ['above', 'above']);
  assert.deepEqual(placeUprightLabels([50, 55], 0, 100, 400, () => 80), ['above', 'hide']);
});

test('data bars start at zero and run left for negatives', () => {
  const scale = barScale([-20, 10, 30, null]);
  assert.deepEqual(scale, { min: -20, max: 30 });
  assert.deepEqual(barSpan(30, scale), { from: 40, to: 100 });
  assert.deepEqual(barSpan(-20, scale), { from: 0, to: 40 });
  assert.equal(barSpan(0, scale), null);
  assert.equal(barSpan(null, scale), null);
  // Only positive numbers: the bar starts at the left edge.
  assert.deepEqual(barSpan(5, barScale([5, 10])), { from: 0, to: 50 });
  assert.equal(barSpan(5, barScale([0, 0])), null);
  assert.match(barStyle({ from: 0, to: 50 }), /linear-gradient/);
  assert.equal(barStyle(null), '');
});

test('good and bad come with a glyph, against the threshold', () => {
  const up = { kind: 'goodbad', threshold: 100 };
  assert.deepEqual(goodBad(150, up), { glyph: '▲', tone: 'good', word: 'Above' });
  assert.deepEqual(goodBad(50, up), { glyph: '▼', tone: 'bad', word: 'Below' });
  assert.equal(goodBad(100, up), null);
  assert.equal(goodBad(null, up), null);
  // Lower is better (costs, delays): above the line is bad, and the glyph still points up.
  assert.deepEqual(goodBad(150, { ...up, better: 'down' }), { glyph: '▲', tone: 'bad', word: 'Above' });
  assert.deepEqual(goodBad(50, { ...up, better: 'down' }), { glyph: '▼', tone: 'good', word: 'Below' });
  // A change is read against 0, whatever the threshold says.
  assert.equal(goodBad(0.02, { kind: 'goodbad', threshold: 100 }, 'pct_change').tone, 'good');
  assert.equal(goodBad(-5, { kind: 'goodbad', threshold: -100 }, 'change').tone, 'bad');
  // No threshold: against 0.
  assert.equal(goodBad(-1, { kind: 'goodbad' }).tone, 'bad');
});
