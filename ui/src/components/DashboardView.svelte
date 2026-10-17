<script lang="ts">
  import { onMount, tick as nextTick, untrack } from 'svelte';
  import { api, MeasureRefused, type Bins, type Cell, type Dashboard, type DashFilter, type DrillState, type Filter, type Grain, type SavedView, type NamedMeasure, type QueryResult, type RowsBehind, type Tile, type TileFilter, type TileType } from '../lib/api';
  import { app, go } from '../lib/state.svelte';
  import { cell, dateTimeMedium, dimLabel, pretty, stampLabel, tileTitle } from '../lib/format';
  import { portal } from '../lib/portal';
  import FilterView from './dash/FilterView.svelte';
  import { filterText } from '../lib/filterText';
  import { canExclude, colFilter, everyPageFilters, everySource, hasPeriod, markFilter, newPinId, PERIODS, pinFilter, reachOf, sameFilter, tileFilters, withGrain } from '../lib/filters';
  import ReachSheet from './dash/ReachSheet.svelte';
  import AlertSheet from './dash/AlertSheet.svelte';
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
  import { shownTile, TOP } from '../lib/drilldown';
  import { currentState, newViewId, sameAsView, tilesWithView, viewClicks, viewDrills, viewOf } from '../lib/views';
  import { paramValues } from '../lib/params';
  import ViewsMenu from './dash/ViewsMenu.svelte';
  import { flowOrder, insertInOrder, isFlow, moveInOrder } from '../lib/flow';
  import FlowLayout from './dash/FlowLayout.svelte';
  import { DRILL, drillOf } from '../lib/site';
  import { buildClip, clipText, dashboardCopy, freeName, withReach, joinMeasures, joinStyles, landTiles, measureIds, placeBelow, readClip, renamesFor, rewriteFormula, sameWorkspace, type Clip, type Matches } from '../lib/clipboard';
  import MatchFields from './dash/MatchFields.svelte';
  import CardStyles from './dash/CardStyles.svelte';
  import { t, tn } from '../lib/i18n';
  // tenun.cloud is not live yet; build with VITE_TENUN_CLOUD=1 to show Publish.
  const CLOUD_ON = import.meta.env.VITE_TENUN_CLOUD === '1';

  let { id }: { id: string } = $props();

  const ROW = 76;
  const GAP = 12;

  const dash = $derived(app.dashboards.find((d) => d.id === id));
  let filters = $state<DashFilter[]>([]);
  /** A drill-through that opened this page: its filter is the first, and Back returns where it came from (build 12). */
  let drillFrom = $state<{ id: string; title: string } | null>(null);
  // This view is made again for each page (App keys it by the page), so reading the page once is right.
  // svelte-ignore state_referenced_locally
  if (app.drill && app.drill.page === id) {
    filters = [app.drill.filter];
    drillFrom = app.drill.from;
    app.drill = null;
  }
  /** Where each tile is drilled to (v5 build 01), kept here so a saved view can hold it (build 03). */
  let drills = $state<Record<string, DrillState>>({});
  /** The saved view the dashboard was last set to (build 03). */
  let openView = $state<string | null>(null);
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
  // The selections of filter tiles on other pages that apply to every page (v5 build 02, V5-D37).
  const everyPage = $derived(
    dash ? everyPageFilters(app.dashboards, dash.id, (t, c) => app.table(t)?.columns.find((x) => x.name === c)?.dtype) : [],
  );
  const allFilters = $derived<DashFilter[]>([...pins, ...everyPage, ...selections, ...filters]);
  /** The page an every-page filter comes from, for its chip. */
  const pageTitle = (fromTile: string) => app.dashboards.find((d) => d.id === everySource(fromTile)?.dashboard)?.title || t('another page');
  /** Clears an every-page filter where it comes from: the filter tile on its page. */
  function clearEvery(fromTile: string) {
    const src = everySource(fromTile);
    const d = app.snap?.workspace.dashboards.find((x) => x.id === src?.dashboard);
    const t = d?.tiles.find((x) => x.id === src?.tile);
    if (!t?.filter) return;
    t.filter = { ...t.filter, values: [], range: undefined, preset: undefined };
    app.saveDashboards();
  }
  /** The tile whose "Filters other tiles" sheet is open (v5 build 02). */
  let reachFor = $state<string | null>(null);
  /** The number tile whose alert sheet is open (v5 build 12). */
  let alertFor = $state<string | null>(null);

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
    // A page opens on its default view, unless a drill-through opened it (build 03).
    const first = dash?.defaultView ? dash.views?.find((v) => v.id === dash.defaultView) : undefined;
    if (first && !drillFrom) applyView(first, false);
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
    if (e.key === 'Escape' && filterPop) {
      filterPop = null;
      return;
    }
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

  /** The sources one tile reads. */
  function sourceIdsOf(t: Tile): Set<string> {
    const ids = new Set<string>();
    // A named measure reads the tables of its formula.
    const named = t.measures.filter((m) => m.named).flatMap((m) => app.snap!.measure_info?.[m.named!]?.tables ?? []);
    const refs = [...t.measures.filter((m) => !m.named).map((m) => m.table), ...named, ...t.dims.map((d) => d.table), ...(t.cols ? [t.cols.table] : []), ...(t.repeat ? [t.repeat.table] : [])];
    for (const table of refs) {
      // A source that failed to load has no tables left, so fall back to the id prefix.
      if (table) ids.add(app.table(table)?.source_id ?? table.split('/')[0]);
    }
    return ids;
  }
  // The sources this dashboard reads: the chips under the title, the freshness stamp and the failed-refresh banner.
  const usedSources = $derived.by(() => {
    const ids = new Set<string>();
    for (const t of dash?.tiles ?? []) for (const id of sourceIdsOf(t)) ids.add(id);
    return app.snap!.workspace.sources.filter((s) => ids.has(s.id));
  });
  /** A tile's subtitle: the sources it reads, when the dashboard reads more than one (so it tells something). */
  function subtitleOf(tl: Tile): string {
    if (usedSources.length < 2 || ['kpi', 'text', 'image', 'filter', 'param'].includes(tl.type)) return '';
    const ids = sourceIdsOf(tl);
    const names = usedSources.filter((s) => ids.has(s.id)).map((s) => s.name);
    if (!names.length) return '';
    return t('From {sources}', { sources: names.length > 1 ? t('{list} and {last}', { list: names.slice(0, -1).join(', '), last: names.at(-1)! }) : names[0] });
  }
  const DB_KINDS = new Set(['sqlite', 'duckdb', 'postgres', 'mysql']);
  /** Undo and redo name their keys the way this computer writes them. */
  const MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
  const KEYS = { undo: MAC ? '⌘Z' : 'Ctrl+Z', redo: MAC ? '⇧⌘Z' : 'Ctrl+Shift+Z' };

  // ---- the filter bar: the dashboard's own filters, shown in one row (look v3) ----
  /** The filter tiles of this page, each one a button in the filter bar that opens its control. */
  const filterTiles = $derived((dash?.tiles ?? []).filter((t) => t.type === 'filter' && t.dims[0]));
  function filterChip(tl: Tile) {
    const d = tl.dims[0];
    const sel = selections.find((f) => f.fromTile === tl.id);
    const dtype = app.table(d.table)?.columns.find((c) => c.name === d.column)?.dtype;
    const name = tl.title?.trim() || pretty(dimLabel({ table: d.table, column: d.column, via: d.via }, app.tables));
    const text = sel ? filterText(sel, app.tables) : '';
    return { name, value: sel ? text.slice(text.indexOf(': ') + 2) || text : t('All'), on: !!sel, date: dtype === 'date' || dtype === 'datetime' };
  }
  /** The filter tile whose control is open under its button in the filter bar. */
  let filterPop = $state<{ id: string; x: number; y: number } | null>(null);
  function openFilter(t: Tile, e: MouseEvent) {
    if (filterPop?.id === t.id) return void (filterPop = null);
    // Before the tile has its values there is nothing to show, so the tile itself is shown.
    if (!results[t.id]?.res) {
      const el = document.querySelector<HTMLElement>(`[data-tile="${CSS.escape(t.id)}"]`);
      el?.scrollIntoView({ block: 'nearest' });
      el?.focus({ preventScroll: true });
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    filterPop = { id: t.id, x: Math.max(8, Math.min(r.left, innerWidth - 288)), y: r.bottom + 4 };
  }
  function clearFilterTile(t: Tile) {
    setFilter(t.id, { style: t.filter?.style ?? 'list', ...(t.filter?.single ? { single: true } : {}) });
  }
  /** A chart whose marks can be clicked: the hint at the end of the filter bar only says what is true. */
  const clickable = $derived((dash?.tiles ?? []).some((t) => !['kpi', 'text', 'image', 'filter', 'param', 'cohort'].includes(t.type) && t.dims.length > 0 && !t.action));
  const loadedAt = $derived(app.snap!.loaded_at ?? {});
  const newestLoad = $derived(
    usedSources.map((s) => loadedAt[s.id]).filter(Boolean).sort().at(-1) ?? null,
  );
  const stampTitle = $derived(
    usedSources
      .map((s) => {
        const at = loadedAt[s.id] ? stampLabel(loadedAt[s.id]) : '';
        if (app.snap!.errors[s.id]) return at ? t('{source}: could not be read (last loaded {at})', { source: s.name, at }) : t('{source}: could not be read', { source: s.name });
        return `${s.name}: ${at || t('not loaded')}`;
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
      // A new parameter tile moves the first parameter (v5 build 05); its options pick another.
      ...(type === 'param' && app.snap?.workspace.params?.length ? { param: { id: app.snap.workspace.params[0].id } } : {}),
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
  /** What a click on a tile's mark does: filter this page (null), or open a page (drill-through, build 12). */
  function setAction(id: string, page: string | null) {
    const t = dash?.tiles.find((x) => x.id === id);
    if (!t) return;
    const { action: _old, ...rest } = t;
    change(page ? { ...rest, action: { page } } : rest);
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
    // A tile with an action opens its page, keeping only the clicked value; it does not filter this page (build 12).
    const drill = dash ? drillOf(tile, dash, app.dashboards, value, otherOf, bins, stage, results[tile.id]?.res?.geo) : null;
    if (drill) {
      app.drill = drill;
      go({ view: 'dashboard', id: drill.page });
      return;
    }
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
    // The click filters the tiles the source tile reaches (v5 build 02); not set, every tile.
    const source = dash?.tiles.find((x) => x.id === from);
    const reach = source ? reachOf(source) : undefined;
    filters = [...others, { ...f, fromTile: from, ...(reach ? { to: reach } : {}) }];
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

  // ---- saved views (build 03) ----
  const views = $derived(dash?.views ?? []);
  const stateNow = $derived(dash ? currentState(dash, filters, drills, paramValues(app.snap?.workspace.params ?? [])) : null);
  const viewChanged = $derived.by(() => {
    const v = views.find((x) => x.id === openView);
    return !!v && !!stateNow && !sameAsView(stateNow, v);
  });
  /** Sets the dashboard as the view reads it: the filter tiles' selections, the grain, the clicks and the drill paths. */
  function applyView(v: SavedView, step = true) {
    if (!dash) return;
    const tiles = tilesWithView(dash.tiles, v);
    const grain = v.grain ?? null;
    if (JSON.stringify(tiles) !== JSON.stringify(dash.tiles) || (dash.grain ?? null) !== grain) {
      if (step) record();
      dash.tiles = tiles;
      dash.grain = grain;
      app.saveDashboards();
    }
    filters = viewClicks(dash.tiles, v);
    drills = viewDrills(dash.tiles, v);
    openView = v.id;
    // The view's parameter values (v5 build 05): saved in the workspace, as a moved control saves them.
    const values = paramValues(app.snap?.workspace.params ?? []);
    const moved = Object.fromEntries(Object.entries(v.params ?? {}).filter(([id, x]) => id in values && values[id] !== x));
    if (Object.keys(moved).length) app.setParamValues(moved);
  }
  function saveViews(list: SavedView[], extra: Partial<Dashboard> = {}) {
    if (!dash) return;
    record();
    dash.views = list.length ? list : undefined;
    Object.assign(dash, extra);
    app.saveDashboards();
  }
  function saveView(name: string) {
    if (!dash || !stateNow) return;
    const id = newViewId(views);
    saveViews([...views, viewOf(id, name, $state.snapshot(stateNow) as typeof stateNow)]);
    openView = id;
  }
  function updateView(id: string) {
    if (!stateNow) return;
    saveViews(views.map((v) => (v.id === id ? viewOf(id, v.name, $state.snapshot(stateNow) as typeof stateNow) : v)));
  }
  function deleteView(id: string) {
    saveViews(
      views.filter((v) => v.id !== id),
      dash?.defaultView === id ? { defaultView: undefined } : {},
    );
    if (openView === id) openView = null;
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
  /** `shown`: a drilled tile as drawn at its level (v5 build 01); its marks are values of that level's field. */
  async function showRows(id: string, value?: Cell, otherOf?: Cell[], bins?: Bins, shown?: Tile) {
    const real = dash?.tiles.find((x) => x.id === id);
    if (!real) return;
    const t = shown ?? eff(real);
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
    const tl = dash?.tiles.find((x) => x.id === id);
    if (!dash || !tl) return;
    const copy = copyOf(tl, dash.tiles, tl.title ? t('{name} copy', { name: tl.title }) : '');
    update((ts) => [...ts, copy]);
    selectedTile = copy.id;
  }

  /** Copy a tile to another dashboard (or a new one when `to` is null). */
  function copyTo(id: string, to: string | null) {
    const tl = dash?.tiles.find((x) => x.id === id);
    if (!tl) return;
    let target = to ? app.dashboards.find((d) => d.id === to) : undefined;
    if (!target) {
      target = newDashboard(app.dashboards.length);
      app.snap!.workspace.dashboards.push(target);
      target = app.dashboards.find((d) => d.id === target!.id)!;
    }
    // On another page, the tiles its reach names are not there (v5 build 02).
    target.tiles = [...target.tiles, withReach(copyOf(tl, target.tiles, tl.title), new Map())];
    app.saveDashboards();
    app.toast(t('Copied to {page}', { page: target.title }));
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
    // A drilled tile goes in as it is drawn at its level (v5 build 01).
    const hierarchies = app.snap?.workspace.hierarchies ?? [];
    const shown = (dash!.tiles ?? []).map((t) => shownTile(drawnTile(shownOf(t), app.snap?.workspace.cardStyles), results[t.id]?.res?.drill, hierarchies));
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
    exporting = t('Preparing…');
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
    exporting = t('Preparing…');
    const fs = $state.snapshot(filters) as DashFilter[];
    try {
      if (target) {
        await api.exportTo(dash.id, fs, target);
        app.toast(t('Exported {file}', { file: target.split(/[\\/]/).pop() ?? '' }));
      } else {
        const html = await api.exportHtml(dash.id, fs);
        const a = document.createElement('a');
        a.href = URL.createObjectURL(new Blob([html], { type: 'text/html' }));
        a.download = `${dash.title.replace(/[\\/:*?"<>|]/g, '_')}.html`;
        a.click();
        setTimeout(() => URL.revokeObjectURL(a.href), 5000);
        app.toast(t('Exported'));
      }
    } catch (e) {
      app.error(e);
    } finally {
      exporting = null;
    }
  }

  /** Past this size an exported site says which pages make it big (build 13). */
  const SITE_WARN_BYTES = 25 * 1024 * 1024;
  const mb = (n: number) => `${(n / 1024 / 1024).toFixed(1)} MB`;

  /** The whole site, as one file or as a folder in a ZIP (build 13, V4-D15). */
  async function exportSite(zip: boolean) {
    if (exporting) return;
    const name = (app.snap?.workspace.site?.title || app.snap?.workspace.name || 'site').replace(/[\\/:*?"<>|]/g, '_');
    let target: string | null = null;
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: zip ? 'save_zip' : 'save_html', name });
      if (!paths[0]) return;
      const ext = zip ? '.zip' : '.html';
      target = paths[0].endsWith(ext) ? paths[0] : paths[0] + ext;
    }
    exporting = t('Preparing…');
    app.siteProgress = null;
    const tick = setInterval(() => app.siteProgress && (exporting = t('Page {done} of {total}…', { done: app.siteProgress.done, total: app.siteProgress.total })), 200);
    try {
      const { bytes, sizes } = await api.exportSite(zip, target ?? undefined);
      if (!target) {
        const a = document.createElement('a');
        a.href = URL.createObjectURL(new Blob([bytes as BlobPart], { type: zip ? 'application/zip' : 'text/html' }));
        a.download = `${name}${zip ? '.zip' : '.html'}`;
        a.click();
        setTimeout(() => URL.revokeObjectURL(a.href), 5000);
      }
      const total = target ? Object.values(sizes).reduce((t, n) => t + n, 0) : bytes.length;
      if (total > SITE_WARN_BYTES) {
        const big = Object.entries(sizes).sort((x, y) => y[1] - x[1]).slice(0, 3).map(([t, n]) => `${t} (${mb(n)})`).join(', ');
        app.toast(t('The site is {size}, which is slow to open. The biggest pages: {pages}.', { size: mb(total), pages: big }), 'error');
      } else app.toast(target ? t('Exported {file}', { file: target.split(/[\\/]/).pop() ?? '' }) : t('Exported the site'));
    } catch (e) {
      app.error(e);
    } finally {
      clearInterval(tick);
      exporting = null;
      app.siteProgress = null;
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

  const copiedWords = (n: number) => tn('Copied the tile', 'Copied {n} tiles', n);

  async function copyToClipboard(clip: Clip, said: string) {
    try {
      await navigator.clipboard.writeText(clipText(clip));
      app.toast(said);
    } catch {
      app.toast(t('The clipboard is not open to this page. Use Ctrl+C (⌘C on a Mac) on a tile instead.'), 'error');
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
      app.toast(t('The clipboard is not open to this page. Press Ctrl+V (⌘V on a Mac) on the dashboard instead.'), 'error');
      return;
    }
    const clip = readClip(text);
    if (clip) paste(clip);
    else app.toast(t('The clipboard holds no Tenun tiles. Copy a tile or a dashboard first.'));
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
        if (left.length) app.toast(t('Not pasted: {list}', { list: left.join(' ') }), 'error');
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
    if (left.length) app.toast(t('Not pasted: {list}', { list: left.join(' ') }), 'error');
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
      app.toast(t('Pasted the dashboard as {name}', { name: d.title }));
      go({ view: 'dashboard', id: d.id });
      return;
    }
    const placed = placeBelow(tiles, dash.tiles);
    update((ts) => [...ts, ...placed]);
    picked = placed.length > 1 ? placed.map((t) => t.id) : [];
    app.toast(tn('Pasted the tile', 'Pasted {n} tiles', placed.length));
    await nextTick();
    document.querySelector<HTMLElement>(`[data-tile="${CSS.escape(placed[0]?.id ?? '')}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  /** A copy of this dashboard, after it in the list. Undo takes it away. */
  function duplicateDashboard() {
    if (!dash) return;
    const d = dashboardCopy($state.snapshot(dash) as Dashboard, freeName(t('{name} copy', { name: dash.title }), app.dashboards.map((x) => x.title)), newId);
    const at = app.dashboards.findIndex((x) => x.id === dash.id);
    app.snap!.workspace.dashboards.splice(at + 1, 0, d);
    app.saveDashboards();
    past.push({ added: structuredClone(d) });
    if (past.length > MAX_STEPS) past.shift();
    future.length = 0;
    historyTick++;
    app.toast(t('Duplicated as {name}', { name: d.title }));
  }

  function removeDashboard() {
    if (!dash || !confirm(t('Delete the dashboard “{name}”?', { name: dash.title }))) return;
    app.snap!.workspace.dashboards = app.dashboards.filter((d) => d.id !== dash.id);
    app.saveDashboards();
    const next = app.dashboards[0];
    go(next ? { view: 'dashboard', id: next.id } : { view: 'home' });
  }

  const sel = $derived(dash?.tiles.find((x) => x.id === selectedTile));
  /** The words of the grains in the grain switch. */
  const grainWord = (g: Grain) => ({ day: t('day'), week: t('week'), month: t('month'), quarter: t('quarter'), year: t('year') } as Record<string, string>)[g] ?? g;
</script>

<svelte:window
  onkeydown={onKeydown}
  oncopy={onCopy}
  onpaste={onPaste}
  onpointerdown={(e) => filterPop && !(e.target as HTMLElement).closest('.fpop, .fchip') && (filterPop = null)} />

{#if dash && publishing}
  <PublishSheet {dash} onclose={() => (publishing = false)} />
{/if}

{#if dash}
  <!-- The toolbar: where you are, how fresh the data is, and what you can do (look v3). -->
  <header class="toolbar" class:phone>
    {#if !phone}
      <nav class="crumbs" aria-label={t('Breadcrumb')}>
        <button class="crumb" title={t('Home of this workspace')} onclick={() => go({ view: 'home' })}>{app.snap?.workspace.name || t('Workspace')}</button>
        <svg class="sep" width="12" height="12" viewBox="0 0 20 20" aria-hidden="true"><path d="M8 5l5 5-5 5" /></svg>
        <span class="crumb plain">{story ? t('Stories') : t('Dashboards')}</span>
        <svg class="sep" width="12" height="12" viewBox="0 0 20 20" aria-hidden="true"><path d="M8 5l5 5-5 5" /></svg>
        <span class="crumb here" aria-current="page">{dash.title}</span>
      </nav>
    {/if}
    <span class="spacer"></span>
    {#if newestLoad || failed.length}
      <span class="status" class:bad={failed.length > 0} title={stampTitle}>
        <i aria-hidden="true"></i>
        {#if failed.length}
          {tn('{n} source failed', '{n} sources failed', failed.length)}{newestLoad ? ` · ${t('updated {when}', { when: stampLabel(newestLoad) })}` : ''}
        {:else}
          {hasLive && dash.refresh ? t('Live · updated {when}', { when: stampLabel(newestLoad!) }) : t('Updated {when}', { when: stampLabel(newestLoad!) })}
        {/if}
      </span>
    {/if}
    {#if hasLive}
      <label class="refresh" title={t('How often database data is refreshed')}>
        <Icon name="clock" size={14} />
        <select
          value={String(dash.refresh)}
          onchange={(e) => {
            dash.refresh = Number(e.currentTarget.value);
            app.saveDashboards();
          }}>
          <option value="0">{t('Off')}</option>
          <option value="15">15 s</option>
          <option value="60">{t('{n} min', { n: 1 })}</option>
          <option value="300">{t('{n} min', { n: 5 })}</option>
        </select>
      </label>
    {/if}
    <span class="vsep" aria-hidden="true"></span>
    <div class="group" role="group" aria-label={t('History and refresh')}>
      {#if !phone}
        <button class="tb" aria-label={t('Undo')} disabled={!canUndo} onclick={undo}><Icon name="undo" size={16} /><span class="tip" role="tooltip">{t('Undo')} <kbd>{KEYS.undo}</kbd></span></button>
        <button class="tb" aria-label={t('Redo')} disabled={!canRedo} onclick={redo}><Icon name="redo" size={16} /><span class="tip" role="tooltip">{t('Redo')} <kbd>{KEYS.redo}</kbd></span></button>
      {/if}
      <button class="tb" aria-label={t('Refresh now')} onclick={() => tick++}><Icon name="refresh" size={16} /><span class="tip" role="tooltip">{t('Refresh now')}</span></button>
    </div>
    {#if !phone && CLOUD_ON}
    <button
      class="btn"
      class:live={!!dash.publish?.url}
      class:warn={!!dash.publish?.error}
      onclick={() => (publishing = true)}
      disabled={!dash.tiles.length}
      title={dash.publish?.error ?? (dash.publish?.url ? t('Published on tenun.cloud') : t('Share this dashboard online'))}>
      <Icon name={dash.publish?.error ? 'warning' : 'globe'} /> {dash.publish?.url ? t('Published') : t('Publish')}
    </button>
    {/if}
    <button
      class="btn"
      aria-haspopup="menu"
      aria-expanded={!!exportMenu}
      disabled={!dash.tiles.length || !!exporting}
      title={t('Save this dashboard as a web page, a picture or a PDF')}
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        exportMenu = exportMenu ? null : { x: r.right - 200, y: r.bottom + 4 };
      }}>
      <Icon name="export" /> {exporting ?? t('Export')}
    </button>
    {#if !phone}
    <div class="add">
      <button class="btn primary" onclick={() => (addMenu = !addMenu)} aria-expanded={addMenu}><Icon name="plus" /> {t('Add tile')}</button>
      {#if addMenu}
        <div class="menu" role="menu">
          {#each [['kpi', t('Number')], ['bar', t('Bar chart')], ['line', t('Line chart')], ['area', t('Area chart')], ['share', t('Share of total')], ['pie', t('Pie chart')], ['donut', t('Donut chart')], ['table', t('Table')], ['pivot', t('Pivot table')], ['map', t('Map')], ['filter', t('Filter')], ['text', t('Text')], ['image', t('Image')], ...(advanced ? [['heatmap', t('Heat map')], ['funnel', t('Funnel')], ['box', t('Box plot')], ['bullet', t('Bullet chart')], ['compare', t('Compare two values')], ['sankey', 'Sankey'], ['calendar', t('Calendar heat map')], ['cohort', t('Cohort grid')], ...(hasTwoDates(app.tables) ? [['gantt', t('Gantt chart')]] : []), ['param', t('Parameter')]] : [])] as [type, label]}
            <button role="menuitem" onclick={() => addTile(type as TileType)}>
              <Icon name={(type === 'calendar' ? 'calendarheat' : type) as IconName} /> {label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button
      class="btn icon more"
      aria-label={t('More for this dashboard')}
      aria-haspopup="menu"
      aria-expanded={!!dashMenu}
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        dashMenu = dashMenu ? null : { x: r.right - 200, y: r.bottom + 4 };
      }}><Icon name="more" /><span class="tip end" role="tooltip">{t('More')}</span></button>
    {/if}
  </header>

  {#snippet pageTop()}
  <!-- The page header: the title, the sources it reads, the saved views and the grain. -->
  <div class="pagehead" class:phone>
    <div class="titles">
      {#if editingTitle && !phone}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="field title-input"
          value={dash.title}
          aria-label={t('Name of the dashboard')}
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
        <h1><button class="title" title={phone ? undefined : t('Rename')} disabled={phone} onclick={() => (editingTitle = true)}>{dash.title}</button></h1>
      {/if}
      {#if usedSources.length}
        <div class="sources">
          <span>{usedSources.length === 1 ? t('From') : t('From {n} sources', { n: usedSources.length })}</span>
          {#each usedSources as src (src.id)}
            {@const bad = !!app.snap!.errors[src.id]}
            <span class="src" class:bad title={bad ? t('{source} could not be read', { source: src.name }) : loadedAt[src.id] ? `${src.name}: ${stampLabel(loadedAt[src.id])}` : src.name}>
              {#if bad}
                <Icon name="warning" size={12} />
              {:else if DB_KINDS.has(src.kind)}
                <svg width="12" height="12" viewBox="0 0 20 20" aria-hidden="true"><path d="M4.5 5.5c0-1.1 2.5-2 5.5-2s5.5.9 5.5 2-2.5 2-5.5 2-5.5-.9-5.5-2zM4.5 5.5v9c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2v-9M4.5 10c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2" /></svg>
              {:else}
                <svg width="12" height="12" viewBox="0 0 20 20" aria-hidden="true"><path d="M4 3.5h12v13H4zM4 7.5h12M4 11.5h12M8.5 3.5v13" /></svg>
              {/if}
              {src.name}
            </span>
          {/each}
        </div>
      {/if}
    </div>
    <div class="headtools">
      <ViewsMenu
        {views}
        open={openView}
        changed={viewChanged}
        defaultView={dash.defaultView}
        editing={advancedOn() && !phone}
        onopen={(id) => {
          const v = views.find((x) => x.id === id);
          if (v) applyView(v);
        }}
        onsave={saveView}
        onupdate={updateView}
        onrename={(id, name) => saveViews(views.map((v) => (v.id === id ? { ...v, name } : v)))}
        ondelete={deleteView}
        ondefault={(id) => saveViews(views, { defaultView: id ?? undefined })} />
      {#if grainOn}
        <div class="seg" role="radiogroup" aria-label={t('Time grain for every chart over time')}>
          <button role="radio" aria-checked={!dash.grain} class:on={!dash.grain} title={t('Each tile uses its own grain')} onclick={() => setGrain(null)}>{t('Auto')}</button>
          {#each PERIODS as g}
            <button role="radio" aria-checked={dash.grain === g} class:on={dash.grain === g} onclick={() => setGrain(g)}>{grainWord(g)}</button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <!-- The filter bar: every filter this page has now, from its filter tiles, other pages, saved filters and clicks. -->
  {#if !phone || drillFrom || filterTiles.length || pins.length || everyPage.length || filters.length}
    <div class="filterbar" class:phone role="toolbar" aria-label={t('Filters')}>
      {#if drillFrom}
        <button class="fchip back" title={t('Back to {page}', { page: drillFrom.title })} onclick={() => drillFrom && go({ view: 'dashboard', id: drillFrom.id })}><Icon name="chevronLeft" size={12} /> {drillFrom.title}</button>
        <span class="vsep" aria-hidden="true"></span>
      {/if}
      {#each filterTiles as ftile (ftile.id)}
        {@const c = filterChip(ftile)}
        <span class="fchip" class:on={c.on} class:open={filterPop?.id === ftile.id}>
          <button class="say" aria-haspopup="dialog" aria-expanded={filterPop?.id === ftile.id} title={t('Filter by {name}', { name: c.name })} onclick={(e) => openFilter(ftile, e)}>
            {#if c.date}<Icon name="calendar" size={13} />{/if}
            <span class="fname">{c.name}</span>
            <b>{c.value}</b>
            {#if !c.on}<Icon name="chevronDown" size={11} />{/if}
          </button>
          {#if c.on}
            <button class="x" aria-label={t('Clear the filter {name}', { name: c.name })} onclick={() => clearFilterTile(ftile)}><Icon name="x" size={11} /></button>
          {/if}
        </span>
      {/each}
      {#each everyPage as f (f.fromTile)}
        <span class="fchip saved" title={t('Chosen on {page}; it filters every page', { page: pageTitle(f.fromTile) })}>
          <Icon name="filter" size={12} />
          <span class="say static">{t('Every page: {filter}', { filter: filterText(f, app.tables) })}</span>
          <button class="x" aria-label={t('Clear the filter on every page')} onclick={() => clearEvery(f.fromTile)}><Icon name="x" size={11} /></button>
        </span>
      {/each}
      {#each pins as f (f.fromTile)}
        <span class="fchip saved" title={t('Saved with the dashboard')}>
          <Icon name="pin" size={12} />
          <span class="say static">{filterText(f, app.tables)}</span>
          <button class="x" aria-label={t('Remove saved filter')} onclick={() => unpin(f)}><Icon name="x" size={11} /></button>
        </span>
      {/each}
      {#each filters as f (f.fromTile)}
        <span class="fchip click" data-chip={f.fromTile}>
          <button
            class="say"
            aria-haspopup="menu"
            title={t('Keep only or exclude')}
            onclick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              chipMenu = chipMenu?.id === f.fromTile ? null : { id: f.fromTile, x: r.left, y: r.bottom + 4 };
            }}>
            <Icon name="filter" size={12} />
            {f.fromTile === DRILL && drillFrom ? t('From {page}: {filter}', { page: drillFrom.title, filter: filterText(f, app.tables) }) : filterText(f, app.tables)}
          </button>
          <button class="x" aria-label={t('Clear filter')} onclick={() => (filters = filters.filter((x) => x !== f))}><Icon name="x" size={11} /></button>
        </span>
      {/each}
      {#if filters.length > 1}
        <button class="fchip ghostbtn" onclick={() => (filters = [])}>{t('Clear all')}</button>
      {/if}
      {#if !phone && app.tables.length}
        <button class="fchip add" title={t('Add a filter tile to this page')} onclick={() => addTile('filter')}><Icon name="plus" size={12} /> {t('Filter')}</button>
      {/if}
      {#if !phone && clickable && !filters.length}
        <span class="spacer"></span>
        <span class="hint">{t('Click a bar, a slice or a row to filter the other tiles')}</span>
      {/if}
    </div>
  {/if}
  {/snippet}
  <!-- On a phone the page header scrolls with the tiles, so the tiles get the screen. -->
  {#if !phone}{@render pageTop()}{/if}
  {#if filterPop}
    {@const ft = dash.tiles.find((x) => x.id === filterPop?.id)}
    {#if ft && results[ft.id]?.res}
      <div class="fpop {ft.filter?.style ?? 'list'}" role="dialog" aria-label={t('Filter')} use:portal style:left="{filterPop.x}px" style:top="{filterPop.y}px">
        <FilterView
          tile={ft.filter?.style === 'dropdown' ? { ...ft, filter: { ...ft.filter, style: 'list' } } : ft}
          res={results[ft.id]!.res}
          tables={app.tables}
          onchange={(f) => setFilter(ft.id, { ...f, style: ft.filter?.style ?? f.style })} />
      </div>
    {/if}
  {/if}

  {#each failed as src (src.id)}
    <div class="banner" role="alert">
      <Icon name="warning" size={14} />
      <span>
        {t('{source} could not be read: {error}. Tiles that use it show an error until it is fixed.', { source: src.name, error: app.snap!.errors[src.id].replace(/\.$/, '') })}
      </span>
    </div>
  {/each}

  {#if dashMenu}
    <Menu
      x={dashMenu.x}
      y={dashMenu.y}
      items={[
        { label: t('Copy dashboard'), run: () => copyToClipboard(clipOf(dash!.tiles, true), t('Copied the dashboard')), disabled: !dash.tiles.length },
        { label: t('Duplicate dashboard'), run: duplicateDashboard },
        { label: t('Paste'), run: pasteFromMenu },
        { label: t('Card styles…'), run: () => (stylesOpen = true) },
        ...(app.snap?.workspace.site?.on ? [{ label: t('Preview site'), run: () => (app.preview = dash!.id) }] : [{ label: t('Site…'), run: () => (app.siteOpen = true) }]),
        { label: t('Layout'), sub: [{ label: story ? t('Grid') : `${t('Grid')}  ✓`, run: () => setLayout('grid') }, { label: story ? `${t('Story')}  ✓` : t('Story'), run: () => setLayout('flow') }] },
        { label: t('Delete dashboard…'), run: removeDashboard },
      ]}
      onclose={() => (dashMenu = null)} />
  {/if}
  {#if insertAt}
    {@const place = insertAt.at}
    <Menu
      x={insertAt.x}
      y={insertAt.y}
      items={[['text', t('Text')], ['kpi', t('Number')], ['bar', t('Bar chart')], ['line', t('Line chart')], ['area', t('Area chart')], ['pie', t('Pie chart')], ['map', t('Map')], ['table', t('Table')], ['image', t('Image')]].map(([type, label]) => ({ label, run: () => addTile(type as TileType, place) }))}
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
        ...(app.snap?.workspace.site?.on
          ? [
              { label: t('Whole site (one file)'), run: () => exportSite(false) },
              { label: t('Whole site (folder, ZIP)'), run: () => exportSite(true) },
            ]
          : []),
        { label: app.snap?.workspace.site?.on ? t('This page (HTML)') : t('Web page (HTML)'), run: exportHtml },
        { label: t('Image (PNG)'), run: () => exportPicture('png') },
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
        items={[{ label: t('Keep only'), run: () => keep(from, 'keep') }, ...(canExclude(f) ? [{ label: t('Exclude'), run: () => keep(from, 'exclude') }] : [])]}
        onclose={() => (chipMenu = null)} />
    {/if}
  {/if}

  {#if focusTile && results[focusTile.id]}
    <FocusSheet
      tile={shownTile(drawnTile(eff(focusTile), app.snap?.workspace.cardStyles), results[focusTile.id]!.res?.drill, app.snap?.workspace.hierarchies ?? [])}
      res={results[focusTile.id]!.res}
      colors={readColors()}
      title={tileTitle(eff(focusTile), app.tables)}
      tables={app.tables}
      onclose={() => (focusId = null)} />
  {/if}
  {#if reachFor && dash.tiles.find((x) => x.id === reachFor)}
    {@const rt = dash.tiles.find((x) => x.id === reachFor)!}
    <ReachSheet
      tile={rt}
      tiles={dash.tiles}
      titleOf={(x) => tileTitle(eff(x), app.tables)}
      onsave={(reach) => change({ ...rt, reach })}
      onclose={() => (reachFor = null)} />
  {/if}
  {#if alertFor && dash.tiles.find((x) => x.id === alertFor)}
    {@const at = dash.tiles.find((x) => x.id === alertFor)!}
    <AlertSheet {dash} tile={at} title={tileTitle(eff(at), app.tables)} onclose={() => (alertFor = null)} />
  {/if}
  {#if rowsReq}
    <RowsSheet title={rowsReq.title} data={rowsReq.data} error={rowsReq.error} onclose={() => (rowsSeq++, (rowsReq = null))} />
  {/if}

  <div class="body">
    <div class="scroll" class:paper={story} onpointerdown={(e) => e.target === e.currentTarget && (selectedTile = null)} role="presentation">
      {#if phone}<div class="phonetop">{@render pageTop()}</div>{/if}
      {#if !dash.tiles.length}
        <div class="empty">
          <Icon name="dashboard" size={32} />
          <h2>{t('An empty dashboard')}</h2>
          <p class="muted">
            {app.tables.length ? t('Add a tile, then pick a value and how to group it.') : t('Add some data first, then come back here.')}
          </p>
          {#if app.tables.length}
            <div class="quick">
              <button class="btn" onclick={() => addTile('kpi')}><Icon name="kpi" /> {t('Number')}</button>
              <button class="btn" onclick={() => addTile('bar')}><Icon name="bar" /> {t('Bar chart')}</button>
              <button class="btn" onclick={() => addTile('line')}><Icon name="line" /> {t('Line chart')}</button>
            </div>
          {:else}
            <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> {t('Add data')}</button>
          {/if}
        </div>
      {/if}
      {#snippet tileView(t: Tile, inFlow: boolean)}
        <TileView
          tile={shownOf(t)}
          filters={allFilters}
          {tick}
          active={selectedTile === t.id}
          onpick={(v, otherOf, bins, stage, shown) => pick(shown ?? shownOf(t), v, otherOf, bins, stage)}
          onpickcol={(v) => pickCol(shownOf(t), v)}
          onresult={(r, k) => setResult(t.id, r, k)}
          onfilter={(f) => setFilter(t.id, f)}
          onrows={(v, otherOf, bins, shown) => showRows(t.id, v, otherOf, bins, shown)}
          onfocus={() => (focusId = t.id)}
          onselect={() => (selectedTile = t.id)}
          onwidths={(w) => change({ ...(dash.tiles.find((x) => x.id === t.id) ?? t), widths: w })}
          onlayout={(layout) => change({ ...(dash.tiles.find((x) => x.id === t.id) ?? t), layout })}
          {phone}
          dashboards={app.dashboards.filter((d) => d.id !== dash.id).map((d) => ({ id: d.id, title: d.title }))}
          onduplicate={() => duplicate(t.id)}
          oncopy={(to) => copyTo(t.id, to)}
          onaction={(page) => setAction(t.id, page)}
          onreach={advancedOn() ? () => (reachFor = t.id) : undefined}
          onalert={advancedOn() && t.type === 'kpi' ? () => (alertFor = t.id) : undefined}
          drillState={drills[t.id] ?? TOP}
          ondrill={(st) => (drills = { ...drills, [t.id]: st })}
      onclip={() => {
            const tiles = picked.includes(t.id) ? copyTargets() : dash.tiles.filter((x) => x.id === t.id);
            copyToClipboard(clipOf(tiles), copiedWords(tiles.length));
          }}
          subtitle={subtitleOf(t)}
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
        onremove={() => removeTile(sel.id)}
        onduplicate={() => duplicate(sel.id)} />
    {/if}
  </div>
{/if}

<style>
  /* ---- toolbar: 48 px, the surface, a hairline under it (look v3) ---- */
  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 48px;
    padding: 0 12px 0 16px;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    flex: none;
    position: relative;
    z-index: 5;
  }
  .toolbar.phone { height: auto; min-height: 44px; flex-wrap: wrap; padding: 6px 10px; }
  .crumbs { display: flex; align-items: center; gap: 4px; min-width: 0; font-size: 12px; color: var(--text-2); white-space: nowrap; }
  .crumbs .sep { flex: none; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; color: var(--text-3); }
  .crumb { border: 0; background: none; padding: 2px 4px; border-radius: 4px; font-size: 12px; color: var(--text-2); }
  button.crumb:hover { background: var(--hover); color: var(--text); }
  .crumb.here { color: var(--text); font-weight: 500; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .status { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-2); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .status i { width: 6px; height: 6px; border-radius: 3px; background: var(--success); flex: none; }
  .status.bad { color: var(--danger); }
  .status.bad i { background: var(--danger); }
  .vsep { width: 1px; height: 18px; background: var(--sep); margin: 0 4px; flex: none; }
  .group { display: flex; align-items: center; gap: 0; }
  .tb {
    position: relative;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    color: var(--text);
    transition: background 120ms var(--spring), transform 120ms var(--spring);
  }
  .tb:hover:not(:disabled) { background: var(--hover); }
  .tb:active:not(:disabled) { transform: scale(0.94); }
  .tb:disabled { opacity: 0.4; cursor: default; }
  .more { position: relative; }
  /* A tooltip that names the key, under the button, after a short wait. */
  .tip {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    z-index: 40;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--raised);
    box-shadow: var(--shadow-float);
    color: var(--text);
    font-size: 12px;
    font-weight: 400;
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    transform: translate(-50%, -2px);
    transition: opacity 120ms var(--spring), transform 120ms var(--spring);
  }
  .tip.end { left: auto; right: 0; transform: translate(0, -2px); }
  .tip kbd { font: inherit; font-size: 11px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  :is(.tb, .more):hover .tip, :is(.tb, .more):focus-visible .tip { opacity: 1; transform: translate(-50%, 0); transition-delay: 400ms; }
  .more:hover .tip.end, .more:focus-visible .tip.end { transform: translate(0, 0); }
  .tb:disabled .tip { display: none; }
  .btn:active:not(:disabled) { transform: scale(0.98); }

  /* ---- page header: title, sources, views and grain ---- */
  .pagehead {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 8px 16px;
    flex-wrap: wrap;
    padding: 14px 16px 10px;
    background: var(--surface);
    flex: none;
  }
  .pagehead.phone { padding: 10px 10px 8px; }
  .titles { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  h1 { margin: 0; font-size: 20px; line-height: 1.2; }
  .title:disabled { color: var(--text); cursor: default; }
  .title {
    border: 0;
    background: none;
    font: inherit;
    font-weight: 650;
    letter-spacing: -0.01em;
    padding: 1px 6px;
    margin-left: -6px;
    border-radius: var(--r-control);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  .title:hover:not(:disabled) { background: var(--hover); }
  .phone h1 { font-size: 17px; }
  .title-input { width: 320px; height: 30px; font-size: 18px; font-weight: 600; }
  .sources { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; font-size: 12px; color: var(--text-2); }
  .src {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 7px;
    border-radius: 4px;
    background: var(--inset);
    color: var(--text);
    white-space: nowrap;
  }
  .src svg { fill: none; stroke: var(--accent-text); stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; flex: none; }
  .src.bad { color: var(--danger); }
  .headtools { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .seg { display: flex; padding: 2px; border-radius: 8px; background: var(--inset); }
  .seg button {
    height: 26px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-size: 12px;
    font-weight: 500;
    text-transform: capitalize;
    transition: background 150ms var(--spring), color 150ms var(--spring);
  }
  .seg button:hover:not(.on) { color: var(--text); }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%), 0 0 0 0.5px rgb(0 0 0 / 4%); }
  .phone .seg button { padding: 0 7px; }

  /* ---- filter bar ---- */
  .filterbar {
    display: flex;
    align-items: center;
    gap: 6px 8px;
    flex-wrap: wrap;
    padding: 0 16px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--sep);
    flex: none;
  }
  .filterbar.phone { padding: 0 10px 8px; }
  .fchip {
    display: inline-flex;
    align-items: center;
    height: 28px;
    border-radius: var(--r-control);
    border: 1px solid var(--sep-strong);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    white-space: nowrap;
    max-width: 100%;
    min-width: 0;
    transition: background 120ms var(--spring), border-color 120ms var(--spring);
  }
  button.fchip { gap: 5px; padding: 0 10px; }
  button.fchip:hover { background: var(--hover); }
  .fchip .say {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 100%;
    min-width: 0;
    padding: 0 8px 0 10px;
    border: 0;
    border-radius: inherit;
    background: none;
    color: inherit;
    font: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fchip .say.static { padding-left: 4px; display: inline-block; line-height: 26px; }
  .fchip > :global(svg:first-child) { margin-left: 9px; flex: none; color: var(--text-2); }
  .fchip button.say:hover { background: var(--hover); }
  .fchip .fname { color: var(--text-2); }
  .fchip b { font-weight: 600; overflow: hidden; text-overflow: ellipsis; max-width: 220px; }
  .fchip .say :global(svg) { color: var(--text-2); flex: none; }
  .fchip .x {
    width: 22px;
    height: 22px;
    margin-right: 3px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: inherit;
    flex: none;
  }
  .fchip .x:hover { background: color-mix(in srgb, currentColor 14%, transparent); }
  /* A filter that is on: the selected ground, the accent ink. */
  .fchip.on, .fchip.click { border-color: transparent; background: var(--selected); color: var(--accent-text); }
  .fchip.on .fname, .fchip.on .say :global(svg), .fchip.click .say :global(svg) { color: inherit; }
  .fchip.on .say:hover, .fchip.click .say:hover { background: color-mix(in srgb, var(--accent) 10%, transparent); }
  .fchip.open { border-color: var(--accent); }
  .fchip.saved { background: var(--inset); border-color: transparent; }
  .fchip.back { color: var(--accent-text); }
  .fchip.back :global(svg) { color: inherit; }
  /* Dashed means "not finished" in a chart, so an add button is plain: no border until hover. */
  .fchip.add { border-color: transparent; background: none; color: var(--text-2); }
  .fchip.add:hover { background: var(--hover); color: var(--text); }
  .fchip.add:hover { color: var(--text); border-color: var(--text-3); background: none; }
  .fchip.ghostbtn { border-color: transparent; background: none; color: var(--text-2); }
  .hint { font-size: 12px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .fpop {
    position: fixed;
    z-index: 70;
    width: 280px;
    display: flex;
    flex-direction: column;
    padding: 10px 10px 8px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    animation: pop 160ms var(--spring);
    transform-origin: top left;
  }
  .fpop.list, .fpop.dropdown { max-height: 320px; }
  .fpop.range { height: 190px; }

  .spacer { flex: 1; }
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
  .refresh { display: flex; align-items: center; gap: 4px; color: var(--text-2); font-size: 12px; }
  .refresh select { border: 0; background: none; color: var(--text); padding: 2px; font-size: 12px; }
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
    animation: pop 160ms var(--spring);
    transform-origin: top right;
    max-height: calc(100vh - 80px);
    overflow: auto;
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
  .menu button :global(svg) { color: var(--text-2); }
  .menu button:hover, .menu button:focus-visible { background: color-mix(in srgb, var(--text) 8%, var(--raised)); outline: none; }
  .menu button:hover :global(svg) { color: var(--accent-text); }
  .body { flex: 1; min-height: 0; display: flex; }
  .scroll { flex: 1; min-width: 0; overflow: auto; padding: 16px; font-variant-numeric: tabular-nums; }
  @media (max-width: 699px) { .scroll { padding: 10px; } }
  .phonetop { margin: -10px -10px 10px; border-bottom: 1px solid var(--sep); }
  .phonetop .filterbar { border-bottom: 0; }
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
