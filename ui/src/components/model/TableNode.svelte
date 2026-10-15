<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import type { Table } from '../../lib/api';
  import { typeIcon } from '../../lib/ui';
  import Icon from '../Icon.svelte';
  import { LINK_TABLE_LABEL } from '../../lib/linktables';

  let {
    data,
  }: {
    data: {
      table: Table;
      source: string;
      linked: Set<string>;
      /** The columns of the relationship that is picked: they light up. */
      hot?: Set<string>;
      warned: Set<string>;
      /** The table is marked as a link table: a tile may cross it (V4-D112). */
      linkTable?: boolean;
      /** Why the table looks like a link table, when it does and is not marked. */
      suggestion?: string | null;
      /** The table has two or more links that start from it, so it may be one: it can be marked by hand. */
      canMark?: boolean;
      onmark?: () => void;
      onunmark?: () => void;
      ondismiss?: () => void;
    };
  } = $props();
  const t = $derived(data.table);
</script>

<div class="node" class:link={data.linkTable}>
  <header>
    <Icon name="table" size={14} />
    <strong>{t.label}</strong>
    <span>{data.source}</span>
  </header>
  {#if data.linkTable}
    <div class="tag nodrag">
      <span class="chip" title="A chart can group by a table on one side of this table, through the other. A row that has two of them is under both, and the total counts it once.">{LINK_TABLE_LABEL}</span>
      <button class="x" aria-label="Unmark {t.label} as a link table" title="Unmark" onclick={() => data.onunmark?.()}><Icon name="x" size={11} /></button>
    </div>
  {:else if data.canMark && !data.suggestion}
    <div class="tag quiet nodrag">
      <button class="mark" onclick={() => data.onmark?.()} title="Mark this table as a link table: a chart can then group by one table on one side through the other.">Mark as link table</button>
    </div>
  {:else if data.suggestion}
    <div class="tag suggest nodrag" title={data.suggestion}>
      <span class="why">Looks like a link table</span>
      <button class="mark" onclick={() => data.onmark?.()}>Mark it</button>
      <button class="x" aria-label="Not a link table" title="Not a link table" onclick={() => data.ondismiss?.()}><Icon name="x" size={11} /></button>
    </div>
  {/if}
  <ul>
    {#each t.columns as c (c.name)}
      <li class:linked={data.linked.has(c.name)} class:hot={!!data.hot?.has(c.name)}>
        <Handle type="source" position={Position.Left} id={`L:${c.name}`} class="h" />
        <Icon name={typeIcon[c.dtype]} size={11} />
        <span class="c" title={c.label ? c.name : undefined}>{c.label ?? c.name}</span>
        {#if c.calc}<span class="fx" title="A calculated column"><Icon name="fx" size={11} /></span>{/if}
        {#if data.warned.has(c.name)}<span class="warn" title="This column repeats values. See the line."><Icon name="warning" size={11} /></span>{/if}
        <Handle type="source" position={Position.Right} id={`R:${c.name}`} class="h" />
      </li>
    {/each}
  </ul>
</div>

<style>
  .node {
    width: 240px;
    border: 1px solid var(--sep);
    border-radius: var(--r-card);
    background: var(--surface);
    box-shadow: 0 1px 3px rgb(0 0 0 / 6%);
    font-size: 12px;
    color: var(--text);
    transition: border-color 140ms var(--spring), box-shadow 140ms var(--spring);
  }
  :global(.svelte-flow__node.selected) .node, .node:hover { border-color: var(--sep-strong); }
  :global(.svelte-flow__node.dragging) .node { box-shadow: var(--shadow-float); }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    padding: 0 12px;
    border-bottom: 1px solid var(--sep);
    cursor: grab;
  }
  .node.link { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
  .tag { display: flex; align-items: center; gap: 6px; padding: 6px 12px; border-bottom: 1px solid var(--sep); font-size: 11px; }
  .chip { padding: 1px 8px; border-radius: 999px; background: var(--selected); color: var(--accent-text); font-weight: 600; }
  .tag.quiet { opacity: 0; height: 0; padding-top: 0; padding-bottom: 0; overflow: hidden; border-bottom-width: 0; transition: opacity 120ms; }
  .node:hover .tag.quiet, .node:focus-within .tag.quiet, :global(.svelte-flow__node.selected) .tag.quiet { opacity: 1; height: auto; padding-top: 6px; padding-bottom: 6px; border-bottom-width: 1px; }
  /* A touch screen has no pointer to rest on a table, so the switch is always there (V4-D124). */
  @media (hover: none) { .tag.quiet { opacity: 1; height: auto; padding-top: 6px; padding-bottom: 6px; border-bottom-width: 1px; } }
  .suggest { color: var(--text-2); }
  .suggest .why { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag .x { margin-left: auto; display: grid; place-items: center; width: 20px; height: 20px; border: 0; border-radius: 4px; background: none; color: var(--text-2); }
  .tag .x:hover { background: var(--hover); color: var(--text); }
  .suggest .x { margin-left: 0; }
  .mark { height: 22px; padding: 0 9px; border: 1px solid var(--accent); border-radius: 6px; background: none; color: var(--accent-text); font-size: 11px; font-weight: 600; }
  .mark:hover { background: var(--accent); color: #fff; }
  header :global(svg) { color: var(--accent-text); }
  strong { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  header span { margin-left: auto; font-size: 11px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 90px; }
  ul { list-style: none; margin: 0; padding: 4px 0; }
  li {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px;
    transition: background 120ms var(--spring);
  }
  li :global(svg) { color: var(--text-2); }
  li:hover { background: var(--hover); }
  li.linked .c { font-weight: 600; }
  li.hot { background: var(--selected); }
  li.hot .c, li.hot :global(svg) { color: var(--accent-text); }
  .c { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .warn { margin-left: auto; display: inline-flex; }
  li .warn :global(svg) { color: #c47400; }
  li :global(.h) {
    width: 9px;
    height: 9px;
    border: 2px solid var(--surface);
    background: var(--text-3);
    opacity: 0;
    transition: opacity 120ms;
  }
  li:hover :global(.h), li.linked :global(.h), :global(.svelte-flow__node.connecting) li :global(.h) { opacity: 1; }
  li.linked :global(.h) { background: var(--accent); }
</style>
