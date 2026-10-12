<script lang="ts">
  import { api, type Cell, type Column, type NamedMeasure, type Profile, type Source, type TableSetup, type Tile, type TileRef, type UnpivotPlan, type UrlEvery } from '../lib/api';
  import { typeIcon } from '../lib/ui';
  import { app, go } from '../lib/state.svelte';
  import { cell, full, tileTitle } from '../lib/format';
  import { readersOf, tilesUsing, useWords } from '../lib/formula';
  import { failedText, hasUnpivot, isEmptySetup, withCalc, withHeaderRow, withMainDate, withUnpivot, withoutUnpivot } from '../lib/prep';
  import { tilesWith } from '../lib/measures';
  import { looksTabular, sinceWords, EVERY_CHOICES } from '../lib/sources';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import CalcSheet from './data/CalcSheet.svelte';
  import ColumnMenu from './data/ColumnMenu.svelte';
  import MeasureSheet from './data/MeasureSheet.svelte';
  import ProfileCard from './data/ProfileCard.svelte';
  import UnpivotSheet from './data/UnpivotSheet.svelte';
  import PasteSheet from './data/PasteSheet.svelte';
  import LinkSettings from './data/LinkSettings.svelte';

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
    folder: 'Folder',
    url: 'Web link',
    postgres: 'PostgreSQL',
    mysql: 'MySQL',
  };

  async function remove(id: string, name: string) {
    if (!confirm(`Remove ${name}? Its relationships and the tiles that use it will stop working.`)) return;
    if (await app.run(null, () => api.removeSource(id))) go({ view: 'data' });
  }

  // Identifiers read better without thousands separators.
  const isId = (name: string) => /(^|[ _-])(id|code|no|key)$|id$/i.test(name);

  const iconOf = (kind: string) =>
    kind === 'folder' ? 'folder' : kind === 'url' ? 'link' : ['postgres', 'mysql', 'sqlite', 'duckdb'].includes(kind) ? 'database' : 'sheet';

  async function setEvery(id: string, every: UrlEvery) {
    await app.run(null, () => api.setUrlEvery(id, every));
  }

  // Link settings (build 15): the records, the pages and the key of a web link.
  let linkSource = $state<Source | null>(null);

  // --- Paste a table (build 09) -------------------------------------------------------------
  // The paste event carries the text in the browser and in the desktop window. Where a window does
  // not send one for Ctrl/Cmd+V, the clipboard is read directly (it may ask for permission).
  let pasted = $state<string | null>(null);
  let gotPaste = false;

  const editable = (t: EventTarget | null) => {
    const el = t as HTMLElement | null;
    return !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
  };
  const dialogOpen = () => !!document.querySelector('[role="dialog"]');

  function onpaste(e: ClipboardEvent) {
    gotPaste = true;
    if (pasted !== null || editable(e.target) || dialogOpen()) return;
    const text = e.clipboardData?.getData('text/plain') ?? '';
    if (looksTabular(text)) {
      e.preventDefault();
      pasted = text;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey || e.key.toLowerCase() !== 'v') return;
    if (pasted !== null || editable(e.target) || dialogOpen()) return;
    gotPaste = false;
    setTimeout(() => !gotPaste && readClipboard(false), 150);
  }

  async function readClipboard(announce: boolean) {
    try {
      const text = await navigator.clipboard.readText();
      if (looksTabular(text)) pasted = text;
      else if (announce) app.toast('There is no table on the clipboard. Copy some cells in Excel or Google Sheets first.');
    } catch {
      if (announce) app.toast('Tenun cannot read the clipboard from here. Copy the cells, then press Ctrl+V (Cmd+V on a Mac).', 'error');
    }
  }

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

  // The notes: what was detected, then the cells a type override could not read, then the
  // calculated columns that could not be computed.
  const problems = $derived([
    ...(table?.columns ?? []).filter((c) => c.failed).map((c) => `${c.label ?? c.name}: ${failedText(c)}`),
    ...(table?.columns ?? []).filter((c) => c.error).map((c) => `${c.label ?? c.name} could not be computed, so its values are blank. ${c.error}`),
  ]);

  // --- Calculated columns (build 02) ---------------------------------------------------------
  // Advanced (V4-D20). A dashboard that uses a calculated column opens and works with the switch off (V3-D2).
  const canCalc = $derived(app.settings?.advanced === true);
  let calcSheet = $state<{ editing?: string } | null>(null);
  /** The column to delete and what it takes with it, while the question is open. */
  let askDelete = $state<{ name: string; shown: string; tiles: string[]; blank: { label: string; tiles: string[] }[] } | null>(null);
  /** Below 720 px the list of sources is above the table and folded away (V4-D76). */
  let sourcesOpen = $state(false);

  /** The tip of a column: its description, what it is, and the day a calculated one is as of. */
  function tip(c: Column): string {
    const parts = [c.description || (c.calc ? '' : c.raw_type)];
    if (c.calc) parts.push(c.error ? 'Calculated column. It could not be computed.' : 'Calculated column.');
    if (c.as_of) parts.push(`As of ${c.as_of}.`);
    return parts.filter(Boolean).join(' ');
  }

  function wantDelete(name: string) {
    if (!table) return;
    const shown = colOf(name)?.label ?? name;
    const title = (t: Tile) => tileTitle(t, app.tables);
    const direct = tilesUsing(app.dashboards, table.id, name, title);
    // The calculated columns that read it, and the ones that read those, go blank with it. What
    // matters is the tiles that show them.
    const links = app.snap!.workspace.relationships.filter((r) => r.to.table === table.id && r.name).map((r) => r.name!);
    const readers = readersOf(app.snap!.workspace.tables ?? {}, table, name, (id) => app.table(id)?.label ?? id, links);
    const blank = readers.map((r) => ({
      label: `${r.label} › ${r.column}`,
      tiles: tilesUsing(app.dashboards, r.table, r.column, title).map(useWords),
    }));
    // Nothing shows it, directly or through another column: it goes without a question.
    if (!direct.length && !blank.some((b) => b.tiles.length)) removeCalc(name);
    else askDelete = { name, shown, tiles: direct.map(useWords), blank };
  }

  async function removeCalc(name: string) {
    askDelete = null;
    if (!table) return;
    await save(withCalc(setup, (setup?.calc ?? []).filter((c) => c.name !== name)));
  }

  // --- Measures (build 03) -------------------------------------------------------------------
  // Advanced (V4-D20). A dashboard that uses a measure opens and works with the switch off (V3-D2).
  const measures = $derived(app.snap!.workspace.measures ?? []);
  const measureInfo = $derived(app.snap!.measure_info ?? {});
  let measureSheet = $state<{ editing?: string } | null>(null);
  /** The measure to delete and the tiles that use it, while the question is open. */
  let askMeasure = $state<{ id: string; name: string; tiles: TileRef[] } | null>(null);

  const usedIn = (m: NamedMeasure) => tilesWith(app.dashboards, m.id).length;

  /** What a tile is called: its title, or the words it is given when it has none. */
  function tileName(r: TileRef): string {
    const t = app.dashboards.find((d) => d.id === r.dashboard)?.tiles.find((x) => x.id === r.tile);
    const title = t ? tileTitle(t, app.tables) : r.title;
    return `${title.trim() ? title : `A ${r.kind} tile`} on ${r.dashboardTitle || 'a dashboard'}`;
  }

  async function removeMeasure(m: NamedMeasure, confirmed = false) {
    app.busy = 'Saving';
    try {
      const r = await api.setMeasures(
        measures.filter((x) => x.id !== m.id),
        confirmed,
      );
      if ('saved' in r) {
        app.snap = r.saved;
        askMeasure = null;
      } else {
        askMeasure = { id: m.id, name: m.name, tiles: r.used.find((u) => u.measure === m.id)?.tiles ?? [] };
      }
    } catch (e) {
      app.error(e);
    } finally {
      app.busy = null;
    }
  }

  // The main date: the date a measure uses to line this table up with the others over time (V4-D5).
  const dateColumns = $derived((table?.columns ?? []).filter((c) => c.dtype === 'date' || c.dtype === 'datetime'));
  const readByMeasure = $derived(!!table && Object.values(measureInfo).some((i) => i.tables.includes(table.id)));
  const showMainDate = $derived(dateColumns.length >= 2 || (readByMeasure && dateColumns.length >= 1));

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
  <button class="btn" title="Paste cells from Excel or Google Sheets as a new table (Ctrl+V)" onclick={() => readClipboard(true)}><Icon name="table" /> Paste a table</button>
  <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data</button>
