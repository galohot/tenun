<script lang="ts">
  import { api, type Cell, type Column, type UnpivotPlan } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { cell } from '../../lib/format';
  import { t } from '../../lib/i18n';
  import Modal from '../Modal.svelte';

  let {
    tableId,
    plan,
    onapply,
    onclose,
  }: { tableId: string; plan: UnpivotPlan; onapply: (name: string, value: string) => void; onclose: () => void } = $props();

  // The names start from the suggestion and are then the user's.
  // svelte-ignore state_referenced_locally
  let name = $state(plan.name);
  // svelte-ignore state_referenced_locally
  let value = $state(plan.value);
  let before = $state<{ columns: Column[]; rows: Cell[][] } | null>(null);
  let after = $state<{ columns: Column[]; rows: Cell[][] } | null>(null);
  let problem = $state('');

  const clash = $derived(!name.trim() || !value.trim() || name.trim() === value.trim() || plan.keep.includes(name.trim()) || plan.keep.includes(value.trim()));

  // Ask the engine for the rows again a moment after the names stop changing.
  $effect(() => {
    const n = name;
    const v = value;
    if (clash) return;
    const t = setTimeout(() => {
      api
        .unpivotPreview(tableId, plan.keep, n.trim(), v.trim())
        .then((p) => {
          before = p.before;
          after = p.after;
          problem = '';
        })
        .catch((e) => (problem = e.message));
    }, 200);
    return () => clearTimeout(t);
  });

  const show = (v: Cell, c: Column) => (v === null ? '' : c.dtype === 'number' ? cell(v, 'number') : String(v));
  const listed = $derived(plan.columns.length > 6 ? t('{list} and {n} more', { list: plan.columns.slice(0, 5).join(', '), n: plan.columns.length - 5 }) : plan.columns.join(', '));
  /** What one of the columns holds, by the kind the engine found. */
  const KIND: Record<string, () => string> = { Year: () => t('year'), Quarter: () => t('quarter'), Month: () => t('month'), Period: () => t('period') };
  const kind = $derived(KIND[plan.name]?.() ?? plan.name.toLowerCase());
</script>

{#snippet grid(p: { columns: Column[]; rows: Cell[][] } | null)}
  {#if p}
    <div class="scroll">
      <table>
        <thead><tr>{#each p.columns as c (c.name)}<th class:num={c.dtype === 'number'}>{c.name}</th>{/each}</tr></thead>
        <tbody>
          {#each p.rows as r, i (i)}
            <tr>{#each r as v, j (j)}<td class:num={p.columns[j].dtype === 'number'}>{show(v, p.columns[j])}</td>{/each}</tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    <p class="muted">{t('Loading…')}</p>
  {/if}
{/snippet}

<Modal title={t('Turn columns into rows')} width={640} {onclose}>
  <p class="lead">
    {t('These {n} columns hold one {kind} each: {list}.', { n: plan.columns.length, kind, list: listed })}
    {t('Each one becomes a row, so you can chart by {kind}. Your file is not changed.', { kind })}
  </p>
  <div class="names">
    <label>
      <span class="label">{t('Name of the new column')}</span>
      <input class="field" bind:value={name} />
    </label>
    <label>
      <span class="label">{t('Name of the values')}</span>
      <input class="field" bind:value={value} />
    </label>
  </div>
  {#if clash}
    <p class="warn">{t('Give the two new columns different names that no other column uses.')}</p>
  {:else if plan.dates}
    <p class="muted small">{t('{name} will be read as a date.', { name: name.trim() })}</p>
  {/if}
  {#if problem}<p class="warn">{problem}</p>{/if}
  <h3>{t('Before')}</h3>
  {@render grid(before)}
  <h3>{t('After')}</h3>
  {@render grid(after)}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('Cancel')}</button>
    <button class="btn primary" disabled={clash || !!app.busy} onclick={() => onapply(name.trim(), value.trim())}>{t('Turn into rows')}</button>
  {/snippet}
</Modal>

<style>
  .lead { margin: 0 0 12px; }
  .names { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .names label { display: flex; flex-direction: column; gap: 4px; }
  h3 { margin: 14px 0 6px; font-size: 12px; color: var(--text-2); font-weight: 600; }
  .scroll { overflow: auto; border-radius: var(--r-control); background: var(--inset); }
  table { border-collapse: collapse; font-size: 12px; min-width: 100%; }
  th { text-align: left; font-weight: 600; padding: 6px 10px; white-space: nowrap; border-bottom: 1px solid var(--sep); }
  td { padding: 5px 10px; white-space: nowrap; border-bottom: 1px solid var(--sep); }
  tr:last-child td { border-bottom: 0; }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  .warn { color: var(--danger); font-size: 12px; margin: 8px 0 0; }
  .small { font-size: 12px; margin: 6px 0 0; }
</style>
