<script lang="ts">
  // The drill bar (v5 build 01, GRANDPLAN §6): the path as a breadcrumb ("All years › 2026 › by quarter"), Up, and
  // Next level (every value one level down). A part of the path goes back to that level.
  import type { Cell, DrillInfo, Grain } from '../../lib/api';
  import { crumbs } from '../../lib/drilldown';
  import { formatDate, OTHER_KEY } from '../../lib/format';
  import { t } from '../../lib/i18n';
  import Icon from '../Icon.svelte';

  let {
    info,
    onto,
    onup,
    onnext,
    note = '',
  }: {
    info: DrillInfo;
    /** Back to a level: 0 is the top. */
    onto: (depth: number) => void;
    onup: () => void;
    /** Not set: the bar offers no "Next level" (an exported file that does not hold it). */
    onnext?: () => void;
    /** What the reader should know, such as what an exported file does not hold. */
    note?: string;
  } = $props();

  const fmt = (v: Cell, grain: Grain | null): string => {
    if (v === null || v === undefined) return t('(blank)');
    if (v === OTHER_KEY) return t('Other');
    if (grain && typeof v === 'string' && /^\d{4}-\d{2}-\d{2}/.test(v)) return formatDate(v, grain, 'date');
    return String(v);
  };
  const bar = $derived(crumbs(info, fmt));
</script>

<nav class="drill" aria-label={t('Drill-down')}>
  <ol>
    {#each bar.parts as p, i (i)}
      <li>
        {#if i < bar.parts.length - 1}
          <button class="link" onclick={() => onto(p.depth)}>{p.label}</button>
          <span class="sep" aria-hidden="true">›</span>
        {:else}
          <span class="here" aria-current="location">{p.label}</span>
        {/if}
      </li>
    {/each}
    <li class="showing">{bar.showing}</li>
  </ol>
  <span class="actions">
    <button class="btn plain small" disabled={info.at === 0} onclick={onup} title={t('Up one level (Esc)')}>
      <Icon name="chevronDown" size={12} class="flip" /> {t('Up')}
    </button>
    {#if onnext}
      <button class="btn plain small" disabled={!info.canDown} onclick={onnext} title={t('Every value, one level down')}>
        {t('Next level')} <Icon name="chevronDown" size={12} />
      </button>
    {/if}
  </span>
  {#if note}<p class="note">{note}</p>{/if}
</nav>

<style>
  .drill { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 10px; padding: 0 12px 6px; font-size: 12px; color: var(--text-2); }
  ol { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin: 0; padding: 0; list-style: none; min-width: 0; flex: 1; }
  li { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .link { border: 0; background: none; padding: 0; font: inherit; color: var(--accent-text); cursor: pointer; }
  .link:hover { text-decoration: underline; }
  .here { color: var(--text); font-weight: 600; }
  .sep { color: var(--text-3, var(--text-2)); }
  .showing { color: var(--text-2); }
  .actions { display: flex; gap: 2px; }
  .btn.small { height: 24px; padding: 0 6px; font-size: 12px; gap: 3px; }
  .actions :global(.flip) { transform: rotate(180deg); }
  .note { flex-basis: 100%; margin: 0; font-size: 11px; color: var(--text-2); }
</style>
