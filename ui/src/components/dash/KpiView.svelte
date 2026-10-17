<script lang="ts">
  // The number tile. It says what the number covers, compares it fairly, and
  // shows where the period sits among the last twelve.
  import type { KpiInfo, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { compact, pctChange, pctOfTarget, tip, toneFor } from '../../lib/format';
  import { targetInfo, targetWords } from '../../lib/kpi';
  import { barBoxes, changeTone } from '../../lib/card';
  import { paceElapsed, whyNotAdded, type Parts } from '../../lib/measures';
  import { t } from '../../lib/i18n';
  import Icon from '../Icon.svelte';

  let {
    kpi,
    tile,
    colors,
    parts,
    delta = 'inline',
  }: {
    kpi: KpiInfo;
    tile: Tile;
    colors: Colors;
    parts?: Parts;
    /** `none`: the tile's header shows the change as a pill (look v3), so it is not repeated here. */
    delta?: 'inline' | 'none';
  } = $props();

  const fmt = $derived(tile.formats?.[0] ?? null);
  // A format that marks the value (red negatives, a code's [Red] or [Green]) colours the number.
  const TONE_INK: Record<string, string | undefined> = { bad: 'var(--danger)', good: 'var(--success)' };
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
  // One row high: the value and the change, nothing else. The rest moves to the tooltip.
  const mini = $derived(tile.h <= 1);
  // In a narrow tile the target line would wrap and push the figure into the title. The chip stays
  // and the words move to its tooltip.
  const narrow = $derived(w < 200);
  // The value is 30 px in every number tile (look v3); a one-row tile fits it to its height.
  const size = $derived(mini ? Math.round(Math.max(16, Math.min(30, h - 6, w / 7))) : 30);
  // The history sits under the value and fills what is left of the tile, when that is enough to read.
  let room = $state(0);
  const showBars = $derived(!mini && kpi.bars.length > 2 && room >= 16);
  // A value that does not add up has no pace: the period is not over, and the line says why there is none.
  const noPace = $derived(typeof kpi.elapsed === 'number' && kpi.elapsed > 0 && kpi.elapsed < 1 ? whyNotAdded(tile, 0, parts) : null);
  const goalWords = $derived(
    target === null || !goal
      ? ''
      : `${t('Target {value} · {share} of target', { value: compact(target, fmt), share: pctOfTarget(goal.share) })}${pace ? ` · ${pace}` : ''}${noPace ? ` · ${t('{why}, so no pace', { why: noPace })}` : ''}`,
  );
  const ends = (v: number) => t('At this pace the period ends near {value}', { value: tip(v, fmt) });
  const soFar = $derived(kpi.partial ? ` · ${t('so far')}` : '');

  // Under half a percent is "about the same", not a trend.
  const tone = $derived(changeTone(kpi.delta, tile.better ?? 'up'));
  const pct = $derived(kpi.delta === null ? '' : pctChange(kpi.delta));

  // Column chart geometry, in a 0..100 × 0..40 box (shared with the cards of build 08).
  const bars = $derived(
    barBoxes(kpi.bars.map((b) => b.value)).map((g, i, all) => ({ ...g, label: kpi.bars[i].label, value: kpi.bars[i].value, last: i === all.length - 1, prev: i === all.length - 2 })),
  );
</script>

<div class="kpi" class:mini bind:clientWidth={w} bind:clientHeight={h}>
  <div class="main">
    <span
      class="value"
      style:font-size="{size}px"
      style:color={kpi.value === null ? undefined : TONE_INK[toneFor(kpi.value, fmt) ?? '']}
      title={kpi.value === null ? '' : `${tip(kpi.value, fmt)}${mini ? ` · ${kpi.period}${soFar}` : ''}`}>
      {kpi.value === null ? '–' : compact(kpi.value, fmt)}
    </span>
    {#if kpi.versus}
      <span class="compare" title={mini ? t('vs {what}', { what: kpi.versus }) : undefined}>
        {#if kpi.delta === null}
          {#if !mini}<span class="muted">{t('Nothing to compare in {period}', { period: kpi.versus })}</span>{/if}
        {:else}
          {#if delta === 'inline' || mini}
            <span class="delta {tone}">
              {tone === 'flat' ? '≈' : kpi.delta > 0 ? '▲' : '▼'}
              {tone === 'flat' ? t('Same') : pct}
            </span>
          {/if}
          {#if !mini}<span class="muted">{t('vs {what}', { what: `${kpi.prev === null ? '' : `${compact(kpi.prev, fmt)} · `}${kpi.versus}` })}</span>{/if}
        {/if}
      </span>
    {/if}
  </div>
  {#if goal && target !== null && !mini}
    <div class="goal" title={narrow ? `${goalWords}${goal.projected === null ? '' : `. ${ends(goal.projected)}`}.` : goal.projected === null ? '' : `${ends(goal.projected)}.`}>
      <span class="status {goal.onTrack ? 'good' : 'bad'}">
        <Icon name={goal.onTrack ? 'check' : 'warning'} size={12} />
        {words?.chip}
      </span>
      {#if !narrow}<span class="muted">{goalWords}</span>{/if}
    </div>
  {/if}
  {#if !mini}
    <div class="room" bind:clientHeight={room}>
      {#if showBars}
        <svg class="bars" viewBox="0 0 100 40" preserveAspectRatio="none" role="img" aria-label={t('The last {n} periods', { n: kpi.bars.length })}>
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
  {/if}
  {#if !mini}
    <div class="foot">
      {#if showBars && w >= 200}<span>{t('Last {n} periods', { n: kpi.bars.length })}</span>{/if}
      <span class="period">{kpi.period}{soFar}</span>
    </div>
  {/if}
</div>

<style>
  .kpi { height: 100%; display: flex; flex-direction: column; gap: 8px; min-height: 0; padding-top: 6px; }
  .main { display: flex; align-items: baseline; flex-wrap: wrap; gap: 2px 8px; min-width: 0; }
  .value {
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .compare { display: flex; gap: 6px; align-items: baseline; font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .compare .muted { overflow: hidden; text-overflow: ellipsis; }
  .mini { flex-direction: row; align-items: center; padding-top: 0; }
  .mini .main { flex-wrap: nowrap; gap: 10px; }
  .delta { font-weight: 600; font-variant-numeric: tabular-nums; }
  .delta.good { color: var(--success); }
  .delta.bad { color: var(--danger); }
  .delta.flat { color: var(--text-2); }
  .goal { display: flex; flex-wrap: wrap; gap: 2px 6px; align-items: center; font-size: 12px; overflow: hidden; flex: none; }
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
  /* The history fills the rest of the tile, under the value. */
  .room { flex: 1; min-height: 0; display: flex; align-items: flex-end; }
  .bars { width: 100%; height: 100%; max-height: 72px; display: block; }
  .foot { display: flex; justify-content: space-between; gap: 8px; font-size: 11px; color: var(--text-2); white-space: nowrap; overflow: hidden; flex: none; }
  .foot span { overflow: hidden; text-overflow: ellipsis; }
  .foot .period:only-child { font-size: 12px; font-weight: 500; color: var(--text); }
</style>
