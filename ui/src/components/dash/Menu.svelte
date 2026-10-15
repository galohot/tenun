<script lang="ts">
  // A small menu at a place on the screen: under a button, or where the pointer was on a right-click.
  // An item can hold a few more items (`sub`) that open under it. Arrow keys move, Enter picks,
  // Right and Left open and close a group, and Esc closes the menu and gives the focus back.
  import { onMount } from 'svelte';
  import { portal } from '../../lib/portal';
  import Icon from '../Icon.svelte';

  interface Item {
    label: string;
    run?: () => void;
    sub?: Item[];
    disabled?: boolean;
  }

  let {
    x,
    y,
    items,
    onclose,
  }: {
    x: number;
    y: number;
    items: Item[];
    onclose: () => void;
  } = $props();

  let open = $state<number | null>(null);
  let el: HTMLDivElement;

  // Keep it inside the window.
  const rows = $derived(items.length + (open === null ? 0 : (items[open].sub?.length ?? 0)));
  const left = $derived(Math.max(8, Math.min(x, innerWidth - 208)));
  const top = $derived(Math.max(8, Math.min(y, innerHeight - 16 - rows * 30)));

  // The element that had the focus gets it back when the menu goes.
  onMount(() => {
    const before = document.activeElement as HTMLElement | null;
    buttons()[0]?.focus();
    return () => before?.focus?.();
  });

  const buttons = () => [...el.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];

  function choose(it: Item, i: number) {
    if (it.sub) {
      open = open === i ? null : i;
      return;
    }
    it.run?.();
    onclose();
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
      return;
    }
    const all = buttons();
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const next = (at + (e.key === 'ArrowDown' ? 1 : -1) + all.length) % all.length;
      all[next]?.focus();
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      all[e.key === 'Home' ? 0 : all.length - 1]?.focus();
    } else if (e.key === 'ArrowRight') {
      const i = Number((document.activeElement as HTMLElement | null)?.dataset.group);
      if (Number.isInteger(i) && open !== i) {
        e.preventDefault();
        open = i;
        queueMicrotask(() => buttons()[at + 1]?.focus());
      }
    } else if (e.key === 'ArrowLeft' && open !== null) {
      e.preventDefault();
      const group = open;
      open = null;
      queueMicrotask(() => el.querySelector<HTMLButtonElement>(`[data-group="${group}"]`)?.focus());
    }
  }
</script>

<svelte:window
  onpointerdown={(e) => !(e.target as HTMLElement).closest('.menu-pop') && onclose()}
  onblur={onclose}
  onresize={onclose} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="menu-pop" role="menu" tabindex="-1" bind:this={el} onkeydown={onkey} use:portal style:left="{left}px" style:top="{top}px">
  {#each items as it, i}
    <button
      role="menuitem"
      disabled={it.disabled}
      data-group={it.sub ? i : undefined}
      aria-haspopup={it.sub ? 'menu' : undefined}
      aria-expanded={it.sub ? open === i : undefined}
      onclick={() => choose(it, i)}>
      <span>{it.label}</span>
      {#if it.sub}<Icon name={open === i ? 'chevronDown' : 'chevronRight'} size={12} />{/if}
    </button>
    {#if it.sub && open === i}
      {#each it.sub as s}
        <button role="menuitem" class="sub" disabled={s.disabled} onclick={() => choose(s, -1)}>{s.label}</button>
      {/each}
    {/if}
  {/each}
</div>

<style>
  .menu-pop {
    position: fixed;
    z-index: 70;
    width: 200px;
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
    justify-content: space-between;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
    font-size: 13px;
    transition: background 120ms var(--spring);
  }
  button.sub { padding-left: 22px; }
  button :global(svg) { color: var(--text-2); }
  button:hover:not(:disabled), button:focus-visible { background: color-mix(in srgb, var(--text) 8%, var(--raised)); outline: none; }
  button:hover:not(:disabled) :global(svg), button:focus-visible :global(svg) { color: var(--text); }
  button:disabled { color: var(--text-3); cursor: default; }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
</style>
