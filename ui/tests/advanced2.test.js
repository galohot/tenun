// The arithmetic and layouts under the second set of Advanced charts (build 13): bullet, compare,
// Sankey, calendar, cohort, Gantt, bubble size, trend lines and the expandable pivot.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { at, bandsOf, bulletDomain, bulletTicks, shareOfTarget } from '../src/lib/charts/bullet.ts';
import { calendarModel, rowOf, weekdayOfRow } from '../src/lib/charts/calendar.ts';
import { cohortGrid, cohortRange, periodWord } from '../src/lib/charts/cohort.ts';
import { sparkKey, sparkPath } from '../src/lib/charts/spark.ts';
import { bubbleSamples, bubbleSize } from '../src/lib/charts/bubble.ts';
import { flowModel } from '../src/lib/charts/flow.ts';
import { isGood, moveOf, moveWords, relative } from '../src/lib/charts/pairs.ts';
import { barOf, dayMs, dayStart, spanOf, todayIn } from '../src/lib/charts/timeline.ts';
import { groupKeys, isExpandable, pivotModel, pivotTree, visibleNodes } from '../src/lib/pivot.ts';
import { tileQuery } from '../src/lib/tiles.ts';
import { normalizeSnapshot } from '../src/lib/api.ts';

const T = 'orders';
const col = (column, grain) => ({ table: T, column, ...(grain ? { grain } : {}) });
const m = (column, agg = 'sum') => ({ table: T, column, agg });

test('bullet: share of target, scale and bands', () => {
  assert.equal(shareOfTarget(82, 100), 0.82);
  assert.equal(shareOfTarget(150, 100), 1.5);
  assert.equal(shareOfTarget(5, 0), null);
  assert.equal(shareOfTarget(null, 100), null);
  // One scale for all bars, from zero to a round number above the biggest value or target.
  assert.deepEqual(bulletDomain([82, 120, 90]), { lo: 0, hi: 200 });
  assert.deepEqual(bulletDomain([-30, 40]), { lo: -50, hi: 50 });
  assert.deepEqual(bulletDomain([]), { lo: 0, hi: 1 });
  const d = { lo: 0, hi: 200 };
  assert.equal(at(100, d), 0.5);
  assert.equal(at(500, d), 1);
  assert.deepEqual(bulletTicks(d, 4), [0, 50, 100, 150, 200]);
  // Up to three bands, lowest first; a band with no range is left out.
  const lines = [
    { kind: 'band', value: 70, to: 100 },
    { kind: 'band', value: 0, to: 40, label: 'Poor' },
    { kind: 'band', value: 40, to: 70 },
    { kind: 'band', value: 100, to: 120 },
    { kind: 'band', value: 5, to: 5 },
    { kind: 'target', value: 50 },
  ];
  const bands = bandsOf(lines);
  assert.equal(bands.length, 3);
  assert.deepEqual(bands.map((b) => [b.from, b.to]), [[0, 40], [40, 70], [70, 100]]);
  assert.equal(bands[0].label, 'Poor');
});

test('compare: which way a pair moved and what that means', () => {
  assert.equal(moveOf(100, 130), 'up');
  assert.equal(moveOf(100, 60), 'down');
  assert.equal(moveOf(5, 5), 'flat');
  assert.equal(moveOf(null, 5), 'unknown');
  assert.equal(isGood('up'), true);
  assert.equal(isGood('down'), false);
  assert.equal(isGood('down', 'down'), true);
  assert.equal(isGood('flat'), null);
  assert.equal(isGood('unknown'), null);
  assert.equal(moveWords('up').glyph, '▲');
  assert.equal(moveWords('down').word, 'fell');
  assert.equal(relative(100, 130), 0.3);
  assert.equal(relative(0, 5), null);
  assert.equal(relative(-50, -25), 0.5);
});

