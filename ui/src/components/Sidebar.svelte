<script lang="ts">
  import { app, go, type ThemePref } from '../lib/state.svelte';
  import { api } from '../lib/api';
  import { newDashboard, newId } from '../lib/tiles';
  import { newStory } from '../lib/flow';
  import Menu from './dash/Menu.svelte';
  import Icon from './Icon.svelte';
  import FileBrowser from './FileBrowser.svelte';

  let renaming = $state(false);
  let nameDraft = $state('');
  let browser = $state<null | 'open' | 'save'>(null);

  const ws = $derived(app.snap!.workspace);
  const route = $derived(app.route);

  function startRename() {
    nameDraft = ws.name;
    renaming = true;
  }

  async function commitRename() {
    renaming = false;
    const name = nameDraft.trim();
    if (name && name !== ws.name) await app.run(null, () => api.rename(name));
  }

  /** A new page: a grid of tiles, or a story that reads like a document (build 11). */
  let newMenu = $state<{ x: number; y: number } | null>(null);
  function addDashboard(kind: 'grid' | 'story' = 'grid') {
    const n = app.dashboards.length;
    const d = kind === 'story' ? newStory(newId('d'), app.dashboards.filter((x) => x.layout === 'flow').length, newId) : newDashboard(n);
    app.snap!.workspace.dashboards.push(d);
    app.saveDashboards();
    go({ view: 'dashboard', id: d.id });
  }

  async function save() {
    if (app.snap!.path) {
      if (await app.run(null, () => api.save())) app.toast('Saved');
      return;
    }
    await saveAs();
  }

  async function saveAs() {
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: 'save_workspace', name: ws.name });
      if (paths[0] && (await app.run('Saving…', () => api.save(paths[0])))) app.toast('Saved');
    } else {
      browser = 'save';
    }
  }

  async function open() {
    if (app.health?.native) {
      const { paths } = await api.pick({ kind: 'open_workspace' });
      if (paths[0]) await openPath(paths[0]);
    } else {
      browser = 'open';
    }
  }

  async function openPath(p: string) {
    if (await app.run('Opening…', () => api.open(p))) {
      const first = app.dashboards[0];
      go(first ? { view: 'dashboard', id: first.id } : { view: 'data' });
    }
  }

  async function newWs() {
    if (app.snap!.dirty && !confirm('This workspace is not saved. Start a new one anyway?')) return;
    if (await app.run(null, () => api.newWorkspace())) go({ view: 'home' });
  }

  const themes: { id: ThemePref; icon: 'sun' | 'moon' | 'auto'; label: string }[] = [
    { id: 'light', icon: 'sun', label: 'Light' },
    { id: 'dark', icon: 'moon', label: 'Dark' },
    { id: 'system', icon: 'auto', label: 'Match system' },
  ];
</script>

