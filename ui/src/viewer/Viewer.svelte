<script lang="ts">
  // The exported dashboard. Everything it shows was computed when the file was
  // made; clicking a bar or a slice switches to the stored filtered results.
  import { onMount } from 'svelte';
  import type { Cell, QueryResult, Tile } from '../lib/api';
  import { shownTile } from '../lib/drilldown';
  import { reachOf } from '../lib/filters';
  import DrillBar from '../components/dash/DrillBar.svelte';
  import type { ExportPayload } from '../lib/export';
  import { key, readColors, type Colors } from '../lib/chart';
  import { cell, dateMedium, dateTimeMedium, dimLabel, OTHER_KEY, tileTitle, timeShort } from '../lib/format';
  import Icon from '../components/Icon.svelte';
  import FocusSheet from '../components/dash/FocusSheet.svelte';
  import TileBody from '../components/dash/TileBody.svelte';
  import FlowLayout from '../components/dash/FlowLayout.svelte';
  import { flowOrder } from '../lib/flow';
  import { DRILL } from '../lib/site';
  import { cardGround } from '../lib/card';
  import { filterText } from '../lib/filterText';
  import { partialNote } from '../lib/run';
  import { setLinkNames } from '../lib/links';
  import { setKnownFromFile } from '../lib/measures';
  import { isColFilter, stageOf } from '../lib/filters';
  import { EMPTY_RESULT, isStatic } from '../lib/tiles';
  import { phoneLayout, PHONE_MAX, readingOrder } from '../lib/layout';
  import { paramValues, stepKey, stepsOf, withParams } from '../lib/params';

  let {
    data: root,
    hosted = null,
    embedded = false,
    pages,
    onpage,
    ondrill,
    drillFrom,
    note,
    everyAt = null,
    onpickchange,
    startPick = null,
    useHash = true,
  }: {
    data: ExportPayload;
    /** Set on tenun.cloud: when and where the snapshot was published, and how often it should update. */
    hosted?: { updated: string; source: string; every: number | null } | null;
    /** Drawn inside the app (the site preview, build 12): the app keeps its own theme. */
    embedded?: boolean;
    /** A site's pages: a page link in a text opens one (build 12). Not set: page links are plain text. */
    pages?: ReadonlySet<string>;
    onpage?: (id: string) => void;
    /** An exported site (build 13): a click on a mark of a tile with an action opens its page. True when it did. */
    ondrill?: (tile: Tile, value: Cell) => boolean;
    /** A page a drill-through opened: where from, and the way back. */
    drillFrom?: { title: string; back: () => void } | null;
    /** A line under the title: why a page a click opened is not kept to the clicked value. */
    note?: string;
    /**
     * An exported site (v5 build 02): a value picked in another page's filter tile that applies to every page. `results`
     * are this page's tiles with it (not set: the file does not hold that value).
     */
    everyAt?: { label: string; results?: Record<string, QueryResult>; onclear: () => void } | null;
    /** A site hears what is picked here, for its every-page filters. */
    onpickchange?: (tile: string, value: Cell | null) => void;
    /** The pick this page starts with (a site: the every-page value picked here before). */
    startPick?: { tile: string; value: Cell } | null;
    /** The address holds the saved view (`#v=<id>`). A site's address holds its page instead. */
    useHash?: boolean;
  } = $props();

  // Saved views (v5 build 03): the file holds each one in full; the reader picks one. A file opens on the view its
  // address names, else on the dashboard's default view.
  const viewFromHash = () => {
    const m = /^#v=(.+)$/.exec(location.hash);
    return m ? decodeURIComponent(m[1]) : null;
  };
  // svelte-ignore state_referenced_locally
  let viewId = $state<string | null>((useHash && viewFromHash()) || root.defaultView || null);
  const data = $derived(viewId ? (root.views?.find((v) => v.id === viewId)?.payload ?? root) : root);
  function pickView(id: string | null) {
    viewId = id;
    pick = null;
    drilled = {};
    deeper = {};
    stepAt = {};
    if (useHash) history.replaceState(null, '', id ? `#v=${encodeURIComponent(id)}` : location.pathname + location.search);
  }
  const ROW = 76;
  const GAP = 12;

  let dark = $state(matchMedia('(prefers-color-scheme: dark)').matches);
  let colors = $state<Colors | null>(null);
  let width = $state(1000);
  /** The window's width: below 700 px the dashboard is one column. */
  let screen = $state(innerWidth);
  /** `col` is a click on a pivot column header. */
  // svelte-ignore state_referenced_locally
  let pick = $state<{ tile: string; value: Cell; col?: boolean; stage?: number } | null>(startPick ?? null);
  /** The tile shown large. */
  let focusId = $state<string | null>(null);
  /** How the engine names the export's stored result for a click on a pivot column header, or on a Sankey node of a stage. */
  const COL_PREFIX = '\u0001col:';
  const STAGE_PREFIX = '\u0002stage:';
  const crossKey = (v: Cell, col?: boolean, stage?: number) => (stage !== undefined ? `${STAGE_PREFIX}${stage}:` : col ? COL_PREFIX : '') + key(v);

  onMount(() => {
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => (dark = e.matches));
  });

  // The names of the links the tiles use, so a title can say "Shipped from › region".
  $effect.pre(() => setLinkNames(data.links));
  // The same for the named measures: their names for the titles, and whether their values add up.
  $effect.pre(() => setKnownFromFile(data.measures));

  $effect(() => {
    if (!embedded) document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    requestAnimationFrame(() => (colors = readColors()));
  });

  const narrow = $derived(screen < PHONE_MAX);
  /** A story reads like a document: a column in its own order (build 11). */
  const story = $derived(data.layout === 'flow');
  const colW = $derived((width - GAP * 11) / 12);

  // In reading order, so Tab goes through the tiles the way a person reads them.
  const layout = $derived(
    narrow
      ? phoneLayout(data.tiles, width, ROW, GAP)
      : readingOrder(data.tiles).map((t) => ({ t, x: t.x * (colW + GAP), y: t.y * (ROW + GAP), w: t.w * colW + (t.w - 1) * GAP, h: t.h * ROW + (t.h - 1) * GAP })),
  );
  const height = $derived(Math.max(0, ...layout.map((l) => l.y + l.h)));

  /** Arrow keys go to the next or the previous tile in reading order. Enter opens the tile large. */
  function onkey(e: KeyboardEvent, t: Tile) {
    if (e.target !== e.currentTarget) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      if (resultFor(t).res && !isStatic(t.type) && t.type !== 'filter') focusId = t.id;
      return;
    }
    const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const order = readingOrder(data.tiles);
    const next = order[order.findIndex((x) => x.id === t.id) + step];
    if (next) document.querySelector<HTMLElement>(`[data-tile="${CSS.escape(next.id)}"]`)?.focus();
  }

  // Parameters offline (v5 build 05, V5-D40): the file holds the tiles that read a parameter at each of its values, for
  // the page as it opens. One parameter moves at a time; moving it takes away the clicks and the drill-down, and a click
  // or a drill puts it back to its saved value.
  let stepAt = $state<Record<string, Cell>>({});
  const params = $derived(data.params ?? []);
  const values = $derived(paramValues(params, stepAt));
  function moveParam(id: string, v: Cell) {
    const was = pick;
    pick = null;
    drilled = {};
    deeper = {};
    if (was && !was.col) onpickchange?.(was.tile, null);
    const saved = params.find((p) => p.id === id)?.value;
    stepAt = stepKey(v) === stepKey(saved ?? null) ? {} : { [id]: v };
  }
  /** A parameter tile's control: it moves when the file holds the steps of its parameter. */
  function controlFor(t: Tile) {
    const p = params.find((x) => x.id === t.param?.id);
    if (!p) return { param: undefined, value: null };
    const value = values[p.id] ?? null;
    if (viewId) return { param: p, value, note: "In a saved view, this file holds only the view's value." };
    if (everyAt) return { param: p, value, note: 'With a value picked for every page, this file holds only the saved value.' };
    if (!stepsOf(p)) return { param: p, value, note: 'This file holds only the saved value.' };
    return { param: p, value, onchange: (v: Cell) => moveParam(p.id, v) };
  }

  /** The key of "Next level" in `down` (`DRILL_ALL_KEY` in the engine). */
  const ALL = '\u0000all';
  /** Drill-down offline (v5 build 01): the tiles a click drills, and the level-1 result each one shows. One level only. */
  let drillMode = $state<Record<string, boolean>>({});
  let drilled = $state<Record<string, { key: string; value?: Cell }>>({});
  /** A tile whose reader asked for a level the file does not hold. */
  let deeper = $state<Record<string, boolean>>({});
  const canDrill = (t: Tile) => !!data.down?.[t.id] && !!data.base[t.id]?.drill?.canDown;

  function drillClick(t: Tile, v: Cell): boolean {
    if (!drillMode[t.id] || !canDrill(t) || v === OTHER_KEY) return false;
    stepAt = {};
    const r = data.down?.[t.id]?.[key(v)];
    if (drilled[t.id] || !r) deeper = { ...deeper, [t.id]: true };
    else drilled = { ...drilled, [t.id]: { key: key(v), value: v } };
    return true;
  }
  function drillTo(t: Tile, depth: number) {
    if (depth > 0) return;
    const { [t.id]: _gone, ...rest } = drilled;
    drilled = rest;
    deeper = { ...deeper, [t.id]: false };
  }

  function resultFor(t: Tile) {
    // A moved parameter: the tiles that read it at its value, as the file holds them (v5 build 05).
    const moved = Object.keys(stepAt)[0];
    if (moved !== undefined && data.base[t.id]?.params?.includes(moved)) {
      const r = data.steps?.[moved]?.[stepKey(stepAt[moved])]?.[t.id];
      return { res: r ?? data.base[t.id], offline: !r };
    }
    // A value picked on another page for every page: this page's results with it, as the file holds them.
    if (everyAt) {
      const r = everyAt.results?.[t.id];
      return { res: r ?? data.base[t.id], offline: !!pick || !r };
    }
    // A click does not reach a tile outside its tile's reach: that tile is as it is (v5 build 02).
    if (pick && pick.tile !== t.id) {
      const source = data.tiles.find((x) => x.id === pick!.tile);
      const reach = source ? reachOf(source) : undefined;
      if (reach && !reach.includes(t.id)) return { res: data.base[t.id], offline: false };
    }
    const d = drilled[t.id];
    // A drilled tile shows its stored level; a click on another tile cannot filter it in the file.
    if (d) return { res: data.down?.[t.id]?.[d.key] ?? data.base[t.id], offline: !!pick && pick.tile !== t.id };
    if (!pick || pick.tile === t.id) return { res: data.base[t.id], offline: false };
    const r = data.cross[pick.tile]?.[crossKey(pick.value, pick.col, pick.stage)]?.[t.id];
    return r ? { res: r, offline: false } : { res: data.base[t.id], offline: true };
  }

  function onpick(t: Tile, v: Cell, col = false, stage?: number) {
    // A click puts the parameters back to their saved values: the file holds its clicks at those (v5 build 05).
    stepAt = {};
    // With drill-down on, a click opens the level below (v5 build 01).
    if (!col && drillClick(t, v)) return;
    // A tile with an action opens its page, kept to the clicked value (build 13).
    if (!col && t.action && ondrill?.(t, v)) return;
    // A drilled tile's marks are of its level below: the file holds no filters for them.
    if (drilled[t.id]) return;
    if (!data.cross[t.id]?.[crossKey(v, col, stage)]) return;
    pick = pick && pick.tile === t.id && !!pick.col === col && pick.stage === stage && key(pick.value) === key(v) ? null : { tile: t.id, value: v, col, stage };
    if (!col) onpickchange?.(t.id, pick ? v : null);
  }

  function label(tileId: string, v: Cell, col = false, stage = 0): string {
    const t = data.tiles.find((x) => x.id === tileId)!;
    // A histogram bin is a range of its number column.
    const bins = t.type === 'histogram' ? data.base[tileId]?.bins : undefined;
    const m = t.measures[0];
    const d = col && t.cols ? t.cols : bins && m?.column ? { table: m.table, column: m.column, via: m.via } : (t.dims[stage] ?? t.dims[0]);
    const dtype = data.tables.find((x) => x.id === d.table)?.columns.find((c) => c.name === d.column)?.dtype ?? 'text';
    const name = dimLabel(d, data.tables);
    const shown =
      bins && typeof v === 'number'
        ? `${cell(v, 'number', null, true)} to ${cell(Math.round((v + bins.width) * 1e10) / 1e10, 'number', null, true)}`
        : cell(v, dtype, d.grain);
    return `${name.charAt(0).toUpperCase() + name.slice(1)}: ${shown}`;
  }

  /** A filter saved with the export that came from a click on a pivot column header. It names the field of the columns, link included. */
  const colFilter = (f: ExportPayload['filters'][number]) => isColFilter(data.tiles.find((x) => x.id === f.fromTile), f);

  /** The stage of a Sankey node a saved filter came from, link included; 0 for every other tile. */
  const stageFor = (f: ExportPayload['filters'][number]) => stageOf(data.tiles.find((x) => x.id === f.fromTile), f);

  const when = $derived(dateTimeMedium(data.generated));

  // Freshness: a published dashboard says how old it is, and says so loudly when an update is late.
  let clock = $state(Date.now());
  onMount(() => {
    const h = setInterval(() => (clock = Date.now()), 60_000);
    return () => clearInterval(h);
  });
  const updatedAt = $derived(hosted ? Date.parse(hosted.updated) : NaN);
  const age = $derived.by(() => {
    if (!Number.isFinite(updatedAt)) return '';
    const s = (clock - updatedAt) / 1000;
    if (s < 90) return 'just now';
    if (s < 5400) return `${Math.round(s / 60)} minutes ago`;
    const d = new Date(updatedAt);
    const today = new Date(clock).toDateString() === d.toDateString();
    const t = timeShort(d);
    return today ? `at ${t} today` : `${dateMedium(d)} ${t}`;
  });
  // Late = more than twice the schedule (daily schedules get two extra hours of slack).
  const late = $derived(
    !!hosted?.every && Number.isFinite(updatedAt) && clock - updatedAt > (2 * hosted.every + (hosted.every >= 1440 ? 120 : 0)) * 60_000,
  );
