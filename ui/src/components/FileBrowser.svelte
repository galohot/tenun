<script lang="ts">
  // Picks files in browser mode, where there is no native dialog. The list
  // comes from the Tenun server, so paths are real paths on that computer.
  import { onMount } from 'svelte';
  import { api, type FsListing } from '../lib/api';
  import { fileSize } from '../lib/format';
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';

  let {
    mode,
    suggestedName = 'Untitled',
    onpick,
    onclose,
  }: {
    mode: 'files' | 'open' | 'save';
    suggestedName?: string;
    onpick: (paths: string[]) => void;
    onclose: () => void;
  } = $props();

  let listing = $state<FsListing | null>(null);
  let chosen = $state<string[]>([]);
  let name = $state('');
  let typed = $state('');

  onMount(() => {
    name = suggestedName;
    load(localStorage.getItem('tenun-last-dir') ?? undefined);
  });

  async function load(path?: string) {
    try {
      listing = await api.fs(path);
      typed = listing.path;
      localStorage.setItem('tenun-last-dir', listing.path);
      chosen = [];
    } catch (e) {
      if (path) return load();
      app.error(e);
    }
  }

  const visible = $derived(
    (listing?.entries ?? []).filter((e) => e.dir || (mode === 'files' ? e.supported && !e.name.endsWith('.tenun') : e.name.endsWith('.tenun'))),
  );

  function toggle(path: string) {
    if (mode === 'files') chosen = chosen.includes(path) ? chosen.filter((p) => p !== path) : [...chosen, path];
    else chosen = [path];
  }

  function confirm() {
    if (mode === 'save') {
      const sep = listing!.path.includes('\\') ? '\\' : '/';
      const n = name.trim() || 'Untitled';
      onpick([`${listing!.path.replace(/[\\/]$/, '')}${sep}${n.endsWith('.tenun') ? n : n + '.tenun'}`]);
    } else if (chosen.length) {
      onpick(chosen);
    }
  }

  const title = $derived(mode === 'files' ? 'Choose data files' : mode === 'open' ? 'Open a workspace' : 'Save workspace');
</script>

<Modal {title} {onclose} width={640}>
  <div class="bar">
    <button class="btn icon" title="Up one folder" disabled={!listing?.parent} onclick={() => load(listing?.parent ?? undefined)}>
      <Icon name="chevronLeft" />
    </button>
    <input
      class="field path"
      bind:value={typed}
      aria-label="Folder"
      onkeydown={(e) => e.key === 'Enter' && load(typed)} />
  </div>
  <div class="body">
    <aside>
      {#each listing?.places ?? [] as p}
        <button class="place" class:on={listing?.path === p.path} onclick={() => load(p.path)}>
          <Icon name="folder" size={14} />{p.name}
        </button>
      {/each}
    </aside>
    <div class="list" role="listbox" aria-multiselectable={mode === 'files'}>
      {#each visible as e (e.path)}
        <button
          role="option"
          aria-selected={chosen.includes(e.path)}
          class="row"
          class:on={chosen.includes(e.path)}
          ondblclick={() => (e.dir ? load(e.path) : mode !== 'save' && onpick([e.path]))}
          onclick={() => (e.dir ? (mode === 'save' ? load(e.path) : load(e.path)) : mode !== 'save' && toggle(e.path))}>
          <Icon name={e.dir ? 'folder' : e.name.match(/\.(sqlite3?|db|duckdb)$/) ? 'database' : 'sheet'} size={15} />
          <span class="n">{e.name}</span>
          {#if !e.dir}<span class="sz">{fileSize(e.size)}</span>{/if}
        </button>
      {:else}
        <p class="none">
          {mode === 'files'
            ? 'No Excel, CSV, Parquet, SQLite or DuckDB files here.'
            : mode === 'open'
              ? 'No .tenun files here.'
              : 'This folder is empty.'}
        </p>
      {/each}
    </div>
  </div>
  {#snippet footer()}
    {#if mode === 'save'}
      <label class="save-name">
        <span class="label">Name</span>
        <input class="field" bind:value={name} onkeydown={(e) => e.key === 'Enter' && confirm()} />
      </label>
    {:else}
      <span class="hint">{mode === 'files' ? 'Click to select one or more files.' : ''}</span>
    {/if}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={mode !== 'save' && !chosen.length} onclick={confirm}>
      {mode === 'files' ? (chosen.length > 1 ? `Add ${chosen.length} files` : 'Add') : mode === 'open' ? 'Open' : 'Save'}
    </button>
  {/snippet}
</Modal>

<style>
  .bar { display: flex; gap: 8px; margin-bottom: 10px; }
  .path { font-family: var(--mono); font-size: 12px; }
  .body {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 10px;
    height: 340px;
  }
  aside { display: flex; flex-direction: column; gap: 1px; }
  .place {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    text-align: left;
  }
  .place :global(svg) { color: var(--accent-text); }
  .place:hover { background: var(--hover); }
  .place.on { background: var(--selected); }
  .list {
    overflow: auto;
    border-radius: var(--r-card);
    background: var(--inset);
    padding: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    text-align: left;
  }
  .row :global(svg) { color: var(--text-2); }
  .row:hover { background: var(--hover); }
  .row.on { background: var(--accent); color: #fff; }
  .row.on :global(svg), .row.on .sz { color: #fff; }
  .n { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sz { font-size: 11px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .none { margin: 16px; color: var(--text-2); }
  .hint { margin-right: auto; color: var(--text-2); font-size: 12px; }
  .save-name { display: flex; align-items: center; gap: 8px; margin-right: auto; flex: 1; max-width: 320px; }
</style>
