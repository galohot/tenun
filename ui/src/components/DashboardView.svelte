<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Cell, type Tile, type TileType } from '../lib/api';
  import { app, go } from '../lib/state.svelte';
  import { cell, dimLabel } from '../lib/format';
  import { COLS, compact, defaultSize, newId, place, type DashFilter } from '../lib/tiles';
  import { key } from '../lib/chart';
  import { buildHtml, buildPayload } from '../lib/export';
  import Icon from './Icon.svelte';
  import TileView from './dash/TileView.svelte';
  import Inspector from './dash/Inspector.svelte';

  let { id }: { id: string } = $props();

  const ROW = 76;
  const GAP = 12;

  const dash = $derived(app.dashboards.find((d) => d.id === id));
  let filters = $state<DashFilter[]>([]);
  let selectedTile = $state<string | null>(null);
  let gridEl = $state<HTMLDivElement>();
  let width = $state(1000);
  let tick = $state(0);
  let editingTitle = $state(false);
  let addMenu = $state(false);

  /** The live layout while a drag is in progress; null otherwise. */
  let preview = $state<Tile[] | null>(null);
  let dragging = $state<{ id: string; dx: number; dy: number } | null>(null);

  const colW = $derived((width - GAP * (COLS - 1)) / COLS);
  const tiles = $derived(preview ?? dash?.tiles ?? []);
  const height = $derived(Math.max(0, ...tiles.map((t) => t.y + t.h)) * (ROW + GAP));
  const hasLive = $derived(app.snap!.workspace.sources.some((s) => s.kind === 'postgres' || s.kind === 'mysql'));

  onMount(() => {
    if (!dash) go({ view: 'home' });
    const ro = new ResizeObserver(() => gridEl && (width = gridEl.clientWidth));
    if (gridEl) ro.observe(gridEl);
    return () => ro.disconnect();
  });

  // Live databases refresh on the dashboard's interval.
  $effect(() => {
    const secs = dash?.refresh ?? 0;
    if (!secs || !hasLive) return;
    const h = setInterval(() => tick++, secs * 1000);
    return () => clearInterval(h);
  });

  function update(mut: (tiles: Tile[]) => Tile[]) {
    if (!dash) return;
    dash.tiles = mut(dash.tiles);
    app.saveDashboards();
  }

  function addTile(type: TileType) {
    addMenu = false;
    const size = defaultSize(type);
    const at = place(dash!.tiles, size.w, size.h);
    const t: Tile = { id: newId('t'), type, title: '', ...size, ...at, measures: [], dims: [], horizontal: type === 'bar' };
    update((ts) => [...ts, t]);
    selectedTile = t.id;
  }

  function pick(tile: Tile, value: Cell) {
    const d = tile.dims[0];
    if (!d) return;
    const mine = filters.find((f) => f.fromTile === tile.id);
    const others = filters.filter((f) => f.fromTile !== tile.id);
    if (mine && mine.values.length === 1 && key(mine.values[0]) === key(value)) {
      filters = others;
    } else {
      filters = [...others, { table: d.table, column: d.column, grain: d.grain ?? null, op: 'in', values: [value], fromTile: tile.id }];
    }
  }

  function filterLabel(f: DashFilter): string {
    const t = app.table(f.table);
    const dtype = t?.columns.find((c) => c.name === f.column)?.dtype ?? 'text';
    const name = dimLabel({ table: f.table, column: f.column, grain: f.grain }, app.tables);
    return `${name.charAt(0).toUpperCase() + name.slice(1)}: ${f.values.map((v) => cell(v, dtype, f.grain)).join(', ')}`;
  }

  // Dragging and resizing work in grid units and compact as you go.
  function startDrag(e: PointerEvent, tile: Tile, mode: 'move' | 'resize') {
    if (e.button !== 0) return;
    e.preventDefault();
    const start = { x: e.clientX, y: e.clientY };
    const orig = { ...tile };
    const base = $state.snapshot(dash!.tiles) as Tile[];
    let moved = false;
    const onMove = (ev: PointerEvent) => {
      const dx = ev.clientX - start.x;
      const dy = ev.clientY - start.y;
      if (!moved && Math.hypot(dx, dy) < 4) return;
      moved = true;
      const gx = Math.round(dx / (colW + GAP));
      const gy = Math.round(dy / (ROW + GAP));
      const next =
        mode === 'move'
          ? { ...orig, x: Math.max(0, Math.min(COLS - orig.w, orig.x + gx)), y: Math.max(0, orig.y + gy) }
          : { ...orig, w: Math.max(2, Math.min(COLS - orig.x, orig.w + gx)), h: Math.max(2, orig.h + gy) };
      preview = compact(base.map((t) => (t.id === tile.id ? next : t)), tile.id);
      dragging = mode === 'move' ? { id: tile.id, dx, dy } : null;
    };
    const onUp = () => {
      removeEventListener('pointermove', onMove);
      removeEventListener('pointerup', onUp);
      if (moved && preview) {
        const final = compact(preview);
        update(() => final);
      }
      preview = null;
      dragging = null;
    };
    addEventListener('pointermove', onMove);
    addEventListener('pointerup', onUp);
  }

  function px(t: Tile) {
    return {
      left: t.x * (colW + GAP),
      top: t.y * (ROW + GAP),
      w: t.w * colW + (t.w - 1) * GAP,
      h: t.h * ROW + (t.h - 1) * GAP,
    };
  }

  let exporting = $state<string | null>(null);

  async function exportHtml() {
    if (!dash || exporting) return;
    let target: string | null = null;
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: 'save_workspace', name: dash.title });
      if (!paths[0]) return;
      target = paths[0].replace(/\.tenun$/, '') + '.html';
    }
    exporting = 'Preparing…';
    try {
      const payload = await buildPayload($state.snapshot(dash) as typeof dash, app.tables, app.snap!.workspace.name, $state.snapshot(filters) as DashFilter[], (d, t) => {
        exporting = `Preparing ${Math.round((d / t) * 100)}%`;
      });
      const html = await buildHtml(payload);
      if (target) {
        await api.writeExport(target, html);
        app.toast(`Exported ${target.split(/[\\/]/).pop()}`);
      } else {
        const a = document.createElement('a');
        a.href = URL.createObjectURL(new Blob([html], { type: 'text/html' }));
        a.download = `${dash.title.replace(/[\\/:*?"<>|]/g, '_')}.html`;
        a.click();
        setTimeout(() => URL.revokeObjectURL(a.href), 5000);
        app.toast('Exported');
      }
    } catch (e) {
      app.error(e);
    } finally {
      exporting = null;
    }
  }

  function removeDashboard() {
    if (!dash || !confirm(`Delete the dashboard “${dash.title}”?`)) return;
    app.snap!.workspace.dashboards = app.dashboards.filter((d) => d.id !== dash.id);
    app.saveDashboards();
    const next = app.dashboards[0];
    go(next ? { view: 'dashboard', id: next.id } : { view: 'home' });
  }

  const sel = $derived(dash?.tiles.find((t) => t.id === selectedTile));
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (filters.length ? (filters = []) : (selectedTile = null))} />