</script>

<svelte:window bind:innerWidth={screen} />

<div class="page">
  <header>
    <div>
      <h1>{data.title}</h1>
      {#if hosted}
        <p>Updated {age}{hosted.source ? ` from ${hosted.source}` : ''}</p>
      {:else}
        <p>{data.workspace} · exported {when}</p>
      {/if}
    </div>
    <div class="chips">
      {#if root.views?.length}
        <label class="viewpick">
          <span class="sr">View</span>
          <select value={viewId ?? ''} onchange={(e) => pickView(e.currentTarget.value || null)} aria-label="Saved view">
            <option value="">As saved</option>
            {#each root.views as v (v.id)}<option value={v.id}>{v.name}</option>{/each}
          </select>
        </label>
      {/if}
      {#each data.every ?? [] as f}
        <span class="chip static" title="Chosen on another page; it filters every page">Every page: {filterText(f, data.tables)}</span>
      {/each}
      {#if everyAt}
        <span class="chip">
          Every page: {everyAt.label}
          <button aria-label="Clear" onclick={everyAt.onclear}>×</button>
        </span>
      {/if}
      {#each data.saved ?? [] as f}
        <span class="chip static" title="Saved with the dashboard"><Icon name="pin" size={12} /> {filterText(f, data.tables)}</span>
      {/each}
      {#each data.filters as f}
        {#if f.fromTile === DRILL}
          <!-- A drill-through's filter: where it came from, and the way back (build 13). -->
          <span class="chip static">{drillFrom ? `From ${drillFrom.title}: ` : ''}{filterText(f, data.tables)}</span>
          {#if drillFrom}<button class="chip back" onclick={drillFrom.back}><Icon name="chevronLeft" size={12} /> Back</button>{/if}
        {:else}
          <span class="chip static">{label(f.fromTile, f.op === 'not_in' ? OTHER_KEY : f.values[0], colFilter(f), stageFor(f))}</span>
        {/if}
      {/each}
      {#if pick}
        <span class="chip">
          {label(pick.tile, pick.value, pick.col, pick.stage)}
          <button
            aria-label="Clear"
            onclick={() => {
              const was = pick;
              pick = null;
              if (was && !was.col) onpickchange?.(was.tile, null);
            }}>×</button>
        </span>
      {/if}
    </div>
  </header>

  {#if note}<p class="drillnote" role="note">{note}</p>{/if}

  {#if late}
    <div class="late" role="status">
      This dashboard has not updated since {dateTimeMedium(new Date(updatedAt))}.
      It should update every {hosted!.every! >= 1440 ? 'day' : hosted!.every! >= 120 ? `${Math.round(hosted!.every! / 60)} hours` : 'hour'}.
      The numbers below may be out of date.
    </div>
  {/if}

  <!-- One tile's frame: on the grid at its place, or in a story's column (build 11). -->
  {#snippet frame(t: Tile, box: { x: number; y: number; w: number; h: number } | null)}
    {@const r = resultFor(t)}
    <!-- A drilled tile is drawn with the field of its level (v5 build 01). -->
    {@const shown = shownTile(t, r.res?.drill, data.hierarchies ?? [])}
    {@const name = data.titles?.[t.id] ?? tileTitle(t, data.tables, r?.res?.parts)}
    <!-- A number card fills its tile with its ground and draws its own title (build 08). -->
    {@const ground = cardGround(t)}
    <!-- A tile is a stop for the keyboard: arrows go to the next and the previous tile, Enter opens it large. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <article
      class="tile"
      tabindex="0"
      data-tile={t.id}
      aria-label={name || 'Tile'}
      onkeydown={(e) => onkey(e, t)}
      class:flowtile={!box}
      class:grounded={!!ground && ground !== 'surface'}
      style:transform={box ? `translate(${box.x}px, ${box.y}px)` : undefined}
      style:width={box ? `${box.w}px` : undefined}
      style:height={box ? `${box.h}px` : undefined}
      style:background={ground ? `var(--${ground})` : undefined}>
      {#if name !== '' && !ground}
        <h3>{name}</h3>
      {/if}
      {#if r.res && !isStatic(t.type) && t.type !== 'filter'}
        <button class="expand" title="Open large" aria-label="Open large" onclick={() => (focusId = t.id)}><Icon name="expand" size={14} /></button>
      {/if}
      {#if canDrill(t)}
        <button
          class="expand drillsw"
          class:on={drillMode[t.id]}
          aria-pressed={!!drillMode[t.id]}
          title={drillMode[t.id] ? 'A click drills down. Click to make a click filter again.' : 'Drill down: make a click open the level below.'}
          aria-label="Drill down"
          onclick={() => (drillMode = { ...drillMode, [t.id]: !drillMode[t.id] })}><Icon name="drill" size={14} /></button>
      {/if}
      {#if r.res?.drill && (drillMode[t.id] || drilled[t.id])}
        <DrillBar
          info={r.res.drill}
          onto={(d) => drillTo(t, d)}
          onup={() => drillTo(t, 0)}
          onnext={!drilled[t.id] && data.down?.[t.id]?.[ALL] ? () => (drilled = { ...drilled, [t.id]: { key: ALL } }) : undefined}
          note={deeper[t.id] ? 'This file holds one level of drill-down. Open the workspace in Tenun to go further.' : ''} />
      {/if}
      <div class="body" class:bare={((isStatic(t.type) || t.type === 'filter') && !t.title) || !!ground}>
        {#if r.res || isStatic(t)}
          <TileBody
            tile={withParams(shown, values)}
            res={r.res ?? EMPTY_RESULT}
            control={t.type === 'param' ? controlFor(t) : undefined}
            colors={colors!}
            tables={data.tables}
            title={name}
            {pages}
            {onpage}
            selected={pick?.tile === t.id && !pick.col ? [pick.value] : null}
            selectedCol={pick?.tile === t.id && pick.col ? [pick.value] : null}
            selectedStage={pick?.tile === t.id && !pick.col ? (pick.stage ?? null) : null}
            onpick={data.cross[t.id] || (ondrill && t.action) || canDrill(t) ? (v, stage) => onpick(t, v, false, stage) : undefined}
            onpickcol={data.cross[t.id] && t.cols ? (v) => onpick(t, v, true) : undefined} />
        {/if}
      </div>
      {#if r.res && t.type !== 'kpi' && partialNote(r.res, shown.dims[0]?.grain)}
        <p class="note">{partialNote(r.res, shown.dims[0]?.grain)}</p>
      {/if}
      {#each r.res?.notes ?? [] as n (n)}
        <p class="note link-note"><Icon name="warning" size={12} /> {n}</p>
      {/each}
      {#if r.offline && !isStatic(t) && t.type !== 'filter'}<p class="note">Not filtered in this file</p>{/if}
    </article>
  {/snippet}

  {#if story}
    <div class="story" bind:clientWidth={width}>
      {#if colors}
        <FlowLayout tiles={flowOrder({ tiles: data.tiles, order: data.order })}>
          {#snippet cell(t)}{@render frame(t, null)}{/snippet}
        </FlowLayout>
      {/if}
    </div>
  {:else}
  <div class="grid" bind:clientWidth={width} style:height="{height}px">
    {#if colors}
      {#each layout as l (l.t.id)}
        {@render frame(l.t, l)}
      {/each}
    {/if}
  </div>
  {/if}

  {#if focusId && colors}
    {@const ft = data.tiles.find((t) => t.id === focusId)}
    {@const fr = ft ? resultFor(ft).res : undefined}
    {#if ft && fr}
      <FocusSheet
        tile={ft}
        res={fr}
        {colors}
        title={data.titles?.[ft.id] ?? tileTitle(ft, data.tables, fr?.parts)}
        tables={data.tables}
        onclose={() => (focusId = null)} />
    {/if}
  {/if}

  <footer>
    {#if hosted}
      Published with <a href="https://tenun.cloud" target="_blank" rel="noopener">Tenun</a> · the numbers update when the owner's computer publishes them.
    {:else}
      Made with <a href="https://github.com/galohot/tenun" target="_blank" rel="noopener">Tenun</a> · the data in this file is a snapshot.
    {/if}
  </footer>
</div>

<style>
  :global(body) { background: var(--window); }
  .page { width: 100%; max-width: 1440px; margin: 0 auto; padding: 24px; }
  @media (max-width: 699px) {
    .page { padding: 12px; }
    h1 { font-size: 19px; }
    /* There is no pointer to hover with: the button to open a tile large stays in view. */
    .expand { opacity: 1; }
  }
  header { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 16px; flex-wrap: wrap; }
  h1 { font-size: 22px; }
  header p { margin: 4px 0 0; color: var(--text-2); font-size: 12px; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .viewpick select { height: 26px; padding: 0 8px; border: 1px solid var(--sep); border-radius: 13px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 4px 0 10px;
    border-radius: 12px;
    background: var(--selected);
    color: var(--accent-text);
    font-size: 12px;
    font-weight: 500;
  }
  .chip.static { display: inline-flex; gap: 5px; padding-right: 10px; background: var(--inset); color: var(--text-2); }
  .chip.back { display: inline-flex; align-items: center; gap: 4px; border: 0; background: none; color: var(--accent-text); cursor: pointer; font: inherit; font-size: 12px; }
  .drillnote { margin: 0 0 12px; font-size: 12px; color: var(--text-2); }
  .chip button { border: 0; background: none; color: inherit; font-size: 15px; line-height: 1; padding: 0 6px; cursor: pointer; }
  .grid { position: relative; }
  /* A story: the tiles in a column, without a ring, on the page's ground (build 11). */
  .story { padding-top: 4px; }
  .tile.flowtile { position: relative; width: 100%; height: 100%; background: transparent; box-shadow: none; }
  .tile.flowtile h3 { padding-left: 0; font-size: 15px; }
  .tile.flowtile:not(.grounded) .body { padding-left: 0; padding-right: 0; }
  .tile {
    position: absolute;
    top: 0;
    left: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-card);
    background: var(--surface);
    box-shadow: var(--shadow-tile);
    overflow: hidden;
  }
  h3 { padding: 10px 14px 0; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .expand {
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 2;
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 12px;
    background: var(--surface);
    color: var(--text-2);
    opacity: 0;
    cursor: pointer;
  }
  .tile:hover .expand, .tile:focus-within .expand, .expand:focus-visible { opacity: 1; }
  .tile:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }
  .expand:hover { color: var(--text); background: var(--hover); }
  /* The drill switch sits left of the expand button and stays in view once it is on. */
  .expand.drillsw { right: 34px; }
  .expand.drillsw.on { opacity: 1; color: var(--accent-text); background: var(--inset); }
  .body { flex: 1; min-height: 0; padding: 6px 14px 10px; }
  .body.bare { padding-top: 12px; }
  .note { margin: 0; padding: 0 14px 8px; font-size: 11px; color: var(--text-2); }
  .note.link-note { display: flex; align-items: flex-start; gap: 5px; line-height: 1.35; }
  .note.link-note :global(svg) { flex: none; margin-top: 1px; color: #c47400; }
  .late {
    margin: 0 0 16px;
    padding: 10px 14px;
    border-radius: var(--r-card);
    background: color-mix(in srgb, #c47400 14%, var(--surface));
    color: var(--text);
    font-size: 13px;
  }
  footer { margin-top: 24px; font-size: 12px; color: var(--text-2); text-align: center; }
  footer a { color: var(--accent-text); }
</style>
