<script lang="ts">
  import { untrack } from 'svelte';
  import type { Bins, Cell, QueryResult, Tile } from '../../lib/api';
  import { partialNote, runTile } from '../../lib/run';
  import { app } from '../../lib/state.svelte';
  import { readColors, type Colors } from '../../lib/chart';
  import { OTHER_KEY, tileTitle } from '../../lib/format';
  import { appliedFilters, EMPTY_RESULT, isStatic, resultKey, tileQuery, type DashFilter } from '../../lib/tiles';
  import Icon from '../Icon.svelte';
  import TileBody from './TileBody.svelte';

  let {
    tile,
    filters,
    tick,
    active,
    onpick,
    onpickcol,
    onresult,
    onselect,
    ondragstart,
    onresizestart,
  }: {
    tile: Tile;
    filters: DashFilter[];
    tick: number;
    active: boolean;
    onpick: (value: Cell, otherOf?: Cell[], bins?: Bins) => void;
    /** A click on a pivot column header. */
    onpickcol: (value: Cell) => void;
    /** Tells the dashboard what this tile shows now (the Inspector's suggestions reuse it). */
    onresult?: (res: QueryResult | null, key: string) => void;
    onselect: () => void;
    ondragstart: (e: PointerEvent) => void;
    onresizestart: (e: PointerEvent) => void;
  } = $props();

  let res = $state<QueryResult | null>(null);
  let error = $state('');
  let loading = $state(false);
  let colors = $state<Colors>(readColors());

  const q = $derived(tileQuery(tile, filters));
  const qKey = $derived(resultKey(tile, filters));
  const fixed = $derived(isStatic(tile));

  // Re-query when the question, the filters or the data change.
  $effect(() => {
    void tick;
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
    runTile($state.snapshot(tile) as Tile, $state.snapshot(filters) as DashFilter[])
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

  const ignored = $derived(
    res ? res.ignored_filters.map((i) => appliedFilters(tile, filters)[i]).filter(Boolean) : [],
  );
  // A click on "Other" is a not_in filter; the tile shows it as the Other group being picked.
  // A click on a histogram bin is a range; the tile shows the bin that starts it.
  const mine = $derived(filters.find((x) => x.fromTile === tile.id));
  // On a pivot, a click on a column header filters by the column field instead of the first row field.
  const onCol = $derived(tile.type === 'pivot' && !!mine && !!tile.cols && mine.table === tile.cols.table && mine.column === tile.cols.column);
  const selected = $derived(mine && !onCol ? (mine.op === 'not_in' ? [OTHER_KEY] : mine.op === 'range' ? mine.values.slice(0, 1) : mine.values) : null);
  const selectedCol = $derived(mine && onCol ? mine.values : null);
  const note = $derived(res && tile.type !== 'kpi' ? partialNote(res, tile.dims[0]?.grain) : null);
  const title = $derived(tileTitle(tile, app.tables));
  // A note or a picture with no title has no header, so the whole tile drags.
  const bare = $derived(fixed && !title);
  const canPick = $derived(tile.type === 'histogram' ? tile.measures.length > 0 : tile.type !== 'kpi' && tile.dims.length > 0);
</script>

<article class="tile" class:active class:bare>
  <header role="group" aria-label="Drag to move" onpointerdown={ondragstart}>
    <h3 title={title}>{title}</h3>
    {#if loading}<span class="spin" aria-label="Loading"></span>{/if}
    <button class="btn plain icon edit" title="Edit tile" onpointerdown={(e) => e.stopPropagation()} onclick={onselect}>
      <Icon name="pencil" size={14} />
    </button>
  </header>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onpointerdown={bare ? (e) => !(e.target as HTMLElement).closest('a, button') && ondragstart(e) : undefined}>
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
      <TileBody
        {tile}
        {res}
        {colors}
        {selected}
        {selectedCol}
        tables={app.tables}
        onpick={canPick ? (v) => onpick(v, res?.otherOf, res?.bins) : undefined}
        onpickcol={tile.type === 'pivot' && tile.cols ? onpickcol : undefined} />
    {/if}
  </div>
  {#if ignored.length || note}
    <footer>
      {#if note}<span>{note}</span>{/if}
      {#if ignored.length}
        <span title="This tile's table is not related to the filtered field.">
          <Icon name="info" size={12} /> Not filtered by {ignored.map((f) => f.column).join(', ')}
        </span>
      {/if}
    </footer>
  {/if}
  <button class="resize" aria-label="Resize tile" onpointerdown={onresizestart}><Icon name="resize" size={14} /></button>
</article>

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
  .tile.active { box-shadow: 0 0 0 2px var(--accent); }
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
  .tile:hover .edit, .tile.active .edit, .edit:focus-visible { opacity: 1; }
  .body { flex: 1; min-height: 0; padding: 0 14px 10px; position: relative; }
  /* No title: the header shrinks to the edit button, which floats over the corner. */
  .bare header { position: absolute; top: 4px; right: 4px; z-index: 2; height: 24px; padding: 0; background: var(--surface); border-radius: 12px; }
  .bare h3 { display: none; }
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
  footer {
    display: flex;
    align-items: center;
    gap: 12px;
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
  .tile:hover .resize { opacity: 1; }
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
