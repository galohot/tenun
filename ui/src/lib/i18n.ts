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

/**
 * The text in the chosen language, with `{name}` filled from `vars`. A missing translation reads in English.
 * One English word with two meanings gets a context after `@@`, which English never shows: `t('Change@@noun')`.
 */
export function t(text: string, vars?: Record<string, string | number>): string {
  const s = TABLES[lang][text] || plain(text);
  return vars ? fill(s, vars) : s;
}

/** One of two texts by count: `tn('{n} table', '{n} tables', n)`. Indonesian has one form; its table keys the plural. */
export function tn(one: string, many: string, n: number, vars?: Record<string, string | number>): string {
  return t(n === 1 && lang === 'en' ? one : many, { n, ...vars });
}

/**
 * Marks a text that is translated later, where it is shown: a label in a module-level list is made once, before
 * the language is known, so the list holds `tk('Light')` and the screen shows `t(item.label)`. The key test reads
 * `tk()` keys as it reads `t()` keys.
 */
export const tk = (text: string): string => text;

const plain = (text: string) => {
  const i = text.indexOf('@@');
  return i < 0 ? text : text.slice(0, i);
};

function fill(s: string, vars: Record<string, string | number>): string {
  return s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

/**
 * A translated text cut at its `{name}` places, for a sentence with markup inside it:
 * `{#each parts(t('Press {key} to search')) as p}{#if p.slot === 'key'}<kbd>K</kbd>{:else}{p.text}{/if}{/each}`.
 * The whole sentence stays one key, so a language can move the marked part.
 */
export function parts(s: string): { text: string; slot?: string }[] {
  return s.split(/(\{\w+\})/).filter(Boolean).map((x) => (/^\{\w+\}$/.test(x) ? { text: x, slot: x.slice(1, -1) } : { text: x }));
}
