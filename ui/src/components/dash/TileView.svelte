<script lang="ts">
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { partialNote, runTile } from '../../lib/run';
  import { app } from '../../lib/state.svelte';
  import { readColors, type Colors } from '../../lib/chart';
  import { tileTitle } from '../../lib/format';
  import { appliedFilters, tileQuery, type DashFilter } from '../../lib/tiles';
  import Icon from '../Icon.svelte';
  import TileBody from './TileBody.svelte';

  let {
    tile,
    filters,
    tick,
    active,
    onpick,
    onselect,
    ondragstart,
    onresizestart,
  }: {
    tile: Tile;
    filters: DashFilter[];
    tick: number;
    active: boolean;
    onpick: (value: Cell) => void;
    onselect: () => void;
    ondragstart: (e: PointerEvent) => void;
    onresizestart: (e: PointerEvent) => void;
  } = $props();

  let res = $state<QueryResult | null>(null);
  let error = $state('');
  let loading = $state(false);
  let colors = $state<Colors>(readColors());

  const q = $derived(tileQuery(tile, filters));
  // The type and the comparison setting change what a number tile fetches.
  const qKey = $derived(JSON.stringify([q, tile.type === 'kpi' ? tile.dims : null]));

  // Re-query when the question, the filters or the data change.
  $effect(() => {
    void tick;
    void app.dataVersion;
    const key = qKey;
    if (!q) {
      res = null;
      error = '';
      return;
    }
    loading = true;
    runTile($state.snapshot(tile) as Tile, $state.snapshot(filters) as DashFilter[])
      .then((r) => {
        if (key === qKey) {
          res = r;
          error = '';
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
  const selected = $derived(filters.find((f) => f.fromTile === tile.id)?.values ?? null);
  const note = $derived(res && tile.type !== 'kpi' ? partialNote(res, tile.dims[0]?.grain) : null);
  const title = $derived(tileTitle(tile, app.tables));
  const canPick = $derived(tile.type !== 'kpi' && tile.dims.length > 0);
</script>

<article class="tile" class:active>
  <header role="group" aria-label="Drag to move" onpointerdown={ondragstart}>
    <h3 title={title}>{title}</h3>
    {#if loading}<span class="spin" aria-label="Loading"></span>{/if}
    <button class="btn plain icon edit" title="Edit tile" onpointerdown={(e) => e.stopPropagation()} onclick={onselect}>
      <Icon name="pencil" size={14} />
    </button>
  </header>
  <div class="body">
    {#if !q}
      <button class="setup" onclick={onselect}>
        <Icon name="plus" />
        Choose what to show
      </button>
    {:else if error}
      <p class="error"><Icon name="warning" size={14} /> {error}</p>
    {:else if res}
      <TileBody {tile} {res} {colors} {selected} onpick={canPick ? onpick : undefined} />
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
