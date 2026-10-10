// Data prep helpers (build 08): column labels, the words for what was detected, and small
// edits of a table's setup. Pure functions, so the node tests can load them.

import type { Column, ColumnSetup, DType, Table, TableSetup, Transform, UnpivotPlan } from './api';

type WithColumns = Pick<Table, 'id'> & { columns?: Column[] };

/** The column of a table, or undefined. */
export function findColumn(tables: WithColumns[] | undefined, table: string, column: string): Column | undefined {
  return tables?.find((t) => t.id === table)?.columns?.find((c) => c.name === column);
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
  return b > BLANK_NOTE_FROM ? `${Math.round(b * 100)} % blank` : '';
}

const READ_AS: Record<DType, [string, string]> = {
  date: ['a date', 'dates'],
  datetime: ['a date and time', 'dates and times'],
  number: ['a number', 'numbers'],
  bool: ['a yes or no value', 'yes or no values'],
  text: ['text', 'text'],
};

/** "12 values could not be read as dates." after a type override. */
export function failedText(c: Pick<Column, 'failed' | 'dtype'>): string {
  const n = c.failed ?? 0;
  if (n <= 0) return '';
  const [one, many] = READ_AS[c.dtype];
  return n === 1 ? `1 value could not be read as ${one}.` : `${n} values could not be read as ${many}.`;
}

export const TYPE_CHOICES: { dtype: DType; label: string }[] = [
  { dtype: 'text', label: 'Text' },
  { dtype: 'number', label: 'Number' },
  { dtype: 'date', label: 'Date' },
  { dtype: 'datetime', label: 'Date and time' },
  { dtype: 'bool', label: 'Yes or no' },
];

export const DATE_FORMATS: { value: 'auto' | 'dmy' | 'mdy'; label: string }[] = [
  { value: 'auto', label: 'Automatic' },
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
  return out;
}

export const isEmptySetup = (s: TableSetup | undefined): boolean => !s || Object.keys(tidy(s)).length === 0;

/** The values of a histogram as bar heights, 0–1. */
export function barHeights(counts: number[]): number[] {
  const top = Math.max(0, ...counts);
  return counts.map((n) => (top ? n / top : 0));
}
