// The help a formula field gives while you type (build 02): which names fit, and what a choice
// writes into the text. Pure functions, so the node tests can load them. The engine decides what
// a formula means (`POST /api/formula/check`); this file only suggests and cuts.

import type { Column, Dashboard, DType, FormulaError, Relationship, Table, Tile } from './api';
import functions from './functions.json' with { type: 'json' };
import { t } from './i18n.ts';

// ---- the function list -------------------------------------------------------------------------

export interface FnInfo {
  name: string;
  /** The arguments as the signature writes them: `test, a, [b]`. `[x]` is optional, `…` repeats. */
  args: string;
  /** `row` works on one row; `agg` totals many rows (a measure); `either` does both (MIN and MAX). */
  class: 'row' | 'agg' | 'either';
  hint: string;
}

/** Every function of the engine, with a one-line hint. The engine's own list is tested against this one. */
export const FUNCTIONS: readonly FnInfo[] = functions as FnInfo[];
/** Functions a calculated column cannot use: they read a tile's groups, filters or controls (v5 builds 05 and 06). */
export const MEASURE_ONLY: readonly string[] = ['TOTAL', 'UNFILTERED', 'CLOSING', 'OPENING', 'PARAM'];

export const signature = (f: FnInfo): string => `${f.name}(${f.args})`;

// ---- what is being typed -------------------------------------------------------------------------

/** What the formula is for: a column of a table, or (build 03) a measure or a group value. */
export type FormulaKind = 'column' | 'measure' | 'group';

export interface Scope {
  /** The table of the formula. A measure has none: it reads any table by its name, `[Sales].[amount]`. */
  table: Table | null;
  tables: readonly Table[];
  rels: readonly Relationship[];
  kind: FormulaKind;
  /** The tables the formula can read besides its own. Not set: all of them (a measure totals any table). */
  reach?: ReadonlySet<string> | null;
  /** The first links of every shortest way from the table of the formula to another: `firstLinks` of `links.ts`. */
  firsts?: (table: string) => string[];
  /**
   * The calculated column being edited. The columns of the table from it on are not offered,
   * because a column may use the ones above it, never itself or the ones below (V4-D3).
   */
  editing?: string;
}

export type ItemKind = 'column' | 'table' | 'link' | 'function';

export interface Item {
  kind: ItemKind;
  /** The name in the list. */
  label: string;
  /** What replaces the typed part. */
  insert: string;
  /** Where the caret lands, counted from the start of the replaced part. */
  caret: number;
  /** A word for the right side of the row: the type of a column, or the signature of a function. */
  detail: string;
  /** One line about it. */
  hint?: string;
  dtype?: DType;
}

export interface Completion {
  /** The part of the text the choice replaces. */
  from: number;
  to: number;
  /** What was typed so far. */
  query: string;
  items: Item[];
}

/** Where the caret is in the text: in code, in a text between quotes, or in a name between brackets. */
type Place = { in: 'code' } | { in: 'text' } | { in: 'name'; start: number };

function placeAt(text: string, caret: number): Place {
  let place: Place = { in: 'code' };
  for (let i = 0; i < caret; i++) {
    const c = text[i];
    if (place.in === 'code') {
      if (c === '"') place = { in: 'text' };
      else if (c === '[') place = { in: 'name', start: i };
    } else if (place.in === 'text') {
      if (c === '"') {
        // A doubled quote stays inside the text.
        if (text[i + 1] === '"') i++;
        else place = { in: 'code' };
      }
    } else if (c === ']') {
      // A doubled bracket stays inside the name.
      if (text[i + 1] === ']') i++;
      else place = { in: 'code' };
    }
  }
  return place;
}

/** A name as a formula writes it between brackets: `]` is written twice. */
export const bracketed = (name: string): string => `[${name.replace(/]/g, ']]')}]`;

const unbracket = (s: string): string => s.replace(/]]/g, ']');

/**
 * The name in the bracket pair that ends right before `at` (`[Table].` then `at`), or null. The text is
 * read from its start the way the engine reads it, so a name that holds `[` or a doubled `]` (a table
 * called "Sales [2024]" is written `[Sales [2024]]]`) is one name, not a part of one.
 */
function qualifierBefore(text: string, at: number): string | null {
  if (text[at - 1] !== '.' || text[at - 2] !== ']') return null;
  let last: { start: number; end: number } | null = null;
  let place: 'code' | 'text' | 'name' = 'code';
  let start = 0;
  for (let i = 0; i < at - 1; i++) {
    const c = text[i];
    if (place === 'code') {
      if (c === '"') place = 'text';
      else if (c === '[') {
        place = 'name';
        start = i;
      }
    } else if (place === 'text') {
      if (c === '"') {
        if (text[i + 1] === '"') i++;
        else place = 'code';
      }
    } else if (c === ']') {
      if (text[i + 1] === ']') i++;
      else {
        place = 'code';
        last = { start, end: i + 1 };
      }
    }
  }
  return last && last.end === at - 1 ? unbracket(text.slice(last.start + 1, last.end - 1)) : null;
}

