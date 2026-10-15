<script lang="ts">
  import { api } from '../lib/api';
  import { LinkForm } from '../lib/linkform.svelte';
  import { DEFAULT_PATTERN } from '../lib/sources';
  import { app, go } from '../lib/state.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import LinkFields from './LinkFields.svelte';
  import Modal from './Modal.svelte';
  import FileBrowser from './FileBrowser.svelte';
  import NewTableSheet from './data/NewTableSheet.svelte';
  import { OPS, type DerivedOp } from '../lib/derived';

  let { onclose }: { onclose: () => void } = $props();

  let tab = $state<'file' | 'folder' | 'link' | 'database' | 'new'>('file');
  /** New tables from other tables are Advanced, and need a table to start from (build 04). */
  const canMake = $derived(app.settings?.advanced === true && app.tables.length > 0);
  /** The way of making a new table whose sheet is open. */
  let making = $state<DerivedOp | null>(null);
  let browsing = $state(false);
  let kind = $state<'postgres' | 'mysql'>('postgres');
  let host = $state('localhost');
  let port = $state('');
  let database = $state('');
  let user = $state('');
  let password = $state('');
  let schema = $state('');
  let path = $state('');
  // A folder of files.
  let browsingFolder = $state(false);
  let folder = $state('');
  let pattern = $state(DEFAULT_PATTERN);
  // A web link to CSV or JSON data.
  const link = new LinkForm();

  type Kind = 'file' | 'folder' | 'link' | 'database' | 'new';
  const kinds = $derived<{ id: Kind; icon: IconName; label: string; words: string }[]>([
    { id: 'file', icon: 'sheet', label: 'File', words: 'Excel, CSV, Parquet' },
    { id: 'folder', icon: 'folder', label: 'Folder', words: 'Many files, one table' },
    { id: 'link', icon: 'link', label: 'Link', words: 'Sheet, CSV or API' },
    { id: 'database', icon: 'database', label: 'Database', words: 'PostgreSQL, MySQL' },
    ...(canMake ? [{ id: 'new' as Kind, icon: 'newtable' as IconName, label: 'New table', words: 'From your tables' }] : []),
  ]);

  async function addFiles(paths: string[]) {
    let last: string | undefined;
    for (const p of paths) {
      const ok = await app.run(`Reading ${p.split(/[\\/]/).pop()}…`, async () => {
        const r = await api.addFile(p);
        last = r.added;
        return r;
      });
      if (!ok) return;
    }
    onclose();
    const t = app.tables.find((x) => x.source_id === last);
    go({ view: 'data', table: t?.id });
    app.toast(paths.length > 1 ? `Added ${paths.length} files` : 'Added');
  }

  async function chooseFiles() {
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: 'data_files' });
      if (paths.length) await addFiles(paths);
    } else {
      browsing = true;
    }
  }

  async function chooseFolder() {
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: 'folder' });
      if (paths[0]) folder = paths[0];
    } else {
      browsingFolder = true;
    }
  }

  async function addFolder(e: SubmitEvent) {
    e.preventDefault();
    let added = '';
    const ok = await app.run('Reading the folder…', async () => {
      const r = await api.addFolder(folder, pattern.trim());
      added = r.added;
      return r;
    });
    if (ok) {
      onclose();
      const t = app.tables.find((x) => x.source_id === added);
      go({ view: 'data', table: t?.id });
      app.toast('Added the folder');
    }
  }

  async function addLink(e: SubmitEvent) {
    e.preventDefault();
    link.tidy();
    let added = '';
    const ok = await app.run('Fetching the link…', async () => {
      const r = await api.addUrl(link.url.trim(), link.every, link.name.trim(), link.settings());
      added = r.added;
      return r;
    });
    if (ok) {
      onclose();
      const t = app.tables.find((x) => x.source_id === added);
      go({ view: 'data', table: t?.id });
      app.toast('Added the link');
    }
  }

  async function connect(e: SubmitEvent) {
    e.preventDefault();
    let added = '';
    const ok = await app.run('Connecting…', async () => {
      const r = await api.addDatabase(kind, {
        host,
        port: port ? Number(port) : null,
        database,
        user,
        password,
        schema: schema || null,
      });
      added = r.added;
      return r;
    });
    if (ok) {
      onclose();
      const t = app.tables.find((x) => x.source_id === added);
      go({ view: 'data', table: t?.id });
      app.toast(`Connected to ${database}`);
    }
  }
</script>

