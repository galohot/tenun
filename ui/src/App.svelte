<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/state.svelte';
  import { setLocale } from './lib/format';
  import { setCalendar } from './lib/locale';
  import Sidebar from './components/Sidebar.svelte';
  import Welcome from './components/Welcome.svelte';
  import DataView from './components/DataView.svelte';
  import ModelView from './components/ModelView.svelte';
  import DashboardView from './components/DashboardView.svelte';
  import AddSource from './components/AddSource.svelte';
  import SettingsSheet from './components/SettingsSheet.svelte';
  import SiteSheet from './components/site/SiteSheet.svelte';
  import SitePreview from './components/site/SitePreview.svelte';
  import Toasts from './components/Toasts.svelte';
  import Icon from './components/Icon.svelte';
  import Palette from './components/Palette.svelte';
  import StatusBar from './components/StatusBar.svelte';

  onMount(() => {
    app.init();
  });

  $effect(() => {
    document.documentElement.dataset.theme = app.theme;
  });

  // Numbers and dates follow the open workspace, and so does the calendar of its periods (v5 build 04).
  $effect(() => {
    setLocale(app.locale);
    setCalendar(app.snap?.workspace.calendar);
  });

  // The drawer closes when a page is picked.
  $effect(() => {
    void app.route;
    app.navOpen = false;
  });

  const hasData = $derived((app.snap?.workspace.sources.length ?? 0) > 0);
</script>

{#if app.snap}
  <div class="shell" class:phone={app.phone}>
    {#if app.phone}
      <header class="phonebar">
        <button class="btn plain icon" aria-label="Menu" aria-expanded={app.navOpen} onclick={() => (app.navOpen = !app.navOpen)}><Icon name="rows" /></button>
        <img src="/icon.svg" alt="" width="20" height="20" />
        <span>{app.snap.workspace.name}</span>
      </header>
      {#if app.navOpen}
        <div class="scrim" role="presentation" onclick={() => (app.navOpen = false)}></div>
        <div class="drawer"><Sidebar /></div>
      {/if}
    {:else}
      <Sidebar />
    {/if}
    <main>
      <div class="view">
      {#if app.route.view === 'data'}
        <DataView tableId={app.route.table} />
      {:else if app.route.view === 'model'}
        <ModelView />
      {:else if app.route.view === 'dashboard'}
        {#key app.route.id}
          <DashboardView id={app.route.id} />
        {/key}
      {:else}
        <Welcome {hasData} />
      {/if}
      </div>
      {#if !app.phone}
        <StatusBar />
      {/if}
    </main>
  </div>
  {#if app.addOpen}
    <AddSource onclose={() => (app.addOpen = false)} />
  {/if}
  {#if app.settingsOpen}
    <SettingsSheet onclose={() => (app.settingsOpen = false)} />
  {/if}
  {#if app.siteOpen}
    <SiteSheet onclose={() => (app.siteOpen = false)} />
  {/if}
  {#if app.preview}
    <SitePreview />
  {/if}
  <Palette />
{:else}
  <div class="boot">Starting Tenun…</div>
{/if}
<Toasts />

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--sidebar-w) 1fr;
    height: 100%;
  }
  .shell.phone { display: flex; flex-direction: column; }
  .phonebar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 44px;
    padding: 0 8px;
    flex: none;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    font-weight: 600;
  }
  .phonebar span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .scrim { position: fixed; inset: 0; z-index: 40; background: rgb(0 0 0 / 35%); }
  .drawer { position: fixed; top: 0; bottom: 0; left: 0; z-index: 41; width: 272px; max-width: 85vw; display: flex; box-shadow: var(--shadow-float); animation: drawer-in var(--dur) var(--ease); }
  @keyframes drawer-in { from { transform: translateX(-16px); opacity: 0; } }
  .drawer :global(.side) { width: 100%; }
  main {
    min-width: 0;
    height: 100%;
    overflow: hidden;
    background: var(--window);
    display: flex;
    flex-direction: column;
  }
  .shell.phone main { flex: 1; height: auto; min-height: 0; }
  /* The open view takes the room the status bar leaves. */
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  .boot {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
</style>
