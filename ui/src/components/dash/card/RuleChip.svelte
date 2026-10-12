<script lang="ts">
  // The word a matching rule adds beside the value ("Below target"), with an icon. The colour only backs them up.
  import type { CardTone } from '../../../lib/card';
  import Icon from '../../Icon.svelte';

  let { tone, words }: { tone: CardTone; words: string } = $props();
</script>

<span class="chip {tone}"><Icon name={tone === 'good' ? 'check' : 'warning'} size={12} />{words}</span>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 20px;
    padding: 0 7px 0 5px;
    border-radius: 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
  }
  .good { background: color-mix(in srgb, var(--success) 16%, var(--surface)); }
  .good :global(svg) { color: var(--success); }
  .bad { background: color-mix(in srgb, var(--danger) 14%, var(--surface)); }
  .bad :global(svg) { color: var(--danger); }
  .warn { background: color-mix(in srgb, var(--warn) 16%, var(--surface)); }
  .warn :global(svg) { color: var(--warn); }
</style>
