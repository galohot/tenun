// Pie and donut (build 07, V4-D7): the order of the slices, the labels, the centre of a donut, the colours across
// filters, and the words that refuse a value that cannot be cut into slices.
import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import * as echarts from 'echarts';
import { centre, DONUT_HOLE, hiddenLabels, INSIDE_AT, inkOn, labelInside, pieOption, shareText, shortName, slicesOf } from '../src/lib/charts/pieparts.ts';
import { assignSlots } from '../src/lib/charts/math.ts';
import { setKnownMeasures, tileProblem } from '../src/lib/measures.ts';

const OTHER = '\u0000other';
const colors = { surface: '#fff', raised: '#fff', text: '#111', text2: '#666', series: ['s1', 's2', 's3', 's4', 's5', 's6', 's7', 's8'] };
const tile = (type, extra = {}) => ({ id: `t-${type}`, type, title: '', x: 0, y: 0, w: 4, h: 4, measures: [{ table: 's1/sales', column: 'amount', agg: 'sum' }], dims: [{ table: 's1/sales', column: 'category' }], ...extra });
const option = (rows, extra = {}) => {
  const slices = slicesOf(rows, (v) => (v === OTHER ? 'Other' : String(v)), (keys) => keys.map((_, i) => i));
  return { slices, option: pieOption({ slices, colors, donut: false, listed: false, picked: null, format: String, measure: 'Revenue', hovered: null, loc: 'en', esc: String, ...extra }) };
};

beforeEach(() => setKnownMeasures([], {}));

test('slices are the largest first, Other last and grey, and nothing below or at zero', () => {
  const s = slicesOf([['B', 30], [OTHER, 50], ['A', 70], ['Z', 0], ['C', 10]], String, (keys) => keys.map((_, i) => i));
  assert.deepEqual(s.map((x) => x.label), ['A', 'B', 'C', 'Other']);
  assert.equal(s[3].slot, null);
  assert.equal(s[3].other, true);
  assert.ok(Math.abs(s.reduce((t, x) => t + x.share, 0) - 1) < 1e-12);
  assert.equal(s[0].share, 70 / 160);
});

test('a slice keeps its colour when a filter takes another away', () => {
  const memory = new Map();
  const slots = (keys) => assignSlots(memory, keys, 8);
  const before = slicesOf([['A', 3], ['B', 2], ['C', 1]], String, slots);
  const after = slicesOf([['A', 3], ['C', 1]], String, slots);
  const slotOf = (list, name) => list.find((x) => x.label === name).slot;
  assert.equal(slotOf(after, 'A'), slotOf(before, 'A'));
  assert.equal(slotOf(after, 'C'), slotOf(before, 'C'));
  assert.notEqual(slotOf(before, 'A'), slotOf(before, 'C'));
});

test('labels: the share on the slice when it is big enough, outside on a leader line when small, one decimal under 10 %', () => {
  assert.equal(INSIDE_AT, 0.15);
  assert.equal(labelInside(0.15), true);
  assert.equal(labelInside(0.149), false);
  assert.equal(shareText(0.074), '7.4%');
  assert.equal(shareText(0.23), '23%');
  assert.equal(shareText(0.074, 'id'), '7,4%');
  const { option: o } = option([['A', 95], ['B', 5]]);
  const [a, small] = o.series[0].data;
  assert.equal(a.label.position, 'inside');
  assert.equal(a.label.formatter, 'A\n95%');
  assert.equal(small.label.position, 'outside');
  assert.equal(small.label.formatter, 'B\n5.0%');
  assert.equal(small.labelLine.show, true);
  // From twelve o'clock, clockwise, a 1 px separator in the surface colour, square slices, no rose.
  const s = o.series[0];
  assert.equal(s.startAngle, 90);
  assert.equal(s.clockwise, true);
  assert.deepEqual(s.itemStyle, { borderColor: '#fff', borderWidth: 1, borderRadius: 0 });
  assert.equal(s.roseType, undefined);
  assert.equal(s.radius[0], '0%');
  assert.equal(o.graphic, undefined);
  // On a phone the labels are a list under the chart, not on it.
  const { option: listed } = option([['A', 95], ['B', 5]], { listed: true });
  assert.equal(listed.series[0].data[0].label.show, false);
  // A pick dims the other slices.
  const { option: picked } = option([['A', 95], ['B', 5]], { picked: new Set(['B']) });
  assert.deepEqual(picked.series[0].data.map((d) => d.itemStyle.opacity), [0.35, 1]);
});

