// Helpers for the folder, paste and web link sources (builds 09 and 15). Pure functions, so the
// node tests can load them.

import type { KeyPlace, LinkLook, Paging, UrlEvery } from './api';
import { t, tn } from './i18n.ts';

/** The patterns a new folder source reads. */
export const DEFAULT_PATTERN = '*.csv;*.xlsx';

export const EVERY_CHOICES: { value: UrlEvery; label: string }[] = [
  { value: '15min', get label() { return t('Every 15 minutes'); } },
  { value: 'hourly', get label() { return t('Every hour'); } },
  { value: 'daily', get label() { return t('Every day'); } },
  { value: 'manual', get label() { return t('Only when I press Refresh'); } },
];

export const everyLabel = (e: UrlEvery | undefined): string =>
  EVERY_CHOICES.find((c) => c.value === (e ?? 'hourly'))?.label ?? t('Every hour');

/**
 * True when clipboard text may be a table. A tab or a line break is enough: the engine reads it
 * and says if it is not a table. One plain line is left to the browser.
 */
export function looksTabular(text: string | null | undefined): boolean {
  const s = (text ?? '').replace(/^﻿/, '').trim();
  if (!s) return false;
  return s.includes('\t') || /\r?\n/.test(s);
}

/** Why a link cannot be used, in words, or an empty string when it can. Local addresses may use http. */
export function linkProblem(url: string): string {
  const s = url.trim();
  if (!s) return '';
  let u: URL;
  try {
    u = new URL(s);
  } catch {
    return t('That does not look like a web link.');
  }
  if (u.protocol === 'https:') return '';
  if (u.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(u.hostname)) return '';
  return t('The link must start with https://.');
}

/** A short name for a table made from a pasted text: "Pasted table", "Pasted table 2". */
export function pastedName(taken: string[]): string {
  const have = new Set(taken.map((n) => n.toLowerCase()));
  const base = t('Pasted table');
  if (!have.has(base.toLowerCase())) return base;
  for (let n = 2; ; n++) if (!have.has(`${base} ${n}`.toLowerCase())) return `${base} ${n}`;
}

/** "3 min ago", "2 h ago", "yesterday": when a link was last fetched. */
export function sinceWords(iso: string | undefined, now = Date.now()): string {
  if (!iso) return '';
  const at = Date.parse(iso);
  if (!Number.isFinite(at)) return '';
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 60) return t('just now');
  if (s < 3600) return t('{n} min ago', { n: Math.round(s / 60) });
  if (s < 86400) return t('{n} h ago', { n: Math.round(s / 3600) });
  return t('{n} d ago', { n: Math.round(s / 86400) });
}

// --- A key in a link, records and pages (build 15) --------------------------------------------

/** Link parameter names that hold a key, in lower case. */
export const KEY_NAMES = [
  'key',
  'api_key',
  'apikey',
  'api-key',
  'token',
  'access_token',
  'auth',
  'appid',
  'app_key',
  'secret',
  'password',
  'sig',
  'signature',
];

/** The name a key starts with: a header is `Authorization`, a link parameter is `api_key`. */
export const keyStart = (place: KeyPlace): string => (place === 'header' ? 'Authorization' : 'api_key');

const decode = (t: string): string => {
  try {
    return decodeURIComponent(t.replace(/\+/g, ' '));
  } catch {
    return t;
  }
};

/**
 * Takes the first parameter of a link that looks like a key out of the link. Returns the link
 * without it, and the name and value that were taken. A later parameter of the same name goes too,
 * so no second value stays in the link. The rest of the link is left as it was typed.
 */
