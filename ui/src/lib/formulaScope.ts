// The scope of a formula for the field that helps write it: the table, and the tables it can read.
// The pure parts are in `formula.ts`, which the node tests load; this file joins them to the search
// of `links.ts` that the engine's own path search is tested against.

import type { LinkHealth, Relationship, Table } from './api';
import type { FormulaKind, Scope } from './formula';
import { firstLinks, reachableFrom } from './links';

export function scopeFor(
  table: Table,
  tables: readonly Table[],
  rels: readonly Relationship[],
  health: Record<string, LinkHealth> | undefined,
  kind: FormulaKind,
  editing?: string,
): Scope {
  const list = rels as Relationship[];
  return {
    table,
    tables,
    rels,
    kind,
    editing,
    // A calculated column reads the tables it reaches many to one. A measure totals any table.
    reach: kind === 'column' ? reachableFrom(table.id, list, health) : null,
    firsts: (t) => firstLinks(table.id, t, list, health),
  };
}
