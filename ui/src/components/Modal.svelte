<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    title,
    width = 520,
    onclose,
    children,
    footer,
  }: { title: string; width?: number; onclose: () => void; children: Snippet; footer?: Snippet } = $props();

  let dialog: HTMLDivElement;

  $effect(() => {
    const prev = document.activeElement as HTMLElement | null;
    // The first thing to type in, else the main button: never the Close button of the header.
    const first =
      dialog?.querySelector<HTMLElement>('.content input:not([type=hidden]), .content select, .content textarea') ??
      dialog?.querySelector<HTMLElement>('button.primary') ??
      dialog?.querySelector<HTMLElement>('.content button, footer button');
    first?.focus();
    return () => prev?.focus?.();
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-label={title} style:width="{width}px" bind:this={dialog}>
    <header>
      <h2>{title}</h2>
      <button class="btn plain icon" aria-label="Close" onclick={onclose}><Icon name="x" /></button>
    </header>
    <div class="content">{@render children()}</div>
    {#if footer}
      <footer>{@render footer()}</footer>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 28%);
    display: grid;
    place-items: center;
    z-index: 50;
    animation: fade 180ms ease-out;
  }
  .sheet {
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--r-sheet);
    box-shadow: var(--shadow-float);
    animation: pop 250ms var(--spring);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 14px 10px 20px;
  }
  .content { padding: 0 20px 16px; overflow: auto; }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--sep);
  }
  @keyframes fade { from { opacity: 0; } }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
</style>
