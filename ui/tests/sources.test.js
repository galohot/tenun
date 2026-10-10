import test from 'node:test';
import assert from 'node:assert/strict';
import { looksTabular, linkProblem, pastedName, sinceWords, everyLabel } from '../src/lib/sources.ts';

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
