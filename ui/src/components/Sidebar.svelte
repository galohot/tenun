<script lang="ts">
  import { app, go, type ThemePref } from '../lib/state.svelte';
  import { api } from '../lib/api';
  import { newDashboard, newId } from '../lib/tiles';
  import { newStory } from '../lib/flow';
  import Menu from './dash/Menu.svelte';
  import Icon from './Icon.svelte';
  import FileBrowser from './FileBrowser.svelte';
  import { palette, modKey } from './Palette.svelte';

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
  /** The workspace switcher (look 3): the start page, rename, save, open and new, under the card. */
  let wsMenu = $state<{ x: number; y: number } | null>(null);
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
  {#if renaming}
    <div class="ws-card editing">
      <img src="/icon.svg" alt="" width="28" height="28" />
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="field rename"
        aria-label="Workspace name"
        bind:value={nameDraft}
        autofocus
        onblur={commitRename}
        onkeydown={(e) => {
          if (e.key === 'Enter') commitRename();
          if (e.key === 'Escape') renaming = false;
        }} />
    </div>
  {:else}
    <button
      class="ws-card"
      title="Workspace: double-click to rename"
      aria-haspopup="menu"
      aria-expanded={!!wsMenu}
      ondblclick={() => {
        wsMenu = null;
        startRename();
      }}
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        wsMenu = wsMenu ? null : { x: r.left, y: r.bottom + 4 };
      }}>
      <img src="/icon.svg" alt="" width="28" height="28" />
      <span class="ws-text">
        <span class="ws-name">{ws.name}</span>
        <span class="state" class:unsaved={app.snap!.dirty}>
          {#if app.snap!.path}{app.snap!.dirty ? 'Unsaved changes' : 'Saved automatically'}{:else}Not saved yet{/if}
        </span>
      </span>
      <Icon name="chevronUpDown" size={14} />
    </button>
  {/if}
  {#if wsMenu}
    <Menu
      x={wsMenu.x}
      y={wsMenu.y}
      items={[
        { label: 'Start page', run: () => go({ view: 'home' }) },
        { label: 'Rename…', run: startRename },
        { label: app.snap!.path ? 'Save' : 'Save as…', run: save },
        ...(app.snap!.path ? [{ label: 'Save as…', run: saveAs }] : []),
        { label: 'Open workspace…', run: open },
        { label: 'New workspace', run: newWs },
        ...(app.health?.native ? [{ label: 'Open in browser', run: () => void api.openBrowser() }] : []),
      ]}
      onclose={() => (wsMenu = null)} />
  {/if}

  <button class="search" onclick={() => (palette.open = true)} title="Search and commands ({modKey}K)">
    <Icon name="search" size={14} />
    <span>Search</span>
    {#if !app.phone}<kbd>{modKey}K</kbd>{/if}
  </button>

  <div class="heading section-label"><span>Workspace</span></div>
  <div class="group">
    <button class="item" class:on={route.view === 'data'} aria-current={route.view === 'data' ? 'page' : undefined} onclick={() => go({ view: 'data' })}>
      <Icon name="table" /> Data
      {#if app.tables.length}<span class="count">{app.tables.length}</span>{/if}
    </button>
    <button class="item" class:on={route.view === 'model'} aria-current={route.view === 'model' ? 'page' : undefined} onclick={() => go({ view: 'model' })}>
      <Icon name="model" /> Model
      {#if ws.relationships.length}<span class="count">{ws.relationships.length}</span>{/if}
    </button>
  </div>

  <div class="heading section-label">
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
      {@const on = route.view === 'dashboard' && route.id === d.id}
      <button class="item" class:on aria-current={on ? 'page' : undefined} onclick={() => go({ view: 'dashboard', id: d.id })}>
        <Icon name={d.layout === 'flow' ? 'text' : 'dashboard'} />
        <span class="ellipsis">{d.title}</span>
      </button>
    {:else}
      <p class="empty">No dashboards yet.</p>
    {/each}
  </div>

  <div class="foot">
    <button class="site-link" onclick={() => (app.siteOpen = true)}>
      <Icon name="globe" size={14} /> Site…{#if app.snap?.workspace.site?.on}<span class="on-dot" title="This workspace is a site"></span>{/if}
    </button>
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
      <span class="foot-actions">
        {#if app.health?.native}
          <button class="btn plain icon gear" title="Open this workspace in your web browser" aria-label="Open in browser" onclick={() => api.openBrowser()}>
            <Icon name="globe" size={16} />
          </button>
        {/if}
        <button class="btn plain icon gear" title="Settings" aria-label="Settings" onclick={() => (app.settingsOpen = true)}>
          <Icon name="settings" size={16} />
        </button>
      </span>
    </div>
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
    gap: 1px;
    padding: 10px;
    background: var(--window);
    border-right: 1px solid var(--sep);
    min-height: 0;
    overflow: auto;
  }
  .ws-card {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 46px;
    padding: 7px 8px;
    margin-bottom: 8px;
    border: 1px solid var(--sep);
    border-radius: 8px;
    background: var(--surface);
    text-align: left;
    color: var(--text);
    transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
  }
  button.ws-card:hover { border-color: var(--sep-strong); }
  button.ws-card[aria-expanded='true'] { border-color: var(--sep-strong); background: var(--hover); }
  .ws-card img { border-radius: 6px; flex: none; }
  .ws-card > :global(svg) { color: var(--text-2); }
  .ws-text { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .ws-name { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rename { height: 28px; font-weight: 600; }
  .state { font-size: var(--fs-label); color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .state.unsaved { color: var(--text); }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 6px 0 8px;
    margin-bottom: 10px;
    border: 1px solid transparent;
    border-radius: var(--r-control);
    background: var(--inset);
    color: var(--text-2);
    text-align: left;
    transition: border-color var(--dur-fast) var(--ease);
  }
  .search:hover { border-color: var(--sep-strong); }
  .search span { flex: 1; }
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
    color: var(--text);
    transition: background var(--dur-fast) var(--ease);
  }
  .item :global(svg) { color: var(--text-2); }
  .item:hover { background: var(--hover); }
  .item.on { background: var(--selected); color: var(--accent-text); }
  .item.on :global(svg) { color: var(--accent-text); }
  .count {
    margin-left: auto;
    min-width: 20px;
    padding: 0 6px;
    border-radius: 9px;
    background: var(--inset);
    font-size: var(--fs-label);
    line-height: 18px;
    text-align: center;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .item.on .count { background: var(--surface); }
  .ellipsis { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 26px;
    padding: 6px 4px 4px 8px;
  }
  .heading + .group { margin-bottom: 0; }
  .group + .heading { margin-top: 12px; }
  .small { width: 22px; height: 22px; color: var(--text-2); }
  .small:hover { color: var(--text); }
  .empty { margin: 4px 8px; color: var(--text-2); font-size: 12px; }
  .dashboards { flex: 1; }
  .foot {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
  }
  .site-link {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    font-size: 12px;
    color: var(--text-2);
    transition: background var(--dur-fast) var(--ease);
  }
  .site-link:hover { background: var(--hover); color: var(--text); }
  .on-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--success); }
  .theme-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 2px 0 4px;
    border-top: 1px solid var(--sep);
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
    transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
  }
  .seg button:hover { color: var(--text); }
  .seg button.on { background: var(--raised); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%), 0 0 0 1px var(--sep); }
  .foot-actions { display: flex; gap: 2px; }
  .gear { width: 28px; height: 28px; color: var(--text-2); }
  .gear:hover { color: var(--text); }
</style>
