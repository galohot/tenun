<script lang="ts">
  import { onMount, tick as nextTick, untrack } from 'svelte';
  import { api, MeasureRefused, type Bins, type Cell, type Dashboard, type DashFilter, type Filter, type Grain, type NamedMeasure, type QueryResult, type RowsBehind, type Tile, type TileFilter, type TileType } from '../lib/api';
  import { app, go } from '../lib/state.svelte';
  import { cell, dateTimeMedium, stampLabel, tileTitle } from '../lib/format';
  import { filterText } from '../lib/filterText';
  import { canExclude, colFilter, hasPeriod, markFilter, newPinId, PERIODS, pinFilter, sameFilter, tileFilters, withGrain } from '../lib/filters';
  import { COLS, compact, defaultSize, minHeight, newDashboard, newId, place } from '../lib/tiles';
  import { phoneLayout, readingOrder } from '../lib/layout';
  import { downloadDashboardPdf, downloadDashboardPng, type DashboardPicture } from '../lib/output';
  import { readColors } from '../lib/chart';
  import { advancedOn, ganttOffered as hasTwoDates } from '../lib/tiers';
  import Icon, { type IconName } from './Icon.svelte';
  import TileView from './dash/TileView.svelte';
  import Inspector from './dash/Inspector.svelte';
  import Menu from './dash/Menu.svelte';
  import PublishSheet from './dash/PublishSheet.svelte';
  import RowsSheet from './dash/RowsSheet.svelte';
  import FocusSheet from './dash/FocusSheet.svelte';
  import { drawnTile } from '../lib/card';
  import { flowOrder, insertInOrder, isFlow, moveInOrder } from '../lib/flow';
  import FlowLayout from './dash/FlowLayout.svelte';
  import { buildClip, clipText, dashboardCopy, freeName, joinMeasures, joinStyles, landTiles, measureIds, placeBelow, readClip, renamesFor, rewriteFormula, sameWorkspace, type Clip, type Matches } from '../lib/clipboard';
  import MatchFields from './dash/MatchFields.svelte';
  import CardStyles from './dash/CardStyles.svelte';
  // tenun.cloud is not live yet; build with VITE_TENUN_CLOUD=1 to show Publish.
  const CLOUD_ON = import.meta.env.VITE_TENUN_CLOUD === '1';

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
  let publishing = $state(false);
  /** What each tile shows now. The Inspector's "Also try" previews reuse the selected tile's. */
  let results = $state.raw<Record<string, { res: QueryResult; key: string } | null>>({});

  /** The tile shown large, and the rows being shown behind a tile or one of its marks. */
  let focusId = $state<string | null>(null);
  let rowsReq = $state.raw<{ title: string; data: RowsBehind | null; error: string } | null>(null);
  /** A newer request or a closed sheet makes an older answer stale. */
  let rowsSeq = 0;
  /** The chip whose "Keep only / Exclude" menu is open. */
  let chipMenu = $state<{ id: string; x: number; y: number } | null>(null);

  /** The live layout while a drag is in progress; null otherwise. */
  let preview = $state<Tile[] | null>(null);
  let dragging = $state<{ id: string; dx: number; dy: number } | null>(null);

  const colW = $derived((width - GAP * (COLS - 1)) / COLS);
  // On a phone the tiles stack in one column and nothing can be moved or resized.
  const phone = $derived(app.phone);
  const phoneRects = $derived(phone ? new Map(phoneLayout(dash?.tiles ?? [], width, ROW, GAP).map((p) => [p.t.id, p])) : null);
  // In reading order, so Tab goes through the tiles the way a person reads them.
  const tiles = $derived(readingOrder(preview ?? dash?.tiles ?? []));
  const height = $derived(
    phoneRects ? Math.max(0, ...[...phoneRects.values()].map((p) => p.y + p.h)) : Math.max(0, ...tiles.map((t) => t.y + t.h)) * (ROW + GAP),
  );
  // Every filter a tile receives: the ones the dashboard keeps, what the filter tiles select, and the
  // marks that were clicked. A tile ignores the ones that came from itself.
  const pins = $derived(dash?.filters ?? []);
  const selections = $derived(
    tileFilters(dash?.tiles ?? [], (t, c) => app.table(t)?.columns.find((x) => x.name === c)?.dtype),
  );
  const allFilters = $derived<DashFilter[]>([...pins, ...selections, ...filters]);

  // The tile as it is drawn: with the dashboard's grain. The same object is returned while nothing
  // changed, so a tile only asks again when its question is different.
  const effCache = new Map<string, { k: string; t: Tile }>();
  function eff(t: Tile): Tile {
    const e = withGrain(t, dash?.grain);
    if (e === t) return t;
    const k = JSON.stringify(e);
    const hit = effCache.get(t.id);
    if (hit?.k === k) return hit.t;
    effCache.set(t.id, { k, t: e });
    return e;
  }
  const shownOf = (t: Tile): Tile => eff(dash?.tiles.find((x) => x.id === t.id) ?? t);
  const grainOn = $derived(hasPeriod(dash?.tiles ?? []));

  const hasLive = $derived(app.snap!.workspace.sources.some((s) => s.kind === 'postgres' || s.kind === 'mysql'));

  onMount(() => {
    if (!dash) go({ view: 'home' });
    const ro = new ResizeObserver(() => gridEl && (width = gridEl.clientWidth));
    if (gridEl) ro.observe(gridEl);
    return () => ro.disconnect();
  });

  // A database changes without a load, so its links are read again with every refresh, the
  // interval and "Refresh now" alike (V4-D51). If an answer changed, the tiles ask again to get
  // the new notes.
  let checkedAt = 0;
  $effect(() => {
    const t = tick;
    if (!t || t === checkedAt) return;
    checkedAt = t;
    api
      .checkLinks()
      .then((r) => {
        if (r.changed) {
          app.dataVersion++;
          app.refresh();
        }
      })
      .catch(() => {});
  });

  // Live databases refresh on the dashboard's interval.
  $effect(() => {
    const secs = dash?.refresh ?? 0;
    if (!secs || !hasLive) return;
    const h = setInterval(() => tick++, secs * 1000);
    return () => clearInterval(h);
  });

  // Undo and redo. One step is the tiles and the title as they were before a change. The history
  // lives as long as this view, so it starts fresh when you open another dashboard.
  interface Step {
    title: string;
    tiles: Tile[];
    grain: Grain | null;
    filters: DashFilter[];
    /** A story's layout and order (build 11). */
    layout: Dashboard['layout'];
    order: string[] | undefined;
  }
  /** A dashboard this view made (Duplicate dashboard): undo takes it away again, redo puts it back. */
  interface Added {
    added: Dashboard;
  }
  type Entry = Step | Added;
  const MAX_STEPS = 50;
  /** Edits with the same key close together (typing a title) make one step. */
  const BURST_MS = 1200;
  const past: Entry[] = [];
  const future: Entry[] = [];
  let burst = { key: '', at: 0 };
  let historyTick = $state(0);
  const canUndo = $derived((void historyTick, past.length > 0));
  const canRedo = $derived((void historyTick, future.length > 0));
  // A step taken before another window's edit would undo that edit too, so start fresh.
  $effect(() => {
    void app.outsideEdits;
    untrack(() => {
      past.length = 0;
      future.length = 0;
      historyTick++;
    });
  });

  function snap(): Step {
    return {
      title: dash!.title,
      tiles: $state.snapshot(dash!.tiles) as Tile[],
      grain: dash!.grain ?? null,
      filters: $state.snapshot(dash!.filters ?? []) as DashFilter[],
      layout: dash!.layout,
      order: dash!.order ? [...dash!.order] : undefined,
    };
  }

  /** Call before a change. */
  function record(burstKey = '') {
    const now = Date.now();
    if (burstKey && burst.key === burstKey && now - burst.at < BURST_MS) {
      burst.at = now;
      return;
    }
    burst = { key: burstKey, at: now };
    past.push(snap());
    if (past.length > MAX_STEPS) past.shift();
    future.length = 0;
    historyTick++;
  }

  function restore(step: Step) {
    dash!.title = step.title;
    dash!.tiles = step.tiles;
    dash!.grain = step.grain;
    dash!.filters = step.filters.length ? step.filters : undefined;
    dash!.layout = step.layout;
    dash!.order = step.order;
    burst = { key: '', at: 0 };
    if (selectedTile && !step.tiles.some((t) => t.id === selectedTile)) selectedTile = null;
    filters = filters.filter((f) => step.tiles.some((t) => t.id === f.fromTile));
    app.saveDashboards();
    historyTick++;
  }

  function undo() {
    const step = past.pop();
    if (!dash || !step) return;
    if ('added' in step) {
      app.snap!.workspace.dashboards = app.dashboards.filter((d) => d.id !== step.added.id);
      app.saveDashboards();
      future.push(step);
      historyTick++;
      return;
    }
    future.push(snap());
    restore(step);
  }

  function redo() {
    const step = future.pop();
    if (!dash || !step) return;
    if ('added' in step) {
      app.snap!.workspace.dashboards.push(structuredClone(step.added));
      app.saveDashboards();
      past.push(step);
      historyTick++;
      return;
    }
    past.push(snap());
    restore(step);
  }

  function update(mut: (tiles: Tile[]) => Tile[], burstKey = '') {
    if (!dash) return;
    record(burstKey);
    dash.tiles = mut(dash.tiles);
    app.saveDashboards();
  }

  /** A change from the Inspector. Typing in the title, text or description box is one step, not one per key. */
  function change(nt: Tile) {
    const old = dash?.tiles.find((x) => x.id === nt.id);
    const keys = old ? (Object.keys({ ...old, ...nt }) as (keyof Tile)[]).filter((k) => JSON.stringify(old[k]) !== JSON.stringify(nt[k])) : [];
    // Typing merges into one step. Picking a picture or changing how it fits is a step of its own.
    const typing = keys.length === 1 && (['title', 'text'].includes(keys[0]) || (keys[0] === 'image' && old?.image?.data === nt.image?.data && old?.image?.fit === nt.image?.fit));
    update((ts) => ts.map((x) => (x.id === nt.id ? nt : x)), typing ? `${keys[0]}:${nt.id}` : '');
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      // A sheet or a menu closes itself first; Esc must not also clear the filters behind it.
      if (chipMenu || focusId || rowsReq || document.querySelector('[role="dialog"]')) return;
      if (filters.length) filters = [];
      else selectedTile = null;
      return;
    }
    const el = e.target as HTMLElement | null;
    if (el && (['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName) || el.isContentEditable)) return;
    // A sheet is open in front of the dashboard: undo must not change what it hides.
    if (app.settingsOpen || app.addOpen || publishing || focusId || rowsReq) return;
    if (!(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (k === 'z' && !e.shiftKey) {
      e.preventDefault();
      undo();
    } else if ((k === 'z' && e.shiftKey) || (k === 'y' && e.ctrlKey)) {
      e.preventDefault();
      redo();
    }
  }

  // The sources this dashboard reads, for the freshness stamp and the failed-refresh banner.
  const usedSources = $derived.by(() => {
    const ids = new Set<string>();
    for (const t of dash?.tiles ?? []) {
      // A named measure reads the tables of its formula.
      const named = t.measures.filter((m) => m.named).flatMap((m) => app.snap!.measure_info?.[m.named!]?.tables ?? []);
      const refs = [...t.measures.filter((m) => !m.named).map((m) => m.table), ...named, ...t.dims.map((d) => d.table), ...(t.cols ? [t.cols.table] : []), ...(t.repeat ? [t.repeat.table] : [])];
      for (const table of refs) {
        // A source that failed to load has no tables left, so fall back to the id prefix.
        ids.add(app.table(table)?.source_id ?? table.split('/')[0]);
      }
    }
    return app.snap!.workspace.sources.filter((s) => ids.has(s.id));
  });
  const loadedAt = $derived(app.snap!.loaded_at ?? {});
  const newestLoad = $derived(
    usedSources.map((s) => loadedAt[s.id]).filter(Boolean).sort().at(-1) ?? null,
  );
  const stampTitle = $derived(
    usedSources
      .map((s) => {
        const at = loadedAt[s.id] ? stampLabel(loadedAt[s.id]) : '';
        if (app.snap!.errors[s.id]) return `${s.name}: could not be read${at ? ` (last loaded ${at})` : ''}`;
        return `${s.name}: ${at || 'not loaded'}`;
      })
      .join('\n'),
  );
  const failed = $derived(usedSources.filter((s) => app.snap!.errors[s.id]));

  // Advanced chart types join the Add tile menu while the switch is on.
  const advanced = $derived(advancedOn());

  function addTile(type: TileType, where?: number) {
    addMenu = false;
    insertAt = null;
    const size = defaultSize(type);
    const at = place(dash!.tiles, size.w, size.h);
    const t: Tile = {
      id: newId('t'),
      type,
      title: '',
      ...size,
      ...at,
      measures: [],
      dims: [],
      horizontal: type === 'bar',
      ...(type === 'filter' ? { filter: { style: 'list' as const } } : {}),
      // A new map starts on Indonesia's provinces (build 06).
      ...(type === 'map' ? { map: { level: 'province' as const, country: 'ID' } } : {}),
    };
    // In a story the tile goes where "+" was clicked, or at the end (build 11).
    if (story) {
      record();
      dash!.order = insertInOrder(flowOrder(dash!), where ?? dash!.tiles.length, t.id);
      dash!.tiles = [...dash!.tiles, t];
      app.saveDashboards();
    } else update((ts) => [...ts, t]);
    selectedTile = t.id;
  }

  // ---- story pages (build 11) -------------------------------------------------------------------------------------

  const story = $derived(isFlow(dash));
  /** "+" in a story: the menu of tiles to put in at that place. */
  let insertAt = $state<{ at: number; x: number; y: number } | null>(null);
  function openInsert(at: number, e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    insertAt = { at, x: r.left, y: r.bottom + 4 };
  }
  function moveTile(from: number, to: number) {
    if (!dash) return;
    record();
    dash.order = moveInOrder(flowOrder(dash), from, to);
    app.saveDashboards();
  }
  /** A tile's width or height in a story. */
  function setFlow(id: string, f: NonNullable<Tile['flow']>) {
    const t = dash?.tiles.find((x) => x.id === id);
    if (t) change({ ...t, flow: { ...(t.flow ?? {}), ...f } });
  }
  /** Grid or story: the tiles keep their places on the grid, so the page switches back as it was. */
  function setLayout(layout: 'grid' | 'flow') {
    if (!dash || (dash.layout ?? 'grid') === layout) return;
    record();
    dash.layout = layout === 'grid' ? undefined : 'flow';
    app.saveDashboards();
  }

  /** `otherOf` comes with a click on the "Other" group of a top N: everything except those groups. */
  function pick(tile: Tile, value: Cell, otherOf?: Cell[], bins?: Bins, stage?: number) {
    const f = markFilter(tile, value, otherOf, bins, stage, results[tile.id]?.res?.geo);
    if (!f) return;
    clicked(tile.id, f);
  }

  /** A click on a pivot column header: filters by the column field, and a second click clears it. */
  function pickCol(tile: Tile, value: Cell) {
    const f = colFilter(tile.cols, value);
    if (f) clicked(tile.id, f);
  }

  /** Makes the click a cross-filter, or takes it off when the same mark is clicked again. */
  function clicked(from: string, f: Omit<DashFilter, 'fromTile'>) {
    const mine = filters.find((x) => x.fromTile === from);
    const others = filters.filter((x) => x.fromTile !== from);
    if (mine && sameFilter(mine, f)) {
      filters = others;
      return;
    }
    filters = [...others, { ...f, fromTile: from }];
    openChipMenu(from);
  }

  /** Opens "Keep only / Exclude" under the chip of a click. */
  async function openChipMenu(id: string) {
    await nextTick();
    const el = document.querySelector<HTMLElement>(`[data-chip="${CSS.escape(id)}"]`);
    if (!el) return;
    const r = el.getBoundingClientRect();
    chipMenu = { id, x: r.left, y: r.bottom + 4 };
  }

  /** The click filter becomes one the dashboard keeps: it applies to every tile, the source included. */
  function keep(from: string, mode: 'keep' | 'exclude') {
    const f = filters.find((x) => x.fromTile === from);
    if (!f || !dash) return;
    const pinned = pinFilter($state.snapshot(f) as DashFilter, mode, newPinId());
    if (!pinned) return;
    record();
    // The same filter twice would only show two chips.
    if (!(dash.filters ?? []).some((x) => sameFilter(x, pinned))) dash.filters = [...(dash.filters ?? []), pinned];
    filters = filters.filter((x) => x !== f);
    app.saveDashboards();
  }

  function unpin(f: DashFilter) {
    if (!dash) return;
    record();
    const rest = (dash.filters ?? []).filter((x) => x.fromTile !== f.fromTile);
    dash.filters = rest.length ? rest : undefined;
    app.saveDashboards();
  }

  function setGrain(g: Grain | null) {
    if (!dash || (dash.grain ?? null) === g) return;
    record();
    dash.grain = g;
    app.saveDashboards();
  }

  /** A filter tile's selection changed. Moving a slider is one step, not one per move. */
  function setFilter(id: string, f: TileFilter) {
    update((ts) => ts.map((x) => (x.id === id ? { ...x, filter: f } : x)), `filter:${id}`);
  }

  function setResult(id: string, res: QueryResult | null, key: string) {
    // A tile with nothing to show has nothing to remember.
    if (!res && !results[id]) return;
    results = { ...results, [id]: res ? { res, key } : null };
  }

  /** "Show rows" for a tile, or for the mark with this value. */
  async function showRows(id: string, value?: Cell, otherOf?: Cell[], bins?: Bins) {
    const real = dash?.tiles.find((x) => x.id === id);
    if (!real) return;
    const t = eff(real);
    let click: Filter | null = null;
    let mark = '';
    if (value !== undefined) {
      const f = markFilter(t, value, otherOf, bins, undefined, results[t.id]?.res?.geo);
      click = f ? { table: f.table, column: f.column, grain: f.grain, op: f.op, values: f.values, ...(f.via ? { via: f.via } : {}) } : null;
      const d = t.dims[0];
      const dtype = app.table(d?.table ?? '')?.columns.find((c) => c.name === d?.column)?.dtype ?? 'text';
      mark = f?.label ?? (t.type === 'histogram' ? filterText({ ...f!, fromTile: '' }, app.tables).split(': ')[1] : cell(value, dtype, d?.grain));
    }
    const title = [tileTitle(t, app.tables), mark].filter(Boolean).join(' · ');
    const seq = ++rowsSeq;
    rowsReq = { title, data: null, error: '' };
    try {
      const data = await api.rows($state.snapshot(t) as Tile, $state.snapshot(allFilters), click ? ($state.snapshot(click) as Filter) : null);
      if (seq === rowsSeq) rowsReq = { title, data, error: '' };
    } catch (e) {
      if (seq === rowsSeq) rowsReq = { title, data: null, error: e instanceof Error ? e.message : String(e) };
    }
  }

  const focusTile = $derived(focusId ? dash?.tiles.find((t) => t.id === focusId) : undefined);

  function removeTile(id: string) {
    filters = filters.filter((f) => f.fromTile !== id);
    update((ts) => compact(ts.filter((x) => x.id !== id)));
    if (selectedTile === id) selectedTile = null;
  }

  /** A copy of a tile with a new id, at the first free place of `into`. */
  function copyOf(t: Tile, into: Tile[], title: string): Tile {
    const at = place(into, t.w, t.h);
    return { ...(structuredClone($state.snapshot(t)) as Tile), id: newId('t'), title, ...at };
  }

  function duplicate(id: string) {
    const t = dash?.tiles.find((x) => x.id === id);
    if (!dash || !t) return;
    const copy = copyOf(t, dash.tiles, t.title ? `${t.title} copy` : '');
    update((ts) => [...ts, copy]);
    selectedTile = copy.id;
  }

  /** Copy a tile to another dashboard (or a new one when `to` is null). */
  function copyTo(id: string, to: string | null) {
    const t = dash?.tiles.find((x) => x.id === id);
    if (!t) return;
    let target = to ? app.dashboards.find((d) => d.id === to) : undefined;
    if (!target) {
      target = newDashboard(app.dashboards.length);
      app.snap!.workspace.dashboards.push(target);
      target = app.dashboards.find((d) => d.id === target!.id)!;
    }
    target.tiles = [...target.tiles, copyOf(t, target.tiles, t.title)];
    app.saveDashboards();
    app.toast(`Copied to ${target.title}`);
  }

  /** The arrow keys: the tile after (or before) this one in reading order gets the focus. */
  function navTile(id: string, step: 1 | -1) {
    const order = readingOrder(dash?.tiles ?? []);
    const next = order[order.findIndex((t) => t.id === id) + step];
    if (next) document.querySelector<HTMLElement>(`[data-tile="${CSS.escape(next.id)}"]`)?.focus();
  }

  /** Enter on a tile: edit it, or open it large when this is a phone or the tile cannot be edited. */
  function openTile(id: string) {
    if (!phone) selectedTile = id;
    else if (results[id]) focusId = id;
  }

  // Dragging and resizing work in grid units and compact as you go.
  function startDrag(e: PointerEvent, tile: Tile, mode: 'move' | 'resize') {
    if (e.button !== 0 || phone) return;
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
          : { ...orig, w: Math.max(2, Math.min(COLS - orig.x, orig.w + gx)), h: Math.max(minHeight(orig), orig.h + gy) };
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
    const p = phoneRects?.get(t.id);
    if (p) return { left: p.x, top: p.y, w: p.w, h: p.h };
    return {
      left: t.x * (colW + GAP),
      top: t.y * (ROW + GAP),
      w: t.w * colW + (t.w - 1) * GAP,
      h: t.h * ROW + (t.h - 1) * GAP,
    };
  }

  let exporting = $state<string | null>(null);
  let exportMenu = $state<{ x: number; y: number } | null>(null);

  /** Everything a picture of the dashboard needs: the tiles as they are drawn, with their results. */
  function picture(theme: 'light' | 'dark'): DashboardPicture {
    // A number card goes in with its style merged in, as the app draws it (build 08).
    const shown = (dash!.tiles ?? []).map((t) => drawnTile(shownOf(t), app.snap?.workspace.cardStyles));
    const rs: DashboardPicture['results'] = {};
    for (const t of shown) rs[t.id] = results[t.id]?.res ?? null;
    return {
      title: dash!.title,
      subtitle: `${app.snap!.workspace.name} · ${dateTimeMedium(new Date())}`,
      filters: allFilters.map((f) => filterText(f, app.tables)),
      tiles: shown,
      results: rs,
      tables: app.tables,
      theme,
      // A story prints as a column in its order, on A4 pages (build 11).
      ...(story ? { layout: 'flow' as const, order: flowOrder(dash!).map((t) => t.id), width: 1180 } : {}),
    };
  }

  async function exportPicture(kind: 'png' | 'pdf') {
    if (!dash || exporting) return;
    exporting = 'Preparing…';
    try {
      if (kind === 'png') await downloadDashboardPng(picture(app.theme));
      else await downloadDashboardPdf(picture('light'));
    } catch (e) {
      app.error(e);
    } finally {
      exporting = null;
    }
  }

  async function exportHtml() {
    if (!dash || exporting) return;
    let target: string | null = null;
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: 'save_html', name: dash.title });
      if (!paths[0]) return;
      target = paths[0].endsWith('.html') ? paths[0] : paths[0] + '.html';
    }
    exporting = 'Preparing…';
    const fs = $state.snapshot(filters) as DashFilter[];
    try {
      if (target) {
        await api.exportTo(dash.id, fs, target);
        app.toast(`Exported ${target.split(/[\\/]/).pop()}`);
      } else {
        const html = await api.exportHtml(dash.id, fs);
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

  // ---- copy and paste (build 09, V4-D11) ----------------------------------------------------------------------------

  /** Tiles picked with Shift-click: a copy takes them all. */
  let picked = $state<string[]>([]);
  /** A copy from another workspace, waiting for its fields to be matched. */
  let matching = $state<Clip | null>(null);
  let dashMenu = $state<{ x: number; y: number } | null>(null);
  let stylesOpen = $state(false);
  $effect(() => {
    void dash?.id;
    untrack(() => {
      picked = [];
      lastClicked = null;
    });
  });

  /** The tile clicked last without Shift: the first of the picked ones when Shift-click adds a second. */
  let lastClicked: string | null = null;
  function togglePick(id: string) {
    const first = lastClicked ?? selectedTile;
    const now = picked.length ? picked : first && first !== id && dash?.tiles.some((t) => t.id === first) ? [first] : [];
    picked = now.includes(id) ? now.filter((x) => x !== id) : [...now, id];
  }

  /** What a copy takes: the tiles picked with Shift-click, else the tile with the focus, else the one being edited. */
  function copyTargets(): Tile[] {
    const focused = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>('[data-tile]')?.dataset.tile;
    const ids = picked.length ? picked : focused ? [focused] : selectedTile ? [selectedTile] : [];
    return (dash?.tiles ?? []).filter((t) => ids.includes(t.id));
  }

  function clipOf(tiles: Tile[], whole = false): Clip {
    const tablesOf = Object.fromEntries(Object.entries(app.snap!.measure_info ?? {}).map(([id, m]) => [id, m.tables]));
    return buildClip($state.snapshot(tiles) as Tile[], app.snap!.workspace, app.tables, tablesOf, whole && dash ? { title: dash.title, grain: dash.grain ?? null } : undefined);
  }

  const copiedWords = (n: number) => (n === 1 ? 'Copied the tile' : `Copied ${n} tiles`);

  async function copyToClipboard(clip: Clip, said: string) {
    try {
      await navigator.clipboard.writeText(clipText(clip));
      app.toast(said);
    } catch {
      app.toast('The clipboard is not open to this page. Use Ctrl+C (⌘C on a Mac) on a tile instead.', 'error');
    }
  }

  /** A field, a text box or selected words copy and paste as usual. */
  function ownClipboard(e: ClipboardEvent): boolean {
    const el = e.target as HTMLElement | null;
    if (el && (['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName) || el.isContentEditable)) return false;
    if (e.type === 'copy' && (window.getSelection()?.toString() ?? '') !== '') return false;
    return !(app.settingsOpen || app.addOpen || publishing || focusId || rowsReq || matching || stylesOpen || document.querySelector('[role="dialog"]'));
  }

  function onCopy(e: ClipboardEvent) {
    if (!dash || phone || !ownClipboard(e) || !e.clipboardData) return;
    const tiles = copyTargets();
    if (!tiles.length) return;
    e.preventDefault();
    e.clipboardData.setData('text/plain', clipText(clipOf(tiles)));
    app.toast(copiedWords(tiles.length));
  }

  function onPaste(e: ClipboardEvent) {
    if (!dash || phone || !ownClipboard(e)) return;
    // Text that is not a copy of tiles does nothing on a dashboard (a table is pasted in the Data view).
    const clip = readClip(e.clipboardData?.getData('text/plain') ?? '');
    if (!clip) return;
    e.preventDefault();
    paste(clip);
  }

  async function pasteFromMenu() {
    let text = '';
    try {
      text = await navigator.clipboard.readText();
    } catch {
      app.toast('The clipboard is not open to this page. Press Ctrl+V (⌘V on a Mac) on the dashboard instead.', 'error');
      return;
    }
    const clip = readClip(text);
    if (clip) paste(clip);
    else app.toast('The clipboard holds no Tenun tiles. Copy a tile or a dashboard first.');
  }

  function paste(clip: Clip) {
    if (!dash) return;
    if (sameWorkspace(clip, app.tables)) void land(clip, null);
    else matching = clip;
  }

  /**
   * New named measures are saved with the engine, which checks each formula again. One it refuses is left out and named in
   * a message; its tiles say the measure is missing. Gives the id each copied measure has now.
   */
  async function saveMeasures(here: NamedMeasure[], added: NamedMeasure[], ids: Record<string, string | { added: number }>): Promise<Record<string, string>> {
    const kept = [...added];
    const left: string[] = [];
    while (kept.length) {
      try {
        const r = await api.setMeasures([...here, ...kept]);
        if (!('saved' in r)) break;
        app.snap = r.saved;
        if (left.length) app.toast(`Not pasted: ${left.join(' ')}`, 'error');
        return measureIds(ids, added, kept, r.saved.workspace.measures ?? [], here.length);
      } catch (e) {
        if (!(e instanceof MeasureRefused) || e.measure < here.length) {
          app.error(e);
          break;
        }
        left.push(`${kept[e.measure - here.length].name}: ${e.message}`);
        kept.splice(e.measure - here.length, 1);
      }
    }
    if (left.length) app.toast(`Not pasted: ${left.join(' ')}`, 'error');
    return measureIds(ids, added, [], [], here.length);
  }

  /** Tiles of a copy land on this dashboard (or, for a copied dashboard, on a new one), with what they need. */
  async function land(clip: Clip, m: Matches | null) {
    matching = null;
    if (!dash) return;
    // Styles first: saving measures makes the app reload the workspace.
    const styles = app.snap!.workspace.cardStyles ?? [];
    const st = joinStyles(clip.cardStyles, styles, () => newId('cs-'));
    if (st.list.length !== styles.length) await app.saveCardStyles(st.list);
    const here = $state.snapshot(app.snap!.workspace.measures ?? []) as NamedMeasure[];
    const renames = m ? renamesFor(clip, m, app.tables) : [];
    const joined = joinMeasures(clip.measures, here, (f) => rewriteFormula(f, renames));
    const mids = joined.added.length ? await saveMeasures(here, joined.added, joined.ids) : (joined.ids as Record<string, string>);
    const links = new Set(app.snap!.workspace.relationships.map((r) => r.id));
    const tiles = landTiles(clip, m, { measures: mids, styles: st.ids }, links, () => newId('t'));
    if (clip.dashboard) {
      // A whole dashboard becomes a new one, after this one, with its tiles in their places.
      const d: Dashboard = { ...newDashboard(app.dashboards.length), title: freeName(clip.dashboard.title, app.dashboards.map((x) => x.title)), tiles, grain: clip.dashboard.grain ?? undefined };
      app.snap!.workspace.dashboards.push(d);
      app.saveDashboards();
      app.toast(`Pasted the dashboard as ${d.title}`);
      go({ view: 'dashboard', id: d.id });
      return;
    }
    const placed = placeBelow(tiles, dash.tiles);
    update((ts) => [...ts, ...placed]);
    picked = placed.length > 1 ? placed.map((t) => t.id) : [];
    app.toast(placed.length === 1 ? 'Pasted the tile' : `Pasted ${placed.length} tiles`);
    await nextTick();
    document.querySelector<HTMLElement>(`[data-tile="${CSS.escape(placed[0]?.id ?? '')}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  /** A copy of this dashboard, after it in the list. Undo takes it away. */
  function duplicateDashboard() {
    if (!dash) return;
    const d = dashboardCopy($state.snapshot(dash) as Dashboard, freeName(`${dash.title} copy`, app.dashboards.map((x) => x.title)), newId);
    const at = app.dashboards.findIndex((x) => x.id === dash.id);
    app.snap!.workspace.dashboards.splice(at + 1, 0, d);
    app.saveDashboards();
    past.push({ added: structuredClone(d) });
    if (past.length > MAX_STEPS) past.shift();
    future.length = 0;
    historyTick++;
    app.toast(`Duplicated as ${d.title}`);
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

<svelte:window onkeydown={onKeydown} oncopy={onCopy} onpaste={onPaste} />

{#if dash && publishing}
  <PublishSheet {dash} onclose={() => (publishing = false)} />
{/if}

{#if dash}
  <header class="toolbar" class:phone>
    {#if editingTitle && !phone}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="field title-input"
        value={dash.title}
        autofocus
        onblur={(e) => {
          const next = e.currentTarget.value.trim() || dash.title;
          if (next !== dash.title) {
            record();
            dash.title = next;
          }
          editingTitle = false;
          app.saveDashboards();
        }}
        onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
    {:else}
      <button class="title" title={phone ? undefined : 'Rename'} disabled={phone} onclick={() => (editingTitle = true)}>{dash.title}</button>
    {/if}
    <div class="chips">
      {#each pins as f (f.fromTile)}
        <span class="chip pinned" title="Saved with the dashboard">
          <Icon name="pin" size={12} />
          {filterText(f, app.tables)}
          <button aria-label="Remove saved filter" onclick={() => unpin(f)}><Icon name="x" size={11} /></button>
        </span>
      {/each}
      {#each filters as f (f.fromTile)}
        <span class="chip" data-chip={f.fromTile}>
          <Icon name="filter" size={12} />
          <button
            class="say"
            aria-haspopup="menu"
            title="Keep only or exclude"
            onclick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              chipMenu = chipMenu?.id === f.fromTile ? null : { id: f.fromTile, x: r.left, y: r.bottom + 4 };
            }}>
            {filterText(f, app.tables)}
          </button>
          <button aria-label="Clear filter" onclick={() => (filters = filters.filter((x) => x !== f))}><Icon name="x" size={11} /></button>
        </span>
      {/each}
      {#if filters.length > 1}
        <button class="btn plain small" onclick={() => (filters = [])}>Clear all</button>
      {/if}
    </div>
    <span class="spacer"></span>
    {#if grainOn}
      <div class="seg" role="radiogroup" aria-label="Time grain for every chart over time">
        <button role="radio" aria-checked={!dash.grain} class:on={!dash.grain} title="Each tile uses its own grain" onclick={() => setGrain(null)}>Auto</button>
        {#each PERIODS as g}
          <button role="radio" aria-checked={dash.grain === g} class:on={dash.grain === g} onclick={() => setGrain(g)}>{g}</button>
        {/each}
      </div>
    {/if}
    {#if newestLoad}
      <span class="stamp" title={stampTitle}>Updated {stampLabel(newestLoad)}</span>
    {/if}
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
    {#if !phone}
      <button class="btn plain icon" title="Undo (Ctrl+Z)" aria-label="Undo" disabled={!canUndo} onclick={undo}><Icon name="undo" /></button>
      <button class="btn plain icon" title="Redo (Ctrl+Shift+Z)" aria-label="Redo" disabled={!canRedo} onclick={redo}><Icon name="redo" /></button>
    {/if}
    <button class="btn plain icon" title="Refresh now" aria-label="Refresh now" onclick={() => tick++}><Icon name="refresh" /></button>
    {#if !phone && CLOUD_ON}
    <button
      class="btn"
      class:live={!!dash.publish?.url}
      class:warn={!!dash.publish?.error}
      onclick={() => (publishing = true)}
      disabled={!dash.tiles.length}
      title={dash.publish?.error ?? (dash.publish?.url ? 'Published on tenun.cloud' : 'Share this dashboard online')}>
      <Icon name={dash.publish?.error ? 'warning' : 'globe'} /> {dash.publish?.url ? 'Published' : 'Publish'}
    </button>
    {/if}
    <button
      class="btn"
      aria-haspopup="menu"
      aria-expanded={!!exportMenu}
      disabled={!dash.tiles.length || !!exporting}
      title="Save this dashboard as a web page, a picture or a PDF"
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        exportMenu = exportMenu ? null : { x: r.right - 200, y: r.bottom + 4 };
      }}>
      <Icon name="export" /> {exporting ?? 'Export'}
    </button>
    {#if !phone}
    <div class="add">
      <button class="btn primary" onclick={() => (addMenu = !addMenu)} aria-expanded={addMenu}><Icon name="plus" /> Add tile</button>
      {#if addMenu}
        <div class="menu" role="menu">
          {#each [['kpi', 'Number'], ['bar', 'Bar chart'], ['line', 'Line chart'], ['area', 'Area chart'], ['share', 'Share of total'], ['pie', 'Pie chart'], ['donut', 'Donut chart'], ['table', 'Table'], ['pivot', 'Pivot table'], ['map', 'Map'], ['filter', 'Filter'], ['text', 'Text'], ['image', 'Image'], ...(advanced ? [['heatmap', 'Heat map'], ['funnel', 'Funnel'], ['box', 'Box plot'], ['bullet', 'Bullet chart'], ['compare', 'Compare two values'], ['sankey', 'Sankey'], ['calendar', 'Calendar heat map'], ['cohort', 'Cohort grid'], ...(hasTwoDates(app.tables) ? [['gantt', 'Gantt chart']] : [])] : [])] as [t, label]}
            <button role="menuitem" onclick={() => addTile(t as TileType)}>
              <Icon name={(t === 'calendar' ? 'calendarheat' : t) as IconName} /> {label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button
      class="btn plain icon"
      title="More"
      aria-label="More for this dashboard"
      aria-haspopup="menu"
      aria-expanded={!!dashMenu}
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        dashMenu = dashMenu ? null : { x: r.right - 220, y: r.bottom + 4 };
      }}><Icon name="more" /></button>
    <button class="btn plain icon" title="Delete dashboard" aria-label="Delete dashboard" onclick={removeDashboard}><Icon name="trash" /></button>
    {/if}
  </header>

  {#each failed as src (src.id)}
    <div class="banner" role="alert">
      <Icon name="warning" size={14} />
      <span>
        {src.name} could not be read: {app.snap!.errors[src.id].replace(/\.$/, '')}.
        Tiles that use it show an error until it is fixed.
      </span>
    </div>
  {/each}

  {#if dashMenu}
    <Menu
      x={dashMenu.x}
      y={dashMenu.y}
      items={[
        { label: 'Copy dashboard', run: () => copyToClipboard(clipOf(dash!.tiles, true), 'Copied the dashboard'), disabled: !dash.tiles.length },
        { label: 'Duplicate dashboard', run: duplicateDashboard },
        { label: 'Paste', run: pasteFromMenu },
        { label: 'Card styles…', run: () => (stylesOpen = true) },
        { label: 'Layout', sub: [{ label: story ? 'Grid' : 'Grid  ✓', run: () => setLayout('grid') }, { label: story ? 'Story  ✓' : 'Story', run: () => setLayout('flow') }] },
      ]}
      onclose={() => (dashMenu = null)} />
  {/if}
  {#if insertAt}
    {@const place = insertAt.at}
    <Menu
      x={insertAt.x}
      y={insertAt.y}
      items={[['text', 'Text'], ['kpi', 'Number'], ['bar', 'Bar chart'], ['line', 'Line chart'], ['area', 'Area chart'], ['pie', 'Pie chart'], ['map', 'Map'], ['table', 'Table'], ['image', 'Image']].map(([t, label]) => ({ label, run: () => addTile(t as TileType, place) }))}
      onclose={() => (insertAt = null)} />
  {/if}
  {#if matching}
    <MatchFields clip={matching} tables={app.tables} onpaste={(m) => matching && land(matching, m)} onclose={() => (matching = null)} />
  {/if}
  {#if stylesOpen}
    <CardStyles onclose={() => (stylesOpen = false)} />
  {/if}

  {#if exportMenu}
    <Menu
      x={exportMenu.x}
      y={exportMenu.y}
      items={[
        { label: 'Web page (HTML)', run: exportHtml },
        { label: 'Image (PNG)', run: () => exportPicture('png') },
        { label: 'PDF', run: () => exportPicture('pdf') },
      ]}
      onclose={() => (exportMenu = null)} />
  {/if}

  {#if chipMenu}
    {@const from = chipMenu?.id ?? ''}
    {@const f = filters.find((x) => x.fromTile === from)}
    {#if f}
      <Menu
        x={chipMenu?.x ?? 0}
        y={chipMenu?.y ?? 0}
        items={[{ label: 'Keep only', run: () => keep(from, 'keep') }, ...(canExclude(f) ? [{ label: 'Exclude', run: () => keep(from, 'exclude') }] : [])]}
        onclose={() => (chipMenu = null)} />
    {/if}
  {/if}

  {#if focusTile && results[focusTile.id]}
    <FocusSheet
      tile={drawnTile(eff(focusTile), app.snap?.workspace.cardStyles)}
      res={results[focusTile.id]!.res}
      colors={readColors()}
      title={tileTitle(eff(focusTile), app.tables)}
      tables={app.tables}
      onclose={() => (focusId = null)} />
  {/if}
  {#if rowsReq}
    <RowsSheet title={rowsReq.title} data={rowsReq.data} error={rowsReq.error} onclose={() => (rowsSeq++, (rowsReq = null))} />
  {/if}

  <div class="body">
    <div class="scroll" class:paper={story} onpointerdown={(e) => e.target === e.currentTarget && (selectedTile = null)} role="presentation">
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
      {#snippet tileView(t: Tile, inFlow: boolean)}
        <TileView
          tile={shownOf(t)}
          filters={allFilters}
          {tick}
          active={selectedTile === t.id}
          onpick={(v, otherOf, bins, stage) => pick(shownOf(t), v, otherOf, bins, stage)}
          onpickcol={(v) => pickCol(shownOf(t), v)}
          onresult={(r, k) => setResult(t.id, r, k)}
          onfilter={(f) => setFilter(t.id, f)}
          onrows={(v, otherOf, bins) => showRows(t.id, v, otherOf, bins)}
          onfocus={() => (focusId = t.id)}
          onselect={() => (selectedTile = t.id)}
          onwidths={(w) => change({ ...(dash.tiles.find((x) => x.id === t.id) ?? t), widths: w })}
          onlayout={(layout) => change({ ...(dash.tiles.find((x) => x.id === t.id) ?? t), layout })}
          {phone}
          dashboards={app.dashboards.filter((d) => d.id !== dash.id).map((d) => ({ id: d.id, title: d.title }))}
          onduplicate={() => duplicate(t.id)}
          oncopy={(to) => copyTo(t.id, to)}
          onclip={() => {
            const tiles = picked.includes(t.id) ? copyTargets() : dash.tiles.filter((x) => x.id === t.id);
            copyToClipboard(clipOf(tiles), copiedWords(tiles.length));
          }}
          onremove={() => removeTile(t.id)}
          onnav={(step) => navTile(t.id, step)}
          onopen={() => openTile(t.id)}
          flow={inFlow ? (t.flow ?? {}) : undefined}
          onflow={inFlow ? (f) => setFlow(t.id, f) : undefined}
          ondragstart={(e) => !inFlow && startDrag(e, dash.tiles.find((x) => x.id === t.id)!, 'move')}
          onresizestart={(e) => !inFlow && startDrag(e, dash.tiles.find((x) => x.id === t.id)!, 'resize')} />
      {/snippet}
      {#if story}
        <!-- A story reads like a document: a column of tiles in the page's order (build 11). -->
        <div class="story">
          <FlowLayout tiles={flowOrder(dash).map(shownOf)} editing={!phone} onadd={(at, e) => openInsert(at, e)} onmove={moveTile}>
            {#snippet cell(t)}{@render tileView(t, true)}{/snippet}
          </FlowLayout>
        </div>
      {:else}
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
          <!-- Shift-click picks a tile for a copy, with the others picked (build 09); a plain click lets them go. -->
          <div
            class="cell"
            class:lifted={dragging?.id === t.id}
            class:picked={picked.includes(t.id)}
            onclickcapture={(e) => {
              if (e.shiftKey && !phone) {
                e.preventDefault();
                e.stopPropagation();
                togglePick(t.id);
              } else {
                lastClicked = t.id;
                if (picked.length) picked = [];
              }
            }}
            style:transform={dragging?.id === t.id
              ? `translate(${p.left + dragging.dx}px, ${p.top + dragging.dy}px)`
              : `translate(${p.left}px, ${p.top}px)`}
            style:width="{p.w}px"
            style:height="{p.h}px">
            {@render tileView(t, false)}
          </div>
        {/each}
      </div>
      {/if}
    </div>
    {#if sel && !phone}
      <Inspector
        tile={sel}
        current={results[sel.id] ?? null}
        filters={allFilters}
        dashGrain={dash.grain ?? null}
        onclose={() => (selectedTile = null)}
        onchange={change}
        onremove={() => removeTile(sel.id)} />
    {/if}
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 52px;
    padding: 6px 12px 6px 16px;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    flex: none;
  }
  .toolbar.phone { min-height: 44px; flex-wrap: wrap; padding: 6px 10px; }
  .toolbar.phone .spacer { display: none; }
  .toolbar.phone .chips { flex-wrap: wrap; overflow: visible; }
  .title:disabled { color: var(--text); cursor: default; }
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
  .chips { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; min-width: 0; }
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
    flex: none;
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
  .chip button.say { width: auto; height: 22px; padding: 0 3px; border-radius: 4px; font: inherit; }
  .chip.pinned { background: var(--inset); color: var(--text); padding-left: 9px; }
  .chip.pinned :global(svg:first-child) { color: var(--text-2); }
  .seg { display: flex; padding: 2px; border-radius: 8px; background: var(--inset); }
  .seg button {
    height: 24px;
    padding: 0 9px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-size: 12px;
    text-transform: capitalize;
  }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .small { height: 24px; font-size: 12px; }
  .spacer { flex: 1; }
  .stamp { font-size: 12px; color: var(--text-2); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 16px;
    font-size: 12px;
    background: color-mix(in srgb, var(--danger) 10%, var(--surface));
    border-bottom: 1px solid var(--sep);
    flex: none;
  }
  .banner :global(svg) { color: var(--danger); }
  .btn.live :global(svg) { color: var(--success); }
  .btn.warn :global(svg) { color: var(--danger); }
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
  @media (max-width: 699px) { .scroll { padding: 10px; } }
  .grid { position: relative; min-height: 100%; }
  /* A story is a page: its tiles sit on the surface, in a column (build 11). */
  .scroll.paper { background: var(--surface); }
  .story { padding: 8px 28px 0; }
  .cell {
    position: absolute;
    top: 0;
    left: 0;
    transition: transform 250ms var(--spring), width 250ms var(--spring), height 250ms var(--spring);
  }
  .cell.lifted { transition: none; z-index: 10; }
  .cell.lifted :global(.tile) { box-shadow: var(--shadow-float); }
  /* Picked for a copy with Shift-click. */
  .cell.picked :global(.tile) { box-shadow: 0 0 0 2px var(--accent); }
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
