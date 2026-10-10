<script lang="ts">
  // Draws one tile from a query result. Used by the app and by exported files.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { buildOption, pivot, type Colors } from '../../lib/chart';
  import { cell, compact, full } from '../../lib/format';
  import ChartView from './ChartView.svelte';

  let {
    tile,
    res,
    colors,
    selected,
    onpick,
  }: {
    tile: Tile;
    res: QueryResult;
    colors: Colors;
    selected: Cell[] | null;
    onpick?: (value: Cell) => void;
  } = $props();

  let width = $state(400);

  const option = $derived(
    tile.type === 'kpi' || tile.type === 'table' ? null : buildOption({ res, tile, colors, selected, width }),
  );
  const categories = $derived(tile.type === 'donut' || tile.type === 'kpi' || tile.type === 'table' ? [] : pivot(res, tile).categories);

  // KPI: the total, or with a date the latest period and its change.
  const kpi = $derived.by(() => {
    if (tile.type !== 'kpi') return null;
    const mi = res.columns.findIndex((c) => c.role === 'measure');
    if (mi < 0) return null;
    if (mi === 0) return { value: res.rows[0]?.[0] as number | null, delta: null, points: [], period: '' };
    const pts = [...res.rows].sort((a, b) => String(a[0]).localeCompare(String(b[0]))).map((r) => [r[0], r[mi] as number] as const);
    const last = pts.at(-1);
    const prev = pts.at(-2);
    return {
      value: last?.[1] ?? null,
      delta: last && prev && prev[1] ? (last[1] - prev[1]) / Math.abs(prev[1]) : null,
      points: pts.map((p) => p[1] ?? 0),
      period: last ? cell(last[0], res.columns[0].dtype, tile.dims[0]?.grain) : '',
      prevPeriod: prev ? cell(prev[0], res.columns[0].dtype, tile.dims[0]?.grain) : '',
    };
  });

  function spark(points: number[], w = 120, h = 32): string {
    if (points.length < 2) return '';
    const min = Math.min(...points);
    const max = Math.max(...points);
    const span = max - min || 1;
    return points
      .map((p, i) => `${((i / (points.length - 1)) * w).toFixed(1)},${(h - 2 - ((p - min) / span) * (h - 4)).toFixed(1)}`)
      .join(' ');
  }

  // Table: click a header to sort.
  let sortBy = $state<number | null>(null);
  let sortDesc = $state(true);
  const rows = $derived.by(() => {
    if (sortBy === null) return res.rows;
    const i = sortBy;
    return [...res.rows].sort((a, b) => {
      const x = a[i], y = b[i];
      if (x === y) return 0;
      if (x === null) return 1;
      if (y === null) return -1;
      const c = typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y), undefined, { numeric: true });
      return sortDesc ? -c : c;
    });
  });
  const selKeys = $derived(new Set((selected ?? []).map((v) => String(v))));
</script>

{#if res.rows.length === 0}
  <div class="none">No data for this selection.</div>
{:else if tile.type === 'kpi' && kpi}
  <div class="kpi">
    <div class="kpi-row">
      <div class="value" title={kpi.value === null ? '' : full(kpi.value)}>{kpi.value === null ? '–' : compact(kpi.value)}</div>
      {#if kpi.points.length > 2}
        <svg class="spark" viewBox="0 0 120 32" preserveAspectRatio="none" aria-hidden="true">
          <polyline points={spark(kpi.points)} fill="none" stroke={colors.series[0]} stroke-width="1.6" vector-effect="non-scaling-stroke" stroke-linejoin="round" stroke-linecap="round" />
        </svg>
      {/if}
    </div>
    {#if kpi.period}
      <div class="sub">
        {#if kpi.delta !== null}
          <span class="delta" class:up={kpi.delta >= 0} class:down={kpi.delta < 0}>
            {kpi.delta >= 0 ? '▲' : '▼'} {Math.abs(kpi.delta * 100).toFixed(1)}%
          </span>
          <span class="muted">{kpi.period} vs {kpi.prevPeriod}</span>
        {:else}
          <span class="muted">{kpi.period}</span>
        {/if}
      </div>
    {/if}
  </div>
{:else if tile.type === 'table'}
  <div class="tablewrap">
    <table>
      <thead>
        <tr>
          {#each res.columns as c, i}
            <th class:num={c.dtype === 'number'}>
              <button onclick={() => (sortBy === i ? (sortDesc = !sortDesc) : ((sortBy = i), (sortDesc = c.dtype === 'number')))}>
                {c.role === 'measure' ? c.name : c.name}{sortBy === i ? (sortDesc ? ' ↓' : ' ↑') : ''}
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
            onclick={() => tile.dims.length && onpick?.(r[0])}>
            {#each r as v, i}
              <td class:num={res.columns[i].dtype === 'number'}>{cell(v, res.columns[i].dtype, tile.dims[i]?.grain)}</td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else if option}
  <ChartView
    {option}
    horizontal={!!tile.horizontal && tile.type === 'bar'}
    pie={tile.type === 'donut'}
    onresize={(w) => (width = w)}
    onpick={onpick &&
      ((i, raw) => {
        if (tile.type === 'donut') {
          if (raw !== null && raw !== undefined) onpick?.(raw as Cell);
        } else if (i < categories.length) onpick?.(categories[i]);
      })} />
{/if}

<style>
  .none { height: 100%; display: grid; place-items: center; color: var(--text-2); font-size: 12px; }
  .kpi { height: 100%; display: flex; flex-direction: column; justify-content: flex-end; gap: 6px; padding-bottom: 2px; }
  .kpi-row { display: flex; align-items: flex-end; gap: 12px; }
  .value { font-size: 34px; font-weight: 600; letter-spacing: -0.02em; line-height: 1; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .sub { display: flex; gap: 8px; align-items: baseline; font-size: 12px; white-space: nowrap; overflow: hidden; }
  .delta { font-weight: 600; color: var(--text-2); }
  .delta.up { color: var(--success); }
  .delta.down { color: var(--danger); }
  .spark { flex: 1; min-width: 0; max-width: 160px; height: 30px; margin-left: auto; }
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
</style>
