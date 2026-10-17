<script lang="ts">
  // A starter dashboard (0.7 build 04, AD-D8): first the data (the sample, or the person's own file), then one screen
  // where each role shows the column the engine guessed, three of its values, and a menu to change it.
  import { api, type Cell, type Column, type Starter, type StarterGuess } from '../lib/api';
  import { getLang, t } from '../lib/i18n';
  import { columnsFor, missingRole, starterWord } from '../lib/starters';
  import { app, go } from '../lib/state.svelte';
  import FileBrowser from './FileBrowser.svelte';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  let { starter, onclose }: { starter: Starter; onclose: () => void } = $props();

  let source = $state<string | null>(null);
  let table = $state<string | null>(null);
  let browsing = $state(false);
  let guesses = $state<StarterGuess[]>([]);
  let preview = $state<{ columns: Column[]; rows: Cell[][] } | null>(null);
  let mapping = $state<Record<string, string>>({});

  const tables = $derived(app.tables.filter((x) => x.source_id === source));
  const columns = $derived(app.tables.find((x) => x.id === table)?.columns ?? []);
  const missing = $derived(missingRole(starter.roles, mapping));

  async function useSample() {
    let added = '';
    let first: string | null = null;
    const ok = await app.run(t('Adding the sample…'), async () => {
      const r = await api.starterSample(starter.id);
      added = r.added;
      first = r.table;
      return r;
    });
    if (ok) await pickTable(added, first);
  }

  async function chooseFile() {
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: 'data_files' });
      if (paths[0]) await addFile(paths[0]);
    } else {
      browsing = true;
    }
  }

  async function addFile(path: string) {
    let added = '';
    const ok = await app.run(t('Reading {name}…', { name: path.split(/[\\/]/).pop() ?? path }), async () => {
      const r = await api.addFile(path);
      added = r.added;
      return r;
    });
    if (ok) await pickTable(added, null);
  }

  async function pickTable(src: string, id: string | null) {
    source = src;
    await load(id ?? app.tables.find((x) => x.source_id === src)?.id ?? null);
  }

  async function load(id: string | null) {
    table = id;
    if (!id) return;
    try {
      const [g, p] = await Promise.all([api.starterGuess(starter.id, id), api.preview(id, 3)]);
      guesses = g;
      preview = p;
      mapping = Object.fromEntries(g.filter((x) => x.column).map((x) => [x.role, x.column as string]));
    } catch (e) {
      app.error(e);
    }
  }

  function values(column: string | undefined): string[] {
    if (!column || !preview) return [];
    const i = preview.columns.findIndex((c) => c.name === column);
    if (i < 0) return [];
    const loc = getLang() === 'id' ? 'id-ID' : 'en-US';
    return preview.rows.map((r) => {
      const v = r[i];
      if (v === null || v === undefined || v === '') return '–';
      return typeof v === 'number' ? v.toLocaleString(loc) : String(v);
    });
  }

  const sure = (role: string) => (guesses.find((g) => g.role === role)?.score ?? 0) >= 0.6;

  async function make() {
    if (!table || missing) return;
    let id = '';
    const ok = await app.run(t('Making the dashboard…'), async () => {
      const r = await api.applyStarter(starter.id, table as string, $state.snapshot(mapping), starterWord(starter.name));
      id = r.dashboard;
      return r;
    });
    if (ok) {
      onclose();
      go({ view: 'dashboard', id });
    }
  }
</script>

<Modal
  title={starterWord(starter.name)}
  subtitle={table ? t('Check which column is which. The dashboard uses these columns.') : starterWord(starter.description)}
  width={table ? 640 : 520}
  {onclose}
