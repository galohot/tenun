<script lang="ts">
  // Saved views (v5 build 03): a button next to the dashboard title that names the open view ("Java, this quarter";
  // "changed" when the dashboard no longer reads as it), and a menu to open one. Making, updating, renaming and
  // deleting views is Advanced; opening one is open to every reader.
  import type { SavedView } from '../../lib/api';
  import { MAX_VIEWS } from '../../lib/views';
  import Icon from '../Icon.svelte';
  import Menu from './Menu.svelte';
  import Modal from '../Modal.svelte';

  let {
    views,
    open,
    changed,
    defaultView,
    editing,
    onopen,
    onsave,
    onupdate,
    onrename,
    ondelete,
    ondefault,
  }: {
    views: SavedView[];
    /** The view the dashboard was last set to, or null. */
    open: string | null;
    /** The dashboard no longer reads as the open view. */
    changed: boolean;
    defaultView?: string;
    /** Advanced: the reader may make and change views. */
    editing: boolean;
    onopen: (id: string) => void;
    onsave: (name: string) => void;
    onupdate: (id: string) => void;
    onrename: (id: string, name: string) => void;
    ondelete: (id: string) => void;
    /** Opens the dashboard on this view (null: as it was saved). */
    ondefault: (id: string | null) => void;
  } = $props();

  let menu = $state<{ x: number; y: number } | null>(null);
  /** The name sheet: a new view (`id` empty), or a rename. */
  let naming = $state<{ id: string; name: string } | null>(null);
  const current = $derived(views.find((v) => v.id === open) ?? null);

  const items = $derived([
    ...views.map((v) => ({
      label: `${v.name}${v.id === defaultView ? ' (first)' : ''}${v.id === open ? '  ✓' : ''}`,
      run: () => onopen(v.id),
    })),
    ...(editing
      ? [
          ...(views.length < MAX_VIEWS ? [{ label: 'Save as a new view…', run: () => (naming = { id: '', name: '' }) }] : []),
          ...(current
            ? [
                { label: 'Update this view', run: () => onupdate(current.id), disabled: !changed },
                { label: 'Rename…', run: () => (naming = { id: current.id, name: current.name }) },
                {
                  label: current.id === defaultView ? 'Do not open on this view' : 'Open on this view first',
                  run: () => ondefault(current.id === defaultView ? null : current.id),
                },
                { label: 'Delete this view', run: () => ondelete(current.id) },
              ]
            : []),
        ]
      : []),
  ]);

  function save() {
    if (!naming) return;
    const name = naming.name.trim();
    if (!name) return;
    if (naming.id) onrename(naming.id, name);
    else onsave(name);
    naming = null;
  }
</script>

{#if views.length || editing}
  <button
    class="views"
    class:changed={!!current && changed}
    aria-haspopup="menu"
    title={views.length ? 'Saved views' : 'Save how this dashboard reads now'}
    onclick={(e) => {
      const r = e.currentTarget.getBoundingClientRect();
      menu = menu ? null : { x: r.left, y: r.bottom + 4 };
    }}>
    <Icon name="bookmark" size={13} />
    {#if current}{current.name}{#if changed}<span class="mark">changed</span>{/if}{:else}Views{/if}
    <Icon name="chevronDown" size={11} />
  </button>
{/if}
{#if menu}
  <Menu x={menu.x} y={menu.y} {items} onclose={() => (menu = null)} />
{/if}
{#if naming}
  <Modal title={naming.id ? 'Rename the view' : 'Save as a new view'} width={380} onclose={() => (naming = null)}>
    <label class="name">
      <span>Name</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" maxlength="60" bind:value={naming.name} autofocus onkeydown={(e) => e.key === 'Enter' && save()} placeholder="Java, this quarter" />
    </label>
    {#if !naming.id}
      <p class="hint">The view keeps the clicks, the filters' choices, the grain and the drill-downs of this dashboard now.</p>
    {/if}
    {#snippet footer()}
      <button class="btn plain" onclick={() => (naming = null)}>Cancel</button>
      <button class="btn primary" disabled={!naming?.name.trim()} onclick={save}>Save</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .views {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--sep-strong);
    border-radius: var(--r-control);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition: background 120ms var(--spring);
  }
  .views:hover { background: var(--hover); }
  .views :global(svg) { color: var(--text-2); }
  .mark { padding: 0 5px; border-radius: 8px; background: var(--inset); color: var(--text-2); font-size: 11px; }
  .name { display: flex; flex-direction: column; gap: 6px; font-size: 13px; }
  .hint { margin: 10px 0 0; font-size: 12px; color: var(--text-2); }
</style>