test('sankey: nodes, links, shares and colour slots in a fixed order', () => {
  const rows = [
    [0, 'Web', 'Coffee', 230],
    [0, 'Web', 'Tea', 60],
    [0, 'Shop', 'Coffee', 130],
    [0, 'Shop', 'Tea', 55],
    [1, 'Coffee', 'paid', 330],
    [1, 'Coffee', 'refunded', 30],
    [1, 'Tea', 'paid', 110],
    [1, 'Tea', 'refunded', 5],
  ];
  const f = flowModel(rows, (_s, v) => String(v));
  assert.equal(f.stages, 3);
  assert.equal(f.nodes.length, 6);
  const node = (stage, name) => f.nodes.find((n) => n.stage === stage && n.raw === name);
  assert.equal(node(0, 'Web').value, 290);
  assert.equal(node(1, 'Coffee').value, 360);
  assert.equal(node(2, 'paid').value, 440);
  // Shares of a stage add up to 1.
  for (const s of [0, 1, 2]) assert.ok(Math.abs(f.nodes.filter((n) => n.stage === s).reduce((t, n) => t + n.share, 0) - 1) < 1e-9);
  // Biggest first inside a stage.
  assert.deepEqual(f.nodes.filter((n) => n.stage === 0).map((n) => n.raw), ['Web', 'Shop']);
  // A link is a share of what starts in stage 0 (475 in all).
  assert.ok(Math.abs(f.links[0].share - 230 / 475) < 1e-9);
  // Colour slots go A to Z over the whole chart, so they are all different here and do not follow the sizes.
  const slots = f.nodes.map((n) => n.slot);
  assert.equal(new Set(slots).size, 6);
  assert.ok(node(0, 'Shop').slot < node(0, 'Web').slot);
  // Another stage may use the same name; the nodes stay apart.
  const same = flowModel([[0, 'a', 'a', 1]], (_s, v) => String(v));
  assert.equal(same.nodes.length, 2);
  assert.notEqual(same.nodes[0].id, same.nodes[1].id);
});

test('sankey: "Other" is grey, comes last and has no colour slot', () => {
  const OTHER = '\u0000other';
  const f = flowModel([[0, 'big', 'x', 10], [0, OTHER, 'x', 99]], (_s, v) => (v === OTHER ? 'Other' : String(v)));
  const first = f.nodes.filter((n) => n.stage === 0);
  assert.equal(first.at(-1).raw, OTHER);
  assert.equal(first.at(-1).slot, null);
  assert.notEqual(first[0].slot, null);
});

test('calendar: weeks are columns; Monday first or Sunday first', () => {
  // 2026-03-02 is a Monday.
  assert.equal(rowOf(Date.UTC(2026, 2, 2), true), 0);
  assert.equal(rowOf(Date.UTC(2026, 2, 2), false), 1);
  assert.equal(rowOf(Date.UTC(2026, 2, 8), true), 6); // Sunday
  assert.equal(rowOf(Date.UTC(2026, 2, 8), false), 0);
  assert.equal(weekdayOfRow(0, true), 1);
  assert.equal(weekdayOfRow(0, false), 7);
  assert.equal(weekdayOfRow(3, false), 3);

  const rows = [['2026-03-02', 5], ['2026-03-09', 7], ['2026-04-01', null]];
  const mon = calendarModel(rows, true);
  // March and April are whole: 61 days.
  assert.equal(mon.cells.length, 61);
  assert.equal(mon.months.length, 2);
  const first = mon.cells[0]; // Sunday 1 March; Monday-first puts it on row 6 of week 0.
  assert.deepEqual([first.key, first.week, first.row], ['2026-03-01', 0, 6]);
  const mar2 = mon.cells.find((c) => c.key === '2026-03-02');
  assert.deepEqual([mar2.week, mar2.row, mar2.value], [1, 0, 5]);
  assert.equal(mon.cells.find((c) => c.key === '2026-03-03').value, null);
  const sun = calendarModel(rows, false);
  assert.deepEqual([sun.cells[0].week, sun.cells[0].row], [0, 0]);
  // Every day of a week sits in its own row.
  assert.equal(new Set(mon.cells.filter((c) => c.week === 2).map((c) => c.row)).size, 7);
  // The month outline starts at the first day and ends at the last.
  const mar = mon.months[0];
  assert.deepEqual(mar.outline[0], [0, 6]);
  assert.equal(mar.month, 2);
  assert.deepEqual(calendarModel([], true), { weeks: 0, cells: [], months: [] });
});

test('cohort: a grid of cohorts and periods, empty where a cohort has not got to yet', () => {
  const rows = [
    ['2026-01-01', 0, 1, 3], ['2026-01-01', 1, 1 / 3, 3], ['2026-01-01', 2, 2 / 3, 3],
    ['2026-02-01', 0, 1, 2], ['2026-02-01', 1, 0.5, 2],
    ['2026-03-01', 0, 1, 1],
  ];
  const g = cohortGrid(rows);
  assert.equal(g.columns, 3);
  assert.deepEqual(g.rows.map((r) => r.key), ['2026-01-01', '2026-02-01', '2026-03-01']);
  assert.deepEqual(g.rows[2].cells, [1, null, null]);
  assert.equal(g.rows[0].size, 3);
  // The colours come from the later columns: the first one is always 100 %.
  assert.deepEqual(cohortRange(g), { lo: 0, hi: 2 / 3 });
  assert.deepEqual(cohortRange(cohortGrid([['2026-01-01', 0, 1, 1]])), { lo: 0, hi: 1 });
  assert.equal(cohortRange(cohortGrid([])), null);
  assert.equal(periodWord('week'), 'Week');
  assert.equal(periodWord('month'), 'Month');
  assert.equal(periodWord(null), 'Month');
});

