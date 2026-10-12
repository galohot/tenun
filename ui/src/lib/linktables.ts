// Link tables (build 03b, V4-D112): which table looks like one, and what to say about it. Pure functions
// with no imports at run time, so the node tests can load them. The engine decides what a marked table
// means (links.rs); this file only suggests the mark.

import type { LinkHealth, Relationship, Table } from './api';

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
  return `${words.slice(0, -1).join(', ')} and ${words[words.length - 1]}`;
}

/**
 * The tables that look like a link table (V4-D112). A table qualifies when
 * - nothing points at it,
 * - at least two of its columns are the many side of a link, to two or more different tables,
 * - those columns are at least half of its columns (programme_funders: programme_id, funder_id, share), and
 * - the data says each of those columns repeats, so a programme has several funders and a funder several
 *   programmes. A link nobody has read yet has no record and counts as repeating; one that says its many side
 *   is unique does not (that is a one to one link, which already works).
 * A table that is marked, or that was set aside, is not suggested.
 */
export function suggestLinkTables(
  tables: Named[],
  rels: Relationship[],
  health: Health,
  marked: ReadonlySet<string> = new Set(),
  dismissed: readonly string[] = [],
): LinkTableSuggestion[] {
  const label = (id: string) => tables.find((t) => t.id === id)?.label ?? id;
  const out: LinkTableSuggestion[] = [];
  for (const t of tables) {
    if (marked.has(t.id) || dismissed.includes(t.id)) continue;
    if (rels.some((r) => r.to.table === t.id)) continue;
    const mine = rels.filter((r) => r.from.table === t.id && r.to.table !== t.id && health?.[r.id]?.fromUnique !== true);
    const columns = new Set(mine.map((r) => r.from.column));
    const ones = [...new Set(mine.map((r) => r.to.table))];
    if (columns.size < 2 || ones.length < 2) continue;
    if (columns.size * 2 < t.columns.length) continue;
    const between = ones.map(label);
    out.push({
      table: t.id,
      between,
      why: `${t.label} looks like a link table between ${andList(between)}. Mark it, and a chart can group ${between[0]} by ${between[1]}, or the other way round.`,
    });
  }
  return out;
}

/** What the node of a link table says under its name. */
export const LINK_TABLE_LABEL = 'Link table';