<nav class="side">
  <div class="ws">
    <img src="/icon.svg" alt="" width="22" height="22" />
    {#if renaming}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="field rename"
        bind:value={nameDraft}
        autofocus
        onblur={commitRename}
        onkeydown={(e) => {
          if (e.key === 'Enter') commitRename();
          if (e.key === 'Escape') renaming = false;
        }} />
    {:else}
      <button class="name" title="Rename workspace" ondblclick={startRename} onclick={() => go({ view: 'home' })}>
        {ws.name}
      </button>
    {/if}
  </div>
  <div class="file">
    <span class="state" class:unsaved={app.snap!.dirty}>
      {#if app.snap!.path}{app.snap!.dirty ? 'Unsaved changes' : 'Saved automatically'}{:else}Not saved yet{/if}
    </span>
    <span class="file-actions">
      <button class="btn plain icon" title="New workspace" onclick={newWs}><Icon name="plus" /></button>
      <button class="btn plain icon" title="Open workspace…" onclick={open}><Icon name="open" /></button>
      <button class="btn plain icon" title={app.snap!.path ? 'Save' : 'Save as…'} onclick={save}><Icon name="save" /></button>
    </span>
  </div>

  <div class="group">
    <button class="item" class:on={route.view === 'data'} onclick={() => go({ view: 'data' })}>
      <Icon name="table" /> Data
      <span class="count">{app.tables.length || ''}</span>
    </button>
    <button class="item" class:on={route.view === 'model'} onclick={() => go({ view: 'model' })}>
      <Icon name="model" /> Model
      <span class="count">{ws.relationships.length || ''}</span>
    </button>
  </div>

  <div class="heading">
    <span>Dashboards</span>
    <button
      class="btn plain icon small"
      title="New dashboard or story"
      aria-label="New dashboard or story"
      aria-haspopup="menu"
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        newMenu = newMenu ? null : { x: r.left, y: r.bottom + 4 };
      }}><Icon name="plus" size={14} /></button>
    {#if newMenu}
      <Menu
        x={newMenu.x}
        y={newMenu.y}
        items={[
          { label: 'Dashboard', run: () => addDashboard('grid') },
          { label: 'Story', run: () => addDashboard('story') },
        ]}
        onclose={() => (newMenu = null)} />
    {/if}
  </div>
  <div class="group dashboards">
    {#each app.dashboards as d (d.id)}
      <button
        class="item"
        class:on={route.view === 'dashboard' && route.id === d.id}
        onclick={() => go({ view: 'dashboard', id: d.id })}>
        <Icon name={d.layout === 'flow' ? 'text' : 'dashboard'} />
        <span class="ellipsis">{d.title}</span>
      </button>
    {:else}
      <p class="empty">No dashboards yet.</p>
    {/each}
  </div>

  <div class="foot">
    <div class="theme-row">
      <div class="seg" role="radiogroup" aria-label="Appearance">
        {#each themes as t}
          <button
            role="radio"
            aria-checked={app.themePref === t.id}
            class:on={app.themePref === t.id}
            title={t.label}
            onclick={() => app.setTheme(t.id)}><Icon name={t.icon} size={14} /></button>
        {/each}
      </div>
      <button class="btn plain icon gear" title="Settings" aria-label="Settings" onclick={() => (app.settingsOpen = true)}>
        <Icon name="settings" size={16} />
      </button>
    </div>
    {#if app.health?.native}
      <button class="btn plain small-text" title="Open this workspace in your web browser" onclick={() => api.openBrowser()}>
        <Icon name="globe" size={14} /> Open in browser
      </button>
    {/if}
  </div>
</nav>

{#if browser}
  <FileBrowser
    mode={browser}
    suggestedName={ws.name}
    onclose={() => (browser = null)}
    onpick={async (paths) => {
      const mode = browser;
      browser = null;
      if (mode === 'open') await openPath(paths[0]);
      else if (await app.run('Saving…', () => api.save(paths[0]))) app.toast('Saved');
    }} />
{/if}

<style>
  .side {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 10px;
    background: var(--window);
    border-right: 1px solid var(--sep);
    min-height: 0;
    overflow: auto;
  }
  .ws {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 6px;
  }
  .ws img { border-radius: 5px; flex: none; }
  .name {
    border: 0;
    background: none;
    padding: 2px 4px;
    margin-left: -4px;
    border-radius: var(--r-control);
    font-size: 15px;
    font-weight: 600;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .name:hover { background: var(--hover); }
  .rename { height: 26px; font-weight: 600; }
  .file {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0 6px 36px;
    border-bottom: 1px solid var(--sep);
    margin-bottom: 6px;
  }
  .state { font-size: 11px; color: var(--text-2); }
  .state.unsaved { color: var(--text); }
  .file-actions { display: flex; }
  .file-actions .btn { width: 24px; height: 24px; color: var(--text-2); }
  .file-actions .btn:hover { color: var(--text); }
  .group { display: flex; flex-direction: column; gap: 1px; }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    text-align: left;
    font-weight: 500;
    min-width: 0;
  }
  .item :global(svg) { color: var(--accent-text); }
  .item:hover { background: var(--hover); }
  .item.on { background: var(--selected); }
  .count { margin-left: auto; font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .ellipsis { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 14px;
    padding: 0 4px 2px 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .small { width: 22px; height: 22px; color: var(--text-2); }
  .empty { margin: 4px 8px; color: var(--text-2); font-size: 12px; }
  .dashboards { flex: 1; }
  .foot {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 4px 0;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 8px;
    background: var(--inset);
  }
  .seg button {
    width: 30px;
    height: 22px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
  }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .theme-row { display: flex; align-items: center; gap: 6px; }
  .gear { width: 26px; height: 26px; color: var(--text-2); }
  .gear:hover { color: var(--text); }
  .small-text { font-size: 12px; height: 24px; padding: 0 6px; color: var(--text-2); }
</style>
