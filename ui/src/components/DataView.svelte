<script lang="ts">
  import { api, type Cell, type Column } from '../lib/api';
  import { typeIcon } from '../lib/ui';
  import { app, go } from '../lib/state.svelte';
  import { cell, full } from '../lib/format';
  import Icon from './Icon.svelte';

  let { tableId }: { tableId?: string } = $props();

  const sources = $derived(app.snap!.workspace.sources);
  const errors = $derived(app.snap!.errors);
  const table = $derived(tableId ? app.table(tableId) : app.tables[0]);

  let preview = $state<{ columns: Column[]; rows: Cell[][] } | null>(null);
  let previewError = $state('');

  $effect(() => {
    const id = table?.id;
    void app.dataVersion;
    preview = null;
    previewError = '';
    if (!id) return;
    api
      .preview(id)
      .then((p) => {
        if (table?.id === id) preview = p;
      })
      .catch((e) => (previewError = e.message));
  });

  const kindLabel: Record<string, string> = {
    excel: 'Excel',
    csv: 'CSV',
    parquet: 'Parquet',
    sqlite: 'SQLite',
    duckdb: 'DuckDB',
    postgres: 'PostgreSQL',
    mysql: 'MySQL',
  };

  async function remove(id: string, name: string) {
    if (!confirm(`Remove ${name}? Its relationships and the tiles that use it will stop working.`)) return;
    if (await app.run(null, () => api.removeSource(id))) go({ view: 'data' });
  }

  // Identifiers read better without thousands separators.
  const isId = (name: string) => /(^|[ _-])(id|code|no|key)$|id$/i.test(name);

  const sourceOf = $derived(sources.find((s) => s.id === table?.source_id));
</script>

<header class="toolbar">
  <h2>Data</h2>
  <span class="spacer"></span>
  <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data</button>
</header>

{#if !sources.length}
  <div class="empty">
    <Icon name="sheet" size={32} />
    <h2>No data yet</h2>
    <p class="muted">Add an Excel file, a CSV file or a database to start.</p>
    <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data</button>
  </div>
{:else}
  <div class="split">
    <aside class="sources">
      {#each sources as s (s.id)}
        <section>
          <div class="src">
            <Icon name={s.kind === 'postgres' || s.kind === 'mysql' || s.kind === 'sqlite' || s.kind === 'duckdb' ? 'database' : 'sheet'} size={15} />
            <span class="src-name" title={s.path ?? s.host}>{s.name}</span>
            <span class="kind">{kindLabel[s.kind]}</span>
            <button class="btn plain icon tiny" title="Reload" onclick={() => app.run(null, () => api.reloadSource(s.id))}><Icon name="refresh" size={13} /></button>
            <button class="btn plain icon tiny" title="Remove" onclick={() => remove(s.id, s.name)}><Icon name="trash" size={13} /></button>
          </div>
          {#if errors[s.id]}
            <p class="err"><Icon name="warning" size={13} /> {errors[s.id]}</p>
          {/if}
          {#each app.tables.filter((t) => t.source_id === s.id) as t (t.id)}
            <button class="tbl" class:on={table?.id === t.id} onclick={() => go({ view: 'data', table: t.id })}>
              <Icon name="table" size={14} />
              <span class="tname">{t.name}</span>
              <span class="rows">{t.rows === null ? 'live' : full(t.rows)}</span>
            </button>
          {/each}
        </section>
      {/each}
    </aside>

    <section class="detail">
      {#if table}
        <div class="head">
          <div>
            <h1>{table.label}</h1>
            <p class="muted">
              {kindLabel[sourceOf?.kind ?? ''] ?? ''} · {sourceOf?.name}
              · {table.rows === null ? 'queried live' : `${full(table.rows)} rows`}
              · {table.columns.length} columns
            </p>
          </div>
        </div>
        <div class="cols">
          {#each table.columns as c}
            <span class="col" title={c.raw_type}><Icon name={typeIcon[c.dtype]} size={12} />{c.name}</span>
          {/each}
        </div>
        <div class="grid-wrap">
          {#if previewError}
            <p class="err pad">{previewError}</p>
          {:else if !preview}
            <p class="muted pad">Loading…</p>
          {:else}
            <table>
              <thead>
                <tr>
                  {#each preview.columns as c}<th class:num={c.dtype === 'number'}>{c.name}</th>{/each}
                </tr>
              </thead>
              <tbody>
                {#each preview.rows as r}
                  <tr>
                    {#each r as v, i}
                      <td class:num={preview.columns[i].dtype === 'number'} class:blank={v === null}>
                        {v === null ? '' : preview.columns[i].dtype === 'number' && !isId(preview.columns[i].name) ? cell(v, 'number') : String(v)}
                      </td>
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
            {#if table.rows !== null && table.rows > preview.rows.length}
              <p class="muted pad small">Showing the first {preview.rows.length} of {full(table.rows)} rows.</p>
            {/if}
          {/if}
        </div>
      {:else}
        <p class="muted pad">This source has no tables.</p>
      {/if}
    </section>
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 52px;
    padding: 0 20px;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    flex: none;
  }
  .spacer { flex: 1; }
  .split { flex: 1; min-height: 0; display: grid; grid-template-columns: 280px 1fr; }
  .sources {
    overflow: auto;
    padding: 12px 10px;
    border-right: 1px solid var(--sep);
    background: var(--surface);
  }
  section + section { margin-top: 12px; }
  .src {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    font-weight: 600;
  }
  .src :global(svg) { color: var(--text-2); }
  .src-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .kind { font-size: 11px; font-weight: 500; color: var(--text-2); margin-right: auto; white-space: nowrap; }
  .tiny { width: 22px; height: 22px; color: var(--text-2); }
  .tbl {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px 0 24px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    text-align: left;
  }
  .tbl :global(svg) { color: var(--accent-text); }
  .tbl:hover { background: var(--hover); }
  .tbl.on { background: var(--selected); }
  .tname { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rows { font-size: 11px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .err { display: flex; gap: 6px; margin: 4px 6px 6px; color: var(--danger); font-size: 12px; }
  .detail { min-width: 0; display: flex; flex-direction: column; padding: 20px 24px; gap: 12px; overflow: hidden; }
  .head h1 { font-size: 22px; }
  .head p { margin: 4px 0 0; }
  .cols { display: flex; flex-wrap: wrap; gap: 6px; }
  .col {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border-radius: 12px;
    background: var(--surface);
    box-shadow: var(--shadow-tile);
    font-size: 12px;
  }
  .col :global(svg) { color: var(--text-2); }
  .grid-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border-radius: var(--r-card);
    background: var(--surface);
    box-shadow: var(--shadow-tile);
  }
  table { border-collapse: collapse; font-size: 12px; min-width: 100%; }
  th {
    position: sticky;
    top: 0;
    background: var(--surface);
    text-align: left;
    font-weight: 600;
    padding: 8px 12px;
    border-bottom: 1px solid var(--sep);
    white-space: nowrap;
  }
  td {
    padding: 6px 12px;
    border-bottom: 1px solid var(--sep);
    white-space: nowrap;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  tbody tr:hover td { background: var(--window); }
  .pad { padding: 16px; margin: 0; }
  .small { font-size: 12px; }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .empty :global(svg) { color: var(--accent-text); }
  .empty p { margin: 0 0 8px; }
</style>
