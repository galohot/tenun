// Helpers for the folder, paste and CSV link sources (build 09). Pure functions, so the node
// tests can load them.

import type { UrlEvery } from './api';

/** The patterns a new folder source reads. */
export const DEFAULT_PATTERN = '*.csv;*.xlsx';

export const EVERY_CHOICES: { value: UrlEvery; label: string }[] = [
  { value: '15min', label: 'Every 15 minutes' },
  { value: 'hourly', label: 'Every hour' },
  { value: 'daily', label: 'Every day' },
  { value: 'manual', label: 'Only when I press Refresh' },
];

export const everyLabel = (e: UrlEvery | undefined): string =>
  EVERY_CHOICES.find((c) => c.value === (e ?? 'hourly'))?.label ?? 'Every hour';

/**
 * True when clipboard text may be a table. A tab or a line break is enough: the engine reads it
 * and says if it is not a table. One plain line is left to the browser.
 */
export function looksTabular(text: string | null | undefined): boolean {
  const t = (text ?? '').replace(/^﻿/, '').trim();
  if (!t) return false;
  return t.includes('\t') || /\r?\n/.test(t);
}

/** Why a link cannot be used, in words, or an empty string when it can. Local addresses may use http. */
export function linkProblem(url: string): string {
  const t = url.trim();
  if (!t) return '';
  let u: URL;
  try {
    u = new URL(t);
  } catch {
    return 'That does not look like a web link.';
  }
  if (u.protocol === 'https:') return '';
  if (u.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(u.hostname)) return '';
  return 'The link must start with https://.';
}

/** A short name for a table made from a pasted text: "Pasted table", "Pasted table 2". */
export function pastedName(taken: string[]): string {
  const have = new Set(taken.map((n) => n.toLowerCase()));
  if (!have.has('pasted table')) return 'Pasted table';
  for (let n = 2; ; n++) if (!have.has(`pasted table ${n}`)) return `Pasted table ${n}`;
}

/** "3 min ago", "2 h ago", "yesterday": when a link was last fetched. */
export function sinceWords(iso: string | undefined, now = Date.now()): string {
  if (!iso) return '';
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return '';
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return `${Math.round(s / 86400)} d ago`;
}
