// Story pages (build 11): the order, the rows, the heights, the cuts of a printed page, and a page that switches
// between grid and story keeps its tiles.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CHART_PX, flowHeight, flowOrder, flowRows, FLOW_TEXT, FLOW_WIDE, insertInOrder, isFlow, moveInOrder, newStory, NUMBER_PX, pageCuts, rowWidth } from '../src/lib/flow.ts';

const tile = (id, type, x, y, flow) => ({ id, type, title: '', x, y, w: 4, h: 3, measures: [], dims: [], ...(flow ? { flow } : {}) });
const ids = (list) => list.map((t) => t.id);

test('a story runs in its own order, then in reading order for the tiles it does not name', () => {
  const tiles = [tile('c', 'bar', 0, 6), tile('a', 'text', 4, 0), tile('b', 'kpi', 0, 0), tile('d', 'line', 8, 0)];
  // Reading order: top to bottom, then left to right.
  assert.deepEqual(ids(flowOrder({ tiles })), ['b', 'a', 'd', 'c']);
  // The story's order first; a tile it names that is gone is passed by; new tiles follow in reading order.
  assert.deepEqual(ids(flowOrder({ tiles, order: ['c', 'gone', 'a'] })), ['c', 'a', 'b', 'd']);
  assert.deepEqual(moveInOrder(flowOrder({ tiles }), 3, 0), ['c', 'b', 'a', 'd']);
  assert.deepEqual(moveInOrder(flowOrder({ tiles }), 0, 9), ['b', 'a', 'd', 'c']);
  assert.deepEqual(insertInOrder(flowOrder({ tiles }), 2, 'new'), ['b', 'a', 'new', 'd', 'c']);
  assert.deepEqual(insertInOrder(flowOrder({ tiles }), 99, 'new'), ['b', 'a', 'd', 'c', 'new']);
});

test('two half tiles share a row on a wide page; an odd one keeps its width; a phone stacks them', () => {
  const half = { width: 'half' };
  const t = [tile('h1', 'kpi', 0, 0, half), tile('h2', 'kpi', 0, 1, half), tile('f', 'line', 0, 2), tile('h3', 'kpi', 0, 3, half), tile('t', 'text', 0, 4), tile('h4', 'bar', 0, 5, half), tile('h5', 'bar', 0, 6, half), tile('h6', 'bar', 0, 7, half)];
  assert.deepEqual(flowRows(t, true).map(ids), [['h1', 'h2'], ['f'], ['h3'], ['t'], ['h4', 'h5'], ['h6']]);
  assert.deepEqual(flowRows(t, false).map(ids), t.map((x) => [x.id]));
  // Text keeps to the reading width; anything else may go wide.
  assert.equal(rowWidth([t[4]]), FLOW_TEXT);
  assert.equal(rowWidth([t[0], t[1]]), FLOW_WIDE);
});

test('a text is as tall as its words; a chart and a number take one of three heights', () => {
  assert.equal(flowHeight(tile('t', 'text', 0, 0)), null);
  assert.equal(flowHeight(tile('c', 'line', 0, 0)), CHART_PX.m);
  assert.equal(flowHeight(tile('c', 'line', 0, 0, { height: 'l' })), 460);
  assert.equal(flowHeight(tile('k', 'kpi', 0, 0)), NUMBER_PX.s);
  assert.equal(flowHeight(tile('k', 'kpi', 0, 0, { width: 'half', height: 'm' })), 200);
  assert.deepEqual([CHART_PX.s, CHART_PX.m, CHART_PX.l], [240, 340, 460]);
});

test('a printed story is cut between tiles, never inside a chart; only a text taller than a page is cut', () => {
  // A page is 1000 px. Tiles: a text 0–300, a chart 316–776, a chart 792–1252, a text 1268–2600, a chart 2616–2956.
  const boxes = [
    { top: 0, bottom: 300, split: true },
    { top: 316, bottom: 776, split: false },
    { top: 792, bottom: 1252, split: false },
    { top: 1268, bottom: 2600, split: true },
    { top: 2616, bottom: 2956, split: false },
  ];
  // Page 1 ends above the second chart (792). Page 2 would end at 1792, inside the long text: that is allowed.
  // Page 3 would end at 2792, inside the last chart: it ends above it (2616), which then starts page 4.
  assert.deepEqual(pageCuts(boxes, 2956, 1000), [
    [0, 792],
    [792, 1792],
    [1792, 2616],
    [2616, 2956],
  ]);
  // A page short enough for everything is one page.
  assert.deepEqual(pageCuts(boxes, 2956, 4000), [[0, 2956]]);
  // A chart taller than a page cannot fit anywhere: it is cut.
  assert.deepEqual(pageCuts([{ top: 0, bottom: 1500, split: false }], 1500, 1000), [
    [0, 1000],
    [1000, 1500],
  ]);
});

test('switching between grid and story keeps every tile where it was; a new story starts with a title and a chart', () => {
  const d = { id: 'd1', title: 'Sales', refresh: 60, tiles: [tile('a', 'bar', 0, 0), tile('b', 'kpi', 4, 0)] };
  const story = { ...d, layout: 'flow' };
  assert.equal(isFlow(story), true);
  assert.equal(isFlow(d), false);
  const back = { ...story, layout: 'grid' };
  assert.deepEqual(back.tiles, d.tiles);
  let n = 0;
  const s = newStory('d9', 2, (p) => `${p}${++n}`);
  assert.equal(s.layout, 'flow');
  assert.equal(s.title, 'Story 3');
  assert.deepEqual(s.tiles.map((t) => [t.id, t.type]), [['t1', 'text'], ['t2', 'bar']]);
  assert.match(s.tiles[0].text, /^# /);
});
