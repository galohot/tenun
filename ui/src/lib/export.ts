// Builds a self-contained HTML file for a dashboard: the viewer bundle, the
// layout, and the data of every tile, including the results needed to
// cross-filter offline by clicking (for categories with up to 30 values).

import { api, type Cell, type Dashboard, type QueryResult, type Table } from './api';
import { key, pivot } from './chart';
import { tileTitle } from './format';
import { tileQuery, type DashFilter } from './tiles';

export interface ExportPayload {
  title: string;
  workspace: string;
  generated: string;
  tables: Pick<Table, 'id' | 'name' | 'label' | 'columns'>[];
  tiles: Dashboard['tiles'];
  titles: Record<string, string>;
  base: Record<string, QueryResult>;
  /** cross[fromTile][valueKey][tile] = result with that value picked. */
  cross: Record<string, Record<string, Record<string, QueryResult>>>;
  filters: DashFilter[];
}

const MAX_VALUES = 30;

async function pool<T>(jobs: (() => Promise<T>)[], n = 4): Promise<T[]> {
  const out: T[] = new Array(jobs.length);
  let i = 0;
  await Promise.all(
    Array.from({ length: Math.min(n, jobs.length) }, async () => {
      while (i < jobs.length) {
        const j = i++;
        out[j] = await jobs[j]();
      }
    }),
  );
  return out;
}

export async function buildPayload(
  dash: Dashboard,
  tables: Table[],
  workspace: string,
  filters: DashFilter[],
  progress?: (done: number, total: number) => void,
): Promise<ExportPayload> {
  const tiles = dash.tiles.filter((t) => tileQuery(t, filters));
  const base: Record<string, QueryResult> = {};
  await pool(
    tiles.map((t) => async () => {
      base[t.id] = await api.query(tileQuery(t, filters)!);
    }),
  );

  // Offline cross-filtering: for each clickable tile and each of its values,
  // the result of every other tile.
  const jobs: (() => Promise<void>)[] = [];
  const cross: ExportPayload['cross'] = {};
  for (const src of tiles) {
    if (src.type === 'kpi' || !src.dims[0] || !base[src.id]) continue;
    const d = src.dims[0];
    const values: Cell[] =
      src.type === 'donut' || src.type === 'table'
        ? [...new Map(base[src.id].rows.map((r) => [key(r[0]), r[0]])).values()]
        : pivot(base[src.id], src).categories;
    if (!values.length || values.length > MAX_VALUES) continue;
    cross[src.id] = {};
    for (const v of values) {
      cross[src.id][key(v)] = {};
      const extra: DashFilter = { table: d.table, column: d.column, grain: d.grain ?? null, op: 'in', values: [v], fromTile: src.id };
      const fs = [...filters.filter((f) => f.fromTile !== src.id), extra];
      for (const t of tiles) {
        if (t.id === src.id) continue;
        jobs.push(async () => {
          cross[src.id][key(v)][t.id] = await api.query(tileQuery(t, fs)!);
        });
      }
    }
  }
  let done = 0;
  await pool(
    jobs.map((j) => async () => {
      await j();
      progress?.(++done, jobs.length);
    }),
    6,
  );

  const used = new Set(tiles.flatMap((t) => [...t.measures.map((m) => m.table), ...t.dims.map((d) => d.table)]));
  return {
    title: dash.title,
    workspace,
    generated: new Date().toISOString(),
    tables: tables.filter((t) => used.has(t.id)).map(({ id, name, label, columns }) => ({ id, name, label, columns })),
    tiles,
    titles: Object.fromEntries(tiles.map((t) => [t.id, tileTitle(t, tables)])),
    base,
    cross,
    filters,
  };
}

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

export async function buildHtml(payload: ExportPayload): Promise<string> {
  const assets = await api.viewer();
  // A "</script" inside the data would end the script element early.
  const data = JSON.stringify(payload).replace(/</g, '\\u003c');
  const js = assets.js.replace(/<\/script/gi, '<\\/script');
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<meta name="generator" content="Tenun">
<title>${esc(payload.title)}</title>
<style>${assets.css}</style>
</head>
<body>
<div id="app"></div>
<script>window.TENUN = ${data};</script>
<script type="module">${js}</script>
</body>
</html>
`;
}
