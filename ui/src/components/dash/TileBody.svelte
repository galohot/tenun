<script lang="ts">
  // Draws one tile from its result. Used by the app and by exported files.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { buildOption, CHART_TYPES, type Colors } from '../../lib/chart';
  import { cell, OTHER_KEY, type Named } from '../../lib/format';
  import ChartView from './ChartView.svelte';
  import KpiView from './KpiView.svelte';
  import ShareView from './ShareView.svelte';

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
    /** For naming the values of a chart with several. */
    tables?: Named[];
    onpick?: (value: Cell) => void;
  } = $props();

  let width = $state(400);
  let height = $state(240);

  const isChart = $derived((CHART_TYPES as readonly string[]).includes(tile.type));
  // A scatter needs two values and a field for its dots; without them there is nothing to draw.
  const unfinished = $derived(tile.type === 'scatter' && (tile.measures.length < 2 || !tile.dims.length));
  const built = $derived(isChart && !unfinished ? buildOption({ res, tile, colors, selected, width, height, tables }) : null);

  // Table: click a header to sort.
  let sortBy = $state<number | null>(null);
  let sortDesc = $state(true);
  const rows = $derived.by(() => {
    if (sortBy === null) return res.rows;
    const i = sortBy;
    // "Other" (the rest outside a top N) stays last whatever the order.
    const isOther = (r: Cell[]) => !!res.other && r[0] === OTHER_KEY;
    const sorted = [...res.rows.filter((r) => !isOther(r))].sort((a, b) => {
      const x = a[i],
        y = b[i];
      if (x === y) return 0;
      if (x === null) return 1;
      if (y === null) return -1;
      const c = typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y), undefined, { numeric: true });
      return sortDesc ? -c : c;
    });
    return [...sorted, ...res.rows.filter(isOther)];
  });
  // The format of a value column: the columns are the groups, then the values.
  const groups = $derived(res.columns.filter((c) => c.role === 'dimension').length);
  const formatOf = (i: number) => (res.columns[i]?.role === 'measure' ? (tile.formats?.[i - groups] ?? null) : null);
  const note = $derived(built?.note ?? null);
  const selKeys = $derived(new Set((selected ?? []).map((v) => String(v))));
</script>

{#if tile.type === 'kpi' && res.kpi}
  <KpiView kpi={res.kpi} {tile} {colors} />
{:else if unfinished}
  <div class="none">A scatter chart needs two values and a field to draw one dot for each.</div>
{:else if res.rows.length === 0}
  <div class="none">No data for this selection.</div>
{:else if tile.type === 'share' || tile.type === 'donut'}
  <ShareView {res} {tile} {colors} {selected} {onpick} />
{:else if tile.type === 'table'}
  <div class="tablewrap">
    <table>
      <thead>
        <tr>
          {#each res.columns as c, i}
            <th class:num={c.dtype === 'number'}>
              <button onclick={() => (sortBy === i ? (sortDesc = !sortDesc) : ((sortBy = i), (sortDesc = c.dtype === 'number')))}>
                {c.name}{sortBy === i ? (sortDesc ? ' ↓' : ' ↑') : ''}
              </button>
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each rows as r}
          <tr
            class:dim={selected !== null && !selKeys.has(String(r[0]))}
            class:pick={!!onpick && tile.dims.length > 0}
            class:other={!!res.other && r[0] === OTHER_KEY}
            onclick={() => tile.dims.length && onpick?.(r[0])}>
            {#each r as v, i}
              <td class:num={res.columns[i].dtype === 'number'}>{cell(v, res.columns[i].dtype, tile.dims[i]?.grain, false, formatOf(i))}</td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else if built}
  <div class="chart">
    <div class="plot">
      <ChartView
        option={built.option}
        horizontal={!!tile.horizontal && tile.type === 'bar'}
        onresize={(w, h) => ((width = w), (height = h))}
        onpick={onpick && !built.pick
          ? (i) => {
              const v = built.categories[i];
              if (i < built.categories.length && v !== undefined) onpick?.(v);
            }
          : undefined}
        onitem={onpick && built.pick?.by === 'item'
          ? (si, i) => {
              const v = built.pick?.by === 'item' ? built.pick.value(si, i) : undefined;
              if (v !== undefined) onpick?.(v);
            }
          : undefined}
        onx={onpick && built.pick?.by === 'x'
          ? (x) => {
              const v = built.pick?.by === 'x' ? built.pick.value(x) : undefined;
              if (v !== undefined) onpick?.(v);
            }
          : undefined} />
    </div>
    {#if note}<p class="size-note">{note}</p>{/if}
  </div>
{/if}

<style>
  .chart { height: 100%; display: flex; flex-direction: column; }
  .plot { flex: 1; min-height: 0; }
  .size-note { flex: none; margin: 4px 0 0; font-size: 11px; color: var(--text-2); }
  .none { height: 100%; display: grid; place-items: center; color: var(--text-2); font-size: 12px; }
  .tablewrap { height: 100%; overflow: auto; margin: 0 -14px; }
  table { border-collapse: collapse; width: 100%; font-size: 12px; }
  th { position: sticky; top: 0; background: var(--surface); text-align: left; padding: 0; border-bottom: 1px solid var(--sep); }
  th button {
    width: 100%;
    border: 0;
    background: none;
    padding: 6px 14px;
    font-weight: 600;
    text-align: inherit;
    white-space: nowrap;
  }
  td { padding: 5px 14px; border-bottom: 1px solid var(--sep); white-space: nowrap; }
  .num, .num button { text-align: right; font-variant-numeric: tabular-nums; }
  tr.pick { cursor: pointer; }
  tr.pick:hover td { background: var(--window); }
  tr.dim td { opacity: 0.4; }
  tr.other td { color: var(--text-2); }
</style>
