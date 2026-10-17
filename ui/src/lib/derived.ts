// New tables from other tables (build 04, V4-D6): what the sheet needs before it asks the engine. Pure
// functions; the only import at run time is t(), so the node tests can load them. The engine makes the table
// and has the last word (derived.rs).

import type { Column, DerivedSpec, Source, Table } from './api';
import { t } from './i18n.ts';

export type DerivedOp = DerivedSpec['op'];

type Named = Pick<Table, 'id' | 'label'> & { columns: Pick<Column, 'name' | 'dtype'>[] };

/** The three ways to make a new table, as the Add data sheet offers them. */
export const OPS: { op: DerivedOp; title: string; icon: string; words: string }[] = [
  {
    op: 'append',
    icon: 'append',
    get title() {
      return t('Append tables');
    },
    get words() {
      return t('Stack the rows of tables with the same columns, like one file for each month.');
    },
  },
  {
    op: 'merge',
    icon: 'merge',
    get title() {
      return t('Merge two tables');
    },
    get words() {
      return t('Put the columns of one table beside the rows of another, matched on a key.');
    },
  },
  {
    op: 'group',
    icon: 'group',
    get title() {
      return t('Group a table');
    },
    get words() {
      return t('One row for each group, with totals you write as formulas.');
    },
  },
];

/** The ids of the tables a new table reads. */
export function inputsOf(spec: DerivedSpec): string[] {
  if (spec.op === 'append') return spec.tables;
  if (spec.op === 'merge') return [spec.left, spec.right];
  return [spec.table];
}

/** "From: orders, stores", with the labels of the tables, or the name in the id of one that is gone. */
export function fromWords(spec: DerivedSpec, tables: readonly Named[]): string {
  const names = inputsOf(spec).map((id) => tables.find((x) => x.id === id)?.label ?? id.split('/').slice(1).join('/'));
  return t('From: {names}', { names: [...new Set(names)].join(', ') });
}

/** The spec of a new table source, or null for any other source. */
export const specOf = (s: Pick<Source, 'kind' | 'spec'>): DerivedSpec | null => (s.kind === 'derived' && s.spec ? s.spec : null);

export interface AppendColumn {
  name: string;
  /** The labels of the tables that do not have the column: their rows are blank there. */
  missing: string[];
  /** The kinds of the column in the tables that have it. Two or more kinds become text. */
  types: string[];
}

/** The columns of an append, each name once in the order it first comes, with what each table lacks. */
export function appendColumns(tables: readonly Named[]): AppendColumn[] {
  const out: AppendColumn[] = [];
  for (const t of tables) {
    for (const c of t.columns) {
      let col = out.find((x) => x.name === c.name);
      if (!col) {
        col = { name: c.name, missing: [], types: [] };
        out.push(col);
      }
      if (!col.types.includes(c.dtype)) col.types.push(c.dtype);
    }
  }
  for (const col of out) col.missing = tables.filter((t) => !t.columns.some((c) => c.name === col.name)).map((t) => t.label);
  return out;
}

/** What keeps a new table from being made, in words, before the engine is asked. `null` when it can be asked. */
export function specProblem(spec: DerivedSpec): string | null {
  if (spec.op === 'append') {
    if (spec.tables.length < 2) return t('Pick two or more tables to append.');
    return null;
  }
  if (spec.op === 'merge') {
    if (!spec.left || !spec.right) return t('Pick the two tables.');
    if (spec.left === spec.right) return t('Pick two different tables to merge.');
    if (!spec.on.length || spec.on.some(([a, b]) => !a || !b)) return t('Pick the columns that match: one from each table.');
    return null;
  }
  if (!spec.table) return t('Pick a table.');
  if (!spec.by.length && !spec.values.length) return t('Pick a column to group by, or add a value.');
  if (spec.values.some((v) => !v.name.trim() || !v.formula.trim())) return t('Give each value a name and a formula.');
  return null;
}

/** What is wrong with the name of a new table. The engine says the same (derived.rs `name_problem`). */
export function tableNameProblem(name: string): string | null {
  const n = name.trim();
  if (!n) return t('Give the new table a name.');
  if ([...n].length > 64) return t('A name has 64 characters at most. This one has {n}.', { n: [...n].length });
  if (n.startsWith('__')) return t('A name cannot start with two underscores.');
  return null;
}

/** The tables a new table may read: every table but its own, and none that reads it (that would be a ring). */
export function pickable(tables: readonly Named[], sources: readonly Pick<Source, 'id' | 'kind' | 'spec'>[], editing?: string): Named[] {
  if (!editing) return [...tables];
  const readers = new Set<string>([editing]);
  // The sources that read the one being changed, directly or through others.
  for (let changed = true; changed; ) {
    changed = false;
    for (const s of sources) {
      const spec = specOf(s);
      if (!spec || readers.has(s.id)) continue;
      if (inputsOf(spec).some((id) => readers.has(id.split('/')[0]))) {
        readers.add(s.id);
        changed = true;
      }
    }
  }
  return tables.filter((t) => !readers.has(t.id.split('/')[0]));
}

/**
 * A key pair the sheet starts a merge with: the left column named after the right table and the right table's
 * id (orders.store_id and stores.id), else a key column both tables name alike (code and code). Two plain `id`
 * columns are each table's own row number, so they are not a match. A person can change the guess.
 */
export function guessKey(left: Named | undefined, right: Named | undefined): [string, string] {
  if (!left || !right) return ['', ''];
  const stem = right.label.toLowerCase().replace(/(ies|es|s)$/, '');
  const fk = left.columns.find((c) => c.name.toLowerCase().startsWith(stem) && /(_|\s)?(id|code)$/i.test(c.name));
  const pk = right.columns.find((c) => /^(id|code|key)$/i.test(c.name));
  if (fk && pk) return [fk.name, pk.name];
  const same = left.columns.find((c) => !/^id$/i.test(c.name) && right.columns.some((r) => r.name === c.name) && /(^|_)(id|code|key)$|id$/i.test(c.name));
  if (same) return [same.name, same.name];
  return ['', ''];
}