</header>

{#if !sources.length}
  <div class="empty">
    <Icon name="sheet" size={32} />
    <h2>No data yet</h2>
    <p class="muted">Add an Excel file, a CSV file, a folder of files, a link or a database to start. You can also paste cells from Excel.</p>
    <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data</button>
  </div>
{:else}
  <div class="split">
    <aside class="sources" class:open={sourcesOpen}>
      <button class="sources-toggle" aria-expanded={sourcesOpen} onclick={() => (sourcesOpen = !sourcesOpen)}>
        <Icon name="table" size={14} />
        <span class="now">{table?.label ?? 'Sources'}</span>
        <span class="count">{sources.length} {sources.length === 1 ? 'source' : 'sources'}</span>
        <Icon name="chevronDown" size={12} />
      </button>
      <div class="sources-list">
      {#each sources as s (s.id)}
        <section>
          <div class="src">
            <Icon name={iconOf(s.kind)} size={15} />
            <span class="src-name" title={s.path ?? s.host}>{s.name}</span>
            <span class="kind">{kindLabel[s.kind]}</span>
            <button class="btn plain icon tiny" title={s.kind === 'url' ? 'Fetch the link now' : 'Reload'} onclick={() => app.run(null, () => api.reloadSource(s.id))}><Icon name="refresh" size={13} /></button>
            <button class="btn plain icon tiny" title="Remove" onclick={() => remove(s.id, s.name)}><Icon name="trash" size={13} /></button>
          </div>
          {#if s.kind === 'url'}
            <p class="srcline">
              {#if app.snap?.loaded_at?.[s.id]}Fetched {sinceWords(app.snap.loaded_at[s.id])}{:else}Not fetched yet{/if}
              <select class="every" aria-label="Refresh" value={s.every ?? 'hourly'} onchange={(e) => setEvery(s.id, e.currentTarget.value as UrlEvery)}>
                {#each EVERY_CHOICES as c}<option value={c.value}>{c.label}</option>{/each}
              </select>
              <button class="linkbtn" onclick={() => (linkSource = s)}>Link settings</button>
            </p>
          {:else if s.kind === 'folder'}
            <p class="srcline" title={s.path}>Reads {s.pattern}</p>
          {/if}
          {#if errors[s.id]}
            <p class="err"><Icon name="warning" size={13} /> {errors[s.id]}</p>
          {/if}
          <div class="tbls">
            {#each app.tables.filter((t) => t.source_id === s.id) as t (t.id)}
              <button class="tbl" class:on={table?.id === t.id} onclick={() => ((sourcesOpen = false), go({ view: 'data', table: t.id }))}>
                <Icon name="table" size={14} />
                <span class="tname">{t.name}</span>
                <span class="rows">{t.rows === null ? 'live' : full(t.rows)}</span>
              </button>
            {/each}
          </div>
        </section>
      {/each}
      {#if canCalc || measures.length}
        <section class="measures" aria-label="Measures">
          <div class="src">
            <Icon name="fx" size={15} />
            <span class="src-name">Measures</span>
            <span class="kind">{measures.length ? `${measures.length}` : ''}</span>
          </div>
          {#each measures as m (m.id)}
            {@const info = measureInfo[m.id]}
            <div class="msr" class:bad={!!info?.error}>
              <button class="msr-main" onclick={() => canCalc && (measureSheet = { editing: m.id })} disabled={!canCalc} title={info?.error ?? m.description ?? m.formula}>
                <span class="mname">{m.name}{#if info?.error}<Icon name="warning" size={12} />{/if}</span>
                <span class="mformula">{m.formula}</span>
                <span class="muse">{info?.error ? 'Does not work now' : usedIn(m) ? `Used in ${usedIn(m)} ${usedIn(m) === 1 ? 'tile' : 'tiles'}` : 'Not used yet'}</span>
              </button>
              {#if canCalc}
                <button class="btn plain icon tiny" title="Delete {m.name}" aria-label="Delete {m.name}" onclick={() => removeMeasure(m)}><Icon name="trash" size={13} /></button>
              {/if}
            </div>
          {:else}
            <p class="srcline mhint">A measure is a formula of totals, like SUM([Sales].[amount]) / SUM([Targets].[target]). Any tile can use it.</p>
          {/each}
          {#if canCalc}
            <button class="btn newm" onclick={() => (measureSheet = {})}><Icon name="plus" size={13} /> New measure</button>
          {/if}
        </section>
      {/if}
      </div>
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
          {#if table.prep || canCalc || showMainDate}
            <div class="tools">
              {#if showMainDate}
                <label class="hdr" title="The date a measure uses to line this table up with the others over time">
                  <span class="label">Main date</span>
                  <select
                    class="field"
                    aria-label="Main date"
                    value={setup?.mainDate ?? ''}
                    onchange={(e) => save(withMainDate(setup, e.currentTarget.value || undefined))}>
                    <option value="">{dateColumns.length === 1 ? `${dateColumns[0].label ?? dateColumns[0].name} (the only date)` : 'Not set'}</option>
                    {#each dateColumns as c (c.name)}<option value={c.name}>{c.label ?? c.name}</option>{/each}
                  </select>
                </label>
              {/if}
              {#if canCalc}
                <button class="btn" title="Add a column that is computed from the others" onclick={() => (calcSheet = {})}><Icon name="fx" size={13} /> Add column</button>
              {/if}
              {#if table.prep && isExcel}
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
              {#if table.prep && (plan || turned)}
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
            <span class="col" class:hid={c.hidden} class:calc={c.calc} class:err={!!c.error} title={tip(c)}>
              <Icon name={typeIcon[c.dtype]} size={12} />{c.label ?? c.name}{#if c.calc}<span class="fx" aria-label="Calculated column"><Icon name="fx" size={11} /></span>{/if}{#if c.hidden}<em>hidden</em>{/if}
            </span>
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
                        {#if col?.calc}<span class="fx" aria-label="Calculated column"><Icon name="fx" size={11} /></span>{/if}{col?.label ?? c.name}<Icon name="chevronDown" size={11} />
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
    canRetype={!!table.prep && !menuColumn.calc}
    {table}
    x={menu.x}
    y={menu.y}
    onsave={save}
    onclose={() => (menu = null)}
    onedit={canCalc ? (name) => (calcSheet = { editing: name }) : undefined}
    ondelete={canCalc ? (name) => wantDelete(name) : undefined} />
{/if}

{#if calcSheet && table}
  <CalcSheet {table} {setup} editing={calcSheet.editing} onclose={() => (calcSheet = null)} />
{/if}

{#if measureSheet}
  <MeasureSheet editing={measureSheet.editing} onclose={() => (measureSheet = null)} />
{/if}

{#if askMeasure}
  {@const d = askMeasure}
  <Modal title="Delete {d.name}?" width={460} onclose={() => (askMeasure = null)}>
    <p class="lead">
      {d.tiles.length === 1 ? 'This tile uses' : `These ${d.tiles.length} tiles use`} the measure. {d.tiles.length === 1 ? 'It' : 'They'} will show an error until you pick another measure.
    </p>
    <ul class="uses">
      {#each d.tiles as t, i (i)}<li>{tileName(t)}</li>{/each}
    </ul>
    {#snippet footer()}
      <button class="btn" onclick={() => (askMeasure = null)}>Keep the measure</button>
      <button class="btn danger" onclick={() => removeMeasure({ id: d.id, name: d.name } as NamedMeasure, true)}>Delete measure</button>
    {/snippet}
  </Modal>
{/if}

{#if askDelete}
  {@const d = askDelete}
  <Modal title="Delete {d.shown}?" width={460} onclose={() => (askDelete = null)}>
    {#if d.tiles.length}
      <p class="lead">
        {d.tiles.length === 1 ? 'This tile uses' : `These ${d.tiles.length} tiles use`} the column. {d.tiles.length === 1 ? 'It' : 'They'} will show an error until you pick another field.
      </p>
      <ul class="uses">
        {#each d.tiles as t, i (i)}<li>{t}</li>{/each}
      </ul>
    {/if}
    {#if d.blank.length}
      <p class="lead">
        {d.blank.length === 1 ? 'A calculated column reads it' : `${d.blank.length} calculated columns read it`} and will be blank:
      </p>
      <ul class="uses">
        {#each d.blank as b, i (i)}
          <li>
            <strong>{b.label}</strong>
            {#if b.tiles.length}
              is used by
              <ul class="uses">
                {#each b.tiles as t, j (j)}<li>{t}</li>{/each}
              </ul>
            {:else}
              is not used by a tile.
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => (askDelete = null)}>Keep the column</button>
      <button class="btn danger" onclick={() => removeCalc(d.name)}>Delete column</button>
    {/snippet}
  </Modal>
{/if}

{#if pasted !== null}
  <PasteSheet text={pasted} onclose={() => (pasted = null)} />
{/if}

{#if linkSource}
  <LinkSettings source={linkSource} onclose={() => (linkSource = null)} />
{/if}

{#if unpivotOpen && plan && table}
  <UnpivotSheet tableId={table.id} {plan} onapply={turnIntoRows} onclose={() => (unpivotOpen = false)} />
{/if}

<svelte:window {onpaste} {onkeydown} onmousedown={(e) => prepareOpen && !(e.target as HTMLElement).closest('.prep') && (prepareOpen = false)} />

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
  .srcline { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; margin: 2px 6px 4px 28px; color: var(--text-2); font-size: 11px; }
  .every { height: 24px; max-width: 170px; padding: 0 6px; border: 1px solid var(--sep-strong); border-radius: 5px; background: var(--inset); color: var(--text); font-size: 11px; }
  .every:focus { outline: none; border-color: var(--focus); }
  .linkbtn { border: 0; background: none; padding: 0; color: var(--accent-text); font-size: 11px; cursor: pointer; }
  .linkbtn:hover { text-decoration: underline; }
  .err { display: flex; gap: 6px; margin: 4px 6px 6px; color: var(--danger); font-size: 12px; }
  .detail { min-width: 0; display: flex; flex-direction: column; padding: 20px 24px; gap: 12px; overflow: hidden; }
  .head { display: flex; align-items: flex-start; gap: 8px 16px; flex-wrap: wrap; }
  .title { flex: 1 1 240px; min-width: 0; }
  .head h1 { font-size: 22px; }
  .head p { margin: 4px 0 0; }
  .notes { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-direction: column; gap: 2px; font-size: 12px; color: var(--text-2); }
  .notes li { display: flex; align-items: center; gap: 6px; }
  .notes li.bad { color: var(--danger); }
  .tools { display: flex; align-items: center; gap: 8px; flex: none; flex-wrap: wrap; }
  .hdr { display: flex; align-items: center; gap: 6px; }
  .hdr .label { white-space: nowrap; }
  .num-box { width: 64px; height: 28px; padding: 0 8px; border-color: var(--sep-strong); }
  .num-box:focus { border-color: var(--focus); }
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
  .col.calc { background: var(--selected); }
  .col.err { box-shadow: inset 0 0 0 1px var(--danger); }
  .fx { display: inline-flex; align-items: center; color: var(--accent-text); }
  .hd .fx { margin-right: 4px; }
  .col .fx :global(svg), .hd .fx :global(svg) { color: var(--accent-text); opacity: 1; }
  .col .fx { margin-left: 1px; }
  .measures { margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--sep); }
  .msr { display: flex; align-items: flex-start; gap: 2px; border-radius: var(--r-control); }
  .msr:hover { background: var(--hover); }
  .msr-main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 1px;
    padding: 5px 8px 5px 28px;
    border: 0;
    background: none;
    text-align: left;
    color: inherit;
    cursor: pointer;
  }
  .msr-main:disabled { cursor: default; }
  .msr .tiny { margin-top: 5px; }
  .mname { display: flex; align-items: center; gap: 5px; font-weight: 600; min-width: 0; }
  .msr.bad .mname { color: var(--danger); }
  .mformula { font: 11px/16px var(--mono); color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .muse { font-size: 11px; color: var(--text-2); }
  .mhint { margin: 2px 8px 6px 28px; display: block; }
  .newm { margin: 6px 6px 0 28px; }
  .lead { margin: 0 0 10px; }
  .uses { margin: 0 0 10px; padding-left: 18px; font-size: 13px; }
  .uses li { margin: 2px 0; }
  .uses .uses { margin: 2px 0 4px; font-size: 12px; color: var(--text-2); }

  /* The list of sources: beside the table above 720 px, above it and folded below (V4-D76). */
  .sources-toggle { display: none; }
  .tbls { display: block; }
  @media (max-width: 719.98px) {
    .split { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
    .sources { border-right: 0; border-bottom: 1px solid var(--sep); padding: 8px 12px; max-height: 50vh; display: flex; flex-direction: column; }
    .sources-toggle {
      display: flex;
      align-items: center;
      gap: 8px;
      width: 100%;
      height: 32px;
      padding: 0 8px;
      border: 0;
      border-radius: var(--r-control);
      background: var(--inset);
      text-align: left;
    }
    .sources-toggle :global(svg) { color: var(--text-2); flex: none; }
    .sources-toggle .now { flex: 1; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
    .sources-toggle .count { color: var(--text-2); font-size: 12px; white-space: nowrap; }
    .sources.open .sources-toggle :global(svg:last-child) { transform: rotate(180deg); }
    .sources-list { display: none; }
    .sources.open .sources-list { display: block; overflow: auto; margin-top: 8px; min-height: 0; }
    .tbls { display: flex; flex-wrap: wrap; gap: 6px; padding: 2px 6px 4px 28px; }
    .tbl { width: auto; height: 26px; padding: 0 10px; border-radius: 13px; background: var(--window); }
    .tbl.on { background: var(--selected); }
    .detail { padding: 14px 14px 0; }
    .title { flex: 1 1 100%; }
    .tools { flex: 1 1 100%; }
    .toolbar { padding: 0 14px; }
  }
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
