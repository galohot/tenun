<script lang="ts">
  // A text tile. The Markdown subset becomes plain text nodes, so nothing in it can run.
  import { parseMarkdown, type Inline } from '../../lib/markdown';

  let { text, editing = false }: { text: string; editing?: boolean } = $props();
  const blocks = $derived(parseMarkdown(text));
</script>

{#snippet inl(nodes: Inline[])}
  {#each nodes as n}
    {#if n.t === 'text'}{n.text}
    {:else if n.t === 'b'}<strong>{@render inl(n.children)}</strong>
    {:else if n.t === 'i'}<em>{@render inl(n.children)}</em>
    {:else if n.t === 'a'}<a href={n.href} target="_blank" rel="noopener noreferrer">{n.text}</a>
    {:else}<br />
    {/if}
  {/each}
{/snippet}

<div class="text">
  {#if !blocks.length && editing}
    <p class="empty">Write something in the panel on the right.</p>
  {/if}
  {#each blocks as b}
    {#if b.t === 'h1'}<h4 class="h1">{@render inl(b.children)}</h4>
    {:else if b.t === 'h2'}<h5 class="h2">{@render inl(b.children)}</h5>
    {:else if b.t === 'ul'}
      <ul>
        {#each b.items as item}<li>{@render inl(item)}</li>{/each}
      </ul>
    {:else}<p>{@render inl(b.children)}</p>
    {/if}
  {/each}
</div>

<style>
  /* The fixed type scale, in text colours. */
  .text { height: 100%; overflow: auto; font-size: 13px; line-height: 1.5; color: var(--text); }
  p, ul { margin: 0 0 8px; }
  h4, h5 { margin: 0 0 6px; font-weight: 600; letter-spacing: -0.01em; }
  .h1 { font-size: 17px; }
  .h2 { font-size: 14px; }
  ul { padding-left: 18px; }
  li { margin: 0 0 2px; }
  a { color: var(--accent-text); }
  .text > :last-child { margin-bottom: 0; }
  .empty { color: var(--text-2); }
</style>
