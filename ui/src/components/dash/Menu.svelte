<script lang="ts">
  // A small menu at a place on the screen: under a button, or where the pointer was on a right-click.
  import { portal } from '../../lib/portal';

  let {
    x,
    y,
    items,
    onclose,
  }: {
    x: number;
    y: number;
    items: { label: string; run: () => void }[];
    onclose: () => void;
  } = $props();

  // Keep it inside the window.
  const left = $derived(Math.max(8, Math.min(x, innerWidth - 188)));
  const top = $derived(Math.max(8, Math.min(y, innerHeight - 8 - items.length * 30)));
</script>

<svelte:window
  onpointerdown={(e) => !(e.target as HTMLElement).closest('.menu-pop') && onclose()}
  onkeydown={(e) => e.key === 'Escape' && onclose()}
  onblur={onclose}
  onresize={onclose} />

<div class="menu-pop" role="menu" use:portal style:left="{left}px" style:top="{top}px">
  {#each items as it}
    <button
      role="menuitem"
      onclick={() => {
        it.run();
        onclose();
      }}>
      {it.label}
    </button>
  {/each}
</div>

<style>
  .menu-pop {
    position: fixed;
    z-index: 70;
    width: 180px;
    padding: 5px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    animation: pop 160ms var(--spring);
    transform-origin: top left;
  }
  button {
    display: flex;
    align-items: center;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
    font-size: 13px;
  }
  button:hover { background: var(--accent); color: #fff; }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
</style>