>
  {#if !table}
    <p class="section-label">{t('Your data')}</p>
    <div class="choices">
      <button class="choice" onclick={useSample} disabled={!!app.busy}>
        <Icon name="sparkles" />
        <b>{t('Use the sample')}</b>
        <span>{t('Made-up data, to see the dashboard first.')}</span>
      </button>
      <button class="choice" onclick={chooseFile} disabled={!!app.busy}>
        <Icon name="sheet" />
        <b>{t('Use my file')}</b>
        <span>{t('Excel, CSV or Parquet. Tenun reads it where it is.')}</span>
      </button>
    </div>
    {#if app.busy}<p class="busy muted">{app.busy}</p>{/if}
  {:else}
    {#if tables.length > 1}
      <label class="tablepick">
        <span class="section-label">{t('Table')}</span>
        <select class="field" value={table} onchange={(e) => load(e.currentTarget.value)}>
          {#each tables as x (x.id)}<option value={x.id}>{x.label}</option>{/each}
        </select>
      </label>
    {/if}
    <ul class="roles">
      {#each starter.roles as r (r.name)}
        {@const options = columnsFor(r, columns)}
        <li>
          <div class="who">
            <div class="name">
              <b>{starterWord(r.label)}</b>
              <span class="tag" class:req={r.required}>{r.required ? t('Needed') : t('Optional')}</span>
              {#if mapping[r.name] && sure(r.name)}<span class="tag ok"><Icon name="check" /> {t('Guessed')}</span>{/if}
            </div>
            <p>{starterWord(r.description)}</p>
          </div>
          <div class="pick">
            <select
              class="field"
              class:empty={!mapping[r.name]}
              aria-label={starterWord(r.label)}
              value={mapping[r.name] ?? ''}
              onchange={(e) => (mapping = { ...mapping, [r.name]: e.currentTarget.value })}
            >
              <option value="">{r.required ? t('Choose a column') : t('None')}</option>
              {#each options as c (c.name)}<option value={c.name}>{c.label ?? c.name}</option>{/each}
            </select>
            {#if !options.length}
              <span class="note">{r.kind === 'date' ? t('No column of dates in this table') : t('No column of numbers in this table')}</span>
            {:else}
              <span class="vals">
                {#each values(mapping[r.name]) as v, i (i)}<span>{v}</span>{/each}
              </span>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    {#if table}
      {#if missing}<span class="why muted">{t('Choose a column for {role}.', { role: starterWord(missing.label) })}</span>{/if}
      <button class="btn" onclick={onclose}>{t('Cancel')}</button>
      <button class="btn primary" onclick={make} disabled={!!missing || !!app.busy}>{app.busy ?? t('Make the dashboard')}</button>
    {:else}
      <button class="btn" onclick={onclose}>{t('Cancel')}</button>
    {/if}
  {/snippet}
</Modal>

{#if browsing}
  <FileBrowser mode="files" onclose={() => (browsing = false)} onpick={(p) => { browsing = false; if (p[0]) addFile(p[0]); }} />
{/if}

<style>
  .section-label { margin: 6px 0 8px; display: block; }
  .choices { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .choice {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 12px 14px 14px;
    border: 1px solid var(--sep);
    border-radius: var(--r-control);
    background: var(--surface);
    text-align: left;
    color: var(--accent-text);
  }
  .choice:hover:not(:disabled) { border-color: var(--sep-strong); background: var(--hover); }
  .choice:disabled { opacity: 0.5; cursor: default; }
  .choice b { margin-top: 4px; color: var(--text); font-size: 13px; font-weight: 600; }
  .choice span { color: var(--text-2); font-size: 12px; line-height: 1.4; }
  .busy { margin: 10px 0 0; font-size: 12px; }

  .tablepick { display: flex; align-items: center; gap: 10px; margin: 4px 0 6px; }
  .tablepick .section-label { margin: 0; }
  .tablepick select { width: auto; min-width: 200px; }
  .roles { list-style: none; margin: 0; padding: 0; }
  .roles li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.1fr);
    gap: 16px;
    align-items: start;
    padding: 12px 0;
    border-top: 1px solid var(--sep);
  }
  .roles li:first-child { border-top: 0; }
  .name { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; }
  .name b { font-size: 13px; font-weight: 600; }
  .who p { margin: 3px 0 0; font-size: 12px; line-height: 1.4; color: var(--text-2); }
  /* A hairline tag, in the mono voice of the section labels. */
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 18px;
    padding: 0 6px;
    border: 1px solid var(--sep-strong);
    border-radius: 4px;
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .tag.req { color: var(--text); }
  .tag.ok { color: var(--accent-text); border-color: color-mix(in srgb, var(--accent) 40%, transparent); }
  .tag.ok :global(svg) { width: 11px; height: 11px; }
  .pick { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .pick select.empty { color: var(--text-2); }
  .vals { display: flex; flex-wrap: wrap; gap: 4px; min-height: 20px; }
  .vals span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 1px 6px;
    border: 1px solid var(--sep);
    border-radius: 4px;
    font-size: 11.5px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .note { font-size: 12px; color: var(--danger); }
  .why { margin-right: auto; font-size: 12px; }
  @media (max-width: 600px) {
    .choices { grid-template-columns: 1fr; }
    .roles li { grid-template-columns: 1fr; gap: 8px; }
  }
</style>