export function splitKey(raw: string): { url: string; key: { name: string; value: string } | null } {
  const t = raw.trim();
  const hash = t.indexOf('#');
  const tail = hash >= 0 ? t.slice(hash) : '';
  const head = hash >= 0 ? t.slice(0, hash) : t;
  const q = head.indexOf('?');
  if (q < 0) return { url: t, key: null };
  let key: { name: string; value: string } | null = null;
  const keep: string[] = [];
  for (const part of head.slice(q + 1).split('&')) {
    const eq = part.indexOf('=');
    const name = decode(eq < 0 ? part : part.slice(0, eq));
    const value = eq < 0 ? '' : decode(part.slice(eq + 1));
    if (!key && value && KEY_NAMES.includes(name.toLowerCase())) key = { name, value };
    else if (key && name.toLowerCase() === key.name.toLowerCase()) continue;
    else if (part) keep.push(part);
  }
  if (!key) return { url: t, key: null };
  return { url: head.slice(0, q) + (keep.length ? `?${keep.join('&')}` : '') + tail, key };
}

/** The "More pages" choices of a JSON link, as the sheet shows them. */
export type PagesBy = 'none' | 'link' | 'field' | 'cursor' | 'page';

export interface PagesForm {
  by: PagesBy;
  /** `field` and `cursor`: where the next link or the token sits in the answer. */
  path: string;
  /** `cursor` and `page`: the link parameter. */
  param: string;
}

export const NO_PAGES: PagesForm = { by: 'none', path: '', param: '' };

export const PAGES_CHOICES: { value: PagesBy; label: string }[] = [
  { value: 'none', get label() { return t('One page'); } },
  { value: 'link', get label() { return t('Follow the Link header'); } },
  { value: 'field', get label() { return t('A field with the next link'); } },
  { value: 'cursor', get label() { return t('A cursor field'); } },
  { value: 'page', get label() { return t('A page number'); } },
];

/** The link parameter a cursor field usually goes back in. */
export function cursorParam(path: string): string {
  const last = path.split('.').pop() ?? '';
  return /page_?token/i.test(last) ? 'page_token' : 'cursor';
}

/** The choice that the engine's `pages_hint` points at. */
export function pagesFromHint(hint: string | undefined): PagesForm {
  const h = hint ?? '';
  if (h === 'link') return { by: 'link', path: '', param: '' };
  if (h === 'page') return { by: 'page', path: '', param: 'page' };
  if (h.startsWith('field:')) return { by: 'field', path: h.slice(6), param: '' };
  if (h.startsWith('cursor:')) {
    const path = h.slice(7);
    return { by: 'cursor', path, param: cursorParam(path) };
  }
  return { ...NO_PAGES };
}

/** What the sheet sends for a choice, or null for one page. A choice that lacks a field is one page. */
export function pagesToApi(f: PagesForm): Paging | null {
  const path = f.path.trim();
  const param = f.param.trim();
  switch (f.by) {
    case 'link':
      return { by: 'link' };
    case 'field':
      return path ? { by: 'field', path } : null;
    case 'cursor':
      return path && param ? { by: 'cursor', path, param } : null;
    case 'page':
      return param ? { by: 'page', param } : null;
    default:
      return null;
  }
}

/** The choice for a saved setting. */
export function pagesFromApi(p: Paging | undefined): PagesForm {
  if (!p) return { ...NO_PAGES };
  return { by: p.by, path: 'path' in p ? p.path : '', param: 'param' in p ? p.param : '' };
}

/** "data.items — 120 rows", or "The whole answer — 120 rows". */
export function listLabel(l: { path: string; rows: number }, locale = 'en'): string {
  const where = l.path === '.' ? t('The whole answer') : l.path;
  if (l.rows < 0) return where;
  return tn('{where} — {n} row', '{where} — {n} rows', l.rows, { where, n: l.rows.toLocaleString(locale) });
}

/** The lists to offer in the Records choice: the ones found, and the chosen one if it is not among them (rows -1). */
export function recordChoices(look: Pick<LinkLook, 'lists'> | null, chosen: string): { path: string; rows: number }[] {
  const lists = look?.lists ?? [];
  if (chosen && !lists.some((l) => l.path === chosen)) return [{ path: chosen, rows: -1 }, ...lists];
  return lists;
}
