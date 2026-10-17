<script lang="ts">
  // "Start from a starter" on Welcome (0.7 build 04): five dashboards made for a kind of data. A pick opens the sheet
  // that asks for the data and which column is which.
  import { onMount } from 'svelte';
  import { api, type Starter } from '../lib/api';
  import { t } from '../lib/i18n';
  import { starterWord } from '../lib/starters';
  import Icon, { type IconName } from './Icon.svelte';
  import StarterSheet from './StarterSheet.svelte';

  const ICON: Record<string, IconName> = { sales: 'line', budget: 'bar', stock: 'table', attendance: 'calendar', survey: 'kpi' };

  let list = $state<Starter[]>([]);
  let open = $state<Starter | null>(null);

  onMount(async () => {
    try {
      list = await api.starters();
    } catch {
      list = [];
    }
  });
</script>

{#if list.length}
  <section class="starters" aria-label={t('Start from a starter')}>
    <p class="section-label">{t('Start from a starter')}</p>
    <ul>
      {#each list as s (s.id)}
        <li>
          <button onclick={() => (open = s)}>
            <span class="ic"><Icon name={ICON[s.id] ?? 'dashboard'} /></span>
            <b>{starterWord(s.name)}</b>
            <span class="d">{starterWord(s.description)}</span>
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

{#if open}
  <StarterSheet starter={open} onclose={() => (open = null)} />
{/if}

<style>
  .starters { margin-top: 36px; }
  .section-label { margin: 0 0 10px; }
  ul { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 8px; margin: 0; padding: 0; list-style: none; }
  button {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 12px 12px;
    border: 1px solid var(--sep);
    border-radius: var(--r-control);
    background: var(--surface);
    text-align: left;
    transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
  }
  button:hover { border-color: var(--sep-strong); background: var(--hover); }
  .ic { color: var(--accent-text); display: inline-flex; margin-bottom: 2px; }
  b { font-size: 13px; font-weight: 600; }
  .d { font-size: 11.5px; line-height: 1.4; color: var(--text-2); }
  @media (max-width: 900px) {
    ul { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
</style>
