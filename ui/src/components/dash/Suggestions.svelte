<script lang="ts">
  // "Also try": up to three small live previews of other tile types that suit the chosen fields.
  // A preview reuses the tile's own result when the new type asks the same question, and asks the
  // engine once more when it needs a different one. A type that fails or has no data is not offered.
  import type { QueryResult, Tile, TileType } from '../../lib/api';
  import { buildOption, CHART_TYPES, readColors, type Colors } from '../../lib/chart';
  import { miniOption } from '../../lib/charts/mini';
  import { runTile } from '../../lib/run';
  import { app } from '../../lib/state.svelte';
  import { suggestTypes, type Suggestion } from '../../lib/suggest';
  import { resultKey, tileQuery, type DashFilter } from '../../lib/tiles';
  import ChartView from './ChartView.svelte';
  import TileBody from './TileBody.svelte';

  let {
    tile,
    current,
    filters,
    onapply,
  }: {
    tile: Tile;
    /** What the tile shows now, with the key of the question it answers. */
    current: { res: QueryResult; key: string } | null;
    filters: DashFilter[];
    onapply: (t: Tile) => void;
  } = $props();
  // A result that answers an older question (the fields just changed) is not used.
  const res = $derived(current && current.key === resultKey(tile, filters) ? current.res : null);

  const NAMES: Partial<Record<TileType, string>> = {
    bar: 'Bar', line: 'Line', area: 'Area', share: 'Share', kpi: 'Number', table: 'Table',
    scatter: 'Scatter', histogram: 'Histogram', pivot: 'Pivot',
  };
  /** The preview box, and the size the non-chart types are drawn at before they are scaled down. */
  const W = 96;
  const H = 60;
  const FULL_W = 320;
  const FULL_H = 200;

  let colors = $state<Colors>(readColors());
  $effect(() => {
    void app.theme;
    requestAnimationFrame(() => (colors = readColors()));
  });

  // How many different values the first group-by field has: a bar or a share strip suits 12 or fewer.
  const groups = $derived.by(() => {
    if (!res || res.columns[0]?.role !== 'dimension' || tile.type === 'histogram') return null;
    if (res.partial || res.other) return null;
    return new Set(res.rows.map((r) => String(r[0]))).size;
  });
  const candidates = $derived(suggestTypes($state.snapshot(tile) as Tile, app.tables, groups));
  const sig = $derived(JSON.stringify([candidates.map((c) => c.variant), filters, app.dataVersion, app.locale]));

  let got = $state.raw<Record<string, QueryResult>>({});
  let gen = 0;

  // Same question as the tile asks now: the tile's result can be drawn again as it is.
  const reusable = (s: Suggestion) =>
    !!res &&
    !['kpi', 'histogram'].includes(s.variant.type) &&
    !['kpi', 'histogram', 'pivot', 'table'].includes(tile.type) &&
    JSON.stringify(tileQuery(s.variant, [])) === JSON.stringify(tileQuery(tile, []));

  $effect(() => {
    void sig;
    void res;
    const mine = ++gen;
    const todo = candidates;
    const next: Record<string, QueryResult> = {};
    const timer = setTimeout(async () => {
      await Promise.all(
        todo.map(async (s) => {
          try {
            const r = reusable(s) ? res! : await runTile(JSON.parse(JSON.stringify(s.variant)) as Tile, $state.snapshot(filters) as DashFilter[]);
            if (r.rows.length > 0 || r.kpi) next[s.type] = r;
          } catch {
            // The type does not fit these fields: it is left out.
          }
        }),
      );
      if (mine === gen) got = next;
    }, 250);
    return () => clearTimeout(timer);
  });

  const shown = $derived(candidates.filter((s) => got[s.type]));

  /** The thumbnail's chart option, or null when the chart cannot be built. */
  function option(s: Suggestion) {
    try {
      return miniOption(buildOption({ res: got[s.type], tile: s.variant, colors, selected: null, width: W, height: H, tables: app.tables }).option);
    } catch {
      return null;
    }
  }
</script>

{#if shown.length}
  <div class="sec">
    <span class="label">Also try</span>
    <div class="row">
      {#each shown as s (s.type)}
        {@const o = (CHART_TYPES as readonly string[]).includes(s.type) ? option(s) : null}
        {#if o || !(CHART_TYPES as readonly string[]).includes(s.type)}
          <button class="card" title="Show this as a {NAMES[s.type] ?? s.type}" onclick={() => onapply(s.variant)}>
            <span class="thumb" style:width="{W}px" style:height="{H}px" aria-hidden="true">
              {#if o}
                <ChartView option={o} />
              {:else}
                <span class="scaled" style:width="{FULL_W}px" style:height="{FULL_H}px" style:transform="scale({W / FULL_W})">
                  <TileBody tile={s.variant} res={got[s.type]} {colors} selected={null} tables={app.tables} />
                </span>
              {/if}
            </span>
            <span class="name">{NAMES[s.type] ?? s.type}</span>
          </button>
        {/if}
      {/each}
    </div>
  </div>
{/if}

<style>
  .sec { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; gap: 6px; }
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0;
    border: 0;
    background: none;
    text-align: center;
    font-size: 11px;
    color: var(--text-2);
  }
  .thumb {
    position: relative;
    display: block;
    overflow: hidden;
    border: 1px solid var(--sep);
    border-radius: 8px;
    background: var(--surface);
    box-sizing: border-box;
  }
  .thumb :global(.chart) { min-height: 0; pointer-events: none; }
  .card:hover .thumb { border-color: var(--accent); }
  .card:hover .name { color: var(--accent-text); }
  .scaled { position: absolute; left: 0; top: 0; transform-origin: 0 0; padding: 8px 12px; box-sizing: border-box; pointer-events: none; }
</style>
