<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import { t } from '../lib/i18n';

  // Every sheet has the same parts: a header (title, an optional line under it, Close), the body, and a
  // footer with its actions on the right.
  let {
    title,
    subtitle,
    width = 520,
    flush = false,
    onclose,
    children,
    footer,
  }: {
    title: string;
    /** One line under the title: what the sheet is for. */
    subtitle?: string;
    width?: number;
    /** The body has no padding of its own: its rows run edge to edge. */
    flush?: boolean;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let dialog: HTMLDivElement;

  $effect(() => {
    const prev = document.activeElement as HTMLElement | null;
    // The first thing to type in, else the main button: never the Close button of the header.
    const first =
      dialog?.querySelector<HTMLElement>('.content input:not([type=hidden]):not([disabled]), .content select:not([disabled]), .content textarea:not([disabled])') ??
      dialog?.querySelector<HTMLElement>('button.primary') ??
      dialog?.querySelector<HTMLElement>('.content button, footer button');
    first?.focus();
    return () => prev?.focus?.();
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-label={title} style:width="{width}px" bind:this={dialog}>
    <header class:rule={flush}>
      <div class="titles">
        <h2>{title}</h2>
        {#if subtitle}<p>{subtitle}</p>{/if}
      </div>
      <button class="btn plain icon close" title={t('Close  Esc')} aria-label={t('Close')} onclick={onclose}><Icon name="x" /></button>
    </header>
    <div class="content" class:flush>{@render children()}</div>
    {#if footer}
      <footer>{@render footer()}</footer>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 32%);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 50;
    animation: fade 160ms ease-out;
  }
  .sheet {
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--r-sheet);
    box-shadow: 0 20px 60px rgb(0 0 0 / 22%), 0 0 0 1px rgb(0 0 0 / 6%);
    animation: pop 180ms var(--spring);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 16px 14px 12px 20px;
    flex: none;
  }
  header.rule { border-bottom: 1px solid var(--sep); }
  .titles { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; padding-top: 3px; }
  .titles h2 { font-size: 17px; font-weight: 600; letter-spacing: -0.01em; }
  .titles p { margin: 0; font-size: 12px; color: var(--text-2); line-height: 1.45; }
  .close { flex: none; color: var(--text-2); }
  .close:hover { color: var(--text); }
  .content { padding: 4px 20px 18px; overflow: auto; min-height: 0; }
  .content.flush { padding: 0; }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--sep);
    flex: none;
  }
  footer :global(.btn) { height: 30px; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
  @media (prefers-reduced-motion: reduce) {
    .scrim, .sheet { animation: none; }
  }
</style>
