<script lang="ts">
  import { api, type Cell, type Column, type NamedMeasure, type Param, type Profile, type Source, type TableSetup, type Tile, type TileRef, type UnpivotPlan, type UrlEvery } from '../lib/api';
  import { typeIcon } from '../lib/ui';
  import { fixedWord, t, tn } from '../lib/i18n';
  import { app, go } from '../lib/state.svelte';
  import { cell, full, numberFormatter, tileTitle } from '../lib/format';
  import { readersOf, tilesUsing, useWords } from '../lib/formula';
  import { failedText, hasUnpivot, isEmptySetup, withCalc, withHeaderRow, withMainDate, withUnpivot, withoutUnpivot } from '../lib/prep';
  import { columnFormat, tilesWith } from '../lib/measures';
  import { looksTabular, sinceWords, EVERY_CHOICES } from '../lib/sources';
  import { fromWords } from '../lib/derived';
  import NewTableSheet from './data/NewTableSheet.svelte';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import CalcSheet from './data/CalcSheet.svelte';
  import ColumnMenu from './data/ColumnMenu.svelte';
  import MeasureSheet from './data/MeasureSheet.svelte';
  import ParamSheet from './data/ParamSheet.svelte';
  import ProfileCard from './data/ProfileCard.svelte';
  import UnpivotSheet from './data/UnpivotSheet.svelte';
  import PasteSheet from './data/PasteSheet.svelte';
  import LinkSettings from './data/LinkSettings.svelte';

  let { tableId }: { tableId?: string } = $props();

  const sources = $derived(app.snap!.workspace.sources);
  const errors = $derived(app.snap!.errors);
  const table = $derived(tableId ? app.table(tableId) : app.tables[0]);
  // A cell of the grid: in its column's format and date pattern when the column has them (formats §2.3); an id
  // keeps its plain digits unless its column says otherwise.
  const gridText = (v: Cell, i: number): string => {
    const c = preview?.columns[i];
    if (!c) return String(v ?? '');
    const own = columnFormat(table?.id, c.name);
    if (v === null) return own?.format?.blank ?? '';
    if (own && (c.dtype === 'number' || c.dtype === 'date' || c.dtype === 'datetime')) return cell(v, c.dtype, null, false, null, null, { table: table?.id, column: c.name });
    return c.dtype === 'number' && !isId(c.name) ? cell(v, 'number') : String(v);
  };

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
    folder: t('Folder'),
    url: t('Web link'),
    postgres: 'PostgreSQL',
    mysql: 'MySQL',
    derived: t('New table'),
  };
  /** The new table whose sheet is open, to change how it is made (build 04). */
  let editingDerived = $state<string | null>(null);

  async function remove(id: string, name: string) {
    if (!confirm(t('Remove {name}? Its relationships and the tiles that use it will stop working.', { name }))) return;
    if (await app.run(null, () => api.removeSource(id))) go({ view: 'data' });
  }

  // Identifiers read better without thousands separators.
  const isId = (name: string) => /(^|[ _-])(id|code|no|key)$|id$/i.test(name);

  const iconOf = (kind: string) =>
    kind === 'folder' ? 'folder' : kind === 'url' ? 'link' : kind === 'derived' ? 'newtable' : ['postgres', 'mysql', 'sqlite', 'duckdb'].includes(kind) ? 'database' : 'sheet';

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
      else if (announce) app.toast(t('There is no table on the clipboard. Copy some cells in Excel or Google Sheets first.'));
    } catch {
      if (announce) app.toast(t('Tenun cannot read the clipboard from here. Copy the cells, then press Ctrl+V (Cmd+V on a Mac).'), 'error');
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
    ...(table?.columns ?? []).filter((c) => c.error).map((c) => t('{name} could not be computed, so its values are blank. {error}', { name: c.label ?? c.name, error: c.error ?? '' })),
  ]);

  // --- Calculated columns (build 02) ---------------------------------------------------------
  // Advanced (V4-D20). A dashboard that uses a calculated column opens and works with the switch off (V3-D2).
  const canCalc = $derived(app.settings?.advanced === true);
  let calcSheet = $state<{ editing?: string } | null>(null);
  /** The column to delete and what it takes with it, while the question is open. */
  let askDelete = $state<{ name: string; shown: string; tiles: string[]; blank: { label: string; tiles: string[] }[] } | null>(null);
  /** Below 720 px the list of sources is above the table and folded away (V4-D76). */
  let sourcesOpen = $state(false);

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
  // Parameters (v5 build 05), Advanced: numbers and choices that measures read with PARAM("…").
  const params = $derived(app.snap!.workspace.params ?? []);
  let paramSheet = $state<{ editing?: string } | null>(null);
  const paramValue = (p: Param) => (typeof p.value === 'number' ? numberFormatter(p.format ?? null).long(p.value) : String(p.value ?? ''));
  const paramRange = (p: Param) =>
    p.kind === 'choice'
      ? t('A choice of {n}', { n: (p.choices ?? []).length })
      : t('{min} to {max}', { min: numberFormatter(p.format ?? null).long(p.min ?? 0), max: numberFormatter(p.format ?? null).long(p.max ?? 0) });
  /** The measure to delete and the tiles that use it, while the question is open. */
  let askMeasure = $state<{ id: string; name: string; tiles: TileRef[] } | null>(null);

  const usedIn = (m: NamedMeasure) => tilesWith(app.dashboards, m.id).length;

  /** What a tile is called: its title, or the words it is given when it has none. */
  function tileName(r: TileRef): string {
    const tl = app.dashboards.find((d) => d.id === r.dashboard)?.tiles.find((x) => x.id === r.tile);
    const title = tl ? tileTitle(tl, app.tables) : r.title;
    const page = r.dashboardTitle || t('a dashboard');
    return title.trim() ? t('{title} on {page}', { title, page }) : t('A {kind} tile on {page}', { kind: r.kind, page });
  }

  async function removeMeasure(m: NamedMeasure, confirmed = false) {
    app.busy = t('Saving');
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
  // Advanced makes and changes what a measure reads, the main date included (V4-D20).
  const showMainDate = $derived(canCalc && (dateColumns.length >= 2 || (readByMeasure && dateColumns.length >= 1)));

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

  /** The key that goes with a shortcut on this computer. */
  const mod = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘' : 'Ctrl+';
  const fileKinds = ['excel', 'csv', 'parquet', 'sqlite', 'duckdb', 'folder'];
  /** The state of a source in one word, for its dot: read and watched, live, or failed. */
  function liveOf(s: Source): { on: boolean; bad: boolean; words: string } | null {
    if (errors[s.id]) return { on: false, bad: true, words: t('It could not be read') };
    if (s.kind === 'derived') return null;
    if (s.kind === 'url') {
      const at = app.snap?.loaded_at?.[s.id];
      return at ? { on: true, bad: false, words: t('Fetched {when}', { when: sinceWords(at) }) } : { on: false, bad: false, words: t('Not fetched yet') };
    }
    if (s.kind === 'postgres' || s.kind === 'mysql') return { on: true, bad: false, words: t('Queried live') };
    return { on: true, bad: false, words: t('Watched: saved changes show up by themselves') };
  }
  /** Where the rows of the table on show come from, under the grid. */
  const readFrom = $derived(
    !sourceOf
      ? ''
      : sourceOf.kind === 'derived'
        ? t('Made from other tables')
        : sourceOf.kind === 'url'
          ? app.snap?.loaded_at?.[sourceOf.id] ? t('Fetched {when}', { when: sinceWords(app.snap.loaded_at[sourceOf.id]) }) : t('Not fetched yet')
          : sourceOf.kind === 'postgres' || sourceOf.kind === 'mysql'
            ? t('Queried live from the database')
            : fileKinds.includes(sourceOf.kind)
              ? sourceOf.kind === 'folder'
                ? t('Read live from the folder · saved changes show up by themselves')
                : t('Read live from the file · saved changes show up by themselves')
              : '',
  );

  /** The columns an unpivot offer names, by the kind the engine found. */
  const PLURAL: Record<string, () => string> = { Year: () => t('years'), Quarter: () => t('quarters'), Month: () => t('months'), Period: () => t('periods') };
  const acrossOf = (name: string) => PLURAL[name]?.() ?? `${name.toLowerCase()}s`;

  function openMenu(e: MouseEvent, name: string) {
    leave();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { column: name, x: Math.min(r.left, innerWidth - 270), y: r.bottom + 4 };
  }
</script>

<header class="toolbar">
  <nav class="crumbs" aria-label={t('Breadcrumb')}>
    <span class="crumb ws">{fixedWord(app.snap?.workspace.name ?? '') || t('Workspace')}</span>
    <Icon name="chevronRight" size={12} />
    {#if table && sources.length}
      <button class="crumb link" onclick={() => go({ view: 'data' })}>{t('Data')}</button>
      <Icon name="chevronRight" size={12} />
      <h2 class="crumb now">{table.label}</h2>
    {:else}
      <h2 class="crumb now">{t('Data')}</h2>
    {/if}
  </nav>
  <span class="spacer"></span>
  <button class="btn" title={t('Paste cells from Excel or Google Sheets as a new table  {key}', { key: `${mod}V` })} onclick={() => readClipboard(true)}><Icon name="table" /> {t('Paste a table')}</button>
  <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> {t('Add data')}</button>
</header>

{#if !sources.length}
  <div class="empty">
    <Icon name="sheet" size={32} />
    <h2>{t('No data yet')}</h2>
    <p class="muted">{t('Add an Excel file, a CSV file, a folder of files, a link or a database to start. You can also paste cells from Excel.')}</p>
    <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> {t('Add data')}</button>
  </div>
{:else}
  <div class="split">
    <aside class="sources" class:open={sourcesOpen}>
      <button class="sources-toggle" aria-expanded={sourcesOpen} onclick={() => (sourcesOpen = !sourcesOpen)}>
        <Icon name="table" size={14} />
        <span class="now">{table?.label ?? t('Sources')}</span>
        <span class="count">{tn('{n} source', '{n} sources', sources.length)}</span>
        <Icon name="chevronDown" size={12} />
      </button>
      <div class="sources-list">
      {#each sources as s (s.id)}
        {@const live = liveOf(s)}
        <section>
          <div class="src">
            <Icon name={iconOf(s.kind)} size={15} />
            <span class="src-name" title={s.path ?? s.host}>{s.name}</span>
            <span class="kind">{kindLabel[s.kind]}</span>
            {#if live}<span class="live" class:on={live.on} class:bad={live.bad} title={live.words} role="img" aria-label={live.words}></span>{/if}
            <button class="btn plain icon tiny" title={s.kind === 'url' ? t('Fetch the link now') : t('Reload')} aria-label={s.kind === 'url' ? t('Fetch the link now') : t('Reload')} onclick={() => app.run(null, () => api.reloadSource(s.id))}><Icon name="refresh" size={13} /></button>
            <button class="btn plain icon tiny rm" title={t('Remove {name}', { name: s.name })} aria-label={t('Remove {name}', { name: s.name })} onclick={() => remove(s.id, s.name)}><Icon name="trash" size={13} /></button>
          </div>
          {#if s.kind === 'url'}
            <p class="srcline">
              {#if app.snap?.loaded_at?.[s.id]}{t('Fetched {when}', { when: sinceWords(app.snap.loaded_at[s.id]) })}{:else}{t('Not fetched yet')}{/if}
              <select class="every" aria-label={t('Refresh')} value={s.every ?? 'hourly'} onchange={(e) => setEvery(s.id, e.currentTarget.value as UrlEvery)}>
                {#each EVERY_CHOICES as c}<option value={c.value}>{c.label}</option>{/each}
              </select>
              <button class="linkbtn" onclick={() => (linkSource = s)}>{t('Link settings')}</button>
            </p>
          {:else if s.kind === 'folder'}
            <p class="srcline" title={s.path}>{t('Reads {pattern}', { pattern: s.pattern ?? '' })}</p>
          {:else if s.spec}
            <p class="srcline">
              <span class="from" title={fromWords(s.spec, app.tables)}>{fromWords(s.spec, app.tables)}</span>
              {#if canCalc}<button class="linkbtn" onclick={() => (editingDerived = s.id)}>{t('Change')}</button>{/if}
            </p>
          {/if}
          {#if errors[s.id]}
            <p class="err"><Icon name="warning" size={13} /> {errors[s.id]}</p>
          {/if}
          <div class="tbls">
            {#each app.tables.filter((tb) => tb.source_id === s.id) as tb (tb.id)}
              <button class="tbl" class:on={table?.id === tb.id} onclick={() => ((sourcesOpen = false), go({ view: 'data', table: tb.id }))}>
                <Icon name="table" size={14} />
                <span class="tname">{tb.name}</span>
                <span class="rows">{tb.rows === null ? t('live') : full(tb.rows)}</span>
              </button>
            {/each}
          </div>
        </section>
      {/each}
      {#if canCalc || measures.length}
        <section class="measures" aria-label={t('Measures')}>
          <div class="group">
            <span>{t('Measures')}{measures.length ? ` · ${measures.length}` : ''}</span>
            <span class="adv" title={t('Measures are made with Advanced on')}>{t('Advanced')}</span>
          </div>
          {#each measures as m (m.id)}
            {@const info = measureInfo[m.id]}
            <div class="msr" class:bad={!!info?.error}>
              <button class="msr-main" onclick={() => canCalc && (measureSheet = { editing: m.id })} disabled={!canCalc} title={info?.error ?? m.description ?? m.formula}>
                <span class="mname">{m.name}{#if info?.error}<Icon name="warning" size={12} />{/if}</span>
                <span class="mformula">{m.formula}</span>
                <span class="muse">{info?.error ? t('Does not work now') : usedIn(m) ? tn('Used in {n} tile', 'Used in {n} tiles', usedIn(m)) : t('Not used yet')}</span>
              </button>
              {#if canCalc}
                <button class="btn plain icon tiny" title={t('Delete {name}', { name: m.name })} aria-label={t('Delete {name}', { name: m.name })} onclick={() => removeMeasure(m)}><Icon name="trash" size={13} /></button>
              {/if}
            </div>
          {:else}
            <p class="srcline mhint">{t('A measure is a formula of totals, like SUM([Sales].[amount]) / SUM([Targets].[target]). Any tile can use it.')}</p>
          {/each}
          {#if canCalc}
            <button class="newm" onclick={() => (measureSheet = {})}><Icon name="plus" size={13} /> {t('New measure')}</button>
          {/if}
        </section>
      {/if}
      {#if canCalc || params.length}
        <section class="measures" aria-label={t('Parameters')}>
          <div class="group">
            <span>{t('Parameters')}{params.length ? ` · ${params.length}` : ''}</span>
            <span class="adv" title={t('Parameters are made with Advanced on')}>{t('Advanced')}</span>
          </div>
          {#each params as p (p.id)}
            <div class="msr">
              <button class="msr-main" onclick={() => canCalc && (paramSheet = { editing: p.id })} disabled={!canCalc} title={p.description ?? `PARAM("${p.name}")`}>
                <span class="mname">{p.name}</span>
                <span class="mformula">PARAM("{p.name}") = {paramValue(p)}</span>
                <span class="muse">{paramRange(p)}</span>
              </button>
            </div>
          {:else}
            <p class="srcline mhint">{t('A parameter is a number or a choice that a reader moves on a dashboard. A measure reads it, like SUM([amount]) * (1 + PARAM("Price rise")).')}</p>
          {/each}
          {#if canCalc}
            <button class="newm" onclick={() => (paramSheet = {})}><Icon name="plus" size={13} /> {t('New parameter')}</button>
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
            <p class="muted meta">
              {kindLabel[sourceOf?.kind ?? ''] ?? ''} · {sourceOf?.name}
              · {table.rows === null ? t('queried live') : t('{n} rows', { n: full(table.rows) })}
              · {t('{n} columns', { n: table.columns.length })}{#if table.columns.some((c) => c.hidden)} · {t('{n} hidden', { n: table.columns.filter((c) => c.hidden).length })}{/if}
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
                <label class="hdr" title={t('The date a measure uses to line this table up with the others over time')}>
                  <span class="label">{t('Main date')}</span>
                  <select
                    class="field"
                    aria-label={t('Main date')}
                    value={setup?.mainDate ?? ''}
                    onchange={(e) => save(withMainDate(setup, e.currentTarget.value || undefined))}>
                    <option value="">{dateColumns.length === 1 ? t('{name} (the only date)', { name: dateColumns[0].label ?? dateColumns[0].name }) : t('Not set')}</option>
                    {#each dateColumns as c (c.name)}<option value={c.name}>{c.label ?? c.name}</option>{/each}
                  </select>
                </label>
              {/if}
              {#if canCalc}
                <button class="btn" title={t('Add a column that is computed from the others')} onclick={() => (calcSheet = {})}><Icon name="fx" size={13} /> {t('Add column')}</button>
              {/if}
              {#if table.prep && isExcel}
                <label class="hdr" title={t('The sheet row that holds the column names')}>
                  <span class="label">{t('Header row')}</span>
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
                  <button class="btn plain" onclick={() => save(withHeaderRow(setup, undefined))}>{t('Detect it again')}</button>
                {/if}
              {/if}
              {#if table.prep && (plan || turned)}
                <div class="prep">
                  <button class="btn" aria-expanded={prepareOpen} onclick={() => (prepareOpen = !prepareOpen)}>{t('Prepare')} <Icon name="chevronDown" size={12} /></button>
                  {#if prepareOpen}
                    <div class="pm" role="menu">
                      {#if turned}
                        <button role="menuitem" onclick={() => ((prepareOpen = false), save(withoutUnpivot(setup)))}>{t('Put the columns back')}</button>
                      {:else}
                        <button role="menuitem" onclick={() => ((prepareOpen = false), (unpivotOpen = true))}>{t('Turn these columns into rows')}</button>
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
            <span>{t('{n} columns look like {what} across the top.', { n: plan.columns.length, what: acrossOf(plan.name) })}</span>
            <button class="btn" onclick={() => (unpivotOpen = true)}>{t('Turn these columns into rows')}</button>
          </div>
        {:else if turned}
          <div class="offer">
            <Icon name="check" size={14} />
            <span>{t('Columns were turned into rows. Your file is not changed.')}</span>
            <button class="btn" onclick={() => save(withoutUnpivot(setup))}>{t('Put the columns back')}</button>
          </div>
        {/if}
        <div class="grid-wrap">
          {#if previewError}
            <p class="err pad">{previewError}</p>
          {:else if !preview}
            <p class="muted pad">{t('Loading…')}</p>
          {:else}
            <table>
              <thead>
                <tr>
                  <th class="rn" aria-label={t('Row')}>#</th>
                  {#each preview.columns as c (c.name)}
                    {@const col = colOf(c.name)}
                    <th class:num={c.dtype === 'number'} class:hid={col?.hidden} class:bad={!!col?.error || !!col?.failed}>
                      <button class="hd" aria-label={t('{name}, column menu', { name: col?.label ?? c.name })} onclick={(e) => openMenu(e, c.name)} onmouseenter={(e) => enter(e, c.name)} onmouseleave={leave} title="">
                        {#if col?.calc}<span class="fx" aria-label={t('Calculated column')}><Icon name="fx" size={11} /></span>{:else}<span class="ty"><Icon name={typeIcon[c.dtype]} size={12} /></span>{/if}<span class="hn">{col?.label ?? c.name}</span>{#if col?.hidden}<em>{t('hidden')}</em>{/if}<span class="chev"><Icon name="chevronDown" size={11} /></span>
                      </button>
                    </th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each preview.rows as r, ri}
                  <tr>
                    <td class="rn">{ri + 1}</td>
                    {#each r as v, i}
                      <td class:num={preview.columns[i].dtype === 'number'} class:blank={v === null}>
                        {gridText(v, i)}
                      </td>
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
        {#if preview}
          <footer class="gridfoot">
            <span class="num">
              {#if table.rows !== null && table.rows > preview.rows.length}{t('Showing the first {n} of {total} rows', { n: full(preview.rows.length), total: full(table.rows) })}{:else}{tn('{n} row', '{n} rows', preview.rows.length, { n: full(preview.rows.length) })}{/if}
            </span>
            {#if readFrom}<span>{readFrom}</span>{/if}
          </footer>
        {/if}
      {:else}
        <p class="muted pad">{t('This source has no tables.')}</p>
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
{#if paramSheet}
  <ParamSheet editing={paramSheet.editing} onclose={() => (paramSheet = null)} />
{/if}

{#if askMeasure}
  {@const d = askMeasure}
  <Modal title={t('Delete {name}?', { name: d.name })} width={460} onclose={() => (askMeasure = null)}>
    <p class="lead">
      {tn('This tile uses the measure. It will show an error until you pick another measure.', 'These {n} tiles use the measure. They will show an error until you pick another measure.', d.tiles.length)}
    </p>
    <ul class="uses">
      {#each d.tiles as t, i (i)}<li>{tileName(t)}</li>{/each}
    </ul>
    {#snippet footer()}
      <button class="btn" onclick={() => (askMeasure = null)}>{t('Keep the measure')}</button>
      <button class="btn danger" onclick={() => removeMeasure({ id: d.id, name: d.name } as NamedMeasure, true)}>{t('Delete measure')}</button>
    {/snippet}
  </Modal>
{/if}

{#if askDelete}
  {@const d = askDelete}
  <Modal title={t('Delete {name}?', { name: d.shown })} width={460} onclose={() => (askDelete = null)}>
    {#if d.tiles.length}
      <p class="lead">
        {tn('This tile uses the column. It will show an error until you pick another field.', 'These {n} tiles use the column. They will show an error until you pick another field.', d.tiles.length)}
      </p>
      <ul class="uses">
        {#each d.tiles as t, i (i)}<li>{t}</li>{/each}
      </ul>
    {/if}
    {#if d.blank.length}
      <p class="lead">
        {tn('A calculated column reads it and will be blank:', '{n} calculated columns read it and will be blank:', d.blank.length)}
      </p>
      <ul class="uses">
        {#each d.blank as b, i (i)}
          <li>
            <strong>{b.label}</strong>
            {#if b.tiles.length}
              {t('is used by')}
              <ul class="uses">
                {#each b.tiles as t, j (j)}<li>{t}</li>{/each}
              </ul>
            {:else}
              {t('is not used by a tile.')}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => (askDelete = null)}>{t('Keep the column')}</button>
      <button class="btn danger" onclick={() => removeCalc(d.name)}>{t('Delete column')}</button>
    {/snippet}
  </Modal>
{/if}

{#if pasted !== null}
  <PasteSheet text={pasted} onclose={() => (pasted = null)} />
{/if}

{#if editingDerived}
  <NewTableSheet op="append" editing={editingDerived} onclose={() => (editingDerived = null)} />
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
    height: 48px;
    padding: 0 12px 0 20px;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    flex: none;
  }
  .crumbs { display: flex; align-items: center; gap: 6px; min-width: 0; color: var(--text-2); }
  .crumbs :global(svg) { flex: none; color: var(--text-3); }
  .crumb { font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .crumb.ws { max-width: 180px; }
  .crumb.link { border: 0; background: none; padding: 2px 4px; margin: 0 -4px; border-radius: 5px; color: var(--text-2); transition: background 120ms var(--spring), color 120ms var(--spring); }
  .crumb.link:hover { background: var(--hover); color: var(--text); }
  .crumb.now { font-size: 13px; font-weight: 600; color: var(--text); letter-spacing: 0; min-width: 0; }
  .spacer { flex: 1; }
  .split { flex: 1; min-height: 0; display: grid; grid-template-columns: 280px 1fr; background: var(--surface); }
  .sources {
    overflow: auto;
    padding: 10px 8px 16px;
    border-right: 1px solid var(--sep);
    background: var(--surface);
  }
  section + section { margin-top: 10px; }
  .src {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 4px 0 8px;
    border-radius: var(--r-control);
    font-weight: 600;
  }
  .src :global(svg) { color: var(--text-2); flex: none; }
  .src-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .kind { flex: none; margin-right: auto; white-space: nowrap; font-family: var(--mono); font-size: 10px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); border: 1px solid var(--sep-strong); border-radius: 4px; padding: 0 4px; line-height: 15px; background: none; }
  .live { flex: none; width: 7px; height: 7px; margin: 0 4px; border-radius: 50%; background: var(--text-3); }
  .live.on { background: var(--success); }
  .live.bad { background: var(--danger); }
  .tiny { width: 24px; height: 24px; color: var(--text-2); flex: none; }
  .tiny:hover { color: var(--text); }
  .src .rm { opacity: 0; transition: opacity 120ms var(--spring); }
  .src:hover .rm, .src .rm:focus-visible { opacity: 1; }
  .tbl {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 30px;
    padding: 0 8px 0 30px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    text-align: left;
    transition: background 120ms var(--spring);
  }
  .tbl :global(svg) { color: var(--text-2); flex: none; }
  .tbl:hover { background: var(--hover); }
  .tbl.on { background: var(--selected); color: var(--accent-text); font-weight: 600; }
  .tbl.on :global(svg) { color: var(--accent-text); }
  .tname { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rows { font-size: 11px; font-weight: 500; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .srcline { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; margin: 0 6px 4px 30px; color: var(--text-2); font-size: 11px; }
  .every { height: 24px; max-width: 170px; padding: 0 6px; border: 1px solid var(--sep-strong); border-radius: 5px; background: var(--inset); color: var(--text); font-size: 11px; }
  .every:focus { outline: none; border-color: var(--focus); }
  .linkbtn { border: 0; background: none; padding: 0; color: var(--accent-text); font-size: 11px; cursor: pointer; }
  .from { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .linkbtn:hover { text-decoration: underline; }
  .err { display: flex; gap: 6px; margin: 4px 6px 6px; color: var(--danger); font-size: 12px; }
  .detail { min-width: 0; display: flex; flex-direction: column; padding: 20px 20px 12px; gap: 14px; overflow: hidden; }
  .head { display: flex; align-items: flex-end; gap: 8px 16px; flex-wrap: wrap; }
  .title { flex: 1 1 240px; min-width: 0; }
  .head h1 { font-size: 20px; font-weight: 650; letter-spacing: -0.015em; }
  .head p { margin: 4px 0 0; }
  .meta { font-size: 12px; font-variant-numeric: tabular-nums; }
  .notes { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-direction: column; gap: 2px; font-size: 12px; color: var(--text-2); }
  .notes li { display: flex; align-items: center; gap: 6px; }
  .notes li.bad { color: var(--danger); }
  .tools { display: flex; align-items: center; gap: 8px; flex: none; flex-wrap: wrap; }
  .hdr { display: flex; align-items: center; gap: 6px; }
  .hdr .label { white-space: nowrap; }
  .num-box { width: 56px; height: 28px; padding: 0 8px; border-color: var(--sep-strong); background: var(--surface); font-variant-numeric: tabular-nums; }
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
  th.hid .hd { opacity: 0.6; }
  .hd em { font-style: normal; font-size: 10px; font-weight: 500; color: var(--text-2); }
  th.bad .hn { color: var(--danger); }
  .fx { display: inline-flex; align-items: center; color: var(--accent-text); }
  .hd .fx :global(svg) { color: var(--accent-text); }
  .measures { margin-top: 14px; padding: 12px 0 0; border-top: 1px solid var(--sep); display: flex; flex-direction: column; gap: 6px; }
  .group { display: flex; align-items: center; justify-content: space-between; padding: 0 8px 2px; font-family: var(--mono); font-size: 10.5px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); }
  .adv { font-family: var(--mono); font-size: 10px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); border: 1px solid var(--sep-strong); border-radius: 4px; padding: 0 4px; line-height: 15px; background: none; }
  .msr { display: flex; align-items: flex-start; gap: 2px; border: 1px solid var(--sep); border-radius: 8px; background: var(--surface); transition: border-color 120ms var(--spring), background 120ms var(--spring); }
  .msr:hover { background: var(--hover); }
  .msr.bad { border-color: var(--danger); }
  .msr-main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
    padding: 8px 4px 8px 10px;
    border: 0;
    background: none;
    text-align: left;
    color: inherit;
    cursor: pointer;
  }
  .msr-main:disabled { cursor: default; }
  .msr .tiny { margin: 6px 4px 0 0; opacity: 0; transition: opacity 120ms var(--spring); }
  .msr:hover .tiny, .msr .tiny:focus-visible { opacity: 1; }
  .mname { display: flex; align-items: center; gap: 5px; font-weight: 600; min-width: 0; }
  .msr.bad .mname { color: var(--danger); }
  .mformula { font: 11px/16px var(--mono); color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .muse { font-size: 11px; color: var(--text-2); }
  .mhint { margin: 0 8px 4px; display: block; line-height: 1.45; }
  .newm {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: var(--r-control);
    background: none;
    color: var(--text-2);
    font-size: 12px;
    transition: background 120ms var(--spring), color 120ms var(--spring);
  }
  .newm:hover { background: var(--hover); color: var(--text); }
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
    .tbls { display: flex; flex-wrap: wrap; gap: 6px; padding: 2px 6px 4px 30px; }
    .tbl { width: auto; height: 26px; padding: 0 10px; border-radius: 13px; background: var(--window); }
    .tbl.on { background: var(--selected); }
    .detail { padding: 14px 14px 8px; }
    .crumb.ws { display: none; }
    .crumbs > :global(svg:first-of-type) { display: none; }
    .title { flex: 1 1 100%; }
    .tools { flex: 1 1 100%; }
    .toolbar { padding: 0 14px; }
  }
  .hd {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 0;
    background: none;
    padding: 0;
    font: inherit;
    font-weight: 600;
    color: inherit;
    cursor: pointer;
  }
  .hd .ty { display: inline-flex; color: var(--text-2); }
  .hd .chev { display: inline-flex; color: var(--text-3); opacity: 0.6; transition: opacity 120ms var(--spring); }
  .hd:hover .chev, .hd:focus-visible .chev { opacity: 1; }
  th.num .hd { justify-content: flex-end; width: 100%; }
  .grid-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--sep);
    border-radius: var(--r-card);
    background: var(--surface);
  }
  table { border-collapse: separate; border-spacing: 0; font-size: 12px; min-width: 100%; font-variant-numeric: tabular-nums; }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    height: 36px;
    background: var(--surface);
    text-align: left;
    font-weight: 600;
    padding: 0 12px;
    border-bottom: 1px solid var(--sep);
    white-space: nowrap;
  }
  td {
    height: 32px;
    padding: 0 12px;
    border-bottom: 1px solid var(--sep);
    white-space: nowrap;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rn { position: sticky; left: 0; width: 44px; min-width: 44px; padding: 0 10px; text-align: right; color: var(--text-2); font-weight: 400; background: var(--window); border-right: 1px solid var(--sep); }
  th.rn { z-index: 2; font-size: 11px; }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  tbody tr:hover td { background: var(--hover); }
  .gridfoot { display: flex; justify-content: space-between; gap: 16px; flex-wrap: wrap; font-size: 11.5px; color: var(--text-2); }
  .pad { padding: 16px; margin: 0; }
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
