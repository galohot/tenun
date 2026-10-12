<script lang="ts">
  // A text tile. The Markdown subset becomes plain text nodes, so nothing in it can run. Build 10 adds a third heading,
  // numbered lists, quotes, callouts (a tinted block with an icon and a word), a line across, links to another
  // dashboard, and live numbers: a named measure's value, change or period, worked out by the engine (`values`).
  import type { Cell, Tile } from '../../lib/api';
  import { parseMarkdown, type Block, type CalloutKind, type Inline } from '../../lib/markdown';
  import { compact, pctChange, tip } from '../../lib/format';
  import { formatOf, isKnown, nameOf, namedMeasure } from '../../lib/measures';
  import { changeTone } from '../../lib/card';
  import Icon, { type IconName } from '../Icon.svelte';

  let {
    text,
    editing = false,
    values,
    style = 'body',
    align = 'start',
    pages,
    onpage,
  }: {
    text: string;
    editing?: boolean;
    /** The live numbers, by `<id>`, `<id>|compare`, `<id>|period`. Not set yet: they are on their way. */
    values?: Record<string, Cell>;
    style?: Tile['textStyle'];
    align?: Tile['textAlign'];
    /** The dashboards a page link may open. Not set (an exported page): a page link is plain text. */
    pages?: ReadonlySet<string>;
    onpage?: (id: string) => void;
  } = $props();
  const blocks = $derived(parseMarkdown(text));

  const CALLOUT: Record<CalloutKind, { icon: IconName; word: string }> = {
    note: { icon: 'info', word: 'Note' },
    good: { icon: 'check', word: 'Good' },
    warn: { icon: 'warning', word: 'Warning' },
  };

  /** The words of a live number, and what its tooltip says. */
  function number(id: string, part: 'value' | 'compare' | 'period'): { text: string; title: string; missing: boolean } {
    if (!isKnown(id)) return { text: '—', title: 'This measure was deleted', missing: true };
    const name = nameOf(id);
    if (!values) return { text: '…', title: name, missing: false };
    const v = values[part === 'value' ? id : `${id}|${part}`];
    if (part === 'period') return { text: typeof v === 'string' ? v : '—', title: `${name}: the period`, missing: v == null };
    if (typeof v !== 'number') return { text: '—', title: part === 'compare' ? `${name}: nothing to compare with` : name, missing: true };
    if (part === 'compare') {
      const tone = changeTone(v);
      return { text: tone === 'flat' ? '≈ Same' : `${v > 0 ? '▲' : '▼'} ${pctChange(v)}`, title: `${name}: the change against the period before`, missing: false };
    }
    const fmt = formatOf({ measures: [namedMeasure(id)] }, 0);
    return { text: compact(v, fmt), title: `${name}: ${tip(v, fmt)}`, missing: false };
  }

  const firstPara = $derived(blocks.findIndex((b) => b.t === 'p'));
  const isLead = (b: Block, i: number) => style === 'lead' && b.t === 'p' && i === firstPara;
</script>

{#snippet inl(nodes: Inline[])}
  {#each nodes as n}
    {#if n.t === 'text'}{n.text}
    {:else if n.t === 'b'}<strong>{@render inl(n.children)}</strong>
    {:else if n.t === 'i'}<em>{@render inl(n.children)}</em>
    {:else if n.t === 'a'}<a href={n.href} target="_blank" rel="noopener noreferrer">{n.text}</a>
    {:else if n.t === 'page'}
      {#if onpage && pages?.has(n.page)}
        <a href="#/d/{n.page}" class="page" onclick={(e) => { e.preventDefault(); onpage(n.page); }}>{n.text}</a>
      {:else}{n.text}
      {/if}
    {:else if n.t === 'num'}
      {@const x = number(n.id, n.part)}
      <span class="num" class:missing={x.missing} title={x.title}>{x.text}</span>
    {:else}<br />
    {/if}
  {/each}
{/snippet}

<div class="text {style}" class:center={align === 'center'}>
  {#if !blocks.length && editing}
    <p class="empty">Write something in the panel on the right.</p>
  {/if}
  {#each blocks as b, i}
    {#if b.t === 'h1'}<h4 class="h1">{@render inl(b.children)}</h4>
    {:else if b.t === 'h2'}<h5 class="h2">{@render inl(b.children)}</h5>
    {:else if b.t === 'h3'}<h6 class="h3">{@render inl(b.children)}</h6>
    {:else if b.t === 'ul'}
      <ul>
        {#each b.items as item}<li>{@render inl(item)}</li>{/each}
      </ul>
    {:else if b.t === 'ol'}
      <ol start={b.start}>
        {#each b.items as item}<li>{@render inl(item)}</li>{/each}
      </ol>
    {:else if b.t === 'quote'}<blockquote>{@render inl(b.children)}</blockquote>
    {:else if b.t === 'callout'}
      <div class="callout {b.kind}" role="note">
        <span class="word"><Icon name={CALLOUT[b.kind].icon} size={13} /> {CALLOUT[b.kind].word}</span>
        <div>{@render inl(b.children)}</div>
      </div>
    {:else if b.t === 'hr'}<hr />
    {:else}<p class:lead={isLead(b, i)}>{@render inl(b.children)}</p>
    {/if}
  {/each}
</div>

<style>
  /* The fixed type scale, in text colours. */
  .text { height: 100%; overflow: auto; font-size: 13px; line-height: 1.5; color: var(--text); }
  .text.note { font-size: 12px; color: var(--text-2); }
  .center { text-align: center; }
  p, ul, ol, blockquote, .callout { margin: 0 0 8px; }
  h4, h5, h6 { margin: 0 0 6px; font-weight: 600; letter-spacing: -0.01em; }
  .h1 { font-size: 17px; }
  .h2 { font-size: 14px; }
  .h3 { font-size: 13px; color: var(--text-2); }
  .lead { font-size: 15px; line-height: 1.45; }
  ul, ol { padding-left: 18px; }
  .center ul, .center ol { list-style-position: inside; padding-left: 0; }
  li { margin: 0 0 2px; }
  a { color: var(--accent-text); }
  blockquote { padding: 2px 0 2px 10px; border-left: 3px solid var(--sep-strong); color: var(--text-2); }
  .center blockquote { border-left: 0; padding-left: 0; }
  /* A callout is a tinted block with an icon and a word: the colour only backs them up. */
  .callout { padding: 8px 10px; border-radius: 8px; text-align: left; }
  .callout .word { display: flex; align-items: center; gap: 4px; font-size: 12px; font-weight: 600; margin-bottom: 2px; }
  .callout.note { background: color-mix(in srgb, var(--accent) 10%, var(--surface)); }
  .callout.note .word :global(svg) { color: var(--accent-text); }
  .callout.good { background: color-mix(in srgb, var(--success) 12%, var(--surface)); }
  .callout.good .word :global(svg) { color: var(--success); }
  .callout.warn { background: color-mix(in srgb, var(--warn) 14%, var(--surface)); }
  .callout.warn .word :global(svg) { color: var(--warn); }
  hr { border: 0; border-top: 1px solid var(--sep); margin: 10px 0; }
  /* A live number reads as a value, with a dotted line under it: point at it for its measure. */
  .num { font-weight: 600; font-variant-numeric: tabular-nums; border-bottom: 1px dotted var(--text-3); cursor: help; }
  .num.missing { color: var(--text-2); }
  .text > :last-child { margin-bottom: 0; }
  .empty { color: var(--text-2); }
</style>
