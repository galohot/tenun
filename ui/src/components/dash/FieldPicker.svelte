<script lang="ts">
  import type { DType, NamedMeasure } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { typeIcon } from '../../lib/ui';
  import { blankNote, isShown } from '../../lib/prep';
  import { fieldGroups, groupOf, labelOfLink } from '../../lib/links';
  import { nameOf } from '../../lib/measures';
  import Icon from '../Icon.svelte';

  let {
    mode,
    table,
    column,
    via,
    base = null,
    roles = false,
    placeholder = 'Choose a field',
    reachable,
    numbersOnly = false,
    datesOnly = false,
    measures = [],
    named = null,
    onnamed,
    onchange,
    onclear,
  }: {
    mode: 'measure' | 'dim';
    table: string | null;
    column: string | null;
    /** The link this field's table is reached through. Not set: the only path, or the first. */
    via?: string;
    /** The table of the tile's first value. Two links to one table list it once for each link, seen from here. */
    base?: string | null;
    /** No first value to measure from (a filter tile): list a table once for each link that leads to it from some table. */
    roles?: boolean;
    placeholder?: string;
    /** Tables that can be used together with the tile's other fields. */
    reachable?: Set<string> | null;
    /** Lists number columns only, and no row count (a histogram bins a column of numbers). */
    numbersOnly?: boolean;
    /** Lists date columns only, and no row count (a Gantt chart reads a start and an end date). */
    datesOnly?: boolean;
    /** The measures of the workspace that can be picked here (build 03). They are listed first, under "Measures". */
    measures?: NamedMeasure[];
    /** The measure this field holds now. */
    named?: string | null;
    /** A measure was picked. Without it no measures are offered. */
    onnamed?: (id: string) => void;
    onchange: (table: string, column: string | null, via?: string) => void;
    onclear?: () => void;
  } = $props();

  let open = $state(false);
  let q = $state('');
  let root: HTMLDivElement;

  const current = $derived(table ? app.table(table) : undefined);
  const label = $derived(
    !table
      ? ''
      : column === null
        ? `Number of ${(current?.label ?? 'rows').toLowerCase()}`
        : (current?.columns.find((c) => c.name === column)?.label ?? column),
  );

  /** Every table once, or once for each link that leads to it (V4-D28). */
  const all = $derived(fieldGroups(app.tables, app.snap?.workspace.relationships ?? [], app.linkHealth, base, roles));
  const here = $derived(groupOf(all, table, via));

  const groups = $derived(
    all
      .map((g) => ({
        g,
        // A hidden column stays in the list only while this field uses it.
        cols: g.table.columns.filter(
          (c) =>
            isShown(c, here?.key === g.key && column === c.name) &&
            (!numbersOnly || c.dtype === 'number') &&
            (!datesOnly || c.dtype === 'date' || c.dtype === 'datetime') &&
            (!q ||
              c.name.toLowerCase().includes(q.toLowerCase()) ||
              c.label?.toLowerCase().includes(q.toLowerCase()) ||
              g.title.toLowerCase().includes(q.toLowerCase()) ||
              g.table.label.toLowerCase().includes(q.toLowerCase())),
        ),
      }))
      .filter((x) => x.cols.length || (mode === 'measure' && !q && !numbersOnly && !datesOnly)),
  );

  function pick(t: string, c: string | null, v?: string) {
    onchange(t, c, v);
    open = false;
    q = '';
  }

  function pickNamed(id: string) {
    onnamed?.(id);
    open = false;
    q = '';
  }

  /** The measures that fit the search. A measure is offered for a value, never for a group. */
  const offered = $derived(
    mode === 'measure' && onnamed && !numbersOnly && !datesOnly
      ? measures.filter((x) => !q || x.name.toLowerCase().includes(q.toLowerCase()))
      : [],
  );

  function outside(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }

  const dim = (d: DType) => d;
</script>

<svelte:window onmousedown={outside} onkeydown={(e) => open && e.key === 'Escape' && (open = false)} />