export function typeWords(d: DType): string {
  switch (d) {
    case 'number': return t('Number');
    case 'text': return t('Text');
    case 'date': return t('Date');
    case 'datetime': return t('Date and time');
    case 'bool': return t('Yes or no');
  }
}

const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/** Items whose name starts with the query come first, then the ones that hold it. */
function rank<T>(items: T[], query: string, names: (x: T) => string[]): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  const score = (x: T) => {
    const hits = names(x).map((n) => n.toLowerCase());
    if (hits.some((n) => n.startsWith(q))) return 0;
    if (hits.some((n) => n.includes(q))) return 1;
    return 2;
  };
  return items
    .map((x, i) => ({ x, i, s: score(x) }))
    .filter((r) => r.s < 2)
    .sort((a, b) => a.s - b.s || a.i - b.i)
    .map((r) => r.x);
}

/** The columns of the table the formula can read: its own, and the calculated ones above `editing`. */
export function visibleColumns(table: Table, editing?: string): Column[] {
  const at = editing === undefined ? -1 : table.columns.findIndex((c) => c.calc && c.name === editing);
  return (at < 0 ? table.columns : table.columns.slice(0, at)).filter((c) => !c.hidden);
}

const shown = (c: Column) => c.label?.trim() || c.name;

function columnItem(c: Column, wrap: (name: string) => string): Item {
  const insert = wrap(shown(c));
  return {
    kind: 'column',
    label: shown(c),
    insert,
    caret: insert.length,
    detail: typeWords(c.dtype),
    hint: c.description,
    dtype: c.dtype,
  };
}

/** The tables the formula can reach, and the links to them. A table two links lead to is offered once for each named link. */
function tableItems(scope: Scope): Item[] {
  const { table, tables, rels } = scope;
  const out: Item[] = [];
  for (const tb of tables) {
    if (tb.id === table?.id || (scope.reach && !scope.reach.has(tb.id))) continue;
    const firsts = scope.firsts ? scope.firsts(tb.id) : [];
    const direct = firsts.map((id) => rels.find((r) => r.id === id)).filter((r): r is Relationship => !!r && r.to.table === tb.id);
    const named = direct.filter((r) => r.name?.trim());
    if (firsts.length >= 2 && named.length) {
      // Two links lead here: each is written by its name (V4-D28).
      for (const r of named) {
        const insert = `${bracketed(r.name!.trim())}.[`;
        out.push({ kind: 'link', label: r.name!.trim(), insert, caret: insert.length, detail: t('Link to {table}', { table: tb.label }) });
      }
      continue;
    }
    const insert = `${bracketed(tb.label)}.[`;
    out.push({
      kind: 'table',
      label: tb.label,
      insert,
      caret: insert.length,
      detail: firsts.length >= 2 ? t('Two links lead here. Name them in Model.') : table ? t('Related table') : t('Table'),
    });
  }
  return out;
}

/** The table a name in front of a column stands for: a table, or the table on the one side of a named link. */
export function tableNamed(scope: Scope, name: string): Table | undefined {
  const t = scope.tables.find((x) => same(x.label, name) || same(x.id, name));
  if (t) return t;
  const r = scope.rels.find((x) => x.name && same(x.name, name));
  return r ? scope.tables.find((x) => x.id === r.to.table) : undefined;
}

/** The list scrolls, so it can hold every column of a wide table. */
const MAX_ITEMS = 30;

/**
 * What to offer with the caret at `caret`. Inside a name between brackets: the columns of the table
 * (and the related tables and links), or, after `[Table].`, the columns of that table. After a word
 * outside brackets: the functions that start with it. Inside a text between quotes: nothing.
 */
