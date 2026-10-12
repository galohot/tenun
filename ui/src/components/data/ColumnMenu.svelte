<script lang="ts">
  import type { Column, DateFormat, TableSetup } from '../../lib/api';
  import { DATE_FORMATS, TYPE_CHOICES, withColumn } from '../../lib/prep';
  import Icon from '../Icon.svelte';

  let {
    column,
    setup,
    canRetype,
    x,
    y,
    onsave,
    onclose,
    onedit,
    ondelete,
  }: {
    column: Column;
    setup: TableSetup | undefined;
    /** The data is local, so the column can be read as another type. */
    canRetype: boolean;
    x: number;
    y: number;
    onsave: (next: TableSetup) => void;
    onclose: () => void;
    /** A calculated column, with Advanced on: change its formula. */
    onedit?: (name: string) => void;
    /** A calculated column, with Advanced on: remove it. */
    ondelete?: (name: string) => void;
  } = $props();

  type Mode = 'main' | 'rename' | 'type' | 'description';
  let mode = $state<Mode>('main');
  const mine = $derived(setup?.columns?.[column.name]);
  // The forms start from the current text. The menu closes after a save, so it never needs to follow.
  // svelte-ignore state_referenced_locally
  let label = $state(column.label ?? '');
  // svelte-ignore state_referenced_locally
  let description = $state(column.description ?? '');
  let root: HTMLDivElement;

  const isDate = (t: string) => t === 'date' || t === 'datetime';
  const format = $derived<DateFormat>(mine?.date_format ?? 'auto');

  function outside(e: MouseEvent) {
    if (root && !root.contains(e.target as Node)) onclose();
  }

  function patch(p: Parameters<typeof withColumn>[2], close = true) {
    onsave(withColumn(setup, column.name, p));
    if (close) onclose();
  }

  // The menu closes first, and the column it named is out of the page by then, so the name goes along.
  function edit() {
    const name = column.name;
    onclose();
    onedit?.(name);
  }

  function remove() {
    const name = column.name;
    onclose();
    ondelete?.(name);
  }

  function pickType(dtype: Column['dtype']) {
    if (dtype === column.dtype) return;
    // A date column asks for its format next, so the menu stays open.
    patch({ dtype }, !isDate(dtype));
  }
</script>

<svelte:window onmousedown={outside} onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="menu" bind:this={root} style:left="{x}px" style:top="{y}px" role="menu">
  {#if mode === 'main'}
    <button role="menuitem" onclick={() => (mode = 'rename')}><Icon name="pencil" size={13} /> Rename</button>
    {#if canRetype}
      <button role="menuitem" onclick={() => (mode = 'type')}>
        <Icon name="text" size={13} /> Type
        <span class="more">{TYPE_CHOICES.find((t) => t.dtype === column.dtype)?.label}<Icon name="chevronRight" size={12} /></span>
      </button>
    {/if}
    <button role="menuitem" onclick={() => patch({ hidden: !column.hidden })}>
      <Icon name={column.hidden ? 'check' : 'x'} size={13} /> {column.hidden ? 'Show in field lists' : 'Hide from field lists'}
    </button>
    <button role="menuitem" onclick={() => (mode = 'description')}><Icon name="info" size={13} /> Description</button>
    {#if column.calc && (onedit || ondelete)}
      <div class="rule" role="separator"></div>
      {#if onedit}
        <button role="menuitem" onclick={edit}><Icon name="fx" size={13} /> Edit formula</button>
      {/if}
      {#if ondelete}
        <button role="menuitem" class="danger" onclick={remove}><Icon name="trash" size={13} /> Delete column</button>
      {/if}
    {/if}
  {:else if mode === 'rename'}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        patch({ label });
      }}>
      <label class="label" for="cm-name">Name shown in Tenun</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="cm-name" class="field" bind:value={label} placeholder={column.name} autofocus />
      <p class="hint">Queries still use "{column.name}".</p>
      <div class="row">
        {#if column.label}<button type="button" class="btn plain" onclick={() => patch({ label: undefined })}>Reset</button>{/if}
        <span class="sp"></span>
        <button type="button" class="btn" onclick={() => (mode = 'main')}>Back</button>
        <button class="btn primary">Save</button>
      </div>
    </form>
  {:else if mode === 'description'}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        patch({ description });
      }}>
      <label class="label" for="cm-desc">Shown as a tip in the field lists</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="cm-desc" class="field" bind:value={description} placeholder="What is in this column?" autofocus />
      <div class="row">
        {#if column.description}<button type="button" class="btn plain" onclick={() => patch({ description: undefined })}>Remove</button>{/if}
        <span class="sp"></span>
        <button type="button" class="btn" onclick={() => (mode = 'main')}>Back</button>
        <button class="btn primary">Save</button>
      </div>
    </form>
  {:else}
    <div class="head"><button class="btn plain icon" aria-label="Back" onclick={() => (mode = 'main')}><Icon name="chevronLeft" size={13} /></button> Read this column as</div>
    {#each TYPE_CHOICES as t (t.dtype)}
      <button role="menuitemradio" aria-checked={column.dtype === t.dtype} class:on={column.dtype === t.dtype} onclick={() => pickType(t.dtype)}>
        <span class="tick">{#if column.dtype === t.dtype}<Icon name="check" size={12} />{/if}</span>{t.label}
      </button>
    {/each}
    {#if isDate(column.dtype) && mine?.dtype}
      <div class="sub">Dates written as text</div>
      {#each DATE_FORMATS as f (f.value)}
        <button role="menuitemradio" aria-checked={format === f.value} class:on={format === f.value} onclick={() => patch({ date_format: f.value }, false)}>
          <span class="tick">{#if format === f.value}<Icon name="check" size={12} />{/if}</span>{f.label}
        </button>
      {/each}
    {/if}
    {#if mine?.dtype}
      <button role="menuitem" onclick={() => patch({ dtype: undefined, date_format: undefined })}>Use the type from the file</button>
    {/if}
    {#if column.failed}
      <p class="hint warn">Cells that cannot be read become blank.</p>
    {/if}
  {/if}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 45;
    min-width: 210px;
    max-width: 260px;
    padding: 6px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    font-size: 12px;
    animation: pop 180ms var(--spring);
    transform-origin: top left;
  }
  button[role^='menuitem'] {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
    font-size: 12px;
  }
  button[role^='menuitem']:hover { background: var(--accent); color: #fff; }
  button[role^='menuitem'] :global(svg) { color: var(--text-2); }
  button[role^='menuitem']:hover :global(svg), button[role^='menuitem']:hover .more { color: #fff; }
  .more { margin-left: auto; display: inline-flex; align-items: center; gap: 2px; color: var(--text-2); }
  .on { font-weight: 600; }
  .rule { height: 1px; margin: 4px 6px; background: var(--sep); }
  button.danger { color: var(--danger); }
  button.danger :global(svg) { color: var(--danger); }
  button.danger:hover, button.danger:hover :global(svg) { color: #fff; }
  .tick { width: 14px; display: inline-grid; place-items: center; }
  .head { display: flex; align-items: center; gap: 4px; padding: 0 4px 4px; font-weight: 600; }
  .sub { padding: 8px 8px 2px; font-size: 11px; font-weight: 600; color: var(--text-2); }
  form { display: flex; flex-direction: column; gap: 6px; padding: 4px; }
  .row { display: flex; align-items: center; gap: 6px; margin-top: 4px; }
  .sp { flex: 1; }
  .hint { margin: 0; font-size: 11px; color: var(--text-2); }
  .warn { padding: 6px 8px 2px; }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
</style>
