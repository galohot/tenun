<script lang="ts">
  // A story page (build 11, V4-D13): its tiles in a centred column, in the page's order. A text is as tall as its
  // words, a chart or a number takes its height; two half-width tiles share a row on a wide page and stack on a narrow
  // one. The frame of each tile comes from the caller (`cell`): the app, the exported page and the picture each have
  // their own. When editing, "+" between rows adds a tile there, and a handle beside a tile drags it to a new place.
  import type { Snippet } from 'svelte';
  import type { Tile } from '../../lib/api';
  import { FLOW_GAP, FLOW_PAIR_FROM, flowHeight, flowRows, flowWidth, rowWidth } from '../../lib/flow';
  import Icon from '../Icon.svelte';

  let {
    tiles,
    editing = false,
    cell,
    onadd,
    onmove,
  }: {
    /** In the page's order (`flowOrder`). */
    tiles: Tile[];
    editing?: boolean;
    cell: Snippet<[Tile]>;
    /** "+" at a place in the order: the caller opens its menu of tiles there. */
    onadd?: (at: number, e: MouseEvent) => void;
    onmove?: (from: number, to: number) => void;
  } = $props();

  let width = $state(1000);
  const rows = $derived(flowRows(tiles, width >= FLOW_PAIR_FROM));
  /** Where each row starts in the order. */
  const starts = $derived(rows.reduce<number[]>((acc, r, i) => [...acc, i ? acc[i - 1] + rows[i - 1].length : 0], []));
  let dragFrom = $state<number | null>(null);
  let over = $state<number | null>(null);
</script>

<div class="flow" class:editing bind:clientWidth={width} style:--gap="{FLOW_GAP}px">
  {#if editing && onadd}
    <button class="plus" title="Add a tile here" aria-label="Add a tile at the top" onclick={(e) => onadd(0, e)}><Icon name="plus" size={12} /></button>
  {/if}
  {#each rows as row, r (row.map((t) => t.id).join('+'))}
    <div class="row" class:pair={row.length === 2} style:max-width="{rowWidth(row)}px">
      {#each row as t, k (t.id)}
        {@const i = starts[r] + k}
        {@const h = flowHeight(t)}
        <div
          class="slot"
          class:half={flowWidth(t) === 'half' && row.length === 1 && width >= FLOW_PAIR_FROM}
          class:over={over === i && dragFrom !== null && dragFrom !== i}
          style:height={h === null ? undefined : `${h}px`}
          role="listitem"
          ondragover={(e) => {
            if (dragFrom === null) return;
            e.preventDefault();
            over = i;
          }}
          ondrop={(e) => {
            e.preventDefault();
            if (dragFrom !== null && onmove) onmove(dragFrom, i);
            dragFrom = null;
            over = null;
          }}>
          {#if editing && onmove}
            <span
              class="handle"
              draggable="true"
              role="button"
              tabindex="-1"
              title="Drag to move"
              aria-label="Drag to move"
              ondragstart={() => (dragFrom = i)}
              ondragend={() => ((dragFrom = null), (over = null))}><Icon name="grip" size={14} /></span>
          {/if}
          {@render cell(t)}
        </div>
      {/each}
    </div>
    {#if editing && onadd}
      <button class="plus" title="Add a tile here" aria-label="Add a tile here" onclick={(e) => onadd(starts[r] + row.length, e)}><Icon name="plus" size={12} /></button>
    {/if}
  {/each}
</div>

<style>
  .flow { display: flex; flex-direction: column; align-items: center; gap: var(--gap); padding-bottom: 40px; }
  .row { width: 100%; display: flex; gap: var(--gap); }
  .slot { position: relative; flex: 1; min-width: 0; }
  .slot.half { flex: 0 0 calc(50% - var(--gap) / 2); }
  .slot.over { outline: 2px dashed var(--accent); outline-offset: 2px; border-radius: var(--r-card); }
  .handle { position: absolute; left: -22px; top: 8px; z-index: 2; color: var(--text-3); cursor: grab; opacity: 0; display: flex; }
  .slot:hover .handle, .handle:focus-visible { opacity: 1; }
  .plus { width: 22px; height: 22px; margin: calc(var(--gap) / -2) 0; padding: 0; display: flex; align-items: center; justify-content: center; border: 1px solid var(--sep); border-radius: 11px; background: var(--surface); color: var(--text-2); opacity: 0; transition: opacity 120ms; }
  .flow:hover .plus, .plus:focus-visible { opacity: 1; }
  .plus:hover { color: var(--accent-text); border-color: var(--accent); }
</style>