<div class="fp" bind:this={root}>
  <button class="trigger" class:empty={!table && !named} onclick={() => (open = !open)} aria-expanded={open}>
    {#if named}
      <span class="tbl fxm" aria-label="Measure"><Icon name="fx" size={12} /></span>
      <span class="col">{nameOf(named)}</span>
    {:else if table}
      <span class="tbl">{labelOfLink(via) ? `${labelOfLink(via)} ›` : current?.label}</span>
      <span class="col">{label}</span>
    {:else}
      <span class="ph">{placeholder}</span>
    {/if}
    <Icon name="chevronDown" size={12} />
  </button>
  {#if (table || named) && onclear}
    <button class="clear" aria-label="Remove field" onclick={onclear}><Icon name="x" size={12} /></button>
  {/if}
  {#if open}
    <div class="pop" role="listbox">
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" placeholder="Search fields" bind:value={q} autofocus />
      <div class="list">
        {#if offered.length}
          <div class="group">
            <div class="gh"><Icon name="fx" size={12} />Measures</div>
            {#each offered as x (x.id)}
              <button class="opt" class:on={named === x.id} title={x.description || x.formula} onclick={() => pickNamed(x.id)}>
                <Icon name="fx" size={12} />
                <span class="nm">{x.name}</span>
              </button>
            {/each}
          </div>
        {/if}
        {#each groups as { g, cols } (g.key)}
          {@const far = reachable && !reachable.has(g.table.id)}
          <div class="group" class:far>
            <div class="gh">
              <Icon name="table" size={12} />{g.title}
              {#if g.sub}<span class="sub">{g.sub}</span>{/if}
              {#if far}<span class="note">not related</span>{/if}
            </div>
            {#if mode === 'measure' && !q && !numbersOnly && !datesOnly}
              <button class="opt" onclick={() => pick(g.table.id, null, g.via)}>
                <Icon name="hash" size={12} /> Number of {g.table.label.toLowerCase()}
              </button>
            {/if}
            {#each cols as c (c.name)}
              <button class="opt" class:on={here?.key === g.key && column === c.name} title={[c.description, c.as_of ? `As of ${c.as_of}.` : ''].filter(Boolean).join(' ') || undefined} onclick={() => pick(g.table.id, c.name, g.via)}>
                <Icon name={typeIcon[dim(c.dtype)]} size={12} />
                <span class="nm">{c.label ?? c.name}</span>
                {#if c.calc}<span class="fx" title={c.error ? 'A calculated column that could not be computed' : 'A calculated column'}><Icon name="fx" size={11} /></span>{/if}
                {#if blankNote(c)}<span class="blank">{blankNote(c)}</span>{/if}
              </button>
            {/each}
          </div>
        {:else}
          {#if !offered.length}<p class="nothing">No field matches “{q}”.</p>{/if}
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .fp { position: relative; display: flex; gap: 4px; }
  .trigger {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: var(--r-control);
    background: var(--inset);
    text-align: left;
  }
  .trigger:hover { border-color: var(--sep-strong); }
  .trigger :global(svg) { margin-left: auto; color: var(--text-2); flex: none; }
  .tbl { color: var(--text-2); font-size: 12px; white-space: nowrap; }
  .tbl.fxm { display: inline-flex; color: var(--accent-text); }
  .trigger .tbl.fxm :global(svg) { margin-left: 0; color: var(--accent-text); }
  .col { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ph { color: var(--text-2); }
  .clear { width: 26px; border: 0; background: none; color: var(--text-2); border-radius: var(--r-control); display: grid; place-items: center; }
  .clear:hover { background: var(--hover); color: var(--text); }
  .pop {
    position: absolute;
    top: 36px;
    left: 0;
    right: 0;
    z-index: 20;
    padding: 8px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    animation: pop 200ms var(--spring);
    transform-origin: top center;
  }
  .list { max-height: 300px; overflow: auto; margin-top: 6px; }
  .group + .group { margin-top: 6px; }
  .group.far { opacity: 0.55; }
  .gh { display: flex; align-items: center; gap: 6px; padding: 4px 6px; font-size: 11px; font-weight: 600; color: var(--text-2); }
  .note { font-weight: 400; margin-left: auto; }
  .sub { font-weight: 400; color: var(--text-3); }
  .opt {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    height: 26px;
    padding: 0 8px 0 20px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
    font-size: 12px;
  }
  .opt :global(svg) { color: var(--text-2); }
  .opt:hover { background: var(--accent); color: #fff; }
  .opt:hover :global(svg) { color: #fff; }
  .opt.on { font-weight: 600; }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fx { display: inline-flex; flex: none; color: var(--text-2); }
  .opt:hover .fx, .opt:hover .fx :global(svg) { color: #fff; }
  .blank { margin-left: auto; flex: none; font-size: 11px; color: var(--text-2); }
  .opt:hover .blank { color: #fff; }
  .nothing { margin: 8px; color: var(--text-2); font-size: 12px; }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
</style>
