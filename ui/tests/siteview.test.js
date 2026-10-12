// An exported site (build 13): which page an address shows, the address of a page and of a drill-through (so back
// and forward move between them), and an unknown page going home.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { hashOf, parseHash } from '../src/lib/siteview.ts';

const PAGES = ['d1', 'd2', 'store'];

test('an address names a page or a drill-through; anything else is the home page', () => {
  assert.deepEqual(parseHash('#/p/d2', PAGES, 'd1'), { page: 'd2' });
  assert.deepEqual(parseHash('', PAGES, 'd1'), { page: 'd1' });
  assert.deepEqual(parseHash('#/p/gone', PAGES, 'd1'), { page: 'd1' });
  assert.deepEqual(parseHash('#/x/d2', PAGES, 'd1'), { page: 'd1' });
  const r = { page: 'store', drill: { tile: 'd1/t7', key: 'Senja Menteng / 2' } };
  // A key with a space and a slash in it goes through the address and back.
  assert.equal(hashOf(r), '#/p/store/d/d1%2Ft7/Senja%20Menteng%20%2F%202');
  assert.deepEqual(parseHash(hashOf(r), PAGES, 'd1'), r);
  assert.deepEqual(parseHash(hashOf({ page: 'd2' }), PAGES, 'd1'), { page: 'd2' });
  // History: each step is its own address, so back goes to the one before.
  const steps = [{ page: 'd1' }, r, { page: 'd2' }].map(hashOf);
  assert.equal(new Set(steps).size, 3);
  assert.deepEqual(steps.map((h) => parseHash(h, PAGES, 'd1').page), ['d1', 'store', 'd2']);
});
