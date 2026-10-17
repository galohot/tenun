<script lang="ts">
  // A site's frame (build 12, V4-D14): its title, the navigation of its pages (a top bar or a sidebar, with named
  // groups), the page in the middle, and the footer line. On a narrow screen the navigation is a menu button that opens
  // a sheet. The site preview in the app draws it, and so will an exported site (13) and tenun.cloud (14), so a page
  // looks the same everywhere. `mini` draws only the frame, small, for the site sheet.
  import type { Snippet } from 'svelte';
  import type { SiteSettings } from '../../lib/api';
  import type { Nav } from '../../lib/site';
  import Icon from '../Icon.svelte';
  import { t } from '../../lib/i18n';

  let {
    site,
    nav,
    current,
    onnavigate,
    mini = false,
    children,
  }: {
    site: SiteSettings;
    nav: Nav;
    current: string | null;
    onnavigate: (id: string) => void;
    mini?: boolean;
    children?: Snippet;
  } = $props();

  let width = $state(1200);
  const narrow = $derived(!mini && width < 700);
  const side = $derived(site.nav === 'side' && !narrow);
  let menu = $state(false);
  let openGroup = $state<number | null>(null);
  const go = (id: string) => {
    menu = false;
    openGroup = null;
    onnavigate(id);
  };
  const inGroup = (g: Nav['groups'][number]) => g.pages.some((p) => p.id === current);
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && ((menu = false), (openGroup = null))} />

{#snippet list()}
  <ul class="pages">
    {#each nav.pages as p (p.id)}
      <li><button class:on={p.id === current} aria-current={p.id === current ? 'page' : undefined} onclick={() => go(p.id)}>{p.title}</button></li>
    {/each}
  </ul>
  {#each nav.groups as g (g.name)}
    {#if g.pages.length}
      <span class="gname">{g.name}</span>
      <ul class="pages">
        {#each g.pages as p (p.id)}
          <li><button class:on={p.id === current} aria-current={p.id === current ? 'page' : undefined} onclick={() => go(p.id)}>{p.title}</button></li>
        {/each}
      </ul>
    {/if}
  {/each}
{/snippet}

<div class="site" class:side class:mini bind:clientWidth={width}>
  {#if side}
    <nav class="sidebar" aria-label={t('Pages of {name}', { name: site.title })}>
      <button class="title" onclick={() => site.home && go(site.home)}>{site.title}</button>
      {@render list()}
    </nav>
  {:else}
    <header class="topbar">
      <button class="title" onclick={() => site.home && go(site.home)}>{site.title}</button>
      {#if narrow}
        <button class="menubtn" aria-label={t('Pages')} aria-expanded={menu} onclick={() => (menu = !menu)}><Icon name="rows" size={16} /> {t('Pages')}</button>
      {:else}
        <nav class="links" aria-label={t('Pages of {name}', { name: site.title })}>
          {#each nav.pages as p (p.id)}
            <button class:on={p.id === current} aria-current={p.id === current ? 'page' : undefined} onclick={() => go(p.id)}>{p.title}</button>
          {/each}
          {#each nav.groups as g, i (g.name)}
            {#if g.pages.length}
              <span class="group">
                <button class:on={inGroup(g)} aria-haspopup="menu" aria-expanded={openGroup === i} onclick={() => (openGroup = openGroup === i ? null : i)}>
                  {g.name} <Icon name="chevronDown" size={12} />
                </button>
                {#if openGroup === i}
                  <span class="drop" role="menu">
                    {#each g.pages as p (p.id)}
                      <button role="menuitem" class:on={p.id === current} onclick={() => go(p.id)}>{p.title}</button>
                    {/each}
                  </span>
                {/if}
              </span>
            {/if}
          {/each}
        </nav>
      {/if}
    </header>
  {/if}
  <div class="main">
    {#if !mini}{@render children?.()}{/if}
    {#if site.footer}<footer>{site.footer}</footer>{/if}
  </div>
  {#if menu && narrow}
    <div class="scrim" role="presentation" onclick={() => (menu = false)}></div>
    <nav class="sheet" aria-label={t('Pages of {name}', { name: site.title })}>
      {@render list()}
    </nav>
  {/if}
</div>

<style>
  .site { position: relative; min-height: 100%; display: flex; flex-direction: column; background: var(--window); color: var(--text); }
  .site.side { flex-direction: row; }
  button { border: 0; background: none; font: inherit; color: inherit; cursor: pointer; text-align: left; }
  .title { font-size: 15px; font-weight: 700; letter-spacing: -0.01em; padding: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .topbar { display: flex; align-items: center; gap: 20px; min-height: 52px; padding: 0 20px; border-bottom: 1px solid var(--sep); background: var(--surface); flex: none; position: sticky; top: 0; z-index: 5; }
  .links { display: flex; align-items: center; gap: 2px; min-width: 0; flex-wrap: wrap; }
  .links button { height: 30px; padding: 0 10px; border-radius: 6px; font-size: 13px; color: var(--text-2); display: inline-flex; align-items: center; gap: 4px; }
  .links button:hover { background: var(--hover); color: var(--text); }
  .links button.on { color: var(--accent-text); background: var(--selected); }
  .group { position: relative; }
  .drop { position: absolute; top: 34px; left: 0; z-index: 6; min-width: 180px; display: flex; flex-direction: column; padding: 4px; border-radius: 8px; background: var(--raised); box-shadow: var(--shadow-float); }
  .drop button { width: 100%; justify-content: flex-start; }
  .menubtn { margin-left: auto; display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 10px; border: 1px solid var(--sep); border-radius: 8px; font-size: 13px; }
  .sidebar { width: 220px; flex: none; padding: 18px 12px; border-right: 1px solid var(--sep); background: var(--surface); display: flex; flex-direction: column; gap: 4px; position: sticky; top: 0; align-self: flex-start; min-height: 100vh; }
  .sidebar .title { margin: 0 8px 12px; }
  .pages { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 1px; }
  .pages button { width: 100%; height: 30px; padding: 0 8px; border-radius: 6px; font-size: 13px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pages button:hover { background: var(--hover); }
  .pages button.on { background: var(--selected); color: var(--accent-text); font-weight: 600; }
  .gname { margin: 12px 8px 4px; font-family: var(--mono); font-size: 10.5px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  footer { margin-top: auto; padding: 16px 24px 20px; font-size: 12px; color: var(--text-2); text-align: center; }
  .scrim { position: fixed; inset: 0; z-index: 20; background: rgb(0 0 0 / 30%); }
  .sheet { position: fixed; left: 0; right: 0; bottom: 0; z-index: 21; max-height: 75vh; overflow: auto; padding: 16px 16px 28px; border-radius: 14px 14px 0 0; background: var(--raised); box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 4px; }
  .sheet .pages button { height: 40px; font-size: 15px; }
  /* The miniature of the site sheet: the frame at a small size, without a page. */
  .mini { min-height: 120px; border: 1px solid var(--sep); border-radius: 8px; overflow: hidden; pointer-events: none; }
  .mini .topbar { min-height: 34px; padding: 0 10px; gap: 10px; }
  .mini .title { font-size: 12px; }
  .mini .links button { height: 22px; padding: 0 6px; font-size: 11px; }
  .mini .sidebar { width: 120px; padding: 10px 6px; min-height: 120px; }
  .mini .pages button { height: 20px; font-size: 11px; }
  .mini .gname { margin: 6px 6px 2px; font-size: 9px; }
  .mini footer { padding: 6px 10px; font-size: 10px; }
</style>
