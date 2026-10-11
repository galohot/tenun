<script lang="ts">
  import { BaseEdge, EdgeLabel, getBezierPath, type EdgeProps } from '@xyflow/svelte';
  import Icon from '../Icon.svelte';

  let { sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data, selected }: EdgeProps = $props();

  interface Data {
    suggestion?: boolean;
    reason?: string;
    onaccept?: () => void;
    ondismiss?: () => void;
    onremove?: () => void;
    /** Opens the sheet of the link. */
    onopen?: () => void;
    /** Pixels to move the label up or down, so two lines between the same tables keep their labels apart. */
    dy?: number;
    /** The name of the link, when it has one. */
    name?: string;
    /** "1 – many", "1 – 1": what the data says the link is. */
    kind?: string;
    /** The warning in plain words, when the data has something to say. */
    note?: string;
    /** Two links join these tables and some has no name. */
    needsName?: boolean;
    /** The link is not used. */
    unusable?: boolean;
  }

  const geo = $derived(getBezierPath({ sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition }));
  const d = $derived(data as Data);
  /** What a person who cannot see the chip hears, and what a long name shows when it is hovered: the name, the kind and the warning. */
  const said = $derived(
    d.needsName
      ? `Name these links. ${d.kind ?? ''}${d.note ? `. Warning: ${d.note}` : ''}`
      : `${d.name ?? 'Link'}, ${d.kind ?? ''}${d.unusable ? ', not used' : ''}${d.note ? `. Warning: ${d.note}` : ''}`,
  );
</script>

<BaseEdge
  path={geo[0]}
  class={d.suggestion ? 'rel suggest' : d.unusable ? 'rel off' : selected ? 'rel on' : d.note ? 'rel warn' : 'rel'}
  markerEnd={d.suggestion ? undefined : 'url(#tenun-one)'} />

<EdgeLabel x={geo[1]} y={geo[2] + (d.dy ?? 0)} class="nodrag nopan" transparent>
  {#if d.suggestion}
    <div class="pill" title={d.reason}>
      <button class="ok" onclick={d.onaccept}><Icon name="check" size={12} /> Link</button>
      <button class="no" aria-label="Dismiss suggestion" onclick={d.ondismiss}><Icon name="x" size={12} /></button>
    </div>
  {:else}
    <div class="pill chip" class:on={selected} class:warn={!!d.note} class:needs={d.needsName}>
      <button class="open" onclick={d.onopen} title={said} aria-label={`Open this link: ${said}`}>
        {#if d.note}<Icon name="warning" size={12} />{/if}
        {#if d.needsName}
          <span class="name needs">Name these links</span>
        {:else if d.name}
          <span class="name">{d.name}</span>
        {/if}
        <span class="kind">{d.kind}</span>
      </button>
      {#if selected}
        <button class="no" onclick={d.onremove}><Icon name="trash" size={12} /> Remove</button>
      {/if}
    </div>
  {/if}
</EdgeLabel>

<style>
  :global(.svelte-flow__edge-path.rel) { stroke: var(--text-2); stroke-width: 1.6; }
  :global(.svelte-flow__edge-path.rel.on) { stroke: var(--accent); stroke-width: 2; }
  :global(.svelte-flow__edge-path.rel.warn) { stroke: #c47400; }
  :global(.svelte-flow__edge-path.rel.off) { stroke: var(--text-3); stroke-dasharray: 2 4; }
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
  .chip { max-width: 200px; }
  .chip.on { box-shadow: 0 0 0 1.5px var(--accent), var(--shadow-float); }
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
  .open { min-width: 0; }
  .open:hover { background: var(--hover); }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .name.needs { color: var(--accent-text); }
  .kind { flex: none; font-size: 11px; font-weight: 500; color: var(--text-2); }
  .warn .open :global(svg) { flex: none; color: #c47400; }
  .ok { background: var(--accent); color: #fff; }
  .no { color: var(--text-2); padding: 0 6px; }
  .no:hover { background: var(--hover); color: var(--text); }
</style>
