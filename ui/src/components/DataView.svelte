<script lang="ts">
  import { api, type Cell, type Column, type Profile, type TableSetup, type UnpivotPlan } from '../lib/api';
  import { typeIcon } from '../lib/ui';
  import { app, go } from '../lib/state.svelte';
  import { cell, full } from '../lib/format';
  import { failedText, hasUnpivot, isEmptySetup, withHeaderRow, withUnpivot, withoutUnpivot } from '../lib/prep';
  import Icon from './Icon.svelte';
  import ColumnMenu from './data/ColumnMenu.svelte';
  import ProfileCard from './data/ProfileCard.svelte';
  import UnpivotSheet from './data/UnpivotSheet.svelte';

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

  // --- Data prep (build 08) ---------------------------------------------------------------
  const setup = $derived<TableSetup | undefined>(table ? app.snap!.workspace.tables?.[table.id] : undefined);
  const isExcel = $derived(sourceOf?.kind === 'excel');
  const turned = $derived(hasUnpivot(setup));
  /** Changes when the table's data does, so a cached profile or a detected wide sheet is read again. */
  const version = $derived(`${table?.id}|${app.snap?.loaded_at?.[table?.source_id ?? '']}|${app.dataVersion}`);

  async function save(next: TableSetup) {
    if (!table) return;
    await app.run(null, () => api.setTableSetup(table.id, isEmptySetup(next) ? null : next));
  }

  function setHeaderRow(e: Event) {
    const n = Number((e.currentTarget as HTMLInputElement).value);
    if (!table || !Number.isFinite(n) || n < 1 || n === table.header_row) return;
    save(withHeaderRow(setup, n));
  }

  let plan = $state<UnpivotPlan | null>(null);
  let unpivotOpen = $state(false);
  let prepareOpen = $state(false);
  $effect(() => {
    const id = table?.id;
    void version;
    plan = null;
    if (!id || !table?.prep || turned) return;
    api
      .unpivotPlan(id)
      .then((p) => {
        if (table?.id === id) plan = p;
      })
      .catch(() => {});
  });

  function turnIntoRows(name: string, value: string) {
    if (!plan) return;
    unpivotOpen = false;
    save(withUnpivot(setup, plan, name, value));
  }

  // The notes: what was detected, then the cells a type override could not read.
  const problems = $derived((table?.columns ?? []).filter((c) => c.failed).map((c) => `${c.label ?? c.name}: ${failedText(c)}`));

  // Column menu and hover profile.
  let menu = $state<{ column: string; x: number; y: number } | null>(null);
  const menuColumn = $derived(table?.columns.find((c) => c.name === menu?.column));
  const colOf = (name: string) => table?.columns.find((c) => c.name === name);
  const profiles = new Map<string, Profile>();
  let hover = $state<{ column: string; x: number; y: number; profile: Profile | null } | null>(null);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;

  function enter(e: MouseEvent, name: string) {
    clearTimeout(hoverTimer);
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const id = table?.id;
    if (!id || menu) return;
    const key = `${version}|${name}`;
    hoverTimer = setTimeout(async () => {
      const x = Math.min(r.left, innerWidth - 256);
      hover = { column: name, x, y: r.bottom + 6, profile: profiles.get(key) ?? null };
      if (hover.profile) return;
      try {
        const p = await api.profile(id, name);
        profiles.set(key, p);
        if (hover?.column === name) hover = { ...hover, profile: p };
      } catch {
        hover = null;
      }
    }, 350);
  }

  function leave() {
    clearTimeout(hoverTimer);
    hover = null;
  }

  function openMenu(e: MouseEvent, name: string) {
    leave();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { column: name, x: Math.min(r.left, innerWidth - 270), y: r.bottom + 4 };
  }
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
          <div class="title">
            <h1>{table.label}</h1>
            <p class="muted">
              {kindLabel[sourceOf?.kind ?? ''] ?? ''} · {sourceOf?.name}
              · {table.rows === null ? 'queried live' : `${full(table.rows)} rows`}
              · {table.columns.length} columns
            </p>
            {#if table.notes?.length || problems.length}
              <ul class="notes">
                {#each table.notes ?? [] as n (n)}<li><Icon name="info" size={12} />{n}</li>{/each}
                {#each problems as n (n)}<li class="bad"><Icon name="warning" size={12} />{n}</li>{/each}
              </ul>
            {/if}
          </div>
          {#if table.prep}
            <div class="tools">
              {#if isExcel}
                <label class="hdr" title="The sheet row that holds the column names">
                  <span class="label">Header row</span>
                  <input
                    class="field num-box"
                    type="number"
                    min="1"
                    step="1"
                    value={table.header_row ?? ''}
                    onchange={setHeaderRow}
                    onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} />
                </label>
                {#if setup?.header_row}
                  <button class="btn plain" onclick={() => save(withHeaderRow(setup, undefined))}>Detect it again</button>
                {/if}
              {/if}
              {#if plan || turned}
                <div class="prep">
                  <button class="btn" aria-expanded={prepareOpen} onclick={() => (prepareOpen = !prepareOpen)}>Prepare <Icon name="chevronDown" size={12} /></button>
                  {#if prepareOpen}
                    <div class="pm" role="menu">
                      {#if turned}
                        <button role="menuitem" onclick={() => ((prepareOpen = false), save(withoutUnpivot(setup)))}>Put the columns back</button>
                      {:else}
                        <button role="menuitem" onclick={() => ((prepareOpen = false), (unpivotOpen = true))}>Turn these columns into rows</button>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          {/if}
        </div>
        {#if plan && !turned}
          <div class="offer">
            <Icon name="sparkles" size={14} />
            <span>{plan.columns.length} columns look like {plan.name.toLowerCase() === 'period' ? 'periods' : `${plan.name.toLowerCase()}s`} across the top.</span>
            <button class="btn" onclick={() => (unpivotOpen = true)}>Turn these columns into rows</button>
          </div>
        {:else if turned}
          <div class="offer">
            <Icon name="check" size={14} />
            <span>Columns were turned into rows. Your file is not changed.</span>
            <button class="btn" onclick={() => save(withoutUnpivot(setup))}>Put the columns back</button>
          </div>
        {/if}
        <div class="cols">
          {#each table.columns as c}
            <span class="col" class:hid={c.hidden} title={c.description ?? c.raw_type}><Icon name={typeIcon[c.dtype]} size={12} />{c.label ?? c.name}{#if c.hidden}<em>hidden</em>{/if}</span>
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
                  {#each preview.columns as c (c.name)}
                    {@const col = colOf(c.name)}
                    <th class:num={c.dtype === 'number'} class:hid={col?.hidden}>
                      <button class="hd" onclick={(e) => openMenu(e, c.name)} onmouseenter={(e) => enter(e, c.name)} onmouseleave={leave} title="">
                        {col?.label ?? c.name}<Icon name="chevronDown" size={11} />
                      </button>
                    </th>
                  {/each}
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

{#if hover && !menu}
  {@const hc = colOf(hover.column)}
  {#if hc}<ProfileCard column={hc} profile={hover.profile} x={hover.x} y={hover.y} />{/if}
{/if}

{#if menu && menuColumn && table}
  <ColumnMenu
    column={menuColumn}
    {setup}
    canRetype={!!table.prep}
    x={menu.x}
    y={menu.y}
    onsave={save}
    onclose={() => (menu = null)} />
{/if}

{#if unpivotOpen && plan && table}
  <UnpivotSheet tableId={table.id} {plan} onapply={turnIntoRows} onclose={() => (unpivotOpen = false)} />
{/if}

<svelte:window onmousedown={(e) => prepareOpen && !(e.target as HTMLElement).closest('.prep') && (prepareOpen = false)} />

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
  .head { display: flex; align-items: flex-start; gap: 16px; }
  .title { flex: 1; min-width: 0; }
  .head h1 { font-size: 22px; }
  .head p { margin: 4px 0 0; }
  .notes { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-direction: column; gap: 2px; font-size: 12px; color: var(--text-2); }
  .notes li { display: flex; align-items: center; gap: 6px; }
  .notes li.bad { color: var(--danger); }
  .tools { display: flex; align-items: center; gap: 8px; flex: none; }
  .hdr { display: flex; align-items: center; gap: 6px; }
  .num-box { width: 64px; height: 28px; padding: 0 8px; }
  .prep { position: relative; }
  .pm {
    position: absolute;
    top: 32px;
    right: 0;
    z-index: 30;
    min-width: 220px;
    padding: 6px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
  }
  .pm button { width: 100%; height: 28px; padding: 0 8px; border: 0; border-radius: 5px; background: none; text-align: left; font-size: 12px; }
  .pm button:hover { background: var(--accent); color: #fff; }
  .offer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-radius: var(--r-card);
    background: var(--selected);
    font-size: 12px;
  }
  .offer :global(svg) { color: var(--accent-text); }
  .offer span { flex: 1; }
  .col.hid, th.hid .hd { opacity: 0.6; }
  .col em { font-style: normal; font-size: 11px; color: var(--text-2); margin-left: 2px; }
  .hd {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 0;
    background: none;
    padding: 0;
    font: inherit;
    font-weight: 600;
    color: inherit;
    cursor: pointer;
  }
  .hd :global(svg) { color: var(--text-3); opacity: 0; }
  .hd:hover :global(svg), .hd:focus-visible :global(svg) { opacity: 1; }
  th.num .hd { flex-direction: row-reverse; }
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
