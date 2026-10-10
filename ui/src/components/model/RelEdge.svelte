<script lang="ts">
  import { BaseEdge, EdgeLabel, getBezierPath, type EdgeProps } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, selected }: EdgeProps = $props();

  const geo = $derived(getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition }));
  const d = $derived(data as { suggestion?: boolean; reason?: string; onaccept?: () => void; ondismiss?: () => void; onremove?: () => void });
</script>

<BaseEdge
  path={geo[0]}
  class={d.suggestion ? 'rel suggest' : selected ? 'rel on' : 'rel'}
  markerEnd={d.suggestion ? undefined : 'url(#tenun-one)'} />

{#if d.suggestion || selected}
<EdgeLabel x={geo[1]} y={geo[2]} class="nodrag nopan" transparent>
  {#if d.suggestion}
    <div class="pill" title={d.reason}>
      <button class="ok" onclick={d.onaccept}><Icon name="check" size={12} /> Link</button>
      <button class="no" aria-label="Dismiss suggestion" onclick={d.ondismiss}><Icon name="x" size={12} /></button>
    </div>
  {:else if selected}
    <div class="pill">
      <button class="no" onclick={d.onremove}><Icon name="trash" size={12} /> Remove</button>
    </div>
  {/if}
</EdgeLabel>
{/if}

<style>
  :global(.svelte-flow__edge-path.rel) { stroke: var(--text-2); stroke-width: 1.6; }
  :global(.svelte-flow__edge-path.rel.on) { stroke: var(--accent); stroke-width: 2; }
  :global(.svelte-flow__edge-path.rel.suggest) { stroke: var(--accent); stroke-dasharray: 5 4; stroke-width: 1.6; }
  .pill {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: 14px;
    background: var(--raised);
    box-shadow: var(--shadow-float);
    pointer-events: all;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 9px;
    border: 0;
    border-radius: 11px;
    background: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
  }
  .ok { background: var(--accent); color: #fff; }
  .no { color: var(--text-2); padding: 0 6px; }
  .no:hover { background: var(--hover); color: var(--text); }
</style>
