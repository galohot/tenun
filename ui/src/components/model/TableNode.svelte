<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import type { Table } from '../../lib/api';
  import { typeIcon } from '../../lib/ui';
  import Icon from '../Icon.svelte';

  let { data }: { data: { table: Table; source: string; linked: Set<string> } } = $props();
  const t = $derived(data.table);
</script>

<div class="node">
  <header>
    <Icon name="table" size={14} />
    <strong>{t.label}</strong>
    <span>{data.source}</span>
  </header>
  <ul>
    {#each t.columns as c (c.name)}
      <li class:linked={data.linked.has(c.name)}>
        <Handle type="source" position={Position.Left} id={`L:${c.name}`} class="h" />
        <Icon name={typeIcon[c.dtype]} size={11} />
        <span class="c">{c.name}</span>
        <Handle type="source" position={Position.Right} id={`R:${c.name}`} class="h" />
      </li>
    {/each}
  </ul>
</div>

<style>
  .node {
    width: 220px;
    border-radius: var(--r-card);
    background: var(--surface);
    box-shadow: var(--shadow-float);
    font-size: 12px;
    color: var(--text);
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 9px 12px;
    border-bottom: 1px solid var(--sep);
    cursor: grab;
  }
  header :global(svg) { color: var(--accent-text); }
  strong { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  header span { margin-left: auto; font-size: 11px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 90px; }
  ul { list-style: none; margin: 0; padding: 4px 0; }
  li {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 12px;
  }
  li :global(svg) { color: var(--text-2); }
  li:hover { background: var(--hover); }
  li.linked .c { font-weight: 600; }
  .c { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
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
