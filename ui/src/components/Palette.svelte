<script lang="ts" module>
  // The command palette (look 3): ⌘K on a Mac, Ctrl+K elsewhere, or the Search field in the sidebar.
  // It jumps to a dashboard, a table, Data or Model, and runs the actions the app already has.
  export const palette = $state({ open: false });

  /** ⌘ on a Mac, Ctrl elsewhere: the name of the key in hints and tooltips. */
  export const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
  export const modKey = isMac ? '⌘' : 'Ctrl+';
</script>

<script lang="ts">
  import { t } from '../lib/i18n';
  import { tick } from 'svelte';
  import { app, go, type ThemePref } from '../lib/state.svelte';
  import { api } from '../lib/api';
  import { newDashboard, newId } from '../lib/tiles';
  import { newStory } from '../lib/flow';
  import { portal } from '../lib/portal';
  import Icon, { type IconName } from './Icon.svelte';

  interface Command {
    id: string;
    group: string;
    label: string;
    icon: IconName;
    /** Words that find it besides its label. */
    keys?: string;
    /** Said on the right: what kind of thing it is, or that it is on now. */
    hint?: string;
    run: () => void;
  }

  let query = $state('');
  let at = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();
  let before: HTMLElement | null = null;

  function addDashboard(kind: 'grid' | 'story') {
    const d = kind === 'story' ? newStory(newId('d'), app.dashboards.filter((x) => x.layout === 'flow').length, newId) : newDashboard(app.dashboards.length);
    app.snap!.workspace.dashboards.push(d);
    app.saveDashboards();
    go({ view: 'dashboard', id: d.id });
  }

  const themeNames: Record<ThemePref, string> = { light: t('Light appearance'), dark: t('Dark appearance'), system: t('Match system appearance') };

  const commands = $derived.by<Command[]>(() => {
    if (!app.snap) return [];
    const out: Command[] = [];
    for (const d of app.dashboards) {
      out.push({ id: `d:${d.id}`, group: t('Dashboards'), label: d.title || t('Untitled'), icon: d.layout === 'flow' ? 'text' : 'dashboard', hint: d.layout === 'flow' ? t('Story') : t('Dashboard'), run: () => go({ view: 'dashboard', id: d.id }) });
    }
    for (const tb of app.tables) {
      const src = app.snap.workspace.sources.find((s) => s.id === tb.source_id);
      out.push({ id: `t:${tb.id}`, group: t('Tables'), label: tb.label || tb.name, icon: 'table', keys: src?.name, hint: src && src.name !== (tb.label || tb.name) ? src.name : t('Table'), run: () => go({ view: 'data', table: tb.id }) });
    }
    out.push(
      { id: 'go:data', group: t('Go to'), label: t('Data'), icon: 'table', keys: 'sources tables', run: () => go({ view: 'data' }) },
      { id: 'go:model', group: t('Go to'), label: t('Model'), icon: 'model', keys: 'relationships links', run: () => go({ view: 'model' }) },
      { id: 'go:home', group: t('Go to'), label: t('Start page'), icon: 'sparkles', keys: 'home welcome', run: () => go({ view: 'home' }) },
      { id: 'a:add', group: t('Actions'), label: t('Add data…'), icon: 'plus', keys: 'source excel csv database import', run: () => (app.addOpen = true) },
      { id: 'a:dash', group: t('Actions'), label: t('New dashboard'), icon: 'dashboard', keys: 'create page', run: () => addDashboard('grid') },
      { id: 'a:story', group: t('Actions'), label: t('New story'), icon: 'text', keys: 'create page document', run: () => addDashboard('story') },
      { id: 'a:site', group: t('Actions'), label: t('Site…'), icon: 'globe', keys: 'publish web', hint: app.snap.workspace.site?.on ? t('On') : undefined, run: () => (app.siteOpen = true) },
      { id: 'a:settings', group: t('Actions'), label: t('Settings'), icon: 'settings', keys: 'preferences options', run: () => (app.settingsOpen = true) },
      { id: 'a:report', group: t('Actions'), label: t('Report a problem…'), icon: 'warning', keys: 'bug issue help feedback', run: () => (app.reportOpen = true) },
    );
    if (app.health?.native) out.push({ id: 'a:browser', group: t('Actions'), label: t('Open in browser'), icon: 'globe', run: () => api.openBrowser() });
    for (const [p, icon] of [['light', 'sun'], ['dark', 'moon'], ['system', 'auto']] as const) {
      out.push({ id: `th:${p}`, group: t('Appearance'), label: themeNames[p], icon, keys: 'theme mode', hint: app.themePref === p ? t('On') : undefined, run: () => app.setTheme(p) });
    }
    return out;
  });

  /** How well a command matches what was typed: 0 is no match. A label that starts with it wins. */
  function score(c: Command, q: string): number {
    const label = c.label.toLowerCase();
    if (label.startsWith(q)) return 4;
    if (label.split(/[\s_\-/.]+/).some((w) => w.startsWith(q))) return 3;
    if (label.includes(q)) return 2;
    const hay = `${c.keys ?? ''} ${c.group}`.toLowerCase();
    if (hay.split(/\s+/).some((w) => w.startsWith(q))) return 1.5;
    // The letters in order, with gaps: "cov" finds "Coffee overview".
    let i = 0;
    for (const ch of label) if (ch === q[i]) i++;
    return i === q.length ? 1 : 0;
  }

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands
      .map((c, i) => ({ c, s: score(c, q), i }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s || a.i - b.i)
      .map((x) => x.c);
  });

  // Rows keep their group heading while nothing is typed; a search is one list, best first.
  const grouped = $derived(!query.trim());

  $effect(() => {
    void query;
    at = 0;
  });

  $effect(() => {
    if (palette.open) {
      before = document.activeElement as HTMLElement | null;
      query = '';
      at = 0;
      tick().then(() => input?.focus());
    }
  });

  function close() {
    palette.open = false;
    before?.focus?.();
  }

  function pick(c: Command | undefined) {
    if (!c) return;
    palette.open = false;
    c.run();
  }

  function onkey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const n = shown.length;
      if (n) at = (at + (e.key === 'ArrowDown' ? 1 : -1) + n) % n;
      tick().then(() => list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }));
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      at = e.key === 'Home' ? 0 : Math.max(0, shown.length - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(shown[at]);
    }
  }

  // ⌘K / Ctrl+K anywhere, unless a field of its own already used the key (the text editor's link).
  function onwindowkey(e: KeyboardEvent) {
    if (e.defaultPrevented || !(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey || e.key.toLowerCase() !== 'k') return;
    if (!app.snap) return;
    e.preventDefault();
    if (palette.open) close();
    else palette.open = true;
  }
</script>

<svelte:window onkeydown={onwindowkey} />

{#if palette.open}
  <div class="scrim" role="presentation" use:portal onpointerdown={close}></div>
  <div class="palette sheet-in" role="dialog" aria-modal="true" aria-label={t('Command palette')} tabindex="-1" use:portal onkeydown={onkey}>
    <div class="search">
      <Icon name="search" size={16} />
      <input
        bind:this={input}
        bind:value={query}
        type="text"
        placeholder={t('Jump to a dashboard or a table, or run an action')}
        aria-label={t('Search')}
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={shown[at] ? `pal-${shown[at].id}` : undefined}
        autocomplete="off"
        spellcheck="false" />
      <kbd>Esc</kbd>
    </div>
    <div class="list" id="palette-list" role="listbox" bind:this={list}>
      {#each shown as c, i (c.id)}
        {#if grouped && (i === 0 || shown[i - 1].group !== c.group)}
          <div class="group section-label" role="presentation">{c.group}</div>
        {/if}
        <div
          id="pal-{c.id}"
          class="row"
          role="option"
          tabindex="-1"
          aria-selected={i === at}
          onpointermove={() => (at = i)}
          onclick={() => pick(c)}
          onkeydown={() => {}}>
          <Icon name={c.icon} size={16} />
          <span class="title">{c.label}</span>
          {#if c.hint}<span class="hint">{c.hint}</span>{/if}
          {#if i === at}<Icon name="enter" size={14} class="enter" />{/if}
        </div>
      {:else}
        <p class="none">{t('Nothing matches “{q}”.', { q: query.trim() })}</p>
      {/each}
    </div>
    <div class="foot">
      <span><kbd>↑</kbd><kbd>↓</kbd> {t('to move')}</span>
      <span><kbd>↵</kbd> {t('to open')}</span>
      <span class="push"><kbd>{modKey}K</kbd> {t('to close')}</span>
    </div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; z-index: 80; background: rgb(0 0 0 / 35%); }
  .palette {
    position: fixed;
    z-index: 81;
    top: 14vh;
    left: 50%;
    width: min(600px, calc(100vw - 32px));
    margin-left: calc(min(600px, calc(100vw - 32px)) / -2);
    display: flex;
    flex-direction: column;
    max-height: min(520px, 76vh);
    border-radius: var(--r-sheet);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    overflow: hidden;
    transform-origin: top center;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 48px;
    padding: 0 14px;
    border-bottom: 1px solid var(--sep-strong);
    color: var(--text-2);
    flex: none;
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    outline: none;
    background: none;
    font-size: 15px;
    color: var(--text);
  }
  .search input::placeholder { color: var(--text-2); }
  .list { overflow: auto; padding: 6px; min-height: 0; }
  .group { padding: 10px 10px 4px; }
  .group:first-child { padding-top: 4px; }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r-control);
    cursor: pointer;
    color: var(--text);
  }
  .row :global(svg) { color: var(--text-2); }
  .row[aria-selected='true'] { background: var(--selected); }
  .row[aria-selected='true'] > :global(svg:first-child) { color: var(--accent-text); }
  .title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint { font-size: 12px; color: var(--text-2); white-space: nowrap; }
  .row :global(.enter) { color: var(--text-2); }
  .none { margin: 18px 10px; color: var(--text-2); text-align: center; }
  .foot {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 34px;
    padding: 0 14px;
    border-top: 1px solid var(--sep-strong);
    font-size: var(--fs-small);
    color: var(--text-2);
    flex: none;
  }
  .foot span { display: inline-flex; align-items: center; gap: 4px; }
  .foot .push { margin-left: auto; }
  .foot kbd { height: 17px; min-width: 17px; justify-content: center; padding: 0 4px; }
</style>
