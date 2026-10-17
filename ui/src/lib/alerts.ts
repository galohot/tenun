// Alerts (v5 build 12, V5-D15): the words of a rule and of its state, a new rule's id, and the checks of the alert
// sheet. The app and `tenun agent` check the rules; tenun.cloud never sees them.
import type { AlertRule, AlertState, ChannelKind } from './api';
import { t } from './i18n.ts';

export interface Channel {
  kind: ChannelKind;
  name: string;
  /** A web address, or a Telegram bot token and chat. */
  needs: 'url' | 'telegram';
  /** Where to find what to paste. */
  hint: string;
}

export const CHANNELS: readonly Channel[] = [
  {
    kind: 'slack',
    name: 'Slack',
    needs: 'url',
    get hint() {
      return t('In Slack, add the Incoming Webhooks app to a channel and copy its webhook URL.');
    },
  },
  {
    kind: 'teams',
    name: 'Microsoft Teams',
    needs: 'url',
    get hint() {
      return t('In Teams, open Workflows, choose "Post to a channel when a webhook request is received", and copy its URL.');
    },
  },
  {
    kind: 'discord',
    name: 'Discord',
    needs: 'url',
    get hint() {
      return t('In Discord, open Server Settings › Integrations › Webhooks, make one, and copy its URL.');
    },
  },
  {
    kind: 'telegram',
    name: 'Telegram',
    needs: 'telegram',
    get hint() {
      return t('Make a bot with @BotFather and copy its token. Add the bot to the chat, then type the chat id.');
    },
  },
  {
    kind: 'webhook',
    get name() {
      return t('Another web address');
    },
    needs: 'url',
    get hint() {
      return t('Tenun sends a JSON POST: the value, the threshold, the period and a sentence.');
    },
  },
];

export const channelOf = (kind: ChannelKind): Channel => CHANNELS.find((c) => c.kind === kind) ?? CHANNELS[CHANNELS.length - 1];

/** A random id. Each computer keeps the rule's state and address under it, so two workspaces never share one. */
export function newAlertId(): string {
  const b = new Uint8Array(8);
  crypto.getRandomValues(b);
  return 'a' + [...b].map((x) => x.toString(16).padStart(2, '0')).join('');
}

/** "below 15M" · "above 2K" · "outside 10 to 20", in the tile's number format. */
export function conditionText(rule: Pick<AlertRule, 'op' | 'value' | 'to'>, fmt: (n: number) => string): string {
  if (rule.op === 'outside') return t('outside {from} to {to}', { from: fmt(rule.value), to: fmt(rule.to ?? rule.value) });
  return rule.op === 'below' ? t('below {value}', { value: fmt(rule.value) }) : t('above {value}', { value: fmt(rule.value) });
}

export interface StateLine {
  text: string;
  tone: 'ok' | 'warn' | 'err' | 'muted';
}

/**
 * One line for the state of a rule on this computer: what stops it first, then whether it holds.
 * `time` writes a clock time; `fmt` writes the value.
 */
export function stateText(
  rule: Pick<AlertRule, 'on'>,
  st: AlertState | undefined,
  fmt: (n: number) => string,
  time: (iso: string) => string,
): StateLine {
  if (!rule.on) return { text: t('Off'), tone: 'muted' };
  if (st && !st.hasAddress) return { text: t('No address on this computer: set where to send it.'), tone: 'err' };
  if (st?.problem) return { text: st.problem, tone: 'err' };
  if (st?.error) return { text: t('Last error: {error}', { error: st.error }), tone: 'err' };
  if (!st?.checked) return { text: t('Not checked yet'), tone: 'muted' };
  const now = st.value == null ? t('no value') : fmt(st.value);
  if (st.holding_since) return { text: t('Holding since {time} · now {now}', { time: time(st.holding_since), now }), tone: 'warn' };
  return { text: t('Not holding · {now} at {time}', { now, time: time(st.checked) }), tone: 'ok' };
}

export interface AlertDraft {
  op: AlertRule['op'];
  value: number | null;
  to: number | null;
  kind: ChannelKind;
  url: string;
  token: string;
  chat: string;
  /** This computer already has an address for the rule. */
  hasAddress: boolean;
}

/** What stops the sheet from saving, in words; null when it can save. */
export function alertProblem(d: AlertDraft): string | null {
  if (d.value == null || !Number.isFinite(d.value)) return t('Type a number to compare with.');
  if (d.op === 'outside') {
    if (d.to == null || !Number.isFinite(d.to)) return t('Type the top of the range too.');
    if (d.to < d.value) return t('The range goes from the lower number to the higher one.');
  }
  const typed = channelOf(d.kind).needs === 'url' ? d.url.trim() !== '' : d.token.trim() !== '' || d.chat.trim() !== '';
  if (!typed && !d.hasAddress) return channelOf(d.kind).needs === 'url' ? t('Paste the web address to send to.') : t('Type the bot token and the chat id.');
  if (channelOf(d.kind).needs === 'url' && typed && !/^https?:\/\//i.test(d.url.trim())) return t('A web address starts with https://.');
  if (channelOf(d.kind).needs === 'telegram' && typed && (!d.token.trim() || !d.chat.trim())) return t('Type both the bot token and the chat id.');
  return null;
}
