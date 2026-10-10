<script lang="ts">
  // "Show rows": the raw rows behind a tile or one mark of it, in a side sheet. Up to 500 rows.
  import type { Cell, RowsBehind } from '../../lib/api';
  import { cell } from '../../lib/format';
  import { saveBlob } from '../../lib/download';
  import { portal } from '../../lib/portal';
  import { csvCell, fileName } from '../../lib/tabular';
  import Icon from '../Icon.svelte';

  let {
    title,
    data,
    error = '',
    onclose,
  }: {
    title: string;
    /** Null while the rows load. */
    data: RowsBehind | null;
    error?: string;
    onclose: () => void;
  } = $props();

  let sortBy = $state<number | null>(null);
  let desc = $state(false);

  const rows = $derived.by(() => {
    if (!data || sortBy === null) return data?.rows ?? [];
    const i = sortBy;
    const num = data.columns[i]?.dtype === 'number';
    return [...data.rows].sort((a, b) => {
      const x = a[i];
      const y = b[i];
      if (x === y) return 0;
      if (x === null) return 1;
      if (y === null) return -1;
      const c = num ? Number(x) - Number(y) : String(x).localeCompare(String(y), undefined, { numeric: true });
      return desc ? -c : c;
    });
  });

  /** A number that names a thing (an id, a code) is not added up, so it has no thousands marks. */
  const isKey = (label: string) => /(^|[\s_.·-])(id|no|code|key|number)$/i.test(label);
  const show = (v: Cell, c: RowsBehind['columns'][number]) => (c.dtype === 'number' && typeof v === 'number' && isKey(c.label) ? String(v) : cell(v, c.dtype, null));

  function sort(i: number) {
    if (sortBy === i) desc = !desc;
    else {
      sortBy = i;
      desc = false;
    }
  }

  function download() {
    if (!data) return;
    const head = data.columns.map((c) => csvCell(c.label, true)).join(',');
    const body = rows.map((r) => r.map((v, i) => csvCell(v, data!.columns[i].dtype === 'text')).join(','));
    const blob = new Blob(['﻿' + [head, ...body].join('\r\n') + '\r\n'], { type: 'text/csv;charset=utf-8' });
    saveBlob(blob, fileName(title, 'csv', 'rows'));
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="scrim" role="presentation" use:portal onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-label="Rows behind {title}">
    <header>
      <div class="t">
        <h2>Rows</h2>
        <p class="muted">{title}</p>
      </div>
      <button class="btn" onclick={download} disabled={!data || !data.rows.length}><Icon name="export" /> Download CSV</button>
      <button class="btn plain icon" aria-label="Close" onclick={onclose}><Icon name="x" /></button>
    </header>
    {#if error}
      <p class="msg error"><Icon name="warning" size={14} /> {error}</p>
    {:else if !data}
      <p class="msg muted">Loading rows…</p>
    {:else if !data.rows.length}
      <p class="msg muted">No rows match.</p>
    {:else}
      <p class="count muted">
        {#if data.total > data.rows.length}
          Showing the first {data.rows.length.toLocaleString('en-US')} of {data.total.toLocaleString('en-US')} rows.
        {:else}
          {data.total.toLocaleString('en-US')} {data.total === 1 ? 'row' : 'rows'}.
        {/if}
      </p>
      <div class="wrap">
        <table>
          <thead>
            <tr>
              {#each data.columns as c, i}
                <th class:num={c.dtype === 'number'}>
                  <button onclick={() => sort(i)}>{c.label}{sortBy === i ? (desc ? ' ↓' : ' ↑') : ''}</button>
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each rows as r}
              <tr>
                {#each r as v, i}
                  <td class:num={data.columns[i].dtype === 'number'}>{show(v, data.columns[i])}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 55; background: rgb(0 0 0 / 28%); display: flex; justify-content: flex-end; animation: fade 180ms ease-out; }
  .sheet {
    width: min(720px, 100vw);
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    box-shadow: var(--shadow-float);
    animation: slide 250ms var(--spring);
  }
  header { display: flex; align-items: center; gap: 8px; padding: 14px 14px 10px 20px; }
  .t { flex: 1; min-width: 0; }
  h2 { font-size: 17px; }
  .t p { margin: 2px 0 0; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { margin: 0; padding: 0 20px 8px; font-size: 12px; }
  .msg { display: flex; gap: 6px; align-items: center; margin: 0; padding: 12px 20px; font-size: 13px; }
  .msg.error { color: var(--danger); }
  .wrap { flex: 1; min-height: 0; overflow: auto; border-top: 1px solid var(--sep); }
  table { border-collapse: collapse; width: 100%; font-size: 12px; }
  th { position: sticky; top: 0; background: var(--surface); text-align: left; padding: 0; border-bottom: 1px solid var(--sep); }
  th button { width: 100%; border: 0; background: none; padding: 7px 12px; font-weight: 600; text-align: inherit; white-space: nowrap; }
  td { padding: 5px 12px; border-bottom: 1px solid var(--sep); white-space: nowrap; max-width: 280px; overflow: hidden; text-overflow: ellipsis; }
  .num, .num button { text-align: right; font-variant-numeric: tabular-nums; }
  tbody tr:hover td { background: var(--window); }
  @keyframes fade { from { opacity: 0; } }
  @keyframes slide { from { transform: translateX(24px); opacity: 0; } }
</style>