test('every label is drawn: a large Other beside a long name keeps its own, and a long name is cut', () => {
  // The case that lost a label: Other at 30 % next to the largest slice, two small slices outside.
  const { option: o } = option([['Jakarta Selatan', 32], [OTHER, 30], ['Denpasar', 15], ['Surabaya', 12], ['Bandung', 11]]);
  const chart = echarts.init(null, null, { renderer: 'svg', ssr: true, width: 384, height: 390 });
  chart.setOption(o);
  const drawn = [...chart.renderToSVGString().matchAll(/<text[^>]*>([^<]*)<\/text>/g)].map((m) => m[1]);
  assert.deepEqual(hiddenLabels(chart), []);
  chart.dispose();
  for (const word of ['Jakarta Selat…', '32%', 'Other', '30%', 'Denpasar', '15%', 'Surabaya', '12%', 'Bandung', '11%']) assert.ok(drawn.includes(word), `${word} in ${drawn.join(' | ')}`);
  assert.equal(shortName('Nusa Tenggara Barat'), 'Nusa Tenggara…');
  assert.equal(shortName('Nusa Tenggara'), 'Nusa Tenggara');
  // Counted in characters: an emoji is one.
  assert.equal(shortName('🙂'.repeat(20)), `${'🙂'.repeat(13)}…`);
});

test('a label on a slice is white or near-black, whichever reads better on the slice', () => {
  // The grey of Other: dark in the light theme, light in the dark theme.
  assert.equal(inkOn('#69696e'), '#fff');
  assert.equal(inkOn('#a8a8ad'), '#111');
  // Yellow, orange and green of the palette take the dark ink; the blue and the purple keep white.
  assert.equal(inkOn('#eda100'), '#111');
  assert.equal(inkOn('#eb6834'), '#111');
  assert.equal(inkOn('#1baf7a'), '#111');
  assert.equal(inkOn('#2a78d6'), '#fff');
  assert.equal(inkOn('#4a3aa7'), '#fff');
  assert.equal(inkOn('#fff'), '#111');
  assert.equal(inkOn('rgb(0, 0, 0)'), '#fff');
  // A colour it cannot read keeps white.
  assert.equal(inkOn('var(--s1)'), '#fff');
  const { option: o } = option([['A', 60], ['B', 40]], { colors: { ...colors, series: ['#eda100', '#4a3aa7'] } });
  assert.deepEqual(o.series[0].data.map((d) => d.label.color), ['#111', '#fff']);
});

test('the total of a donut fits its hole: written short, and smaller when its text is long', () => {
  const rows = [['A', 200000000], ['B', 146904000]];
  // 390 px: the radius is 52 % of half of it, the hole 62 % of that: 390 × 0.52 × 0.62 = 125.7 px across.
  // "346,904,000" is 11 characters: 125.7 × 0.85 / (11 × 0.6) = 16 px. Written short, "347M" keeps 22 px.
  const long = option(rows, { donut: true, size: 390, format: (v) => v.toLocaleString('en-US') }).option.graphic[0].children[0].style;
  assert.deepEqual([long.text, long.fontSize], ['346,904,000', 16]);
  const short = option(rows, { donut: true, size: 390, format: (v) => v.toLocaleString('en-US'), short: (v) => `${Math.round(v / 1e6)}M` }).option.graphic[0].children[0].style;
  assert.deepEqual([short.text, short.fontSize], ['347M', 22]);
});

