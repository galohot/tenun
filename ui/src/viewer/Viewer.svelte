<script lang="ts">
  // The exported dashboard. Everything it shows was computed when the file was
  // made; clicking a bar or a slice switches to the stored filtered results.
  import { onMount } from 'svelte';
  import type { Cell, Tile } from '../lib/api';
  import type { ExportPayload } from '../lib/export';
  import { key, readColors, type Colors } from '../lib/chart';
  import { cell, dateMedium, dateTimeMedium, dimLabel, OTHER_KEY, tileTitle, timeShort } from '../lib/format';
  import Icon from '../components/Icon.svelte';
  import FocusSheet from '../components/dash/FocusSheet.svelte';
  import TileBody from '../components/dash/TileBody.svelte';
  import { cardGround } from '../lib/card';
  import { filterText } from '../lib/filterText';
  import { partialNote } from '../lib/run';
  import { setLinkNames } from '../lib/links';
  import { setKnownFromFile } from '../lib/measures';
  import { isColFilter, stageOf } from '../lib/filters';
  import { EMPTY_RESULT, isStatic } from '../lib/tiles';
  import { phoneLayout, PHONE_MAX, readingOrder } from '../lib/layout';

  let {
    data,
    hosted = null,
  }: {
    data: ExportPayload;
    /** Set on tenun.cloud: when and where the snapshot was published, and how often it should update. */
    hosted?: { updated: string; source: string; every: number | null } | null;
  } = $props();
  const ROW = 76;
  const GAP = 12;

  let dark = $state(matchMedia('(prefers-color-scheme: dark)').matches);
  let colors = $state<Colors | null>(null);
  let width = $state(1000);
  /** The window's width: below 700 px the dashboard is one column. */
  let screen = $state(innerWidth);
  /** `col` is a click on a pivot column header. */
  let pick = $state<{ tile: string; value: Cell; col?: boolean; stage?: number } | null>(null);
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
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    requestAnimationFrame(() => (colors = readColors()));
  });

  const narrow = $derived(screen < PHONE_MAX);
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
      if (resultFor(t).res && !isStatic(t) && t.type !== 'filter') focusId = t.id;
      return;
    }
    const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const order = readingOrder(data.tiles);
    const next = order[order.findIndex((x) => x.id === t.id) + step];
    if (next) document.querySelector<HTMLElement>(`[data-tile="${CSS.escape(next.id)}"]`)?.focus();
  }

  function resultFor(t: Tile) {
    if (!pick || pick.tile === t.id) return { res: data.base[t.id], offline: false };
    const r = data.cross[pick.tile]?.[crossKey(pick.value, pick.col, pick.stage)]?.[t.id];
    return r ? { res: r, offline: false } : { res: data.base[t.id], offline: true };
  }

  function onpick(t: Tile, v: Cell, col = false, stage?: number) {
    if (!data.cross[t.id]?.[crossKey(v, col, stage)]) return;
    pick = pick && pick.tile === t.id && !!pick.col === col && pick.stage === stage && key(pick.value) === key(v) ? null : { tile: t.id, value: v, col, stage };
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
      {#each data.saved ?? [] as f}
        <span class="chip static" title="Saved with the dashboard"><Icon name="pin" size={12} /> {filterText(f, data.tables)}</span>
      {/each}
      {#each data.filters as f}
        <span class="chip static">{label(f.fromTile, f.op === 'not_in' ? OTHER_KEY : f.values[0], colFilter(f), stageFor(f))}</span>
      {/each}
      {#if pick}
        <span class="chip">
          {label(pick.tile, pick.value, pick.col, pick.stage)}
          <button aria-label="Clear" onclick={() => (pick = null)}>×</button>
        </span>
      {/if}
    </div>
  </header>

  {#if late}
    <div class="late" role="status">
      This dashboard has not updated since {dateTimeMedium(new Date(updatedAt))}.
      It should update every {hosted!.every! >= 1440 ? 'day' : hosted!.every! >= 120 ? `${Math.round(hosted!.every! / 60)} hours` : 'hour'}.
      The numbers below may be out of date.
    </div>
  {/if}

  <div class="grid" bind:clientWidth={width} style:height="{height}px">
    {#if colors}
      {#each layout as l (l.t.id)}
        {@const r = resultFor(l.t)}
        {@const name = data.titles?.[l.t.id] ?? tileTitle(l.t, data.tables, r?.res.parts)}
        <!-- A number card fills its tile with its ground and draws its own title (build 08). -->
        {@const ground = cardGround(l.t)}
        <!-- A tile is a stop for the keyboard: arrows go to the next and the previous tile, Enter opens it large. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <article
          class="tile"
          tabindex="0"
          data-tile={l.t.id}
          aria-label={name || 'Tile'}
          onkeydown={(e) => onkey(e, l.t)}
          style:transform="translate({l.x}px, {l.y}px)"
          style:width="{l.w}px"
          style:height="{l.h}px"
          style:background={ground ? `var(--${ground})` : undefined}>
          {#if name !== '' && !ground}
            <h3>{name}</h3>
          {/if}
          {#if r.res && !isStatic(l.t) && l.t.type !== 'filter'}
            <button class="expand" title="Open large" aria-label="Open large" onclick={() => (focusId = l.t.id)}><Icon name="expand" size={14} /></button>
          {/if}
          <div class="body" class:bare={((isStatic(l.t) || l.t.type === 'filter') && !l.t.title) || !!ground}>
            {#if r.res || isStatic(l.t)}
              <TileBody
                tile={l.t}
                res={r.res ?? EMPTY_RESULT}
                {colors}
                tables={data.tables}
                title={name}
                selected={pick?.tile === l.t.id && !pick.col ? [pick.value] : null}
                selectedCol={pick?.tile === l.t.id && pick.col ? [pick.value] : null}
                selectedStage={pick?.tile === l.t.id && !pick.col ? (pick.stage ?? null) : null}
                onpick={data.cross[l.t.id] ? (v, stage) => onpick(l.t, v, false, stage) : undefined}
                onpickcol={data.cross[l.t.id] && l.t.cols ? (v) => onpick(l.t, v, true) : undefined} />
            {/if}
          </div>
          {#if r.res && l.t.type !== 'kpi' && partialNote(r.res, l.t.dims[0]?.grain)}
            <p class="note">{partialNote(r.res, l.t.dims[0]?.grain)}</p>
          {/if}
          {#each r.res?.notes ?? [] as n (n)}
            <p class="note link-note"><Icon name="warning" size={12} /> {n}</p>
          {/each}
          {#if r.offline && !isStatic(l.t) && l.t.type !== 'filter'}<p class="note">Not filtered in this file</p>{/if}
        </article>
      {/each}
    {/if}
  </div>

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
  .page { max-width: 1440px; margin: 0 auto; padding: 24px; }
  @media (max-width: 699px) {
    .page { padding: 12px; }
    h1 { font-size: 19px; }
    /* There is no pointer to hover with: the button to open a tile large stays in view. */
    .expand { opacity: 1; }
  }
  header { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 16px; flex-wrap: wrap; }
  h1 { font-size: 22px; }
  header p { margin: 4px 0 0; color: var(--text-2); font-size: 12px; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
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
  .chip button { border: 0; background: none; color: inherit; font-size: 15px; line-height: 1; padding: 0 6px; cursor: pointer; }
  .grid { position: relative; }
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
