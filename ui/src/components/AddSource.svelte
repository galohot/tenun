<script lang="ts">
  import { api } from '../lib/api';
  import { LinkForm } from '../lib/linkform.svelte';
  import { DEFAULT_PATTERN } from '../lib/sources';
  import { app, go } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
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
<Modal title="Add data" onclose={onclose} width={560}>
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === 'file'} class:on={tab === 'file'} onclick={() => (tab = 'file')}>
      <Icon name="sheet" size={14} /> File
    </button>
    <button role="tab" aria-selected={tab === 'folder'} class:on={tab === 'folder'} onclick={() => (tab = 'folder')}>
      <Icon name="folder" size={14} /> Folder
    </button>
    <button role="tab" aria-selected={tab === 'link'} class:on={tab === 'link'} onclick={() => (tab = 'link')}>
      <Icon name="link" size={14} /> Link
    </button>
    <button role="tab" aria-selected={tab === 'database'} class:on={tab === 'database'} onclick={() => (tab = 'database')}>
      <Icon name="database" size={14} /> Database
    </button>
    {#if canMake}
      <button role="tab" aria-selected={tab === 'new'} class:on={tab === 'new'} onclick={() => (tab = 'new')}>
        <Icon name="newtable" size={14} /> New table
      </button>
    {/if}
  </div>

  {#if tab === 'file'}
    <button class="drop" onclick={chooseFiles} disabled={!!app.busy}>
      <Icon name="sheet" size={28} />
      <strong>{app.busy ?? 'Choose files…'}</strong>
      <span>Excel, CSV, Parquet, SQLite or DuckDB.{app.health?.native ? ' You can also drop files on the window.' : ''}</span>
      <span>Tenun reads the file where it is and updates when you save it.</span>
    </button>
    <form class="paste" onsubmit={(e) => { e.preventDefault(); if (path.trim()) addFiles([path.trim()]); }}>
      <input class="field" placeholder="Or paste a full file path" bind:value={path} />
      <button class="btn" disabled={!path.trim()}>Add</button>
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
  .tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 14px;
    padding: 2px;
    border-radius: 8px;
    background: var(--inset);
    width: max-content;
  }
  .tabs button, .seg button {
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
  .tabs button.on, .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .drop {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 28px 16px;
    border: 1.5px dashed var(--sep-strong);
    border-radius: var(--r-card);
    background: var(--window);
    color: var(--text-2);
  }
  .drop :global(svg) { color: var(--accent-text); margin-bottom: 4px; }
  .drop strong { color: var(--text); font-size: 15px; font-weight: 600; }
  .drop:hover { border-color: var(--accent); }
  .paste { display: flex; gap: 8px; margin-top: 12px; }
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
