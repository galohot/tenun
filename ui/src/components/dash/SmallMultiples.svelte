<script lang="ts">
  // Small multiples (Advanced): one small chart for each value of a field, on one shared scale unless
  // the tile asks for its own. The engine keeps at most twelve charts and says how many more there are.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { chartColumns, sharedScale, splitCharts } from '../../lib/charts/small';
  import { cell, type Named } from '../../lib/format';
  import MiniChart from './MiniChart.svelte';

  let {
    res,
    tile,
    colors,
    selected,
    tables = [],
    onpick,
  }: {
    res: QueryResult;
    tile: Tile;
    colors: Colors;
    selected: Cell[] | null;
    tables?: Named[];
    onpick?: (value: Cell) => void;
  } = $props();

  let width = $state(600);
  let height = $state(300);

  const parts = $derived(splitCharts(res));
  const scale = $derived(tile.ownScale ? null : sharedScale(parts));
  const more = $derived(res.repeat?.more ?? 0);
  // One group, one value, no lines of words: a small chart has room for the shapes only.
  const mini = $derived<Tile>({
    ...tile,
    repeat: undefined,
    ownScale: undefined,
    layout: undefined,
    compare: undefined,
    dims: tile.dims.slice(0, 1),
    measures: tile.measures.slice(0, 1),
    labels: tile.labels ?? 'none',
  });
  const cols = $derived(chartColumns(width, parts.length));
  const rows = $derived(Math.max(1, Math.ceil(parts.length / cols)));
  const rowH = $derived(Math.max(120, Math.floor((height - (more ? 22 : 0) - (rows - 1) * 12) / rows)));
  const title = (key: Cell) => cell(key, res.columns[1]?.dtype ?? 'text', tile.repeat?.grain);
</script>

<div class="multiples" bind:clientWidth={width} bind:clientHeight={height}>
  <div class="grid" style:grid-template-columns="repeat({cols}, minmax(0, 1fr))" style:grid-auto-rows="{rowH}px">
    {#each parts as p (String(p.key))}
      <figure>
        <figcaption>
          <span class="name" title={title(p.key)}>{title(p.key)}</span>
          {#if tile.ownScale}<span class="own">Own scale</span>{/if}
        </figcaption>
        <div class="plot">
          <MiniChart res={p.res} tile={mini} {colors} {selected} {tables} {scale} {onpick} />
        </div>
      </figure>
    {/each}
  </div>
  {#if more > 0}<p class="more">and {more} more</p>{/if}
</div>

<style>
  .multiples { height: 100%; overflow: auto; }
  .grid { display: grid; gap: 12px; }
  figure { margin: 0; min-width: 0; display: flex; flex-direction: column; }
  figcaption { flex: none; display: flex; align-items: baseline; gap: 6px; font-size: 12px; font-weight: 600; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .own { flex: none; font-size: 11px; font-weight: 400; color: var(--text-2); }
  .plot { flex: 1; min-height: 0; }
  .more { margin: 6px 0 0; font-size: 12px; color: var(--text-2); }
</style>