test('gantt: dates become bars on a time axis', () => {
  assert.equal(dayMs('2026-03-01'), Date.UTC(2026, 2, 1));
  assert.equal(dayMs('2026-03-01T09:30:00'), Date.UTC(2026, 2, 1, 9, 30));
  assert.equal(dayMs('nope'), null);
  assert.equal(dayMs(null), null);
  // A date with no time ends at the end of its day: a task on one day is one day long.
  const one = barOf('2026-03-20', '2026-03-20');
  assert.equal(one.days, 1);
  assert.equal(one.end - one.start, 86_400_000);
  const long = barOf('2026-01-15', '2026-03-01');
  assert.equal(long.days, 46);
  // An end before the start has no bar.
  assert.equal(barOf('2026-03-10', '2026-03-01'), null);
  assert.equal(barOf(null, '2026-03-01'), null);
  const span = spanOf([long, null, one]);
  assert.equal(span.min, Date.UTC(2026, 0, 15));
  assert.equal(span.max, Date.UTC(2026, 2, 21));
  assert.equal(spanOf([null]), null);
  assert.equal(todayIn(span, Date.UTC(2026, 1, 1)), true);
  assert.equal(todayIn(span, Date.UTC(2027, 1, 1)), false);
  assert.equal(todayIn(null, 0), false);
  assert.equal(dayStart(Date.UTC(2026, 2, 5, 17, 3)), Date.UTC(2026, 2, 5));
});

test('bubble: the area follows the value', () => {
  const max = 1000;
  // Four times the value is twice as wide.
  assert.ok(Math.abs(bubbleSize(1000, max) / bubbleSize(250, max) - 2) < 1e-9);
  assert.equal(bubbleSize(null, max), 5);
  assert.equal(bubbleSize(-4, max), 5);
  assert.equal(bubbleSize(0, max), 5);
  assert.equal(bubbleSize(5000, max), bubbleSize(1000, max));
  // Never below the smallest dot.
  assert.equal(bubbleSize(1, max), 5);
  assert.deepEqual(bubbleSamples(8432), [8400, 4200, 840]);
  assert.deepEqual(bubbleSamples(0), []);
  assert.equal(new Set(bubbleSamples(1)).size, bubbleSamples(1).length);
});

test('sparkline: own scale, a dot on the last point, gaps break the line', () => {
  const s = sparkPath([1, 2, 3, 4]);
  assert.match(s.d, /^M[\d.]+ [\d.]+L/);
  // The lowest value is at the bottom and the highest at the top (smaller y is higher).
  assert.ok(s.last.y < 6);
  assert.equal(s.last.x, 77);
  // Equal values: a flat line in the middle.
  const flat = sparkPath([5, 5, 5]);
  assert.equal(flat.last.y, 10);
  // A gap lifts the pen.
  const gap = sparkPath([1, null, 3]);
  assert.equal((gap.d.match(/M/g) ?? []).length, 2);
  assert.equal(gap.d.includes('L'), false);
  // Nothing to draw.
  assert.deepEqual(sparkPath([null, null]), { d: '', last: null });
  // The key matches the engine's.
  assert.equal(sparkKey(['A']), 'A');
  assert.equal(sparkKey(['A', 2]), 'A\u001f2');
  assert.equal(sparkKey([null]), '\u0000null');
});

test('pivot: group subtotals come from shorter entries and make a tree', () => {
  const res = {
    columns: [
      { name: 'channel', role: 'dimension', dtype: 'text' },
      { name: 'category', role: 'dimension', dtype: 'text' },
      { name: 'status', role: 'dimension', dtype: 'text' },
      { name: 'amount', role: 'measure', dtype: 'number' },
    ],
    rows: [
      ['Web', 'Coffee', 'paid', 230],
      ['Web', 'Tea', 'paid', 60],
      ['Shop', 'Coffee', 'paid', 130],
      ['Shop', 'Tea', 'refunded', 5],
    ],
    totals: {
      grand: [425],
      // Leaf rows have 3 entries and a number; a group has 1 entry and a number.
      rows: [['Web', 'Coffee', 230], ['Web', 'Tea', 60], ['Shop', 'Coffee', 130], ['Shop', 'Tea', 5], ['Web', 290], ['Shop', 135]],
      cols: [['paid', 420], ['refunded', 5]],
      groups: [['Web', 'paid', 290], ['Shop', 'paid', 130], ['Shop', 'refunded', 5]],
    },
  };
  const model = pivotModel(res, { cols: { table: T, column: 'status' } });
  assert.equal(model.nRows, 2);
  assert.equal(isExpandable(model), true);
  // The leaf totals are not mixed up with the group totals.
  assert.deepEqual(model.rows.find((r) => r.raw[0] === 'Web' && r.raw[1] === 'Coffee').total, [230]);
  assert.deepEqual(model.groups.get('Web').total, [290]);
  assert.deepEqual(model.groups.get('Shop').cells.map((c) => c?.[0] ?? null), [130, 5]);
  assert.deepEqual(model.groups.get('Web').cells.map((c) => c?.[0] ?? null), [290, null]);
  const tree = pivotTree(model);
  assert.deepEqual(tree.map((n) => n.group.raw[0]), ['Web', 'Shop']);
  assert.equal(tree[0].children.length, 2);
  assert.equal(visibleNodes(tree, new Set()).length, 2 + 4);
  // A closed group hides its rows and keeps its own line.
  assert.equal(visibleNodes(tree, new Set(['Web'])).length, 2 + 2);
  assert.deepEqual(groupKeys(tree), ['Web', 'Shop']);
  // One row field cannot open and close.
  const flat = pivotModel({ ...res, columns: res.columns.slice(1), rows: res.rows.map((r) => r.slice(1)), totals: { grand: [425] } }, { cols: { table: T, column: 'status' } });
  assert.equal(isExpandable(flat), false);
});

