<script lang="ts">
  // The target line of a card: the status chip, the share of the target and, for a period that is not over, the pace.
  // The same words as the number tile has always had (KpiView).
  import type { KpiInfo, Tile } from '../../../lib/api';
  import { compact, pctOfTarget, tip } from '../../../lib/format';
  import { targetInfo, targetWords } from '../../../lib/kpi';
  import { paceElapsed, whyNotAdded, type Parts } from '../../../lib/measures';
  import Icon from '../../Icon.svelte';
  import { t } from '../../../lib/i18n';

  let { kpi, tile, parts, narrow = false }: { kpi: KpiInfo; tile: Tile; parts?: Parts; narrow?: boolean } = $props();

  const fmt = $derived(tile.formats?.[0] ?? null);
  const target = $derived(tile.target?.value ?? null);
  const better = $derived(tile.better ?? 'up');
  const goal = $derived(targetInfo(kpi.value, target, paceElapsed(tile, kpi.elapsed, parts), better));
  const words = $derived(goal ? targetWords(goal, better) : null);
  const noPace = $derived(typeof kpi.elapsed === 'number' && kpi.elapsed > 0 && kpi.elapsed < 1 ? whyNotAdded(tile, 0, parts) : null);
  const line = $derived(
    target === null || !goal
      ? ''
      : [
          t('Target {value}', { value: compact(target, fmt) }),
          t('{share} of target', { share: pctOfTarget(goal.share) }),
          words?.pace,
          noPace ? t('{why}, so no pace', { why: noPace }) : '',
        ]
          .filter(Boolean)
          .join(' · '),
  );
  const ends = $derived(goal?.projected == null ? '' : t('At this pace the period ends near {value}.', { value: tip(goal.projected, fmt) }));
</script>

{#if goal && words}
  <div class="goal" title={narrow ? `${line}. ${ends}` : ends}>
    <span class="status {goal.onTrack ? 'good' : 'bad'}">
      <Icon name={goal.onTrack ? 'check' : 'warning'} size={12} />
      {words.chip}
    </span>
    {#if !narrow}<span class="muted">{line}</span>{/if}
  </div>
{/if}

<style>
  .goal { display: flex; flex-wrap: nowrap; gap: 6px; align-items: center; font-size: 12px; min-height: 22px; overflow: hidden; white-space: nowrap; }
  .muted { color: var(--ink-2); overflow: hidden; text-overflow: ellipsis; }
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
    flex: none;
  }
  .status.good { background: color-mix(in srgb, var(--success) 16%, var(--surface)); }
  .status.good :global(svg) { color: var(--success); }
  .status.bad { background: color-mix(in srgb, var(--danger) 14%, var(--surface)); }
  .status.bad :global(svg) { color: var(--danger); }
</style>
