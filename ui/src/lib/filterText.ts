// What a filter chip says: "Channel: Online", "Channel: not Online, Shop", "Date: Last 12 months".

import type { DashFilter, Table } from './api';
import { cell, dimLabel, type Named } from './format';
import { presetLabel } from './filters';

type Tables = (Named & Pick<Table, 'columns'>)[];

export function filterText(f: DashFilter, tables: Tables): string {
  const dtype = tables.find((t) => t.id === f.table)?.columns?.find((c) => c.name === f.column)?.dtype ?? 'text';
  const name = dimLabel({ table: f.table, column: f.column, grain: f.grain, via: f.via }, tables);
  const show = (v: unknown, short = false) => cell(v as never, dtype, f.grain, short);
  const list = () => f.values.map((v) => show(v)).join(', ');
  const body =
    f.label ??
    (() => {
      switch (f.op) {
        case 'not_in':
          return `not ${list()}`;
        case 'range':
          // The end of a range is the start of the next day for dates: show the last day kept.
          return `${show(f.values[0], true)} to ${show(f.values[1], true)}`;
        case 'between':
          return `${show(f.values[0], true)} to ${show(f.values[1], true)}`;
        case 'gte':
          return `${show(f.values[0], true)} or more`;
        case 'lte':
          return `${show(f.values[0], true)} or less`;
        case 'contains':
          return `contains ${list()}`;
        case 'starts_with':
          return `starts with ${list()}`;
        case 'blank':
          return 'is blank';
        case 'not_blank':
          return 'is not blank';
        case 'relative':
          return presetLabel(String(f.values[0]));
        default:
          return list();
      }
    })();
  return `${name.charAt(0).toUpperCase() + name.slice(1)}: ${body}`;
}

