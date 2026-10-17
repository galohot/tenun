// The words on the screen (0.7 builds 02, 03): every text goes through t(), and every t() text reads in Indonesian.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { keys, loose } from '../../scripts/i18n-scan.mjs';
import { getLang, parts, setLang, t, tn } from '../src/lib/i18n.ts';

const id = JSON.parse(readFileSync(new URL('../src/lib/i18n/id.json', import.meta.url), 'utf8'));
const found = keys();
const used = new Set(found.map((k) => k.key));
const slots = (s) => new Set([...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]));

test('every text in the code has Indonesian', () => {
  const missing = [...new Set(found.filter((k) => !id[k.key]?.trim()).map((k) => `${k.file}: ${k.key}`))];
  assert.deepEqual(missing, []);
});

test('every Indonesian text belongs to a text in the code', () => {
  assert.deepEqual(Object.keys(id).filter((k) => !used.has(k)), []);
});

// Indonesian has no plural, so it may use the one form where the code passes two ("All {names}" also gets {name}).
test('the Indonesian uses only the {places} of the English', () => {
  const ok = (k, x) => slots(k).has(x) || slots(k).has(`${x}s`);
  assert.deepEqual(Object.entries(id).filter(([k, v]) => ![...slots(v)].every((x) => ok(k, x))).map(([k]) => k), []);
});

test('no English text is left in the markup outside t()', () => {
  assert.deepEqual(loose().map((x) => `${x.file}:${x.line}  ${x.text}`), []);
});

test('t() fills the places, tn() picks the form, and an unknown text reads in English', () => {
  const was = getLang();
  try {
    setLang('en');
    assert.equal(t('{a} and {b}', { a: 1, b: 'x' }), '1 and x');
    assert.equal(tn('{n} table', '{n} tables', 1), '1 table');
    assert.equal(tn('{n} table', '{n} tables', 2), '2 tables');
    assert.equal(t('Change@@noun'), 'Change');
    setLang('id');
    const [one, many] = ['{n} table', '{n} tables'];
    if (id[many]) assert.equal(tn(one, many, 1), id[many].replace('{n}', '1'));
    assert.equal(t('A text no table has {x}', { x: 3 }), 'A text no table has 3');
  } finally {
    setLang(was);
  }
});

test('parts() cuts a text at its places', () => {
  assert.deepEqual(parts('Press {key} to search'), [{ text: 'Press ' }, { text: '{key}', slot: 'key' }, { text: ' to search' }]);
});
