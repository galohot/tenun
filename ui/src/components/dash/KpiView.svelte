<script lang="ts">
  // The number tile. It says what the number covers, compares it fairly, and
  // shows where the period sits among the last twelve.
  import type { KpiInfo, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { compact, pctChange, tip } from '../../lib/format';

  let { kpi, tile, colors }: { kpi: KpiInfo; tile: Tile; colors: Colors } = $props();

  const fmt = $derived(tile.formats?.[0] ?? null);
  let w = $state(300);
  let h = $state(120);

  const showBars = $derived(kpi.bars.length > 2 && w >= 200);
  const size = $derived(Math.round(Math.max(26, Math.min(48, Math.min(w / 8.5, h / 2.4)))));

  // Under half a percent is "about the same", not a trend.
  const tone = $derived.by(() => {
    if (kpi.delta === null || Math.abs(kpi.delta) < 0.005) return 'flat';
    const up = kpi.delta > 0;
    return up === ((tile.better ?? 'up') === 'up') ? 'good' : 'bad';
  });
  const pct = $derived(kpi.delta === null ? '' : pctChange(kpi.delta));

  // Column chart geometry, in a 0..100 × 0..40 box.
  const bars = $derived.by(() => {
    const vals = kpi.bars.map((b) => b.value ?? 0);
    const hi = Math.max(0, ...vals);
    const lo = Math.min(0, ...vals);
    const range = hi - lo || 1;
    const n = kpi.bars.length;
    const gap = 1.6;
    const bw = (100 - gap * (n - 1)) / n;
    return kpi.bars.map((b, i) => {
      const v = b.value ?? 0;
      const top = 40 - ((Math.max(v, 0) - lo) / range) * 40;
      const bottom = 40 - ((Math.min(v, 0) - lo) / range) * 40;
      return { x: i * (bw + gap), y: top, w: bw, h: Math.max(bottom - top, 0.8), label: b.label, value: b.value, last: i === n - 1, prev: i === n - 2 };
    });
  });
</script>

<div class="kpi" class:with-bars={showBars} bind:clientWidth={w} bind:clientHeight={h}>
  <div class="text">
    <div class="value" style:font-size="{size}px" title={kpi.value === null ? '' : tip(kpi.value, fmt)}>
      {kpi.value === null ? '–' : compact(kpi.value, fmt)}
    </div>
    <div class="period">{kpi.period}{kpi.partial ? ' · so far' : ''}</div>
    {#if kpi.versus}
      <div class="compare">
        {#if kpi.delta === null}
          <span class="muted">Nothing to compare in {kpi.versus}</span>
        {:else}
          <span class="delta {tone}">
            {tone === 'flat' ? '≈' : kpi.delta > 0 ? '▲' : '▼'}
            {tone === 'flat' ? 'Same' : pct}
          </span>
          <span class="muted">vs {kpi.versus}</span>
        {/if}
      </div>
    {/if}
  </div>
  {#if showBars}
    <svg class="bars" viewBox="0 0 100 40" preserveAspectRatio="none" role="img" aria-label="The last {kpi.bars.length} periods">
      {#each bars as b}
        <rect
          x={b.x}
          y={b.y}
          width={b.w}
          height={b.h}
          fill={b.last || (b.prev && kpi.versus) ? colors.series[0] : colors.grid}
          opacity={b.last ? 1 : b.prev && kpi.versus ? 0.35 : 1}>
          <title>{b.label}: {b.value === null ? '–' : tip(b.value, fmt)}</title>
        </rect>
      {/each}
    </svg>
  {/if}
</div>

<style>
  .kpi { height: 100%; display: flex; align-items: flex-end; gap: 16px; min-height: 0; }
  .text { display: flex; flex-direction: column; min-width: 0; flex: none; max-width: 100%; }
  .value {
    font-weight: 600;
    letter-spacing: -0.025em;
    line-height: 1;
    margin-bottom: 8px;
    white-space: nowrap;
    font-variant-numeric: proportional-nums;
  }
  .period { font-size: 12px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .compare { display: flex; gap: 6px; align-items: baseline; font-size: 12px; white-space: nowrap; overflow: hidden; margin-top: 1px; }
  .delta { font-weight: 600; font-variant-numeric: tabular-nums; }
  .delta.good { color: var(--success); }
  .delta.bad { color: var(--danger); }
  .delta.flat { color: var(--text-2); }
  /* The history fills the rest of the tile, top to bottom. */
  .bars { flex: 1; min-width: 60px; height: 100%; max-height: 120px; display: block; }
</style>
