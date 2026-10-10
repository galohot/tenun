// Every text colour against every ground it can land on, in both themes.
// Floors: 4.5:1 for words, 3:1 for marks (chart series, focus ring, icons).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('../src/lib/theme.css', import.meta.url), 'utf8');

function block(selector) {
  const i = css.indexOf(selector + ' {');
  const body = css.slice(i, css.indexOf('\n}', i));
  const vars = {};
  for (const m of body.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})/g)) vars[m[1]] = m[2];
  return vars;
}

const light = block(':root');
const dark = { ...light, ...block(":root[data-theme='dark']") };

function lum(hex) {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const [r, g, b] = c.map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
const ratio = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};

const words = [
  ['text', ['surface', 'window', 'raised', 'inset', 'hover', 'selected']],
  ['text-2', ['surface', 'window', 'raised', 'inset']],
  ['accent-text', ['surface', 'window', 'selected', 'raised']],
  ['danger', ['surface', 'window']],
  ['success', ['surface', 'window']],
  ['axis-text', ['surface']],
];
// Light-mode aqua (s3), yellow (s4) and pink (s5) sit under 3:1 by the
// palette's design; tiles give them visible labels and tooltips (relief rule).
const marksLight = ['s1', 's2', 's6', 's7', 's8', 'focus'];
const marksDark = ['s1', 's2', 's3', 's4', 's5', 's6', 's7', 's8', 'focus'];

for (const [name, theme] of [['light', light], ['dark', dark]]) {
  test(`${name}: words clear 4.5:1`, () => {
    for (const [fg, grounds] of words) {
      for (const bg of grounds) {
        const r = ratio(theme[fg], theme[bg]);
        assert.ok(r >= 4.5, `${fg} ${theme[fg]} on ${bg} ${theme[bg]} is ${r.toFixed(2)}:1`);
      }
    }
  });
  test(`${name}: white text on the accent clears 4.5:1`, () => {
    const r = ratio('#ffffff', theme.accent);
    assert.ok(r >= 4.5, `white on accent ${theme.accent} is ${r.toFixed(2)}:1`);
  });
  test(`${name}: marks clear 3:1`, () => {
    for (const fg of name === 'light' ? marksLight : marksDark) {
      const r = ratio(theme[fg], theme.surface);
      assert.ok(r >= 3, `${fg} ${theme[fg]} on surface is ${r.toFixed(2)}:1`);
    }
  });
}
