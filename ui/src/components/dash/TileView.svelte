<script lang="ts">
  import { untrack } from 'svelte';
  import type { Bins, Cell, QueryResult, Tile, TileFilter } from '../../lib/api';
  import { partialNote, runTile } from '../../lib/run';
  import { app } from '../../lib/state.svelte';
  import { readColors, type Colors } from '../../lib/chart';
  import { OTHER_KEY, tileTitle } from '../../lib/format';
  import { appliedFilters, EMPTY_RESULT, isPivotLike, isStatic, resultKey, tileQuery, type DashFilter } from '../../lib/tiles';
  import { isColFilter, sameField } from '../../lib/filters';
  import { advancedOn } from '../../lib/tiers';
  import Icon from '../Icon.svelte';
  import PlaceNames from '../PlaceNames.svelte';
  import { downloadCsv, downloadTilePng, downloadXlsx } from '../../lib/output';
  import Menu from './Menu.svelte';
  import TileBody from './TileBody.svelte';
  import { cardGround, drawnTile } from '../../lib/card';

  let {
    tile,
    filters,
    tick,
    active,
    onpick,
    onpickcol,
    onresult,
    onfilter,
    onrows,
    onfocus,
    onselect,
    onwidths,
    onlayout,
    ondragstart,
    onresizestart,
    phone = false,
    dashboards = [],
    onduplicate,
    oncopy,
    onclip,
    onremove,
    onnav,
    onopen,
  }: {
    tile: Tile;
    filters: DashFilter[];
    tick: number;
    active: boolean;
    onpick: (value: Cell, otherOf?: Cell[], bins?: Bins, stage?: number) => void;
    /** A click on a pivot column header. */
    onpickcol: (value: Cell) => void;
    /** Tells the dashboard what this tile shows now (the Inspector's suggestions reuse it). */
    onresult?: (res: QueryResult | null, key: string) => void;
    /** Filter tile: the selection changed. */
    onfilter: (f: TileFilter) => void;
    /** "Show rows" for the whole tile, or for the mark with this value. */
    onrows: (value?: Cell, otherOf?: Cell[], bins?: Bins) => void;
    /** Open the tile large. */
    onfocus: () => void;
    onselect: () => void;
    /** A table column was dragged to a new width (or reset with undefined). */
    onwidths: (widths: number[] | undefined) => void;
    /** The note about values of very different sizes offers separate panels (Advanced). */
    onlayout?: (layout: 'panels') => void;
    ondragstart: (e: PointerEvent) => void;
    onresizestart: (e: PointerEvent) => void;
    /** A narrow screen: the editing controls are hidden. */
    phone?: boolean;
    /** The other dashboards of the workspace, for "Copy to dashboard". */
    dashboards?: { id: string; title: string }[];
    onduplicate: () => void;
    /** Copy the tile to another dashboard, or to a new one (null). */
    oncopy: (dashboard: string | null) => void;
    /** Copy the tile to the clipboard, to paste on any dashboard of any workspace (build 09). */
    onclip?: () => void;
    onremove: () => void;
    /** An arrow key on the tile: 1 for the next tile in reading order, -1 for the one before. */
    onnav: (step: 1 | -1) => void;
    /** Enter on the tile: edit it, or open it large on a phone. */
    onopen: () => void;
  } = $props();

  let res = $state<QueryResult | null>(null);
  let error = $state('');
  let loading = $state(false);
  let colors = $state<Colors>(readColors());

  const q = $derived(tileQuery(tile, filters));
  const qKey = $derived(resultKey(tile, filters));
  const fixed = $derived(isStatic(tile));

  /** A map that downloaded another country's boundaries asks again (build 06). */
  let retry = $state(0);
  /** The list of place names, from a map's "not on the map" names. */
  let places = $state(false);

  // Re-query when the question, the filters or the data change.
  $effect(() => {
    void tick;
    void retry;
    void app.dataVersion;
    // The engine writes period names such as "Oct 1–14", so they follow the locale.
    void app.locale;
    const key = qKey;
    if (!q) {
      res = null;
      error = '';
      untrack(() => onresult?.(null, ''));
      return;
    }
    loading = true;
    // A filter tile saves its selection in the tile. That must not make it ask for its values again.
    const asked = tile.type === 'filter' ? untrack(() => $state.snapshot(tile)) : $state.snapshot(tile);
    runTile(asked as Tile, $state.snapshot(filters) as DashFilter[])
      .then((r) => {
        if (key === qKey) {
          res = r;
          error = '';
          onresult?.(r, key);
        }
      })
      .catch((e) => {
        if (key === qKey) error = e.message;
      })
      .finally(() => (loading = false));
  });

  // Colours come from CSS; read them again when the theme flips.
  $effect(() => {
    void app.theme;
    requestAnimationFrame(() => (colors = readColors()));
  });

  // A filter tile offers every value of its field, so a filter that cannot reach it is no news.
  const ignored = $derived(
    res && tile.type !== 'filter' ? res.ignored_filters.map((i) => appliedFilters(tile, filters)[i]).filter(Boolean) : [],
  );
  // A click on "Other" is a not_in filter; the tile shows it as the Other group being picked.
  // A click on a histogram bin is a range; the tile shows the bin that starts it.
  const mine = $derived(filters.find((x) => x.fromTile === tile.id));
  // On a pivot, a click on a column header filters by the column field instead of the first row field.
  const onCol = $derived(isPivotLike(tile.type) && !!mine && isColFilter(tile, mine));
  // A filter tile keeps its own selection in the tile and is not drawn from the cross-filters.
  const selected = $derived(tile.type !== 'filter' && mine && !onCol ? (mine.op === 'not_in' ? [OTHER_KEY] : mine.op === 'range' ? mine.values.slice(0, 1) : mine.values) : null);
  const selectedCol = $derived(mine && onCol ? mine.values : null);
  // On a Sankey chart a click names a node: a value of the field of its stage.
  const selectedStage = $derived.by(() => {
    if (tile.type !== 'sankey' || !mine) return null;
    const i = tile.dims.findIndex((d) => sameField(d, mine));
    return i < 0 ? null : i;
  });
  const note = $derived(res && tile.type !== 'kpi' ? partialNote(res, tile.dims[0]?.grain) : null);
  // A link this tile crosses repeats a key on its one side, so the first row was used (V4-D26).
  const linkNotes = $derived(res?.notes ?? []);
  const title = $derived(tileTitle(tile, app.tables, res?.parts));
  // A number card (build 08) is drawn with its style merged in; it fills the tile with its ground and draws its own title.
  const drawn = $derived(drawnTile(tile, app.snap?.workspace.cardStyles));
  const ground = $derived(cardGround(drawn));
  // A note or a picture with no title has no header, so the whole tile drags. Nor has a card.
  const bare = $derived(((fixed || tile.type === 'filter') && !title) || !!ground);
  const canPick = $derived(tile.type === 'histogram' ? tile.measures.length > 0 : tile.type !== 'kpi' && tile.type !== 'cohort' && tile.type !== 'filter' && tile.dims.length > 0);
  // Tiles with a result to open large and rows behind them.
  const hasData = $derived(!fixed && tile.type !== 'filter' && !!q && !!res && !error);

  let el = $state<HTMLElement>();

  async function guarded(work: () => Promise<void> | void) {
    try {
      await work();
    } catch (e) {
      app.error(e);
    }
  }
  const png = () => guarded(() => downloadTilePng(el!, tile, res, app.tables, title));
  const csv = () => guarded(() => downloadCsv(tile, res!, app.tables, title));
  const xlsx = () => guarded(() => downloadXlsx(tile, res!, filters, app.tables, title, !!app.health?.native));

  // A small menu: from the "more" button (the whole tile) or a right-click (one mark).
  let menu = $state<{ x: number; y: number; mark?: { value: Cell } } | null>(null);
  const menuItems = $derived.by(() => {
    if (menu?.mark) {
      const value = menu.mark.value;
      return [{ label: 'Show rows', run: () => onrows(value, res?.otherOf, res?.bins) }];
    }
    return [
      ...(onclip ? [{ label: 'Copy', run: onclip }] : []),
      { label: 'Duplicate', run: onduplicate },
      { label: 'Copy to dashboard', sub: [...dashboards.map((d) => ({ label: d.title || 'Untitled', run: () => oncopy(d.id) })), { label: 'New dashboard', run: () => oncopy(null) }] },
      { label: 'Download as PNG', run: png },
      ...(hasData
        ? [
            { label: 'Download data', sub: [{ label: 'CSV', run: csv }, { label: 'Excel', run: xlsx }] },
            { label: 'Show rows', run: () => onrows() },
            { label: 'Focus', run: onfocus },
          ]
        : []),
      { label: 'Remove', run: onremove },
    ];
  });

  function onkey(e: KeyboardEvent) {
    // Keys typed inside a control of the tile belong to that control.
    if (e.target !== e.currentTarget) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      onopen();
    } else if (e.key === 'ArrowRight' || e.key === 'ArrowDown' || e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      e.preventDefault();
      onnav(e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : -1);
    }
  }
  function more(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: r.right - 200, y: r.bottom + 4 };
  }
