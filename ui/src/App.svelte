<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/state.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Welcome from './components/Welcome.svelte';
  import DataView from './components/DataView.svelte';
  import ModelView from './components/ModelView.svelte';
  import DashboardView from './components/DashboardView.svelte';
  import AddSource from './components/AddSource.svelte';
  import Toasts from './components/Toasts.svelte';

  onMount(() => {
    app.init();
  });

  $effect(() => {
    document.documentElement.dataset.theme = app.theme;
  });

  const hasData = $derived((app.snap?.workspace.sources.length ?? 0) > 0);
</script>

{#if app.snap}
  <div class="shell">
    <Sidebar />
    <main>
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
    </main>
  </div>
  {#if app.addOpen}
    <AddSource onclose={() => (app.addOpen = false)} />
  {/if}
{:else}
  <div class="boot">Starting Tenun…</div>
{/if}
<Toasts />

<style>
  .shell {
    display: grid;
    grid-template-columns: 232px 1fr;
    height: 100%;
  }
  main {
    min-width: 0;
    height: 100%;
    overflow: hidden;
    background: var(--window);
    display: flex;
    flex-direction: column;
  }
  .boot {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
</style>
