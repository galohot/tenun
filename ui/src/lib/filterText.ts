// What a filter chip says: "Channel: Online", "Channel: not Online, Shop", "Date: Last 12 months".

import type { DashFilter, Table } from './api';
import { cell, dimLabel, type Named } from './format';
import { presetLabel } from './filters';
import { t } from './i18n';

type Tables = (Named & Pick<Table, 'columns'>)[];

export function filterText(f: DashFilter, tables: Tables): string {
  const dtype = tables.find((x) => x.id === f.table)?.columns?.find((c) => c.name === f.column)?.dtype ?? 'text';
  const name = dimLabel({ table: f.table, column: f.column, grain: f.grain, via: f.via }, tables);
  const show = (v: unknown, short = false) => cell(v as never, dtype, f.grain, short);
  const list = () => f.values.map((v) => show(v)).join(', ');
  // The "Other" group of a top N is labelled in English in the dashboard; it reads in the language of the screens.
  const label = f.label === 'Other' ? t('Other') : f.label === 'Not Other' ? t('Not Other') : f.label;
  const body =
    label ??
    (() => {
      switch (f.op) {
        case 'not_in':
          return t('not {values}', { values: list() });
        case 'range':
          // The end of a range is the start of the next day for dates: show the last day kept.
          return t('{from} to {to}', { from: show(f.values[0], true), to: show(f.values[1], true) });
        case 'between':
          return t('{from} to {to}', { from: show(f.values[0], true), to: show(f.values[1], true) });
        case 'gte':
          return t('{value} or more', { value: show(f.values[0], true) });
        case 'lte':
          return t('{value} or less', { value: show(f.values[0], true) });
        case 'contains':
          return t('contains {values}', { values: list() });
        case 'starts_with':
          return t('starts with {values}', { values: list() });
        case 'blank':
          return t('is blank');
        case 'not_blank':
          return t('is not blank');
        case 'relative':
          return presetLabel(String(f.values[0]));
        default:
          return list();
      }
    })();
  return `${name.charAt(0).toUpperCase() + name.slice(1)}: ${body}`;
}