test('pivot without a column field: a group is one cell', () => {
  const res = {
    columns: [
      { name: 'a', role: 'dimension', dtype: 'text' },
      { name: 'b', role: 'dimension', dtype: 'text' },
      { name: 'v', role: 'measure', dtype: 'number' },
    ],
    rows: [['x', 'p', 1], ['x', 'q', 2], ['y', 'p', 4]],
    totals: { grand: [7], rows: [['x', 3], ['y', 4]] },
  };
  const model = pivotModel(res, {});
  assert.equal(isExpandable(model), true);
  assert.deepEqual(model.groups.get('x').cells, [[3]]);
  assert.equal(model.rows.length, 3);
});

test('tile questions: the new types ask what the engine asks', () => {
  const q = (tile) => tileQuery({ id: 't', title: '', x: 0, y: 0, w: 1, h: 1, measures: [], dims: [], ...tile }, []);
  // Bullet: the value and its target, one group.
  const bullet = q({ type: 'bullet', measures: [m('a'), m('b'), m('c')], dims: [col('r'), col('s')] });
  assert.equal(bullet.measures.length, 2);
  assert.equal(bullet.dimensions.length, 1);
  // Compare with two values: the group alone. With one value: the group and the field of the periods.
  assert.equal(q({ type: 'compare', measures: [m('a'), m('b')], dims: [col('r'), col('y')] }).dimensions.length, 1);
  const periods = q({ type: 'compare', measures: [m('a')], dims: [col('r'), col('y', 'year')] });
  assert.equal(periods.dimensions.length, 2);
  assert.equal(periods.measures.length, 1);
  assert.deepEqual(periods.sort, []);
  // Sankey: up to three stages, one value.
  const sk = q({ type: 'sankey', measures: [m('a'), m('b')], dims: [col('a'), col('b'), col('c'), col('d')] });
  assert.equal(sk.dimensions.length, 3);
  assert.equal(sk.measures.length, 1);
  // Calendar: always by day, latest days first, the last 400 days.
  const cal = q({ type: 'calendar', measures: [m('a')], dims: [col('d', 'month')] });
  assert.equal(cal.dimensions[0].grain, 'day');
  assert.deepEqual(cal.sort, [{ by: 0, desc: true }]);
  assert.equal(cal.limit, 400);
  // Gantt: start and end, earliest start first, at most 100 rows.
  const g = q({ type: 'gantt', measures: [m('s', 'min'), m('e', 'max'), m('x')], dims: [col('t'), col('u')] });
  assert.equal(g.measures.length, 2);
  assert.equal(g.dimensions.length, 1);
  assert.deepEqual(g.sort, [{ by: 1, desc: false }]);
  assert.equal(g.limit, 100);
  // Cohort: the entity, a value to add up, the date.
  const c = q({ type: 'cohort', measures: [m('cust', 'count_distinct'), m('amt')], dims: [col('d', 'month'), col('z')] });
  assert.equal(c.measures.length, 2);
  assert.equal(c.dimensions.length, 1);
  // Scatter reads a third value for the size, and a scatter with two still asks two.
  assert.equal(q({ type: 'scatter', measures: [m('a'), m('b'), m('c')], dims: [col('r')] }).measures.length, 3);
  assert.equal(q({ type: 'scatter', measures: [m('a'), m('b')], dims: [col('r')] }).measures.length, 2);
});

test('a ratio with no per.column is read as a count of rows', () => {
  const tiles = [{ measures: [{ table: T, column: 'amount', agg: 'sum', per: { table: T, agg: 'count' } }] }];
  const snap = { workspace: { dashboards: [{ tiles }] } };
  normalizeSnapshot(snap);
  assert.equal(tiles[0].measures[0].per.column, null);
});
