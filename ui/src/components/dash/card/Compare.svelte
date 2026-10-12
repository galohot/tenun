<script lang="ts">
  // The change against the earlier period: an arrow, the change in words, and the good or bad colour behind them.
  import type { KpiInfo } from '../../../lib/api';
  import { pctChange } from '../../../lib/format';
  import { changeTone } from '../../../lib/card';

  let { kpi, better = 'up' }: { kpi: KpiInfo; better?: 'up' | 'down' } = $props();

  const tone = $derived(changeTone(kpi.delta, better));
</script>

<div class="compare">
  {#if kpi.delta === null}
    <span class="muted">Nothing to compare in {kpi.versus}</span>
  {:else}
    <span class="delta {tone}">{tone === 'flat' ? '≈ Same' : `${kpi.delta > 0 ? '▲' : '▼'} ${pctChange(kpi.delta)}`}</span>
    <span class="muted">vs {kpi.versus}</span>
  {/if}
</div>

<style>
  .compare { display: flex; gap: 6px; align-items: baseline; font-size: 12px; white-space: nowrap; overflow: hidden; min-height: 18px; }
  .delta { font-weight: 600; font-variant-numeric: tabular-nums; }
  .delta.good { color: var(--ink-good); }
  .delta.bad { color: var(--ink-bad); }
  .delta.flat, .muted { color: var(--ink-2); }
</style>
