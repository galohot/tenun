<script lang="ts">
  // A pie or a donut (build 07, V4-D7). The slices, their order, colours and labels come from charts/pie.ts; this
  // adds the pointer (a donut's centre shows the slice under it) and, on a narrow tile, the labels as a list under
  // the chart. When ECharts hides a label that would touch another, the list is shown too, as the legend. A click on
  // a slice filters the other tiles by it, as a bar does.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import type { Named } from '../../lib/format';
  import { buildPie, hiddenLabels, LIST_BELOW, LIST_TILE } from '../../lib/charts/pie';
  import { pctShare } from '../../lib/format';
  import { t } from '../../lib/i18n';
  import ChartView from './ChartView.svelte';

  let {
    tile,
    res,
    colors,
    selected,
    tables = [],
    onpick,
  }: {
    tile: Tile;
    res: QueryResult;
    colors: Colors;
    selected: Cell[] | null;
    tables?: Named[];
    onpick?: (value: Cell) => void;
  } = $props();

  let width = $state(400);
  let height = $state(240);
  let hovered = $state<number | null>(null);
  // On a phone, or in a very narrow tile, there is no room for labels beside the slices: they go under it.
  let windowWidth = $state(1440);
  const listed = $derived(windowWidth < LIST_BELOW || width < LIST_TILE);
  const built = $derived(buildPie({ res, tile, colors, selected, width, height, tables, hovered, listed }));
  // A label hidden on the chart: the list under it names every slice.
  let hidden = $state(false);
  const drawn = (chart: unknown) => {
    const h = !listed && hiddenLabels(chart).length > 0;
    if (h !== hidden) hidden = h;
  };
</script>

<svelte:window bind:innerWidth={windowWidth} />

<div class="pie" class:listed>
  <div class="chart" bind:clientWidth={width} bind:clientHeight={height}>
    <ChartView
      option={built.option}
      tapShows={!!tile.tooltip}
      ondrawn={drawn}
      onhover={tile.type === 'donut' ? (i) => (hovered = i) : undefined}
      onitem={onpick ? (_s, i) => {
        const v = built.slices[i]?.raw;
        if (v !== undefined) onpick(v);
      } : undefined} />
  </div>
  {#if listed || hidden}
    <ul class="list" aria-label={t('Slices')}>
      {#each built.slices as s, i (i)}
        <li>
          <span class="dot" style:background={s.slot === null ? colors.text2 : colors.series[s.slot % colors.series.length]}></span>
          <button type="button" class="name" disabled={!onpick} onclick={() => onpick?.(s.raw)}>{s.label}</button>
          <span class="pct">{pctShare(s.share)}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .pie { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .chart { flex: 1; min-height: 120px; }
  .list { margin: 4px 0 0; padding: 0; list-style: none; display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 2px 12px; font-size: 12px; }
  .list li { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: 0; background: none; padding: 0; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .name:disabled { cursor: default; }
  .pct { color: var(--text-2); font-variant-numeric: tabular-nums; }
</style>
