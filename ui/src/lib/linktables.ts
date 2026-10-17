// Link tables (build 03b, V4-D112): which table looks like one, and what to say about it. Pure functions; the only
// import at run time is t(), so the node tests can load them. The engine decides what a marked table
// means (links.rs); this file only suggests the mark.

import type { LinkHealth, Relationship, Table } from './api';
import { t, tk } from './i18n.ts';

export interface LinkTableSuggestion {
  /** The table that looks like a link table. */
  table: string;
  /** The tables it links, by label, in the order of its columns. */
  between: string[];
  /** The sentence for the Model view. */
  why: string;
}

type Named = Pick<Table, 'id' | 'label' | 'columns'>;
type Health = Record<string, Pick<LinkHealth, 'fromUnique'>> | undefined;

/** A join of two lists of words: "A and B", "A, B and C". */
export function andList(words: string[]): string {
  if (words.length < 2) return words.join('');
  return t('{list} and {last}', { list: words.slice(0, -1).join(', '), last: words[words.length - 1] });
}

/** The forms of a name that another name may hold it in: "programmes" → programmes, programme; "categories" → category. */
function stems(word: string): string[] {
  const w = word.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '');
  const out = [w];
  if (w.endsWith('ies')) out.push(`${w.slice(0, -3)}y`);
  else if (w.endsWith('es')) out.push(w.slice(0, -1), w.slice(0, -2));
  else if (w.endsWith('s')) out.push(w.slice(0, -1));
  return out.filter((x) => x.length >= 3);
}

/** A key column without its "id": programme_id → programme, funderId → funder. */
const keyStem = (column: string): string => column.replace(/[\s_-]*(id|key|code)$/i, '');

/**
 * The tables that look like a link table (V4-D112, V4-D124). A table qualifies when
 * - nothing points at it,
 * - at least two of its columns are the many side of a link, to two or more different tables,
 * - those columns are at least half of its columns (programme_funders: programme_id, funder_id, share),
 * - its name names two of those tables, by the table or by the key column (programme_funders names the programmes
 *   and the funders), or every one of its columns is a key (enrolments: student_id, course_id), and
 * - the data says each of those columns repeats, so a programme has several funders and a funder several
 *   programmes. A link nobody has read yet has no record and counts as repeating; one that says its many side
 *   is unique does not (that is a one to one link, which already works).
 * The name rule keeps a table of events out: sales (store_id, product_id, qty) has two links and few columns,
 * and its name names neither table. A table it misses can still be marked by hand.
 * A table that is marked, or that was set aside, is not suggested.
 */
export function suggestLinkTables(
  tables: Named[],
  rels: Relationship[],
  health: Health,
  marked: ReadonlySet<string> = new Set(),
  dismissed: readonly string[] = [],
): LinkTableSuggestion[] {
  const label = (id: string) => tables.find((x) => x.id === id)?.label ?? id;
  const out: LinkTableSuggestion[] = [];
  for (const tb of tables) {
    if (marked.has(tb.id) || dismissed.includes(tb.id)) continue;
    if (rels.some((r) => r.to.table === tb.id)) continue;
    const mine = rels.filter((r) => r.from.table === tb.id && r.to.table !== tb.id && health?.[r.id]?.fromUnique !== true);
    const columns = new Set(mine.map((r) => r.from.column));
    const ones = [...new Set(mine.map((r) => r.to.table))];
    if (columns.size < 2 || ones.length < 2) continue;
    if (columns.size * 2 < tb.columns.length) continue;
    const name = stems(tb.label)[0] ?? '';
    const named = ones.filter((o) =>
      [label(o), ...mine.filter((r) => r.to.table === o).map((r) => keyStem(r.from.column))].some((w) => stems(w).some((x) => name.includes(x))),
    );
    if (named.length < 2 && columns.size < tb.columns.length) continue;
    const between = ones.map(label);
    out.push({
      table: tb.id,
      between,
      why: t('{table} looks like a link table between {tables}. Mark it, and a chart can group {a} by {b}, or the other way round.', {
        table: tb.label,
        tables: andList(between),
        a: between[0],
        b: between[1],
      }),
    });
  }
  return out;
}

/** What the node of a link table says under its name; shown through t(). */
export const LINK_TABLE_LABEL = tk('Link table');

/** A table can be marked by hand when two or more of its columns are the many side of a link (V4-D119). */
export function canMarkLinkTable(table: string, rels: Relationship[]): boolean {
  return new Set(rels.filter((r) => r.from.table === table).map((r) => r.from.column)).size >= 2;
}

/** The tables the person said are not link tables, from the setups of the workspace (V4-D124). */
export function notLinkTablesOf(setups: Record<string, { notLinkTable?: boolean }> | undefined): string[] {
  return Object.entries(setups ?? {})
    .filter(([, s]) => s.notLinkTable)
    .map(([id]) => id);
}
