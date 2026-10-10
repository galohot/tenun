<script lang="ts">
  // One tile, large. A "Table" switch shows the same result as a plain table. Used by the app and by
  // exported files; Esc closes it.
  import type { QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import type { Named } from '../../lib/format';
  import Modal from '../Modal.svelte';
  import TileBody from './TileBody.svelte';

  let {
    tile,
    res,
    colors,
    title,
    tables = [],
    onclose,
  }: { tile: Tile; res: QueryResult; colors: Colors; title: string; tables?: Named[]; onclose: () => void } = $props();

  let asTable = $state(false);

  // The same rows in a plain table. A number tile has no rows, so its bars are the table.
  const plain = $derived.by(() => {
    if (tile.type === 'kpi') {
      const bars = res.kpi?.bars ?? [];
      return {
        tile: { ...tile, type: 'table' as const, dims: [], measures: tile.measures.slice(0, 1), totals: false },
        res: {
          ...res,
          columns: [
            { name: 'Period', role: 'dimension' as const, dtype: 'text' as const },
            { name: tile.measures[0]?.column ?? 'Value', role: 'measure' as const, dtype: 'number' as const },
          ],
          rows: bars.map((b) => [b.label, b.value]),
        },
      };
    }
    return { tile: { ...tile, type: 'table' as const, totals: false }, res };
  });
  const canTable = $derived(!['text', 'image', 'filter', 'table'].includes(tile.type));
</script>

<Modal title={title || 'Tile'} width={1000} {onclose}>
  {#if canTable}
    <div class="bar">
      <button class="switch" role="switch" aria-checked={asTable} class:on={asTable} onclick={() => (asTable = !asTable)}>
        <span class="knob"></span>
        Table
      </button>
    </div>
  {/if}
  <div class="stage">
    {#if asTable && canTable}
      <TileBody tile={plain.tile} res={plain.res} {colors} selected={null} {tables} />
    {:else}
      <TileBody {tile} {res} {colors} selected={null} {tables} />
    {/if}
  </div>
</Modal>

<style>
  .bar { display: flex; justify-content: flex-end; padding-bottom: 8px; }
  .stage { height: min(62vh, 560px); position: relative; }
  .switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    padding: 0 10px 0 4px;
    border: 1px solid var(--sep-strong);
    border-radius: 13px;
    background: none;
    font-size: 12px;
    color: var(--text-2);
  }
  .knob { width: 18px; height: 18px; border-radius: 9px; background: var(--sep-strong); transition: transform 150ms var(--spring), background 150ms; }
  .switch.on { border-color: var(--accent); color: var(--accent-text); background: var(--selected); }
  .switch.on .knob { background: var(--accent); transform: translateX(0); }
</style>
