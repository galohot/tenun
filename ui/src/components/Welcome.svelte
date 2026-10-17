<script lang="ts">
  import { api } from '../lib/api';
  import { app, go } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
  import { t, tn } from '../lib/i18n';
  import { modKey } from './Palette.svelte';

  let { hasData }: { hasData: boolean } = $props();

  async function sample() {
    if (await app.run(t('Opening the sample…'), () => api.sample())) {
      const d = app.dashboards[0];
      go(d ? { view: 'dashboard', id: d.id } : { view: 'data' });
    }
  }

  const KIND: Record<string, string> = {
    excel: 'Excel', csv: 'CSV', parquet: 'Parquet', sqlite: 'SQLite', duckdb: 'DuckDB', folder: t('Folder'),
    url: t('Web link'), postgres: 'PostgreSQL', mysql: 'MySQL', derived: t('New table'),
  };
  // The sources of this workspace, as the threads of the cloth: each keeps a series colour in the fixed order.
  const threads = $derived((app.snap?.workspace.sources ?? []).slice(0, 8));

  const steps = [
    { title: t('Add your data'), text: t('Excel, CSV, Parquet, SQLite, DuckDB, PostgreSQL, MySQL, a folder of files or a web link. Tenun reads them where they are.') },
    { title: t('Draw relationships'), text: t('Link a column in one source to a column in another. Tenun suggests the likely links and checks them against the data.') },
    { title: t('Build the dashboard'), text: t('Pick a value and a group. Click any bar to filter every chart. Save the Excel file and the dashboard updates.') },
  ] as const;
</script>

<div class="welcome">
  <div class="selvedge" aria-hidden="true"></div>
  <div class="inner">
    <p class="mono">{t('Welcome to Tenun')}</p>
    <h1>{t('Weave your spreadsheets and databases into one dashboard.')}</h1>
    <p class="lede">{t('Everything stays on this computer. There is no server to run and no account to make.')}</p>
    <div class="actions">
      <button class="btn primary big" onclick={sample} disabled={!!app.busy}>
        <Icon name="sparkles" /> {app.busy ?? t('Open the sample')}
      </button>
      <button class="btn big" onclick={() => (app.addOpen = true)}><Icon name="plus" /> {t('Add data…')}</button>
      <span class="hint"><kbd>{modKey}K</kbd> {t('Search everything')}</span>
    </div>

    {#if hasData && threads.length}
      <section class="threads" aria-label={t('Sources in this workspace')}>
        <div class="th-head">
          <p class="mono">{t('In this workspace')}</p>
          <button class="link" onclick={() => go({ view: 'data' })}>
            {tn('{n} table · Go to Data', '{n} tables · Go to Data', app.tables.length)}
          </button>
        </div>
        <ul>
          {#each threads as s, i (s.id)}
            <li style="--c: var(--s{i + 1})"><b>{s.name}</b><span>{KIND[s.kind] ?? s.kind}</span></li>
          {/each}
        </ul>
      </section>
    {/if}

    <ol class="steps">
      {#each steps as s, i}
        <li>
          <span class="mono n">0{i + 1}</span>
          <h3>{s.title}</h3>
          <p>{s.text}</p>
        </li>
      {/each}
    </ol>
  </div>
</div>

<style>
  .welcome { flex: 1; overflow: auto; display: flex; flex-direction: column; }
  /* The selvedge: the woven edge of a cloth, in the eight series colours of the fixed order. */
  .selvedge {
    height: 3px;
    flex: none;
    background: repeating-linear-gradient(90deg,
      var(--s1) 0 10px, var(--s2) 10px 20px, var(--s3) 20px 30px, var(--s4) 30px 40px,
      var(--s5) 40px 50px, var(--s6) 50px 60px, var(--s7) 60px 70px, var(--s8) 70px 80px);
  }
  .inner { width: min(880px, 100%); margin: auto; padding: 56px 48px 64px; animation: rise 280ms var(--spring); }
  @keyframes rise { from { opacity: 0; transform: translateY(4px); } }
  .mono {
    margin: 0 0 10px;
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  h1 { max-width: 15em; font-size: 36px; font-weight: 700; line-height: 1.06; letter-spacing: -0.03em; text-wrap: balance; }
  .lede { margin: 14px 0 0; max-width: 34em; font-size: 15px; line-height: 1.5; color: var(--text-2); }
  .actions { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; margin-top: 24px; }
  .big { height: 36px; padding: 0 16px; font-size: 13px; }
  .hint { display: inline-flex; align-items: center; gap: 8px; margin-left: 8px; font-size: 12px; color: var(--text-2); }
  .link { border: 0; background: none; color: var(--accent-text); padding: 0; font-size: 12px; font-weight: 500; }

  .threads { margin-top: 40px; }
  .th-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .threads ul { display: flex; flex-wrap: wrap; gap: 8px; margin: 0; padding: 0; list-style: none; }
  .threads li {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 12px 0 10px;
    border: 1px solid var(--sep);
    border-left: 3px solid var(--c);
    border-radius: var(--r-control);
    background: var(--surface);
    font-size: 12px;
    white-space: nowrap;
  }
  .threads b { font-weight: 600; }
  .threads span { color: var(--text-2); }

  .steps { list-style: none; margin: 48px 0 0; padding: 0; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 32px; }
  .steps li { min-width: 0; padding-top: 14px; border-top: 1px solid var(--sep-strong); }
  .steps .n { display: block; margin-bottom: 8px; color: var(--accent-text); }
  .steps h3 { margin-bottom: 6px; font-size: 14px; }
  .steps p { margin: 0; font-size: 12.5px; line-height: 1.5; color: var(--text-2); }
  @media (max-width: 900px) {
    .inner { padding: 32px 20px 40px; }
    h1 { font-size: 28px; }
    .hint { display: none; }
    .steps { grid-template-columns: 1fr; gap: 20px; }
  }
</style>
