<script lang="ts">
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast" class:error={t.kind === 'error'}>
      <Icon name={t.kind === 'error' ? 'warning' : 'check'} />
      <span>{t.text}</span>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 100;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 560px;
    padding: 9px 14px;
    border-radius: 10px;
    background: var(--raised);
    box-shadow: var(--shadow-float);
    color: var(--text);
    animation: rise 250ms var(--spring);
  }
  .toast.error :global(svg) { color: var(--danger); }
  .toast :global(svg) { color: var(--success); }
  @keyframes rise {
    from { opacity: 0; transform: translateY(6px) scale(0.98); }
  }
</style>
