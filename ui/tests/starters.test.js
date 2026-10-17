// Starter dashboards (0.7 build 04): every word of a starter reads in Indonesian, the role menus offer the right
// columns, and a required role without a column stops the dashboard.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { setLang } from '../src/lib/i18n.ts';
import { columnsFor, missingRole, starterWord, starterWords } from '../src/lib/starters.ts';

const dir = new URL('../../crates/tenun-core/starters/', import.meta.url);
const starters = readdirSync(dir)
  .filter((f) => f.endsWith('.json'))
  .map((f) => JSON.parse(readFileSync(new URL(f, dir), 'utf8')));
const id = JSON.parse(readFileSync(new URL('../src/lib/i18n/id.json', import.meta.url), 'utf8'));

test('every word of every starter is listed and has Indonesian', () => {
  assert.equal(starters.length, 5);
  const known = new Set(starterWords());
  for (const s of starters) {
    for (const w of [s.name, s.description, ...s.roles.flatMap((r) => [r.label, r.description])]) {
      assert.ok(known.has(w), `${s.id}: "${w}" is not in starters.ts`);
      assert.ok(id[w], `${s.id}: "${w}" has no Indonesian`);
    }
  }
  setLang('id');
  assert.equal(starterWord('Budget against actual'), 'Anggaran dan realisasi');
  assert.equal(starterWord('Something new'), 'Something new');
  setLang('en');
  assert.equal(starterWord('Sales'), 'Sales');
});

test('a role offers the columns that can play it', () => {
  const cols = [
    { name: 'when', dtype: 'date' },
    { name: 'at', dtype: 'datetime' },
    { name: 'amount', dtype: 'number' },
    { name: 'city', dtype: 'text' },
    { name: 'secret', dtype: 'text', hidden: true },
  ];
  const names = (kind) => columnsFor({ kind }, cols).map((c) => c.name);
  assert.deepEqual(names('date'), ['when', 'at']);
  assert.deepEqual(names('number'), ['amount']);
  assert.deepEqual(names('text'), ['when', 'at', 'amount', 'city']);
});

test('the first required role without a column stops the dashboard', () => {
  const roles = starters.find((s) => s.id === 'sales').roles;
  assert.equal(missingRole(roles, { date: 'd', product: 'p' })?.name, 'amount');
  assert.equal(missingRole(roles, { date: 'd', product: 'p', amount: 'a' }), null);
  assert.equal(missingRole(roles, { date: 'd', product: 'p', amount: '' })?.name, 'amount');
});
