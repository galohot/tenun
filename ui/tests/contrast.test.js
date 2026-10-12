// Every text colour against every ground it can land on, in both themes.
// Floors: 4.5:1 for words, 3:1 for marks (chart series, focus ring, icons).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { GROUND_TOKEN, INKS as CARD_INKS } from '../src/lib/card.ts';

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

// The heat map ramp (build 12): one hue in seven steps. Numbers are printed on the cells, in white
// or in dark ink, whichever reads better; both themes need 4.5:1 on every step. Light mode runs
// light to dark, dark mode runs dark to bright, so the biggest value always stands out.
const INKS = ['#ffffff', '#1c1c1e'];
for (const [name, theme] of [['light', light], ['dark', dark]]) {
  const steps = [1, 2, 3, 4, 5, 6, 7].map((i) => theme[`ramp-${i}`]);
  test(`${name}: ramp steps exist`, () => {
    assert.ok(steps.every(Boolean), `missing ramp step in ${name}: ${steps}`);
  });
  test(`${name}: ink on every ramp step clears 4.5:1`, () => {
    for (const bg of steps) {
      const best = Math.max(...INKS.map((ink) => ratio(ink, bg)));
      assert.ok(best >= 4.5, `ramp ${bg} reaches only ${best.toFixed(2)}:1`);
    }
  });
  test(`${name}: ramp runs one way and every step is visibly different`, () => {
    const l = steps.map(lum);
    for (let i = 1; i < l.length; i++) {
      const d = name === 'light' ? l[i - 1] - l[i] : l[i] - l[i - 1];
      assert.ok(d > 0, `step ${i + 1} ${steps[i]} does not go the right way`);
      assert.ok(ratio(steps[i - 1], steps[i]) >= 1.15, `steps ${i} and ${i + 1} are too alike (${ratio(steps[i - 1], steps[i]).toFixed(2)})`);
    }
  });
}

// Number cards (build 08): every part on every ground, in both themes. Words 4.5:1; the trend's latest period is a mark,
// 3:1. The other periods and the divider carry no meaning of their own. A chip (the target, a rule) has its own ground.
for (const [name, theme] of [['light', light], ['dark', dark]]) {
  test(`${name}: every card part clears its floor on every ground`, () => {
    for (const [ground, token] of Object.entries(GROUND_TOKEN)) {
      const bg = theme[token];
      assert.ok(bg, `no ${token} in ${name}`);
      const ink = CARD_INKS[ground];
      for (const role of ['text', 'muted', 'good', 'bad', 'warn']) {
        const r = ratio(theme[ink[role]], bg);
        assert.ok(r >= 4.5, `${ground}: ${role} (${ink[role]} ${theme[ink[role]]}) on ${bg} is ${r.toFixed(2)}:1`);
      }
      const m = ratio(theme[ink.mark], bg);
      assert.ok(m >= 3, `${ground}: the trend's mark (${ink.mark}) on ${bg} is ${m.toFixed(2)}:1`);
    }
  });
}
