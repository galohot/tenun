<script lang="ts">
  // The site preview (build 12): the whole window shows the site as an export will draw it: the site's frame
  // (`SiteFrame`) around each page, drawn by the viewer of an exported file from a fresh snapshot. Page links and the
  // navigation move between pages. Esc or "Close preview" returns to the app.
  import { onDestroy } from 'svelte';
  import { api } from '../../lib/api';
  import type { ExportPayload } from '../../lib/export';
  import { app } from '../../lib/state.svelte';
  import { navOf, normaliseSite } from '../../lib/site';
  import Viewer from '../../viewer/Viewer.svelte';
  import Icon from '../Icon.svelte';
  import { t } from '../../lib/i18n';
  import SiteFrame from './SiteFrame.svelte';

  let page = $state<string | null>(app.preview);
  let data = $state<ExportPayload | null>(null);
  let error = $state('');
  const site = $derived(normaliseSite(app.snap?.workspace.site, app.dashboards, app.snap?.workspace.name));
  const nav = $derived(navOf(site, app.dashboards));
  const ids = $derived(new Set(app.dashboards.map((d) => d.id)));

  let asked = 0;
  $effect(() => {
    const id = page;
    if (!id) return;
    const n = ++asked;
    data = null;
    error = '';
    api
      .snapshot(id)
      .then((d) => n === asked && (data = d))
      .catch((e) => n === asked && (error = e instanceof Error ? e.message : String(e)));
  });
  const close = () => (app.preview = null);
  // The viewer remembers the names of a file's measures and links; the app's own come back.
  onDestroy(() => (app.snap = app.snap));
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && !document.querySelector('[role="dialog"][aria-modal="true"]') && close()} />

<div class="preview" role="region" aria-label={t('Preview of the site')}>
  <div class="bar">
    <span><Icon name="globe" size={14} /> {t('Preview of the site: this is how an export will look.')}</span>
    <button class="btn" onclick={close}><Icon name="x" size={14} /> {t('Close preview')}</button>
  </div>
  <div class="frame">
    <SiteFrame {site} {nav} current={page} onnavigate={(id) => (page = id)}>
      {#if error}
        <p class="msg">{t('This page could not be drawn: {error}', { error })}</p>
      {:else if data}
        {#key data}
          <Viewer {data} embedded pages={ids} onpage={(id) => (page = id)} />
        {/key}
      {:else}
        <p class="msg">{t('Drawing the page…')}</p>
      {/if}
    </SiteFrame>
  </div>
</div>

<style>
  .preview { position: fixed; inset: 0; z-index: 50; display: flex; flex-direction: column; background: var(--window); }
  .bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 8px 16px; background: var(--selected); color: var(--text); font-size: 13px; flex: none; }
  .bar span { display: inline-flex; align-items: center; gap: 6px; }
  .frame { flex: 1; min-height: 0; overflow: auto; }
  .msg { padding: 40px; text-align: center; color: var(--text-2); }
</style>
