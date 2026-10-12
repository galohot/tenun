<script lang="ts">
  // The site sheet (build 12, V4-D14): make this workspace a site, name it, pick a top bar or a sidebar, a home page,
  // named groups of pages, which pages the navigation shows, and a footer line. A miniature of the navigation at the
  // top follows every change. Off by default: with it off, nothing about export or publish changes.
  import type { SiteSettings } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { moveGroup, navOf, normaliseSite, setGroup } from '../../lib/site';
  import Icon from '../Icon.svelte';
  import Modal from '../Modal.svelte';
  import SiteFrame from './SiteFrame.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const dashboards = $derived(app.dashboards);
  const site = $derived(normaliseSite(app.snap?.workspace.site, dashboards, app.snap?.workspace.name));
  const nav = $derived(navOf(site, dashboards));
  const save = (next: Partial<SiteSettings>) => app.saveSite(normaliseSite({ ...site, ...next }, dashboards, app.snap?.workspace.name));
  const groupOf = (id: string) => (site.groups ?? []).findIndex((g) => g.pages.includes(id));

  function setShown(id: string, shown: boolean) {
    const d = app.snap?.workspace.dashboards.find((x) => x.id === id);
    if (!d) return;
    d.hidden = shown ? undefined : true;
    app.saveDashboards();
    // The home page must stay in the navigation.
    save({});
  }
  function preview() {
    onclose();
    app.preview = site.home ?? dashboards[0]?.id ?? null;
  }
</script>

<Modal title="Site" width={620} {onclose}>
  <div class="mini">
    <SiteFrame {site} {nav} current={site.home ?? null} onnavigate={() => {}} mini />
  </div>

  <label class="switch">
    <input type="checkbox" checked={site.on} onchange={(e) => save({ on: e.currentTarget.checked })} />
    <span>
      <b>Make this workspace a site</b>
      <span class="muted">An export and a publish then hold every page in the navigation, with links between them.</span>
    </span>
  </label>

  <div class="grid" class:off={!site.on}>
    <label class="field-row">
      <span class="label">Title</span>
      <input class="field" maxlength="80" value={site.title} onchange={(e) => save({ title: e.currentTarget.value })} />
    </label>
    <div class="field-row">
      <span class="label">Navigation</span>
      <div class="seg" role="radiogroup" aria-label="Navigation">
        {#each [['top', 'Top bar'], ['side', 'Sidebar']] as [id, label]}
          <button role="radio" aria-checked={site.nav === id} class:on={site.nav === id} onclick={() => save({ nav: id as 'top' | 'side' })}>{label}</button>
        {/each}
      </div>
    </div>
    <label class="field-row">
      <span class="label">Home page</span>
      <select class="field" value={site.home ?? ''} onchange={(e) => save({ home: e.currentTarget.value })}>
        {#each dashboards.filter((d) => !d.hidden) as d (d.id)}<option value={d.id}>{d.title || 'Untitled'}</option>{/each}
      </select>
    </label>

    <span class="label up">Pages</span>
    <ul class="pages">
      {#each dashboards as d (d.id)}
        <li>
          <span class="pname" title={d.title}>{d.title || 'Untitled'}</span>
          <label class="check"><input type="checkbox" checked={!d.hidden} onchange={(e) => setShown(d.id, e.currentTarget.checked)} /> In the navigation</label>
          <select class="field small" aria-label="Group of {d.title}" value={String(groupOf(d.id))} onchange={(e) => save(setGroup(site, d.id, Number(e.currentTarget.value) < 0 ? null : Number(e.currentTarget.value)))}>
            <option value="-1">No group</option>
            {#each site.groups ?? [] as g, i (i)}<option value={String(i)}>{g.name}</option>{/each}
          </select>
        </li>
      {/each}
    </ul>
    <p class="hint">A page left out of the navigation is still reached by a link in a text, or by a click that opens it.</p>

    <span class="label up">Groups</span>
    {#each site.groups ?? [] as g, i (i)}
      <div class="grow">
        <input class="field" aria-label="Name of the group" maxlength="40" value={g.name} onchange={(e) => save({ groups: (site.groups ?? []).map((x, j) => (j === i ? { ...x, name: e.currentTarget.value } : x)) })} />
        <span class="muted count">{g.pages.length === 1 ? '1 page' : `${g.pages.length} pages`}</span>
        <button class="btn plain icon" aria-label="Move {g.name} up" disabled={i === 0} onclick={() => save(moveGroup(site, i, i - 1))}><Icon name="chevronDown" size={12} class="flip" /></button>
        <button class="btn plain icon" aria-label="Move {g.name} down" disabled={i === (site.groups ?? []).length - 1} onclick={() => save(moveGroup(site, i, i + 1))}><Icon name="chevronDown" size={12} /></button>
        <button class="btn plain icon" aria-label="Remove {g.name}" onclick={() => save({ groups: (site.groups ?? []).filter((_, j) => j !== i) })}><Icon name="x" size={12} /></button>
      </div>
    {/each}
    <button class="link" onclick={() => save({ groups: [...(site.groups ?? []), { name: `Group ${(site.groups ?? []).length + 1}`, pages: [] }] })}>Add a group</button>

    <label class="field-row up">
      <span class="label">Footer</span>
      <input class="field" maxlength="160" placeholder="One line under every page" value={site.footer ?? ''} onchange={(e) => save({ footer: e.currentTarget.value })} />
    </label>
  </div>

  {#snippet footer()}
    <button class="btn plain" onclick={onclose}>Done</button>
    <button class="btn primary" disabled={!site.on} onclick={preview}><Icon name="expand" size={14} /> Preview site</button>
  {/snippet}
</Modal>

<style>
  .mini { margin-bottom: 14px; }
  .switch { display: flex; gap: 10px; align-items: flex-start; margin-bottom: 14px; font-size: 13px; }
  .switch input { margin-top: 3px; }
  .switch span { display: flex; flex-direction: column; gap: 2px; }
  .muted { color: var(--text-2); font-size: 12px; }
  .grid { display: flex; flex-direction: column; gap: 8px; }
  .grid.off { opacity: 0.55; }
  .field-row { display: grid; grid-template-columns: 120px 1fr; align-items: center; gap: 12px; }
  .label.up { margin-top: 8px; }
  .field-row.up { margin-top: 8px; }
  .seg { display: flex; padding: 2px; border-radius: 8px; background: var(--inset); }
  .seg button { flex: 1; height: 26px; border: 0; border-radius: 6px; background: none; color: var(--text-2); font-size: 12px; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .pages { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .pages li { display: grid; grid-template-columns: 1fr auto 140px; gap: 10px; align-items: center; font-size: 13px; }
  .pname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .check { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-2); }
  .field.small { height: 28px; font-size: 12px; }
  .hint { margin: 0; font-size: 11px; line-height: 1.4; color: var(--text-2); }
  .grow { display: grid; grid-template-columns: 1fr auto 26px 26px 26px; gap: 6px; align-items: center; }
  .grow .btn.icon { width: 26px; height: 26px; }
  .grow :global(.flip) { transform: rotate(180deg); }
  .count { white-space: nowrap; }
  .link { align-self: flex-start; border: 0; background: none; padding: 0; font-size: 12px; color: var(--accent-text); cursor: pointer; }
</style>
