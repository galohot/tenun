// Scans the UI for the words on its screens (0.7 build 02). Two reports:
// - keys: every literal `t('…')`, `tn('…', '…')` and `tk('…')` in ui/src, so a test holds them to i18n/id.json;
// - loose: text in .svelte markup that does not go through t(): a text node with English words, or a fixed
//   aria-label, title, placeholder or alt. Names that read the same in every language are allowed (ALLOW).
// Run it on its own (`node scripts/i18n-scan.mjs`) for a list of loose texts by file.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
export const SRC = join(ROOT, 'ui', 'src');

/** Words that stay as they are in every language: product and format names, keys, function names. */
export const ALLOW = new Set([
  'Tenun', 'Excel', 'CSV', 'TSV', 'JSON', 'Parquet', 'SQLite', 'DuckDB', 'PostgreSQL', 'MySQL', 'Power', 'BI',
  'Slack', 'Telegram', 'Discord', 'Teams', 'Microsoft', 'Google', 'Sheets', 'GitHub', 'PDF', 'PNG', 'SVG', 'HTML',
  'URL', 'SQL', 'KPI', 'ID', 'OK', 'Esc', 'Enter', 'Tab', 'Shift', 'Ctrl', 'Cmd', 'Alt', 'Del', 'Backspace',
  'SUM', 'TOTAL', 'AVERAGE', 'COUNT', 'MIN', 'MAX', 'IF', 'AND', 'OR', 'NOT', 'FY', 'YTD', 'QTD', 'MTD', 'Rp',
  'USD', 'IDR', 'tenun', 'cloud', 'px', 'kabupaten', 'kota', 'Indonesia', 'English', 'Bahasa', 'ISO', 'UTC',
  'mcp', 'WhatsApp', 'Email', 'Webhook', 'webhook', 'ntfy', 'Mapbox', 'OpenStreetMap', 'Natural', 'Earth',
]);

/** Whole texts that stay as they are: formula code, a format code, a column name the engine writes, commands. */
export const ALLOW_TEXT = new Set([
  'PARAM(" ")', 'PARAM(" ") =', 'SUM([amount]) * (1 + PARAM(" "))', 'source', 'tenun agent', 'tenun agent " "',
  'placeholder="d MMM yyyy"', 'placeholder="https://"',
]);

const files = (dir) =>
  readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? files(p) : /\.(ts|svelte)$/.test(n) ? [p] : [];
  });

/** A JS string literal's value: '…', "…" or a backtick text without `${`. */
function literal(q, body) {
  if (q === '`') {
    if (body.includes('${')) return null;
    return body.replace(/\\`/g, '`').replace(/\\\\/g, '\\');
  }
  const json = q === "'" ? body.replace(/\\'/g, "'").replace(/(^|[^\\])"/g, '$1\\"') : body;
  try {
    return JSON.parse(`"${json}"`);
  } catch {
    return body;
  }
}

const STR = String.raw`(['"\x60])((?:\\.|(?!\1)[^\\])*)\1`;
const ONE = new RegExp(String.raw`(?<![\w$.])(?:t|tk)\(\s*` + STR, 'g');
const TWO = new RegExp(String.raw`(?<![\w$.])tn\(\s*` + STR + String.raw`\s*,\s*` + STR.replace('\\1', '\\3').replace('\\1', '\\3'), 'g');

/**
 * Every literal key in ui/src: `{ key, file }`. For `tn(one, many, n)` only `many` is a key of id.json, because
 * Indonesian has one form for every count.
 */
export function keys() {
  const out = [];
  for (const f of files(SRC)) {
    if (f.endsWith(join('lib', 'i18n.ts'))) continue;
    const s = readFileSync(f, 'utf8');
    for (const m of s.matchAll(ONE)) {
      const k = literal(m[1], m[2]);
      if (k !== null) out.push({ key: k, file: relative(ROOT, f) });
    }
    for (const m of s.matchAll(TWO)) {
      const k = literal(m[3], m[4]);
      if (k !== null) out.push({ key: k, file: relative(ROOT, f) });
    }
  }
  return out;
}

/** The markup of a .svelte file with scripts, styles, comments and every `{…}` taken out (each `{…}` becomes `\0`). */
function markup(src) {
  const s = src
    .replace(/<script[\s\S]*?<\/script>/g, '')
    .replace(/<style[\s\S]*?<\/style>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '');
  let out = '';
  for (let i = 0; i < s.length; i++) {
    if (s[i] !== '{') {
      out += s[i];
      continue;
    }
    let depth = 0;
    for (; i < s.length; i++) {
      const c = s[i];
      if (c === "'" || c === '"' || c === '`') {
        for (i++; i < s.length && s[i] !== c; i++) if (s[i] === '\\') i++;
      } else if (c === '{') depth++;
      else if (c === '}' && --depth === 0) break;
    }
    out += '\0';
  }
  return out;
}

const ATTRS = new Set(['aria-label', 'title', 'placeholder', 'alt', 'label', 'aria-description', 'aria-roledescription', 'text', 'hint', 'empty']);

/** True when a text holds a word that is not in ALLOW. */
function wordy(text) {
  return (text.match(/[A-Za-z][A-Za-z'’-]+/g) ?? []).some((w) => !ALLOW.has(w));
}

/** Texts in .svelte markup that do not go through t(): `{ file, line, text }`. */
export function loose() {
  const out = [];
  for (const f of files(SRC)) {
    if (!f.endsWith('.svelte')) continue;
    const src = readFileSync(f, 'utf8');
    const m = markup(src);
    const from = Math.max(0, src.lastIndexOf('</script>'));
    const lineOf = (text) => {
      const i = src.indexOf(text.trim().split('\0')[0].trim(), from);
      return i < 0 ? 0 : src.slice(0, i).split('\n').length;
    };
    for (const tag of m.matchAll(/<[A-Za-z][^>]*>/g)) {
      for (const a of tag[0].matchAll(/([\w:-]+)="([^"\0]*)"/g)) {
        if (ATTRS.has(a[1]) && wordy(a[2]) && !ALLOW_TEXT.has(`${a[1]}="${a[2]}"`)) out.push({ file: relative(ROOT, f), line: lineOf(a[2]), text: `${a[1]}="${a[2]}"` });
      }
    }
    for (const text of m.replace(/<[^>]*>/g, '\n').split('\n')) {
      const tx = text.replace(/\0/g, ' ').trim();
      if (tx && wordy(tx) && !ALLOW_TEXT.has(tx)) out.push({ file: relative(ROOT, f), line: lineOf(text), text: tx });
    }
  }
  return out;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const l = loose();
  for (const x of l) console.log(`${x.file}:${x.line}  ${x.text}`);
  console.log(`\n${l.length} loose texts; ${new Set(keys().map((k) => k.key)).size} keys`);
}
