<script lang="ts">
  import type { Column, Profile } from '../../lib/api';
  import { barHeights } from '../../lib/prep';
  import { cell, full } from '../../lib/format';
  import { t, tn } from '../../lib/i18n';

  let {
    column,
    profile,
    x,
    y,
  }: { column: Column; profile: Profile | null; x: number; y: number } = $props();

  const shown = (v: unknown) => (v === null || v === undefined ? '' : cell(v as string | number, column.dtype));
  const bars = $derived(profile?.histogram ? barHeights(profile.histogram) : []);
  const blank = $derived(profile ? Math.round(profile.blank * 1000) / 10 : 0);
</script>

<div class="card" style:left="{x}px" style:top="{y}px" role="tooltip">
  <div class="name">{column.label ?? column.name}{#if column.raw_type && !column.calc}<span class="raw">{column.raw_type}</span>{/if}</div>
  {#if column.description}<p class="desc">{column.description}</p>{/if}
  {#if column.calc}
    <p class="calc">
      {#if column.error}{t('Could not be computed. Its values are blank.')}{:else}{t('Calculated column')}{/if}{#if column.as_of}{t(', as of {date}', { date: column.as_of })}{/if}
    </p>
  {/if}
  {#if !profile}
    <p class="muted">{t('Reading the column…')}</p>
  {:else}
    <p class="line">{tn('{blank} % blank · {n} different value', '{blank} % blank · {n} different values', profile.distinct, { blank, n: full(profile.distinct) })}</p>
    {#if profile.mixed}
      <p class="mixed">{t('The file had text and numbers mixed in this column.')}</p>
    {/if}
    {#if profile.min !== undefined && profile.max !== undefined}
      <p class="line">{t('From {min} to {max}', { min: shown(profile.min), max: shown(profile.max) })}</p>
    {/if}
    {#if bars.length > 1}
      <div class="hist" aria-label={t('How the values spread')}>
        {#each bars as h, i (i)}
          <span style:height="{Math.max(h * 100, h > 0 ? 6 : 0)}%"></span>
        {/each}
      </div>
    {/if}
    {#if profile.top?.length}
      <ul class="top">
        {#each profile.top as t (t.value)}
          <li><span class="v">{t.value}</span><span class="n">{full(t.count)}</span></li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .card {
    position: fixed;
    z-index: 40;
    width: 240px;
    padding: 10px 12px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    font-size: 12px;
    pointer-events: none;
    animation: pop 140ms var(--spring);
  }
  .name { font-weight: 600; margin-bottom: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .raw { margin-left: 6px; font: 500 10px/1 var(--mono); color: var(--text-2); }
  .desc { color: var(--text-2); white-space: normal; }
  p { margin: 2px 0; }
  .line { font-variant-numeric: tabular-nums; }
  .line { color: var(--text); }
  .calc { color: var(--text-2); }
  .mixed { color: var(--text); font-weight: 500; }
  .hist { display: flex; align-items: flex-end; gap: 2px; height: 36px; margin-top: 8px; }
  .hist span { flex: 1; background: var(--s1); border-radius: 1px 1px 0 0; min-height: 0; }
  .top { list-style: none; margin: 6px 0 0; padding: 0; }
  .top li { display: flex; justify-content: space-between; gap: 10px; padding: 2px 0; }
  .v { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .n { color: var(--text-2); font-variant-numeric: tabular-nums; }
  @keyframes pop { from { opacity: 0; transform: translateY(-2px) scale(0.98); } }
</style>
