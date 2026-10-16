// The words on the screen (0.7 build 02). The English text is the key: `t('Added {n} tables', { n })`.
// `i18n/id.json` maps each English text to Indonesian. tests/i18n.test.js holds every `t()` key to it.
// The app reads in the language this computer chose (Settings); an exported or published page reads in its
// workspace's language, because its readers chose nothing.
// Plain module state, no runes, so the node tests can import every module that calls `t()`. A change of
// language draws the screen again from the top (App.svelte keys its tree on the language).

import id from './i18n/id.json' with { type: 'json' };

export type Lang = 'en' | 'id';

const TABLES: Record<Lang, Record<string, string>> = { en: {}, id: id as Record<string, string> };

let lang: Lang = 'en';

export function setLang(l: Lang) {
  lang = l;
  if (typeof document !== 'undefined') document.documentElement.lang = l;
}

export const getLang = (): Lang => lang;

/** The text in the chosen language, with `{name}` filled from `vars`. A missing translation reads in English. */
export function t(text: string, vars?: Record<string, string | number>): string {
  const s = TABLES[lang][text] || text;
  return vars ? fill(s, vars) : s;
}

/** One of two texts by count: `tn('{n} table', '{n} tables', n)`. Indonesian has one form; its table keys the plural. */
export function tn(one: string, many: string, n: number, vars?: Record<string, string | number>): string {
  return t(n === 1 ? one : many, { n, ...vars });
}

function fill(s: string, vars: Record<string, string | number>): string {
  return s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}