{#if making}
  <NewTableSheet
    op={making}
    onclose={() => (making = null)}
    ondone={(t) => {
      making = null;
      onclose();
      go({ view: 'data', table: t });
    }} />
{:else}
<Modal title="Add data" subtitle="Tenun reads your data where it is and never writes to it." onclose={onclose} width={canMake ? 680 : 600}>
  <div class="kinds" role="tablist" aria-label="Kind of data" style:--n={kinds.length}>
    {#each kinds as k (k.id)}
      <button role="tab" aria-selected={tab === k.id} class:on={tab === k.id} onclick={() => (tab = k.id)}>
        <Icon name={k.icon} size={16} />
        <strong>{k.label}</strong>
        <span>{k.words}</span>
      </button>
    {/each}
  </div>

  {#if tab === 'file'}
    <button class="drop" onclick={chooseFiles} disabled={!!app.busy}>
      <svg class="up" width="24" height="24" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 13V3.5M6.5 7 10 3.5 13.5 7M4 11.5v4h12v-4" /></svg>
      <strong>{app.busy ?? 'Choose files…'}</strong>
      <span>Excel, CSV, Parquet, SQLite or DuckDB.{app.health?.native ? ' You can also drop files on the window.' : ''}</span>
      <span>Tenun reads the file where it is and updates when you save it.</span>
    </button>
    <form class="paste" id="pathform" onsubmit={(e) => { e.preventDefault(); if (path.trim()) addFiles([path.trim()]); }}>
      <input class="field" placeholder="Or paste a full file path" aria-label="Full path of a file" bind:value={path} />
    </form>
  {:else if tab === 'folder'}
    <form class="db" id="folderform" onsubmit={addFolder}>
      <button type="button" class="drop" onclick={chooseFolder} disabled={!!app.busy}>
        <Icon name="folder" size={28} />
        <strong>{folder ? folder.split(/[\\/]/).filter(Boolean).pop() : 'A folder of files…'}</strong>
        <span>{folder || 'Excel and CSV files in one folder stack into one table.'}</span>
        <span>Tenun adds a "file" column with the name of each file, and picks up files you add, change or remove.</span>
      </button>
      <label class="stack">
        <span class="label">Files to read</span>
        <input class="field" bind:value={pattern} placeholder={DEFAULT_PATTERN} />
        <span class="help">Separate patterns with a semicolon. Use * for any text.</span>
      </label>
    </form>
  {:else if tab === 'link'}
    <form class="db" id="linkform" onsubmit={addLink}>
      <LinkFields f={link} />
      <p class="note">
        <Icon name="info" size={14} />
        Tenun downloads the link and keeps a copy next to your workspace. It sends no cookies, and reads up to 50 MB.
      </p>
    </form>
  {:else if tab === 'new'}
    <div class="ops">
      {#each OPS as o (o.op)}
        <button class="op" onclick={() => (making = o.op)}>
          <Icon name={o.icon} size={20} />
          <strong>{o.title}</strong>
          <span>{o.words}</span>
        </button>
      {/each}
      <p class="note">
        <Icon name="info" size={14} />
        A new table reads the tables you pick and is made again when they change. Tenun never writes to your files.
      </p>
    </div>
  {:else}
    <form class="db" id="dbform" onsubmit={connect}>
      <div class="seg" role="radiogroup" aria-label="Database type">
        <button type="button" role="radio" aria-checked={kind === 'postgres'} class:on={kind === 'postgres'} onclick={() => (kind = 'postgres')}>PostgreSQL</button>
        <button type="button" role="radio" aria-checked={kind === 'mysql'} class:on={kind === 'mysql'} onclick={() => (kind = 'mysql')}>MySQL</button>
      </div>
      <div class="grid">
        <label class="wide"><span class="label">Host</span><input class="field" bind:value={host} required /></label>
        <label><span class="label">Port</span><input class="field" bind:value={port} placeholder={kind === 'postgres' ? '5432' : '3306'} inputmode="numeric" /></label>
        <label class="wide"><span class="label">Database</span><input class="field" bind:value={database} required /></label>
        {#if kind === 'postgres'}
          <label><span class="label">Schema</span><input class="field" bind:value={schema} placeholder="all" /></label>
        {/if}
        <label class="wide"><span class="label">User</span><input class="field" bind:value={user} required autocomplete="username" /></label>
        <label class="wide"><span class="label">Password</span><input class="field" type="password" bind:value={password} autocomplete="current-password" /></label>
      </div>
      <p class="note">
        <Icon name="info" size={14} />
        Read-only. The password is kept on this computer, never in the workspace file. The first connection downloads
        the {kind === 'postgres' ? 'PostgreSQL' : 'MySQL'} connector once.
      </p>
    </form>
  {/if}

  {#snippet footer()}
    {#if tab === 'database'}
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" type="submit" form="dbform" disabled={!!app.busy}>{app.busy ?? 'Connect'}</button>
    {:else if tab === 'folder'}
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" type="submit" form="folderform" disabled={!!app.busy || !folder}>{app.busy ?? 'Add folder'}</button>
    {:else if tab === 'link'}
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" type="submit" form="linkform" disabled={!!app.busy || !link.url.trim() || !!link.problem}>{app.busy ?? 'Add link'}</button>
    {:else if tab === 'file'}
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" type="submit" form="pathform" title={path.trim() ? undefined : 'Paste a file path first, or choose files above'} disabled={!!app.busy || !path.trim()}>{app.busy ?? 'Add'}</button>
    {:else}
      <button class="btn" onclick={onclose}>Close</button>
    {/if}
  {/snippet}
</Modal>

{/if}

{#if browsingFolder}
  <FileBrowser mode="folder" onclose={() => (browsingFolder = false)} onpick={(p) => { browsingFolder = false; folder = p[0]; }} />
{/if}

{#if browsing}
  <FileBrowser mode="files" onclose={() => (browsing = false)} onpick={(p) => { browsing = false; addFiles(p); }} />
{/if}

<style>
  .kinds { display: grid; grid-template-columns: repeat(var(--n), minmax(0, 1fr)); gap: 8px; margin-bottom: 16px; }
  .kinds button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    min-width: 0;
    padding: 12px;
    border: 1px solid var(--sep);
    border-radius: var(--r-card);
    background: var(--surface);
    color: var(--text-2);
    text-align: left;
    transition: background 120ms var(--spring), border-color 120ms var(--spring), transform 120ms var(--spring);
  }
  .kinds button :global(svg) { color: var(--text); margin-bottom: 5px; }
  .kinds button strong { color: var(--text); font-weight: 600; }
  .kinds button span { font-size: 11.5px; line-height: 1.35; }
  .kinds button:hover { background: var(--hover); }
  .kinds button:active { transform: scale(0.98); }
  .kinds button.on { border-color: var(--accent); background: var(--selected); box-shadow: 0 0 0 1px var(--accent); }
  .kinds button.on strong, .kinds button.on :global(svg) { color: var(--accent-text); }
  .seg button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 14px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-weight: 500;
  }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .drop {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 32px 16px;
    border: 1.5px dashed var(--sep-strong);
    border-radius: var(--r-card);
    background: var(--window);
    color: var(--text-2);
    font-size: 12px;
    transition: border-color 140ms var(--spring), background 140ms var(--spring);
  }
  .drop :global(svg), .drop .up { color: var(--accent-text); margin-bottom: 6px; }
  .drop strong { color: var(--accent-text); font-size: 14px; font-weight: 600; }
  .drop:hover { border-color: var(--accent); background: var(--selected); }
  .paste { display: flex; gap: 8px; margin-top: 12px; }
  .paste .field { height: 32px; }
  .ops { display: flex; flex-direction: column; gap: 8px; }
  .op {
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: 2px 10px;
    align-items: center;
    padding: 12px 14px;
    border: 1px solid var(--sep);
    border-radius: var(--r-card);
    background: var(--window);
    color: var(--text-2);
    text-align: left;
  }
  .op :global(svg) { grid-row: span 2; color: var(--accent-text); }
  .op strong { color: var(--text); font-size: 14px; font-weight: 600; }
  .op span { font-size: 12px; }
  .op:hover { border-color: var(--accent); }
  .seg { display: inline-flex; padding: 2px; border-radius: 8px; background: var(--inset); margin-bottom: 14px; }
  .grid { display: grid; grid-template-columns: 1fr 140px; gap: 10px 12px; }
  .grid label { display: flex; flex-direction: column; gap: 4px; }
  .grid .wide:nth-child(5), .grid .wide:nth-child(6) { grid-column: span 2; }
  .stack { display: flex; flex-direction: column; gap: 4px; margin-top: 12px; }
  .help { color: var(--text-2); font-size: 12px; }
  .note { display: flex; gap: 8px; margin: 14px 0 0; color: var(--text-2); font-size: 12px; }
  .note :global(svg) { margin-top: 1px; }
</style>
