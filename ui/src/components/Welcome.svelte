<script lang="ts">
  import { api } from '../lib/api';
  import { app, go } from '../lib/state.svelte';
  import Icon from './Icon.svelte';

  let { hasData }: { hasData: boolean } = $props();

  async function sample() {
    if (await app.run('Opening the sample…', () => api.sample())) {
      const d = app.dashboards[0];
      go(d ? { view: 'dashboard', id: d.id } : { view: 'data' });
    }
  }

  const steps = [
    { icon: 'sheet', title: 'Add your data', text: 'Excel files, CSV, SQLite, PostgreSQL or MySQL. Tenun reads them where they are.' },
    { icon: 'model', title: 'Draw relationships', text: 'Link a column in one source to a column in another. Tenun suggests the likely ones.' },
    { icon: 'dashboard', title: 'Build the dashboard', text: 'Pick a value and a group. Click any bar to filter every chart. Save the Excel file and it updates.' },
  ] as const;
</script>

<div class="welcome">
  <div class="hero">
    <img src="/icon.svg" alt="" width="64" height="64" />
    <h1>Weave your spreadsheets and databases into one dashboard</h1>
    <p class="muted">Everything stays on this computer. Nothing to install, nothing to set up.</p>
    <div class="actions">
      <button class="btn primary big" onclick={sample} disabled={!!app.busy}>
        <Icon name="sparkles" /> {app.busy ?? 'Open the sample'}
      </button>
      <button class="btn big" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data…</button>
    </div>
    {#if hasData}
      <p class="muted small">
        This workspace has {app.tables.length} table{app.tables.length === 1 ? '' : 's'}.
        <button class="link" onclick={() => go({ view: 'data' })}>Go to Data</button>
      </p>
    {/if}
  </div>
  <ol class="steps">
    {#each steps as s, i}
      <li>
        <span class="n">{i + 1}</span>
        <Icon name={s.icon} size={20} />
        <h3>{s.title}</h3>
        <p class="muted">{s.text}</p>
      </li>
    {/each}
  </ol>
</div>

<style>
  .welcome {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 40px;
    padding: 48px 32px;
  }
  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 12px;
    max-width: 560px;
  }
  .hero img { border-radius: 14px; margin-bottom: 8px; }
  .hero h1 { font-size: 28px; line-height: 1.15; letter-spacing: -0.02em; }
  .hero p { margin: 0; font-size: 15px; }
  .actions { display: flex; gap: 10px; margin-top: 12px; }
  .big { height: 36px; padding: 0 18px; font-size: 14px; border-radius: 8px; }
  .small { font-size: 13px !important; }
  .link { border: 0; background: none; color: var(--accent-text); padding: 0; font-weight: 500; }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 220px));
    gap: 16px;
  }
  .steps li {
    position: relative;
    padding: 18px;
    border-radius: var(--r-card);
    background: var(--surface);
    box-shadow: var(--shadow-tile);
  }
  .steps li :global(svg) { color: var(--accent-text); margin-bottom: 10px; }
  .steps h3 { font-size: 13px; margin-bottom: 4px; }
  .steps p { margin: 0; font-size: 12px; }
  .n {
    position: absolute;
    top: 16px;
    right: 16px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  @media (max-width: 900px) {
    .steps { grid-template-columns: 1fr; }
  }
</style>
