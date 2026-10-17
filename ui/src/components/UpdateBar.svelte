<script lang="ts">
  // A new version of the desktop app (0.7 build 01): one quiet line over the page. Nothing installs
  // until the user presses Install; "Later" hides it until the app starts again.
  import { api } from '../lib/api';
  import { t } from '../lib/i18n';
  import { app } from '../lib/state.svelte';

  let later = $state<string | null>(null);
  let notes = $state(false);
  let installing = $state(false);
  const found = $derived(app.update?.available ?? null);

  async function install() {
    installing = true;
    try {
      // On success the app restarts in the new version, so this never answers.
      await api.installUpdate();
    } catch (e) {
      app.error(e);
      installing = false;
    }
  }
</script>

{#if found && later !== found.version}
  <div class="bar" role="status">
    <div class="line">
      <span class="what">{t('Tenun {version} is ready.', { version: found.version })}</span>
      {#if found.notes}
        <button type="button" class="link" aria-expanded={notes} onclick={() => (notes = !notes)}>{t('What changed')}</button>
      {/if}
      <span class="grow"></span>
      {#if installing}
        <span class="muted">{t('Installing… Tenun restarts when it is done.')}</span>
      {:else}
        <button type="button" class="btn plain" onclick={() => (later = found.version)}>{t('Later')}</button>
        <button type="button" class="btn primary" onclick={install}>{t('Install and restart')}</button>
      {/if}
    </div>
    {#if notes}
      <pre class="notes">{found.notes}</pre>
    {/if}
  </div>
{/if}

<style>
  .bar { flex: none; border-bottom: 1px solid var(--sep); background: var(--surface); font-size: 12px; }
  .line { display: flex; align-items: center; gap: 12px; min-height: 40px; padding: 0 16px; }
  .what { font-weight: 600; }
  .grow { flex: 1; }
  .link { padding: 0; border: 0; background: none; color: var(--accent); font-size: 12px; }
  .link:hover { text-decoration: underline; }
  .notes { margin: 0; max-height: 220px; overflow: auto; padding: 0 16px 12px; font-family: inherit; font-size: 12px; line-height: 1.5; white-space: pre-wrap; color: var(--text-2); }
</style>
