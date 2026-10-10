import test from 'node:test';
import assert from 'node:assert/strict';
import {
  looksTabular,
  linkProblem,
  pastedName,
  sinceWords,
  everyLabel,
  splitKey,
  keyStart,
  pagesFromHint,
  pagesToApi,
  pagesFromApi,
  cursorParam,
  listLabel,
  recordChoices,
} from '../src/lib/sources.ts';

test('tabular text has a tab or more than one line', () => {
  assert.equal(looksTabular('a\tb\n1\t2'), true);
  assert.equal(looksTabular('a,b\n1,2\n'), true);
  assert.equal(looksTabular('a\tb'), true);
  assert.equal(looksTabular('just one line'), false);
  assert.equal(looksTabular('   '), false);
  assert.equal(looksTabular(null), false);
});

test('links must use https, except on this computer', () => {
  assert.equal(linkProblem(''), '');
  assert.equal(linkProblem('https://docs.google.com/spreadsheets/d/e/x/pub?output=csv'), '');
  assert.equal(linkProblem('http://127.0.0.1:8080/a.csv'), '');
  assert.match(linkProblem('http://example.com/a.csv'), /https/);
  assert.match(linkProblem('not a link'), /web link/);
  assert.match(linkProblem('ftp://example.com/a.csv'), /https/);
});

test('pasted tables get a free name', () => {
  assert.equal(pastedName([]), 'Pasted table');
  assert.equal(pastedName(['Pasted table']), 'Pasted table 2');
  assert.equal(pastedName(['pasted table', 'Pasted table 2']), 'Pasted table 3');
});

test('last fetch time in words', () => {
  const now = Date.parse('2026-10-10T12:00:00Z');
  assert.equal(sinceWords('2026-10-10T11:59:40Z', now), 'just now');
  assert.equal(sinceWords('2026-10-10T11:45:00Z', now), '15 min ago');
  assert.equal(sinceWords('2026-10-10T09:00:00Z', now), '3 h ago');
  assert.equal(sinceWords('2026-10-08T12:00:00Z', now), '2 d ago');
  assert.equal(sinceWords(undefined, now), '');
  assert.equal(everyLabel(undefined), 'Every hour');
  assert.equal(everyLabel('15min'), 'Every 15 minutes');
});

test('a key in a pasted link moves out of the link', () => {
  const r = splitKey('https://api.example.com/v1/items?limit=20&api_key=abc123&sort=name');
  assert.equal(r.url, 'https://api.example.com/v1/items?limit=20&sort=name');
  assert.deepEqual(r.key, { name: 'api_key', value: 'abc123' });
  // The name is read in any case, and the value is decoded.
  assert.deepEqual(splitKey('https://h.example/a?Token=a%20b%2Bc').key, { name: 'Token', value: 'a b+c' });
  // The only parameter: the question mark goes too. A fragment stays.
  assert.deepEqual(splitKey('https://h.example/a?appid=k1#top'), {
    url: 'https://h.example/a#top',
    key: { name: 'appid', value: 'k1' },
  });
  // Each name in the list counts.
  for (const n of ['key', 'api_key', 'apikey', 'api-key', 'token', 'access_token', 'auth', 'appid', 'app_key', 'secret', 'password', 'sig', 'signature']) {
    assert.equal(splitKey(`https://h.example/a?x=1&${n.toUpperCase()}=v`).key?.name, n.toUpperCase(), n);
  }
  // No key, an empty key, or a name that only contains one of these words: the link is not touched.
  for (const t of ['https://h.example/a', 'https://h.example/a?page=2', 'https://h.example/a?api_key=', 'https://h.example/a?monkey=1&keyword=2&tokens=3']) {
    assert.deepEqual(splitKey(t), { url: t, key: null }, t);
  }
  // A name written with percent signs is read. A name that comes twice leaves no value in the link.
  assert.deepEqual(splitKey('https://h.example/a?api%5Fkey=abc&x=1'), {
    url: 'https://h.example/a?x=1',
    key: { name: 'api_key', value: 'abc' },
  });
  assert.deepEqual(splitKey('https://h.example/a?key=one&x=1&KEY=two&key=three'), {
    url: 'https://h.example/a?x=1',
    key: { name: 'key', value: 'one' },
  });
  // A value with an equals sign stays whole, and a key in the fragment is not a parameter.
  assert.deepEqual(splitKey('https://h.example/a?token=YWJj==').key, { name: 'token', value: 'YWJj==' });
  assert.deepEqual(splitKey('https://h.example/a#token=abc'), { url: 'https://h.example/a#token=abc', key: null });
  // Spaces around the link are dropped. The first key is moved.
  assert.equal(splitKey('  https://h.example/a?key=1&token=2  ').key?.name, 'key');
  assert.equal(keyStart('header'), 'Authorization');
  assert.equal(keyStart('query'), 'api_key');
});

