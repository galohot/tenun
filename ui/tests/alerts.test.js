// Alerts (v5 build 12): the words of a rule and of its state, and what stops the alert sheet from saving.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { alertProblem, channelOf, conditionText, newAlertId, stateText } from '../src/lib/alerts.ts';

const fmt = (n) => (n >= 1e6 ? `${n / 1e6}M` : String(n));
const time = (iso) => iso.slice(11, 16);

test('a condition reads in the tile format', () => {
  assert.equal(conditionText({ op: 'below', value: 15e6 }, fmt), 'below 15M');
  assert.equal(conditionText({ op: 'above', value: 2000 }, fmt), 'above 2000');
  assert.equal(conditionText({ op: 'outside', value: 10, to: 20 }, fmt), 'outside 10 to 20');
});

test('the state line says first what stops the alert, then whether it holds', () => {
  const on = { on: true };
  assert.deepEqual(stateText({ on: false }, undefined, fmt, time), { text: 'Off', tone: 'muted' });
  assert.equal(stateText(on, { hasAddress: false, tries: 0, told: false }, fmt, time).tone, 'err');
  assert.equal(stateText(on, { hasAddress: true, problem: 'Its tile was removed.', tries: 0, told: false }, fmt, time).text, 'Its tile was removed.');
  assert.equal(
    stateText(on, { hasAddress: true, error: 'Telegram answered 401.', tries: 1, told: false, checked: '2026-10-14T09:10:00Z' }, fmt, time).text,
    'Last error: Telegram answered 401.',
  );
  assert.deepEqual(
    stateText(on, { hasAddress: true, holding_since: '2026-10-14T09:10:00Z', value: 12e6, tries: 0, told: true, checked: '2026-10-14T10:02:00Z' }, fmt, time),
    { text: 'Holding since 09:10 · now 12M', tone: 'warn' },
  );
  assert.deepEqual(stateText(on, { hasAddress: true, value: 18e6, tries: 0, told: false, checked: '2026-10-14T10:02:00Z' }, fmt, time), {
    text: 'Not holding · 18M at 10:02',
    tone: 'ok',
  });
  assert.equal(stateText(on, { hasAddress: true, tries: 0, told: false }, fmt, time).text, 'Not checked yet');
});

test('the sheet saves only a whole rule', () => {
  const base = { op: 'below', value: 10, to: null, kind: 'slack', url: 'https://hooks.slack.com/services/x', token: '', chat: '', hasAddress: false };
  assert.equal(alertProblem(base), null);
  assert.equal(alertProblem({ ...base, value: null }), 'Type a number to compare with.');
  assert.equal(alertProblem({ ...base, op: 'outside' }), 'Type the top of the range too.');
  assert.equal(alertProblem({ ...base, op: 'outside', to: 5 }), 'The range goes from the lower number to the higher one.');
  assert.equal(alertProblem({ ...base, op: 'outside', to: 20 }), null);
  assert.equal(alertProblem({ ...base, url: '' }), 'Paste the web address to send to.');
  // An address kept on this computer is enough; a new one must be a web address.
  assert.equal(alertProblem({ ...base, url: '', hasAddress: true }), null);
  assert.equal(alertProblem({ ...base, url: 'hooks.slack.com' }), 'A web address starts with https://.');
  const tg = { ...base, kind: 'telegram', url: '' };
  assert.equal(alertProblem(tg), 'Type the bot token and the chat id.');
  assert.equal(alertProblem({ ...tg, token: '1:abc' }), 'Type both the bot token and the chat id.');
  assert.equal(alertProblem({ ...tg, token: '1:abc', chat: '-100' }), null);
  assert.equal(channelOf('teams').name, 'Microsoft Teams');
});

test('a new rule has a random id the engine accepts', () => {
  const a = newAlertId();
  assert.match(a, /^a[0-9a-f]{16}$/);
  assert.notEqual(a, newAlertId());
});