export function suggest(text: string, caret: number, scope: Scope): Completion | null {
  const place = placeAt(text, caret);
  if (place.in === 'text') return null;
  if (place.in === 'name') {
    const query = unbracket(text.slice(place.start + 1, caret));
    const to = text[caret] === ']' ? caret + 1 : caret;
    const qualifier = qualifierBefore(text, place.start);
    if (qualifier !== null) {
      const t = tableNamed(scope, qualifier);
      if (!t) return null;
      const cols = rank(visibleColumns(t), query, (c) => [c.name, shown(c)]);
      const items = cols.slice(0, MAX_ITEMS).map((c) => columnItem(c, bracketed));
      return items.length ? { from: place.start, to, query, items } : null;
    }
    // A measure has no table of its own: every column sits inside a total, so it is named with its table.
    const own = scope.table ? rank(visibleColumns(scope.table, scope.editing), query, (c) => [c.name, shown(c)]).map((c) => columnItem(c, bracketed)) : [];
    const others = rank(tableItems(scope), query, (i) => [i.label]);
    const items = [...own, ...others].slice(0, MAX_ITEMS);
    return items.length ? { from: place.start, to, query, items } : null;
  }
  // A word being typed in code. A word that follows `.` or a digit is not a function.
  let start = caret;
  while (start > 0 && /[A-Za-z0-9_]/.test(text[start - 1])) start--;
  const word = text.slice(start, caret);
  if (!word || !/^[A-Za-z_]/.test(word)) return null;
  // A measure or a group value is made of totals, so the totals come first: SU offers SUM before SUBSTITUTE. A column
  // works row by row, and its totals add up a related table (v5 build 06), so they come after. TOTAL, UNFILTERED and
  // PARAM belong in measures only.
  const fits =
    scope.kind === 'column'
      ? [...FUNCTIONS.filter((f) => f.class !== 'agg' && !MEASURE_ONLY.includes(f.name)), ...FUNCTIONS.filter((f) => f.class === 'agg')]
      : [...FUNCTIONS.filter((f) => f.class !== 'row'), ...FUNCTIONS.filter((f) => f.class === 'row')];
  const found = rank([...fits], word, (f) => [f.name]).filter((f) => f.name.toLowerCase().startsWith(word.toLowerCase()));
  // A function that is already typed in full, with its bracket, needs no help.
  if (text[caret] === '(' && FUNCTIONS.some((f) => f.name.toLowerCase() === word.toLowerCase())) return null;
  const items = found.slice(0, MAX_ITEMS).map((f): Item => {
    const insert = f.args === '' ? `${f.name}()` : `${f.name}(`;
    return { kind: 'function', label: f.name, insert, caret: insert.length, detail: signature(f), hint: f.hint };
  });
  return items.length ? { from: start, to: caret, query: word, items } : null;
}

/** The text after a choice, and where the caret goes. */
export function apply(text: string, c: Pick<Completion, 'from' | 'to'>, item: Item): { text: string; caret: number } {
  return { text: text.slice(0, c.from) + item.insert + text.slice(c.to), caret: c.from + item.caret };
}

// ---- the underline ------------------------------------------------------------------------------

/** The text cut in three for the underline: before the problem, the problem, after it. `at` and `len` are UTF-16 units. */
export function cut(text: string, err: Pick<FormulaError, 'at' | 'len'> | null | undefined): [string, string, string] {
  if (!err) return [text, '', ''];
  const at = Math.min(Math.max(err.at, 0), text.length);
  // A length of 0 is a place, not a range: the formula stops here, so the last character is marked.
  const len = err.len > 0 ? Math.min(err.len, text.length - at) : 0;
  if (len === 0) return at >= text.length && text.length ? [text.slice(0, -1), text.slice(-1), ''] : [text.slice(0, at), '', text.slice(at)];
  return [text.slice(0, at), text.slice(at, at + len), text.slice(at + len)];
}

// ---- names ----------------------------------------------------------------------------------------

/** The longest name a calculated column may have. Same as the engine. */
export const MAX_NAME = 100;

/**
 * Why a name cannot be used for a calculated column, in words, or null. The same rules as the engine,
 * so the sheet can say it before the request does. `except` is the column being edited.
 */
export function nameProblem(name: string, table: Table, except?: string): string | null {
  const n = name.trim();
  if (!n) return t('Give the column a name.');
  if (n.length > MAX_NAME) return t('Keep the name to {n} letters or fewer.', { n: MAX_NAME });
  if (/[[\]]/.test(n)) return t('Leave the square brackets out of the name. Formulas use them to write names.');
  if (n.startsWith('__')) return t('A name cannot start with two underscores.');
  const clash = table.columns.some((c) => c.name !== except && (same(c.name, n) || (c.label && same(c.label, n))));
  if (clash) return t('{table} has a column called {name} already. Choose another name.', { table: table.label, name: n });
  return null;
}

// ---- who uses a column ---------------------------------------------------------------------------

export interface Use {
  dashboard: string;
  /** The title of the tile, or the empty text when it has none. */
  tile: string;
  /** What kind of tile it is, in words: "bar chart", "table", "filter". */
  kind: string;
}

