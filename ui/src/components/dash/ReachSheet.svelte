<script lang="ts">
  // Which tiles a click on this tile filters (v5 build 02, V5-D37): all of them (as always), none, or the ones ticked.
  // A filter tile's selection follows the same choice. "Keep only" and "Exclude" still filter every tile.
  import type { Tile } from '../../lib/api';
  import Modal from '../Modal.svelte';

  let {
    tile,
    tiles,
    titleOf,
    onsave,
    onclose,
  }: {
    tile: Tile;
    /** The other tiles of the page. */
    tiles: Tile[];
    titleOf: (t: Tile) => string;
    onsave: (reach: Tile['reach']) => void;
    onclose: () => void;
  } = $props();

  const others = $derived(tiles.filter((t) => t.id !== tile.id && t.type !== 'text' && t.type !== 'image'));
  // The sheet starts from the tile's reach and keeps its own draft until Done.
  // svelte-ignore state_referenced_locally
  let mode = $state<'all' | 'none' | 'some'>(tile.reach === undefined ? 'all' : tile.reach === 'none' ? 'none' : 'some');
  // svelte-ignore state_referenced_locally
  let picked = $state<string[]>(Array.isArray(tile.reach) ? [...tile.reach] : []);

  function save() {
    onsave(mode === 'all' ? undefined : mode === 'none' ? 'none' : others.filter((t) => picked.includes(t.id)).map((t) => t.id));
    onclose();
  }
</script>

<Modal title="Filters other tiles" width={460} {onclose}>
  <p class="intro">What a click on <b>{titleOf(tile) || 'this tile'}</b>{tile.type === 'filter' ? ', or a choice in it,' : ''} filters on this page.</p>
  <div class="choices" role="radiogroup" aria-label="Which tiles it filters">
    <label><input type="radio" name="reach" value="all" bind:group={mode} /> Every tile</label>
    <label><input type="radio" name="reach" value="none" bind:group={mode} /> No tile</label>
    <label><input type="radio" name="reach" value="some" bind:group={mode} /> These tiles</label>
  </div>
  {#if mode === 'some'}
    <ul class="tiles">
      {#each others as t (t.id)}
        <li>
          <label>
            <input
              type="checkbox"
              checked={picked.includes(t.id)}
              onchange={(e) => (picked = e.currentTarget.checked ? [...picked, t.id] : picked.filter((x) => x !== t.id))} />
            {titleOf(t) || 'Untitled'}
          </label>
        </li>
      {:else}
        <li class="muted">No other tile on this page.</li>
      {/each}
    </ul>
  {/if}
  <p class="hint">"Keep only" and "Exclude" still filter every tile.</p>

  {#snippet footer()}
    <button class="btn plain" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={save}>Done</button>
  {/snippet}
</Modal>

<style>
  .intro { margin: 0 0 12px; font-size: 13px; color: var(--text-2); }
  .choices { display: flex; flex-direction: column; gap: 6px; margin-bottom: 10px; font-size: 13px; }
  .choices label, .tiles label { display: flex; align-items: center; gap: 8px; }
  .tiles { list-style: none; margin: 0 0 10px 24px; padding: 0; display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .muted { color: var(--text-2); }
  .hint { margin: 0; font-size: 11px; color: var(--text-2); }
</style>