test('the way to the next pages that the engine hints at is the choice the sheet starts on', () => {
  assert.deepEqual(pagesFromHint('link'), { by: 'link', path: '', param: '' });
  assert.deepEqual(pagesFromHint('field:paging.next'), { by: 'field', path: 'paging.next', param: '' });
  assert.deepEqual(pagesFromHint('cursor:next_cursor'), { by: 'cursor', path: 'next_cursor', param: 'cursor' });
  assert.deepEqual(pagesFromHint('cursor:meta.next_page_token'), { by: 'cursor', path: 'meta.next_page_token', param: 'page_token' });
  assert.deepEqual(pagesFromHint('page'), { by: 'page', path: '', param: 'page' });
  assert.equal(pagesFromHint('').by, 'none');
  assert.equal(pagesFromHint(undefined).by, 'none');
  assert.equal(cursorParam('nextCursor'), 'cursor');
  // What goes to the engine, and back.
  assert.equal(pagesToApi(pagesFromHint('')), null);
  assert.deepEqual(pagesToApi(pagesFromHint('link')), { by: 'link' });
  assert.deepEqual(pagesToApi(pagesFromHint('field:@odata.nextLink')), { by: 'field', path: '@odata.nextLink' });
  assert.deepEqual(pagesToApi(pagesFromHint('cursor:next_cursor')), { by: 'cursor', path: 'next_cursor', param: 'cursor' });
  assert.deepEqual(pagesToApi(pagesFromHint('page')), { by: 'page', param: 'page' });
  // A choice that lacks its field is one page.
  assert.equal(pagesToApi({ by: 'cursor', path: '', param: 'cursor' }), null);
  assert.equal(pagesToApi({ by: 'page', path: '', param: '  ' }), null);
  for (const p of [{ by: 'link' }, { by: 'field', path: 'next' }, { by: 'cursor', path: 'c', param: 'cursor' }, { by: 'page', param: 'page' }]) {
    assert.deepEqual(pagesToApi(pagesFromApi(p)), p);
  }
  assert.equal(pagesFromApi(undefined).by, 'none');
});

test('the lists of records read as words', () => {
  assert.equal(listLabel({ path: 'data.items', rows: 120 }), 'data.items — 120 rows');
  assert.equal(listLabel({ path: '1', rows: 1 }), '1 — 1 row');
  assert.equal(listLabel({ path: '.', rows: 3 }), 'The whole answer — 3 rows');
  assert.equal(listLabel({ path: 'data.items', rows: 12345 }, 'en'), 'data.items — 12,345 rows');
  assert.equal(listLabel({ path: 'x.y', rows: -1 }), 'x.y');
  const look = { lists: [{ path: 'data.items', rows: 3 }] };
  assert.deepEqual(recordChoices(look, 'data.items'), look.lists);
  assert.deepEqual(recordChoices(look, 'old.path'), [{ path: 'old.path', rows: -1 }, ...look.lists]);
  assert.deepEqual(recordChoices(null, ''), []);
});