{#if dash}
  <header class="toolbar">
    {#if editingTitle}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="field title-input"
        value={dash.title}
        autofocus
        onblur={(e) => {
          dash.title = e.currentTarget.value.trim() || dash.title;
          editingTitle = false;
          app.saveDashboards();
        }}
        onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
    {:else}
      <button class="title" title="Rename" onclick={() => (editingTitle = true)}>{dash.title}</button>
    {/if}
    <div class="chips">
      {#each filters as f (f.fromTile)}
        <span class="chip">
          <Icon name="filter" size={12} />
          {filterLabel(f)}
          <button aria-label="Clear filter" onclick={() => (filters = filters.filter((x) => x !== f))}><Icon name="x" size={11} /></button>
        </span>
      {/each}
      {#if filters.length > 1}
        <button class="btn plain small" onclick={() => (filters = [])}>Clear all</button>
      {/if}
    </div>
    <span class="spacer"></span>
    {#if hasLive}
      <label class="refresh" title="How often database data is refreshed">
        <Icon name="clock" size={14} />
        <select
          value={String(dash.refresh)}
          onchange={(e) => {
            dash.refresh = Number(e.currentTarget.value);
            app.saveDashboards();
          }}>
          <option value="0">Off</option>
          <option value="15">15 s</option>
          <option value="60">1 min</option>
          <option value="300">5 min</option>
        </select>
      </label>
    {/if}
    <button class="btn plain icon" title="Refresh now" onclick={() => tick++}><Icon name="refresh" /></button>
    <button class="btn" onclick={exportHtml} disabled={!dash.tiles.length || !!exporting} title="Save this dashboard as one HTML file that works offline">
      <Icon name="export" /> {exporting ?? 'Export'}
    </button>
    <div class="add">
      <button class="btn primary" onclick={() => (addMenu = !addMenu)} aria-expanded={addMenu}><Icon name="plus" /> Add tile</button>
      {#if addMenu}
        <div class="menu" role="menu">
          {#each [['bar', 'Bar chart'], ['line', 'Line chart'], ['area', 'Area chart'], ['donut', 'Donut'], ['kpi', 'Number'], ['table', 'Table']] as [t, label]}
            <button role="menuitem" onclick={() => addTile(t as TileType)}>
              <Icon name={t === 'kpi' ? 'kpi' : t === 'table' ? 'table' : (t as 'bar')} /> {label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="btn plain icon" title="Delete dashboard" onclick={removeDashboard}><Icon name="trash" /></button>
  </header>

  <div class="body">
    <div class="scroll" onpointerdown={(e) => e.target === e.currentTarget && (selectedTile = null)} role="presentation">
      {#if !dash.tiles.length}
        <div class="empty">
          <Icon name="dashboard" size={32} />
          <h2>An empty dashboard</h2>
          <p class="muted">
            {app.tables.length ? 'Add a tile, then pick a value and how to group it.' : 'Add some data first, then come back here.'}
          </p>
          {#if app.tables.length}
            <div class="quick">
              <button class="btn" onclick={() => addTile('kpi')}><Icon name="kpi" /> Number</button>
              <button class="btn" onclick={() => addTile('bar')}><Icon name="bar" /> Bar chart</button>
              <button class="btn" onclick={() => addTile('line')}><Icon name="line" /> Line chart</button>
            </div>
          {:else}
            <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data</button>
          {/if}
        </div>
      {/if}
      <div class="grid" bind:this={gridEl} style:height="{height}px">
        {#if preview}
          {@const ghost = preview.find((t) => t.id === (dragging?.id ?? ''))}
          {#if ghost}
            {@const g = px(ghost)}
            <div class="ghost" style:transform="translate({g.left}px, {g.top}px)" style:width="{g.w}px" style:height="{g.h}px"></div>
          {/if}
        {/if}
        {#each tiles as t (t.id)}
          {@const p = px(dragging?.id === t.id ? (dash.tiles.find((x) => x.id === t.id) ?? t) : t)}
          <div
            class="cell"
            class:lifted={dragging?.id === t.id}
            style:transform={dragging?.id === t.id
              ? `translate(${p.left + dragging.dx}px, ${p.top + dragging.dy}px)`
              : `translate(${p.left}px, ${p.top}px)`}
            style:width="{p.w}px"
            style:height="{p.h}px">
            <TileView
              tile={dash.tiles.find((x) => x.id === t.id) ?? t}
              {filters}
              {tick}
              active={selectedTile === t.id}
              onpick={(v) => pick(t, v)}
              onselect={() => (selectedTile = t.id)}
              ondragstart={(e) => startDrag(e, dash.tiles.find((x) => x.id === t.id)!, 'move')}
              onresizestart={(e) => startDrag(e, dash.tiles.find((x) => x.id === t.id)!, 'resize')} />
          </div>
        {/each}
      </div>
    </div>
    {#if sel}
      <Inspector
        tile={sel}
        onclose={() => (selectedTile = null)}
        onchange={(nt) => update((ts) => ts.map((x) => (x.id === nt.id ? nt : x)))}
        onremove={() => {
          filters = filters.filter((f) => f.fromTile !== sel.id);
          update((ts) => compact(ts.filter((x) => x.id !== sel.id)));
          selectedTile = null;
        }} />
    {/if}
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 52px;
    padding: 0 12px 0 16px;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    flex: none;
  }
  .title {
    border: 0;
    background: none;
    font-size: 17px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: var(--r-control);
    white-space: nowrap;
  }
  .title:hover { background: var(--hover); }
  .title-input { width: 240px; font-size: 15px; font-weight: 600; }
  .chips { display: flex; gap: 6px; align-items: center; min-width: 0; overflow: hidden; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 4px 0 9px;
    border-radius: 12px;
    background: var(--selected);
    color: var(--accent-text);
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
  }
  .chip button {
    width: 18px;
    height: 18px;
    border: 0;
    border-radius: 9px;
    background: none;
    color: inherit;
    display: grid;
    place-items: center;
    padding: 0;
  }
  .chip button:hover { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .small { height: 24px; font-size: 12px; }
  .spacer { flex: 1; }
  .refresh { display: flex; align-items: center; gap: 4px; color: var(--text-2); }
  .refresh select { border: 0; background: none; color: var(--text); padding: 2px; }
  .add { position: relative; }
  .menu {
    position: absolute;
    right: 0;
    top: 34px;
    z-index: 30;
    width: 180px;
    padding: 5px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    animation: pop 200ms var(--spring);
    transform-origin: top right;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
  }
  .menu button :global(svg) { color: var(--accent-text); }
  .menu button:hover { background: var(--accent); color: #fff; }
  .menu button:hover :global(svg) { color: #fff; }
  .body { flex: 1; min-height: 0; display: flex; }
  .scroll { flex: 1; min-width: 0; overflow: auto; padding: 16px; }
  .grid { position: relative; min-height: 100%; }
  .cell {
    position: absolute;
    top: 0;
    left: 0;
    transition: transform 250ms var(--spring), width 250ms var(--spring), height 250ms var(--spring);
  }
  .cell.lifted { transition: none; z-index: 10; }
  .cell.lifted :global(.tile) { box-shadow: var(--shadow-float); }
  .ghost {
    position: absolute;
    top: 0;
    left: 0;
    border-radius: var(--r-card);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    border: 1.5px dashed color-mix(in srgb, var(--accent) 60%, transparent);
    transition: transform 150ms var(--spring);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 80px 16px;
    text-align: center;
  }
  .empty :global(svg) { color: var(--accent-text); }
  .empty p { margin: 0 0 8px; }
  .quick { display: flex; gap: 8px; }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
</style>
