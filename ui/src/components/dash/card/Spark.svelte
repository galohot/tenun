<script lang="ts">
  // The trend of a card: the last periods as columns (the number tile's own) or as a line with a dot on the latest.
  // The latest period has the mark colour; the one it is compared with is lighter; the rest carry no colour. A period
  // that is not over ends the line with a dashed step, as every chart over time draws it.
  import type { KpiInfo, NumFormat } from '../../../lib/api';
  import { tip } from '../../../lib/format';
  import { barBoxes, linePoints } from '../../../lib/card';

  let { kpi, kind = 'bars', fmt = null }: { kpi: KpiInfo; kind?: 'bars' | 'line'; fmt?: NumFormat | null } = $props();

  let w = $state(200);
  let h = $state(40);
  const values = $derived(kpi.bars.map((b) => b.value));
  const boxes = $derived(barBoxes(values));
  const points = $derived(linePoints(values, w, h));
  // A line through points, broken where a value is missing.
  const pathOf = (pts: typeof points) => {
    let d = '';
    let pen = false;
    for (const p of pts) {
      if (Number.isNaN(p.y)) {
        pen = false;
        continue;
      }
      d += `${pen ? 'L' : 'M'}${p.x.toFixed(1)} ${p.y.toFixed(1)}`;
      pen = true;
    }
    return d;
  };
  const solid = $derived(pathOf(kpi.partial ? points.slice(0, -1) : points));
  const dashed = $derived(kpi.partial ? pathOf(points.slice(-2)) : '');
  const last = $derived(points[points.length - 1]);
  const said = (i: number) => `${kpi.bars[i].label}: ${kpi.bars[i].value === null ? '–' : tip(kpi.bars[i].value as number, fmt)}`;
</script>

<div class="spark" bind:clientWidth={w} bind:clientHeight={h}>
  {#if kind === 'line'}
    <svg width={w} height={h} role="img" aria-label="The last {kpi.bars.length} periods">
      <path d={solid} fill="none" stroke="var(--ink-mark)" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" />
      {#if dashed}<path d={dashed} fill="none" stroke="var(--ink-mark)" stroke-width="1.5" stroke-dasharray="3 3" stroke-linecap="round" />{/if}
      {#if last && !Number.isNaN(last.y)}<circle cx={last.x} cy={last.y} r="3" fill="var(--ink-mark)" />{/if}
      {#each points as p, i (i)}
        {#if !Number.isNaN(p.y)}<circle cx={p.x} cy={p.y} r="6" fill="transparent"><title>{said(i)}</title></circle>{/if}
      {/each}
    </svg>
  {:else}
    <svg viewBox="0 0 100 40" preserveAspectRatio="none" role="img" aria-label="The last {kpi.bars.length} periods">
      {#each boxes as b, i (i)}
        {@const lastBar = i === boxes.length - 1}
        {@const prev = i === boxes.length - 2 && !!kpi.versus}
        <rect x={b.x} y={b.y} width={b.w} height={b.h} fill={lastBar || prev ? 'var(--ink-mark)' : 'var(--ink-soft)'} opacity={prev ? 0.35 : 1}>
          <title>{said(i)}</title>
        </rect>
      {/each}
    </svg>
  {/if}
</div>

<style>
  .spark { flex: 1; min-height: 28px; max-height: 120px; min-width: 60px; }
  svg { display: block; width: 100%; height: 100%; overflow: visible; }
</style>