test('a label ECharts hides is found, so the list under the chart can name it', () => {
  // Six slices of about a sixth, with long names, on a small tile: the labels inside cannot all fit.
  const rows = ['Kepulauan Bangka Belitung', 'Nusa Tenggara Barat', 'Kalimantan Timur', 'Sulawesi Tenggara', 'Papua Barat Daya', 'Yogyakarta'].map((n, i) => [n, 17 - i]);
  const { option: o } = option(rows, { size: 220 });
  const chart = echarts.init(null, null, { renderer: 'svg', ssr: true, width: 220, height: 220 });
  chart.setOption(o);
  const hidden = hiddenLabels(chart);
  const drawn = chart.renderToSVGString();
  chart.dispose();
  assert.ok(hidden.length > 0);
  // The rows are largest first, so slice i is row i; a hidden label's name is not on the chart.
  for (const i of hidden) assert.ok(!drawn.includes(`>${shortName(rows[i][0])}<`), rows[i][0]);
  // Not an ECharts instance: nothing is hidden, and nothing throws.
  assert.deepEqual(hiddenLabels({}), []);
  assert.deepEqual(hiddenLabels(null), []);
});

test('a donut has a hole of 62 % and its centre says the total, or the slice under the pointer', () => {
  const { slices, option: o } = option([['A', 60], ['B', 40]], { donut: true });
  const [inner, outer] = o.series[0].radius.map((r) => parseFloat(r));
  assert.ok(Math.abs(inner / outer - DONUT_HOLE) < 0.02, `${inner} / ${outer}`);
  assert.deepEqual(o.graphic[0].children.map((c) => c.style.text), ['100', 'Revenue']);
  // A donut's labels sit outside, on leader lines.
  assert.ok(o.series[0].data.every((d) => d.label.position === 'outside' && d.labelLine.show));
  const fmt = (v) => `Rp ${v}`;
  assert.deepEqual(centre(slices, null, fmt, 'Revenue'), { value: 'Rp 100', caption: 'Revenue' });
  assert.deepEqual(centre(slices, 1, fmt, 'Revenue'), { value: '40%', caption: 'B' });
  const { option: hovered } = option([['A', 60], ['B', 40]], { donut: true, hovered: 0 });
  assert.deepEqual(hovered.graphic[0].children.map((c) => c.style.text), ['60%', 'A']);
});

test('a value that is not a part of a whole is refused in words that offer a bar chart', () => {
  const avg = tile('pie', { measures: [{ table: 's1/sales', column: 'amount', agg: 'avg' }] });
  assert.equal(tileProblem(avg), 'A pie needs parts that add up to a whole. Averages do not. Try a bar chart.');
  const ratio = tile('donut', { measures: [{ table: 's1/sales', column: 'amount', agg: 'sum', per: { table: 's1/sales', column: null, agg: 'count' } }] });
  assert.equal(tileProblem(ratio), 'A donut needs parts that add up to a whole. Ratios do not. Try a bar chart.');
  setKnownMeasures([{ id: 'm1', name: 'Average order size', formula: 'AVERAGE([amount])' }], { m1: { additive: false } });
  const named = tile('pie', { measures: [{ named: 'm1', table: '', column: null, agg: 'sum' }] });
  assert.equal(tileProblem(named), 'A pie needs parts that add up to a whole. Average order size does not. Try a bar chart.');
  assert.equal(tileProblem(tile('pie'), undefined, false, [['A', 10], ['B', -2]]), 'A pie needs parts that add up to a whole. Some values are below 0. Try a bar chart.');
  assert.equal(tileProblem(tile('pie'), undefined, false, [['A', 10], ['B', 2]]), null);
});

test('the centre of a donut fits the hole: smaller on a phone', () => {
  const sizeOf = (size) => option([['A', 60], ['B', 40]], { donut: true, size }).option.graphic[0].children[0].style.fontSize;
  assert.equal(sizeOf(400), 22);
  assert.equal(sizeOf(260), 17);
  // 100 px: the hole is 100 × 0.52 × 0.62 = 32 px across, so the text takes the floor, 11 px.
  assert.equal(sizeOf(100), 11);
});
