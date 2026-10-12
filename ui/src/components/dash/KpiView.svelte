<script lang="ts">
  // The number tile. It says what the number covers, compares it fairly, and
  // shows where the period sits among the last twelve.
  import type { KpiInfo, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { compact, pctChange, pctOfTarget, tip } from '../../lib/format';
  import { targetInfo, targetWords } from '../../lib/kpi';
  import { barBoxes, changeTone } from '../../lib/card';
  import { paceElapsed, whyNotAdded, type Parts } from '../../lib/measures';
  import Icon from '../Icon.svelte';

  let { kpi, tile, colors, parts }: { kpi: KpiInfo; tile: Tile; colors: Colors; parts?: Parts } = $props();

  const fmt = $derived(tile.formats?.[0] ?? null);
  let w = $state(300);
  let h = $state(120);

  // A target adds a line, so the figure gets a little smaller to leave room for it.
  const target = $derived(tile.target?.value ?? null);
  // The pace is the value so far scaled up to the whole period. That is right for a sum or a count and
  // wrong for a ratio or an average, whose value so far is already the value of the period (V4-D85d).
  const goal = $derived(targetInfo(kpi.value, target, paceElapsed(tile, kpi.elapsed, parts), tile.better ?? 'up'));
  // Only a period that is not over has a pace: the value so far, scaled up to the whole period.
  const words = $derived(goal ? targetWords(goal, tile.better ?? 'up') : null);
  const pace = $derived(words?.pace ?? '');
  // With a target there are two more lines of text, so the history waits for a wider tile.
  // One row high: the value and the change, nothing else. The rest moves to the tooltip.
  const mini = $derived(tile.h <= 1);
  const showBars = $derived(!mini && kpi.bars.length > 2 && w >= (goal ? 420 : 200));
  // In a narrow tile the target line would wrap and push the figure into the title. The chip stays
  // and the words move to its tooltip.
  const narrow = $derived(w < 200);
  const size = $derived(
    mini ? Math.round(Math.max(16, Math.min(30, h - 6, w / 7))) : Math.round(Math.max(26, Math.min(48, Math.min(w / 8.5, h / (goal ? (narrow ? 3 : 3.3) : 2.4))))),
  );
  // A value that does not add up has no pace: the period is not over, and the line says why there is none.
  const noPace = $derived(typeof kpi.elapsed === 'number' && kpi.elapsed > 0 && kpi.elapsed < 1 ? whyNotAdded(tile, 0, parts) : null);
  const goalWords = $derived(
    target === null || !goal ? '' : `Target ${compact(target, fmt)} · ${pctOfTarget(goal.share)} of target${pace ? ` · ${pace}` : ''}${noPace ? ` · ${noPace}, so no pace` : ''}`,
  );

  // Under half a percent is "about the same", not a trend.
  const tone = $derived(changeTone(kpi.delta, tile.better ?? 'up'));
  const pct = $derived(kpi.delta === null ? '' : pctChange(kpi.delta));

  // Column chart geometry, in a 0..100 × 0..40 box (shared with the cards of build 08).
  const bars = $derived(
    barBoxes(kpi.bars.map((b) => b.value)).map((g, i, all) => ({ ...g, label: kpi.bars[i].label, value: kpi.bars[i].value, last: i === all.length - 1, prev: i === all.length - 2 })),
  );
</script>

<div class="kpi" class:with-bars={showBars} class:mini bind:clientWidth={w} bind:clientHeight={h}>
  <div class="text">
    <div
      class="value"
      style:font-size="{size}px"
      title={kpi.value === null ? '' : `${tip(kpi.value, fmt)}${mini ? ` · ${kpi.period}${kpi.partial ? ' · so far' : ''}` : ''}`}>
      {kpi.value === null ? '–' : compact(kpi.value, fmt)}
    </div>
    {#if !mini}<div class="period">{kpi.period}{kpi.partial ? ' · so far' : ''}</div>{/if}
    {#if kpi.versus}
      <div class="compare" title={mini ? `vs ${kpi.versus}` : undefined}>
        {#if kpi.delta === null}
          {#if !mini}<span class="muted">Nothing to compare in {kpi.versus}</span>{/if}
        {:else}
          <span class="delta {tone}">
            {tone === 'flat' ? '≈' : kpi.delta > 0 ? '▲' : '▼'}
            {tone === 'flat' ? 'Same' : pct}
          </span>
          {#if !mini}<span class="muted">vs {kpi.versus}</span>{/if}
        {/if}
      </div>
    {/if}
    {#if goal && target !== null && !mini}
      <div class="goal" title={narrow ? `${goalWords}${goal.projected === null ? '' : `. At this pace the period ends near ${tip(goal.projected, fmt)}`}.` : goal.projected === null ? '' : `At this pace the period ends near ${tip(goal.projected, fmt)}.`}>
        <span class="status {goal.onTrack ? 'good' : 'bad'}">
          <Icon name={goal.onTrack ? 'check' : 'warning'} size={12} />
          {words?.chip}
        </span>
        {#if !narrow}<span class="muted">{goalWords}</span>{/if}
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
  .mini { align-items: center; }
  .mini .text { flex-direction: row; align-items: baseline; gap: 10px; }
  .mini .value { margin-bottom: 0; }
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
  .goal { display: flex; flex-wrap: wrap; gap: 2px 6px; align-items: center; font-size: 12px; margin-top: 4px; overflow: hidden; }
  .goal .muted { line-height: 1.35; }
  /* The status is an icon and a word. The colour only backs them up. */
  .status {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 20px;
    padding: 0 7px 0 5px;
    border-radius: 10px;
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
  }
  .status.good { background: color-mix(in srgb, var(--success) 16%, var(--surface)); }
  .status.good :global(svg) { color: var(--success); }
  .status.bad { background: color-mix(in srgb, var(--danger) 14%, var(--surface)); }
  .status.bad :global(svg) { color: var(--danger); }
  /* The history fills the rest of the tile, top to bottom. */
  .bars { flex: 1; min-width: 60px; height: 100%; max-height: 120px; display: block; }
</style>