</script>

<!-- A tile is a stop for the keyboard: arrows go to the next and the previous tile, Enter opens it. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<article class="tile" class:active class:bare class:phone class:onaccent={ground === 'accent'} style:background={ground ? `var(--${ground})` : undefined} bind:this={el} tabindex="0" data-tile={tile.id} aria-label={title || 'Tile'} onkeydown={onkey}>
  <header role="group" aria-label={phone ? undefined : 'Drag to move'} onpointerdown={phone ? undefined : ondragstart}>
    <h3 title={title}>{title}</h3>
    {#if loading}<span class="spin" aria-label="Loading"></span>{/if}
    {#if hasData}
      <button class="btn plain icon edit keep" title="Open large" aria-label="Open large" onpointerdown={(e) => e.stopPropagation()} onclick={onfocus}>
        <Icon name="expand" size={14} />
      </button>
    {/if}
    {#if !phone}
      <button class="btn plain icon edit" title="More" aria-label="More" aria-haspopup="menu" onpointerdown={(e) => e.stopPropagation()} onclick={more}>
        <Icon name="more" size={14} />
      </button>
      <button class="btn plain icon edit" title="Edit tile" aria-label="Edit tile" onpointerdown={(e) => e.stopPropagation()} onclick={onselect}>
        <Icon name="pencil" size={14} />
      </button>
    {/if}
  </header>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onpointerdown={bare && !phone ? (e) => !(e.target as HTMLElement).closest('a, button') && ondragstart(e) : undefined}>
    {#if fixed}
      <TileBody {tile} res={EMPTY_RESULT} {colors} selected={null} editing tables={app.tables} />
    {:else if !q}
      <button class="setup" onclick={onselect}>
        <Icon name="plus" />
        Choose what to show
      </button>
    {:else if error}
      <p class="error"><Icon name="warning" size={14} /> {error}</p>
    {:else if res}
      <!-- One tile that fails to draw says so; the other tiles stay. -->
      <svelte:boundary>
        <TileBody
          tile={drawn}
          {res}
          {colors}
          {selected}
          {selectedCol}
          {selectedStage}
          tables={app.tables}
          onpick={canPick ? (v, stage) => onpick(v, res?.otherOf, res?.bins, stage) : undefined}
          onpickcol={isPivotLike(tile.type) && tile.cols ? onpickcol : undefined}
          oncontext={canPick ? (v, e) => (menu = { x: e.clientX, y: e.clientY, mark: { value: v } }) : undefined}
          {title}
          {onfilter}
          onwidths={tile.type === 'table' ? onwidths : undefined}
          onlayout={advancedOn() ? onlayout : undefined}
          onretry={() => retry++}
          onplaces={() => (places = true)} />
        {#snippet failed()}
          <p class="error"><Icon name="warning" size={14} /> This tile could not be drawn. Check its fields.</p>
        {/snippet}
      </svelte:boundary>
    {/if}
  </div>
  {#if ignored.length || note || linkNotes.length}
    <footer>
      {#each linkNotes as n (n)}
        <span class="link-note"><Icon name="warning" size={12} /> {n}</span>
      {/each}
      {#if note}<span>{note}</span>{/if}
      {#if ignored.length}
        <span title="This tile's table is not related to the filtered field.">
          <Icon name="info" size={12} /> Not filtered by {ignored.map((f) => f.column).join(', ')}
        </span>
      {/if}
    </footer>
  {/if}
  {#if menu}
    <Menu x={menu.x} y={menu.y} items={menuItems} onclose={() => (menu = null)} />
  {/if}
  {#if !phone}
    <button class="resize" aria-label="Resize tile" onpointerdown={onresizestart}><Icon name="resize" size={14} /></button>
  {/if}
</article>

{#if places}
  <PlaceNames level={tile.map?.level ?? 'province'} onclose={() => (places = false)} />
{/if}

<style>
  .tile {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-card);
    background: var(--surface);
    box-shadow: var(--shadow-tile);
    overflow: hidden;
    transition: box-shadow 120ms;
  }
  .tile.active, .tile:focus-visible { box-shadow: 0 0 0 2px var(--accent); outline: none; }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 6px 0 14px;
    cursor: grab;
    flex: none;
  }
  header:active { cursor: grabbing; }
  h3 {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .edit { width: 24px; height: 24px; color: var(--text-2); opacity: 0; }
  .tile:hover .edit, .tile.active .edit, .tile:focus-within .edit, .edit:focus-visible { opacity: 1; }
  /* On a phone there is no pointer to hover with, so the one button left stays in view. */
  .phone .edit.keep { opacity: 1; }
  .phone header { cursor: default; }
  .body { flex: 1; min-height: 0; padding: 0 14px 10px; position: relative; }
  /* No title: the header shrinks to the edit button, which floats over the corner. */
  .bare header { position: absolute; top: 4px; right: 4px; z-index: 2; height: 24px; padding: 0; background: transparent; border-radius: 12px; }
  .bare:hover header, .bare.active header { background: var(--surface); }
  .bare h3 { display: none; }
  /* On the accent ground the words around the card take its ink (build 08). */
  .onaccent footer, .onaccent .resize { color: var(--on-accent); }
  /* A phone shows the one button at all times: on the accent it sits on a pill of the surface, so it reads. */
  .phone.onaccent header { background: var(--surface); }
  .bare .body { padding-top: 12px; cursor: grab; }
  .setup {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1.5px dashed var(--sep-strong);
    border-radius: 8px;
    background: none;
    color: var(--text-2);
  }
  .setup:hover { border-color: var(--accent); color: var(--accent-text); }
  .error { display: flex; gap: 6px; margin: 0; color: var(--danger); font-size: 12px; }
  .error :global(svg) { flex: none; margin-top: 1px; }
  footer span { display: inline-flex; align-items: center; gap: 5px; }
  /* A sentence about a link may be long: it wraps under its mark instead of cutting. */
  footer span.link-note {
    align-items: flex-start;
    flex: 1 1 100%;
    padding: 4px 8px;
    border-radius: 6px;
    background: color-mix(in srgb, #c47400 14%, var(--surface));
    color: var(--text);
    line-height: 1.35;
  }
  footer span.link-note :global(svg) { flex: none; margin-top: 1px; color: #c47400; }
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    padding: 0 14px 8px;
    font-size: 11px;
    color: var(--text-2);
  }
  .resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-3);
    cursor: nwse-resize;
    opacity: 0;
    display: grid;
    place-items: center;
  }
  .tile:hover .resize, .resize:focus-visible { opacity: 1; }
  .spin {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 1.5px solid var(--sep-strong);
    border-top-color: var(--text-2);
    animation: spin 700ms linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
