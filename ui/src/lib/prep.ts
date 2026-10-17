// Data prep helpers (build 08): column labels, the words for what was detected, and small
// edits of a table's setup. Pure functions, so the node tests can load them.

import type { CalcColumn, Column, ColumnSetup, DType, Table, TableSetup, Transform, UnpivotPlan } from './api';
import { t, tn } from './i18n.ts';

type WithColumns = Pick<Table, 'id'> & { columns?: Column[] };

/** The column of a table, or undefined. */
export function findColumn(tables: WithColumns[] | undefined, table: string, column: string): Column | undefined {
  return tables?.find((x) => x.id === table)?.columns?.find((c) => c.name === column);
}

/** The label the user gave a column, or undefined. */
export function labelOf(tables: WithColumns[] | undefined, table: string, column: string): string | undefined {
  const l = findColumn(tables, table, column)?.label;
  return l && l.trim() ? l : undefined;
}

/** True when a hidden column is the one in use, so a picker still shows it. */
export const isShown = (c: Column, inUse = false): boolean => !c.hidden || inUse;

/** Blank cells matter from this share up. Below it the picker stays quiet. */
export const BLANK_NOTE_FROM = 0.1;

/** "23 % blank" when more than 10 % of the cells are blank; otherwise nothing. */
export function blankNote(c: Pick<Column, 'blank'>): string {
  const b = c.blank ?? 0;
  return b > BLANK_NOTE_FROM ? t('{n} % blank', { n: Math.round(b * 100) }) : '';
}

/** "12 values could not be read as dates." after a type override. */
export function failedText(c: Pick<Column, 'failed' | 'dtype'>): string {
  const n = c.failed ?? 0;
  if (n <= 0) return '';
  switch (c.dtype) {
    case 'date': return tn('1 value could not be read as a date.', '{n} values could not be read as dates.', n);
    case 'datetime': return tn('1 value could not be read as a date and time.', '{n} values could not be read as dates and times.', n);
    case 'number': return tn('1 value could not be read as a number.', '{n} values could not be read as numbers.', n);
    case 'bool': return tn('1 value could not be read as a yes or no value.', '{n} values could not be read as yes or no values.', n);
    default: return tn('1 value could not be read as text.', '{n} values could not be read as text.', n);
  }
}

export const TYPE_CHOICES: { dtype: DType; label: string }[] = [
  { dtype: 'text', get label() { return t('Text'); } },
  { dtype: 'number', get label() { return t('Number'); } },
  { dtype: 'date', get label() { return t('Date'); } },
  { dtype: 'datetime', get label() { return t('Date and time'); } },
  { dtype: 'bool', get label() { return t('Yes or no'); } },
];

export const DATE_FORMATS: { value: 'auto' | 'dmy' | 'mdy'; label: string }[] = [
  { value: 'auto', get label() { return t('Automatic'); } },
  { value: 'dmy', label: 'dd/mm/yyyy' },
  { value: 'mdy', label: 'mm/dd/yyyy' },
];

const clean = (c: ColumnSetup): ColumnSetup => {
  const out: ColumnSetup = {};
  if (c.label?.trim()) out.label = c.label.trim();
  if (c.dtype) {
    out.dtype = c.dtype;
    if ((c.dtype === 'date' || c.dtype === 'datetime') && c.date_format && c.date_format !== 'auto') out.date_format = c.date_format;
  }
  if (c.hidden) out.hidden = true;
  if (c.description?.trim()) out.description = c.description.trim();
  return out;
};

/** The setup with one column's override changed. `null` removes the override. */
export function withColumn(setup: TableSetup | undefined, name: string, patch: Partial<ColumnSetup> | null): TableSetup {
  const columns = { ...(setup?.columns ?? {}) };
  const next = patch === null ? {} : clean({ ...columns[name], ...patch });
  if (Object.keys(next).length) columns[name] = next;
  else delete columns[name];
  return tidy({ ...setup, columns });
}

/** The setup with a chosen header row. `undefined` goes back to detection. */
export function withHeaderRow(setup: TableSetup | undefined, row: number | undefined): TableSetup {
  const out: TableSetup = { ...setup };
  if (row && row >= 1) out.header_row = Math.floor(row);
  else delete out.header_row;
  return tidy(out);
}

/** The setup with the columns of a wide sheet turned into rows. */
export function withUnpivot(setup: TableSetup | undefined, plan: Pick<UnpivotPlan, 'keep'>, name: string, value: string): TableSetup {
  const t: Transform = { kind: 'unpivot', keep: plan.keep, name: name.trim(), value: value.trim() };
  return tidy({ ...setup, transforms: [...(setup?.transforms ?? []).filter((x) => x.kind !== 'unpivot'), t] });
}

export const withoutUnpivot = (setup: TableSetup | undefined): TableSetup =>
  tidy({ ...setup, transforms: (setup?.transforms ?? []).filter((x) => x.kind !== 'unpivot') });

export const hasUnpivot = (setup: TableSetup | undefined): boolean => !!setup?.transforms?.some((x) => x.kind === 'unpivot');

/** Drops empty parts, so a cleared menu leaves nothing in the file. */
function tidy(s: TableSetup): TableSetup {
  const out: TableSetup = {};
  if (s.header_row) out.header_row = s.header_row;
  if (s.columns && Object.keys(s.columns).length) out.columns = s.columns;
  if (s.transforms?.length) out.transforms = s.transforms;
  if (s.calc?.length) out.calc = s.calc;
  if (s.mainDate) out.mainDate = s.mainDate;
  if (s.linkTable) out.linkTable = true;
  else if (s.notLinkTable) out.notLinkTable = true;
  return out;
}

export const isEmptySetup = (s: TableSetup | undefined): boolean => !s || Object.keys(tidy(s)).length === 0;

/** The values of a histogram as bar heights, 0–1. */
export function barHeights(counts: number[]): number[] {
  const top = Math.max(0, ...counts);
  return counts.map((n) => (top ? n / top : 0));
}

/** The setup with its calculated columns replaced. An empty list leaves none in the file. */
export const withCalc = (setup: TableSetup | undefined, calc: CalcColumn[]): TableSetup => tidy({ ...setup, calc });

/** The setup with the table marked as a link table, or not (V4-D112). Unmarked, nothing is left in the file. */
export const withLinkTable = (setup: TableSetup | undefined, on: boolean): TableSetup => tidy({ ...setup, linkTable: on || undefined });

/** The setup of a table the person said is not a link table, so the Model view stops suggesting the mark (V4-D124). */
export const withNotLinkTable = (setup: TableSetup | undefined): TableSetup => tidy({ ...setup, linkTable: undefined, notLinkTable: true });

/** The setup with the date a measure lines this table up with over time. `undefined` goes back to the table's only date column. */
export const withMainDate = (setup: TableSetup | undefined, column: string | undefined): TableSetup => tidy({ ...setup, mainDate: column });

const sameName = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

/**
 * Why a name shown cannot be given to a column, in words, or null: another column answers to it
 * already (by its real name or the name shown), so `[name]` in a formula would mean two columns.
 * Names are compared the way the engine does, ignoring case and the spaces at the ends (V4-D77).
 * A column may be given its own real name.
 */
export function labelProblem(label: string, column: Pick<Column, 'name'>, table: { label: string; columns: Pick<Column, 'name' | 'label'>[] }): string | null {
  const n = label.trim();
  if (!n) return null;
  const other = table.columns.find((c) => c.name !== column.name && (sameName(c.name, n) || (c.label && sameName(c.label, n))));
  return other ? t('The name {name} is already used by another column of {table}. Choose another name.', { name: n, table: table.label }) : null;
}