/** A tile type in words. */
export function kindWords(type: string): string {
  switch (type) {
    case 'kpi': return t('number tile');
    case 'bar': return t('bar chart');
    case 'line': return t('line chart');
    case 'area': return t('area chart');
    case 'share': return t('share bar');
    case 'donut': return t('donut chart');
    case 'pie': return t('pie chart');
    case 'map': return t('map');
    case 'table': return t('table');
    case 'scatter': return t('scatter chart');
    case 'histogram': return t('histogram');
    case 'waterfall': return t('waterfall chart');
    case 'pivot': return t('pivot table');
    case 'filter': return t('filter');
    case 'heatmap': return t('heat map');
    case 'funnel': return t('funnel');
    case 'box': return t('box plot');
    case 'bullet': return t('bullet chart');
    case 'compare': return t('compare chart');
    case 'sankey': return t('flow chart');
    case 'calendar': return t('calendar');
    case 'cohort': return t('cohort table');
    case 'gantt': return t('Gantt chart');
    default: return t('tile');
  }
}

/** One line about a tile that uses a column: `Revenue by region (bar chart) on Overview`. */
export const useWords = (u: Use): string =>
  u.tile.trim() ? t('{tile} ({kind}) on {page}', { tile: u.tile.trim(), kind: u.kind, page: u.dashboard }) : t('A {kind} on {page}', { kind: u.kind, page: u.dashboard });

/** The tiles (and the saved filters) that use a column, for the question before it goes. */
export function tilesUsing(dashboards: readonly Dashboard[], table: string, column: string, title: (t: Tile) => string): Use[] {
  const hit = (f?: { table: string; column?: string | null } | null) => !!f && f.table === table && f.column === column;
  const out: Use[] = [];
  for (const d of dashboards) {
    for (const tl of d.tiles ?? []) {
      const used =
        tl.measures?.some((m) => hit(m) || hit(m.per)) ||
        tl.dims?.some(hit) ||
        hit(tl.repeat) ||
        hit(tl.cols) ||
        hit(tl.spark);
      if (used) out.push({ dashboard: d.title, tile: title(tl), kind: kindWords(tl.type) });
    }
    if (d.filters?.some(hit)) out.push({ dashboard: d.title, tile: '', kind: t('saved filter') });
  }
  return out;
}

/** A calculated column that reads another column: `table` is the table id, `label` its name for people. */
export interface Reader {
  table: string;
  label: string;
  column: string;
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * The calculated columns that would go blank if `column` of `table` went: the ones whose formula names
 * it, and the ones that read those, and so on. In the table itself a name stands alone; in another
 * table it follows the table or a link that leads to it. A SQL column reads real names, so it is found
 * by the name in its text. `setups` is `workspace.tables`; `labels` gives each table's name for people.
 */
export function readersOf(
  setups: Record<string, { calc?: { name: string; formula: string; sql?: boolean }[] }>,
  table: { id: string; label: string },
  column: string,
  labels: (id: string) => string,
  links: readonly string[] = [],
): Reader[] {
  const out: Reader[] = [];
  const seen = new Set<string>([`${table.id}\u0000${column}`]);
  // Each column found is looked for in its turn: who reads it?
  const queue: { table: { id: string; label: string }; column: string }[] = [{ table, column }];
  while (queue.length) {
    const target = queue.shift()!;
    const bare = new RegExp(`\\[${escapeRe(target.column)}\\]`, 'i');
    const qualifiers = [target.table.label, target.table.id, ...links].map(escapeRe).join('|');
    const named = new RegExp(`\\[(${qualifiers})\\]\\s*\\.\\s*\\[${escapeRe(target.column)}\\]`, 'i');
    const sqlWord = new RegExp(`(^|[^A-Za-z0-9_])"?${escapeRe(target.column)}"?($|[^A-Za-z0-9_])`, 'i');
    for (const [id, setup] of Object.entries(setups)) {
      for (const c of setup.calc ?? []) {
        const key = `${id}\u0000${c.name}`;
        if (seen.has(key)) continue;
        const own = id === target.table.id;
        const reads = c.sql ? own && sqlWord.test(c.formula) : (own ? bare : named).test(c.formula);
        if (!reads) continue;
        seen.add(key);
        const label = labels(id) ?? id;
        out.push({ table: id, label, column: c.name });
        queue.push({ table: { id, label }, column: c.name });
      }
    }
  }
  return out;
}

/** "Revenue" and "Region" ... "and 3 more": a list of names in one line. */
export function listWords(items: readonly string[], max = 5): string {
  if (items.length <= max) return items.join(', ');
  return t('{list} and {n} more', { list: items.slice(0, max).join(', '), n: items.length - max });
}
