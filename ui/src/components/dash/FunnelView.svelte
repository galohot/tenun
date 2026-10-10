<script lang="ts">
  // A funnel (Advanced): one horizontal bar for each stage, centred, with the share that went on to the
  // next stage printed between the bars and the share of the first stage that got through at the bottom.
  // Bars are plain bars, not trapezoids, so the widths can be compared. The stage order is the engine's.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { funnelShape } from '../../lib/charts/funnel';
  import { cell, numberFormatter, getLocale, pctChange } from '../../lib/format';

  let {
    res,
    tile,
    colors,
    selected,
    onpick,
  }: {
    res: QueryResult;
    tile: Tile;
    colors: Colors;
    selected: Cell[] | null;
    onpick?: (v: Cell) => void;
  } = $props();

  const nd = $derived(res.columns.filter((c) => c.role === 'dimension').length);
  const values = $derived(res.rows.map((r) => (typeof r[nd] === 'number' ? (r[nd] as number) : null)));
  const shape = $derived(funnelShape(values));
  const fmt = $derived(numberFormatter(tile.formats?.[0], getLocale(), res.columns[nd]?.unit));
  const name = (v: Cell) => cell(v, res.columns[0]?.dtype ?? 'text', tile.dims[0]?.grain);
  const key = (v: Cell) => (v === null ? '\u0000null' : String(v));
  const dim = (v: Cell) => selected !== null && !selected.some((s) => key(s) === key(v));
  const first = $derived(res.rows.length ? name(res.rows[0][0]) : '');
  const last = $derived(res.rows.length ? name(res.rows.at(-1)![0]) : '');
</script>

<div class="funnel">
  <ol>
    {#each res.rows as r, i}
      {#if i > 0}
        <li class="between" aria-label="{shape.steps[i] === null ? 'No share' : pctChange(shape.steps[i]!)} of {name(res.rows[i - 1][0])} went on to {name(r[0])}">
          <span class="step">{shape.steps[i] === null ? '–' : `↓ ${pctChange(shape.steps[i]!)}`}</span>
        </li>
      {/if}
      <li class="stage" class:dim={dim(r[0])}>
        {#if onpick}
          <button class="line pick" onclick={() => onpick?.(r[0])} title="Filter the other tiles to this stage">
          <span class="label" title={name(r[0])}>{name(r[0])}</span>
          <span class="track"><span class="bar" style:width="{Math.max(shape.widths[i] * 100, values[i] ? 1.5 : 0)}%" style:background={colors.series[0]}></span></span>
          <span class="value">{values[i] === null ? '–' : fmt.long(values[i]!)}</span>
          </button>
        {:else}
          <div class="line">
          <span class="label" title={name(r[0])}>{name(r[0])}</span>
          <span class="track"><span class="bar" style:width="{Math.max(shape.widths[i] * 100, values[i] ? 1.5 : 0)}%" style:background={colors.series[0]}></span></span>
          <span class="value">{values[i] === null ? '–' : fmt.long(values[i]!)}</span>
          </div>
        {/if}
      </li>
    {/each}
  </ol>
  {#if shape.overall !== null}
    <p class="overall">Overall: {pctChange(shape.overall)} of {first} got through to {last}.</p>
  {/if}
</div>

<style>
  .funnel { height: 100%; display: flex; flex-direction: column; justify-content: center; gap: 6px; overflow: auto; }
  ol { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .stage .line {
    display: grid;
    grid-template-columns: minmax(70px, 26%) 1fr auto;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 2px 0;
    border: 0;
    background: none;
    text-align: left;
    font-size: 12px;
  }
  .line.pick { cursor: pointer; border-radius: 6px; }
  .line.pick:hover { background: var(--hover); }
  .stage.dim { opacity: 0.4; }
  .label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .track { display: flex; justify-content: center; height: 24px; }
  .bar { height: 100%; border-radius: 4px; }
  .value { font-variant-numeric: tabular-nums; min-width: 4.5em; text-align: right; }
  .between { display: grid; grid-template-columns: minmax(70px, 26%) 1fr auto; font-size: 11px; color: var(--text-2); }
  .between .step { grid-column: 2; text-align: center; font-variant-numeric: tabular-nums; line-height: 1.5; }
  .overall { flex: none; margin: 4px 0 0; font-size: 12px; color: var(--text-2); }
</style>
