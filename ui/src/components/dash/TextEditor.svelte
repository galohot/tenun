<script lang="ts">
  // The text of a text tile, as blocks (build 10, V4-D12): each block has a kind (text, a heading, a list, a quote, a
  // callout, a line) and its words. "/" in an empty block, or the kind button, opens the block menu; "+" between blocks
  // adds one; blocks drag to a new place. Bold, italic, a link (to the web or to a page) and a live number go in around
  // the selection, with Ctrl or ⌘ B, I and K too. The words keep their marks as they are written, so what is typed is
  // what is saved; "Edit as text" shows the Markdown itself.
  import { tick } from 'svelte';
  import type { Tile } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { safeUrl, type NumPart } from '../../lib/markdown';
  import { addLink, convert, fromBlocks, insert, insertAt, KINDS, merge, move, numberToken, ONE_LINE, opensMenu, remove, split, toBlocks, toggleMark, type EdBlock, type EdKind, type Edit } from '../../lib/texteditor';
  import Icon from '../Icon.svelte';
  import { t } from '../../lib/i18n';

  let { tile, set }: { tile: Tile; set: (patch: Partial<Tile>) => void } = $props();

  const SHORT: Record<EdKind, string> = { p: t('Text'), h1: 'H1', h2: 'H2', h3: 'H3', ul: t('List'), ol: t('1. List'), quote: t('Quote'), note: t('Note'), good: t('Good'), warn: t('Warn'), hr: t('Line') };
  const HINT: Partial<Record<EdKind, string>> = { p: t('Write, or type / for a block'), ul: t('One item on each line'), ol: t('One item on each line'), quote: t('A quote'), note: t('A note'), good: t('Good news'), warn: t('A warning') };

  let raw = $state(false);
  let blocks = $state<EdBlock[]>([]);
  /** The text this editor wrote last: a different one came from elsewhere (undo, another window). */
  let written = '';
  let lastId = '';
  $effect(() => {
    const text = tile.text ?? '';
    if (tile.id !== lastId || text !== written) {
      lastId = tile.id;
      written = text;
      const b = toBlocks(text);
      blocks = b.length ? b : [{ kind: 'p', text: '' }];
    }
  });

  function commit(next: EdBlock[]) {
    blocks = next.length ? next : [{ kind: 'p', text: '' }];
    written = fromBlocks(blocks);
    set({ text: written });
  }

  // The textareas, to read and set the selection.
  let els = $state<(HTMLTextAreaElement | null)[]>([]);
  let current = $state(0);
  async function focusAt(i: number, caret?: number) {
    await tick();
    const el = els[i];
    if (!el) return;
    el.focus();
    const at = caret ?? el.value.length;
    el.setSelectionRange(at, at);
    grow(el);
  }
  function grow(el: HTMLTextAreaElement) {
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
  }
  const growing = (el: HTMLTextAreaElement) => {
    grow(el);
  };

  // The block menu: change a block's kind, or add a block of a kind.
  let menu = $state<{ at: number; mode: 'convert' | 'insert' } | null>(null);
  function pick(kind: EdKind) {
    if (!menu) return;
    const { at, mode } = menu;
    menu = null;
    if (mode === 'convert') {
      const next = convert(blocks, at, kind);
      // "/" opened it: the slash goes.
      if (opensMenu(blocks[at])) next[at] = { ...next[at], text: '' };
      commit(next);
      if (kind !== 'hr') focusAt(at);
    } else {
      commit(insert(blocks, at, kind));
      if (kind !== 'hr') focusAt(at);
    }
  }

  function typed(i: number, text: string, el: HTMLTextAreaElement) {
    const next = blocks.map((b, j) => (j === i ? { ...b, text } : b));
    grow(el);
    commit(next);
    if (opensMenu(next[i])) menu = { at: i, mode: 'convert' };
  }

  function apply(i: number, e: Edit) {
    commit(blocks.map((b, j) => (j === i ? { ...b, text: e.text } : b)));
    tick().then(() => {
      const el = els[i];
      if (!el) return;
      el.focus();
      el.setSelectionRange(e.start, e.end);
      grow(el);
    });
  }

  const selection = (i: number) => {
    const el = els[i];
    return { start: el?.selectionStart ?? blocks[i]?.text.length ?? 0, end: el?.selectionEnd ?? blocks[i]?.text.length ?? 0 };
  };
  function mark(m: '**' | '*') {
    const i = current;
    if (!blocks[i] || blocks[i].kind === 'hr') return;
    const s = selection(i);
    apply(i, toggleMark(blocks[i].text, s.start, s.end, m));
  }

  function onkey(e: KeyboardEvent, i: number) {
    const el = e.currentTarget as HTMLTextAreaElement;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && !e.shiftKey && !e.altKey && ['b', 'i', 'k'].includes(e.key.toLowerCase())) {
      e.preventDefault();
      const k = e.key.toLowerCase();
      if (k === 'b') mark('**');
      else if (k === 'i') mark('*');
      else openLink();
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey && ONE_LINE.includes(blocks[i].kind)) {
      e.preventDefault();
      commit(split(blocks, i, el.selectionStart));
      focusAt(i + 1, 0);
      return;
    }
    if (e.key === 'Backspace' && el.selectionStart === 0 && el.selectionEnd === 0 && i > 0) {
      e.preventDefault();
      // A line across above is taken away and this block moves up; any other block above takes these words.
      const m = merge(blocks, i);
      commit(m.blocks);
      focusAt(i - 1, m.caret);
      return;
    }
    if (e.key === 'Escape' && menu) {
      e.preventDefault();
      menu = null;
    }
  }

  // Drag a block by its handle.
  let dragFrom = $state<number | null>(null);

  // A link: to a web address or to a page of this workspace.
  let linking = $state<{ i: number; start: number; end: number; url: string; page: string } | null>(null);
  function openLink() {
    const i = current;
    if (!blocks[i] || blocks[i].kind === 'hr') return;
    const s = selection(i);
    linking = { i, ...s, url: 'https://', page: '' };
  }
  const pages = $derived(app.dashboards.map((d) => ({ id: d.id, title: d.title || t('Untitled') })));
  function addWeb() {
    if (!linking) return;
    const url = safeUrl(linking.url.trim());
    if (!url) return;
    const l = linking;
    linking = null;
    apply(l.i, addLink(blocks[l.i].text, l.start, l.end, url, url));
  }
  function addPage() {
    if (!linking || !linking.page) return;
    const l = linking;
    linking = null;
    apply(l.i, addLink(blocks[l.i].text, l.start, l.end, `page:${l.page}`, pages.find((p) => p.id === l.page)?.title ?? t('Page')));
  }

  // A live number: a named measure's value, its change, or its period.
  let numbering = $state<{ i: number; start: number; end: number; id: string; part: NumPart } | null>(null);
  const measures = $derived(app.snap?.workspace.measures ?? []);
  function openNumber() {
    const i = current;
    if (!blocks[i] || blocks[i].kind === 'hr') return;
    numbering = { i, ...selection(i), id: measures[0]?.id ?? '', part: 'value' };
  }
  function addNumber() {
    if (!numbering?.id) return;
    const n = numbering;
    numbering = null;
    apply(n.i, insertAt(blocks[n.i].text, n.start, n.end, numberToken(n.id, n.part)));
  }
</script>

<div class="sec ed">
  <div class="top">
    <span class="label">{t('Text')}</span>
    <label class="check"><input type="checkbox" bind:checked={raw} /> {t('Edit as text')}</label>
  </div>

  {#if raw}
    <textarea
      class="field rawtext"
      rows="10"
      aria-label={t('Text of this tile, in Markdown')}
      value={tile.text ?? ''}
      oninput={(e) => set({ text: e.currentTarget.value })}></textarea>
    <p class="hint">
      {t('# ## ### headings, - and 1. lists, > quotes, > [!note], [!good] or [!warn] callouts, --- a line, **bold**, *italic*, [a link](https://…), [a page](page:id), and a live number: {{measure:id}}, with |compare or |period.')}
    </p>
  {:else}
    <div class="tools" role="toolbar" aria-label={t('Words')}>
      <button class="btn plain icon" title={t('Bold (Ctrl+B)')} aria-label={t('Bold')} onmousedown={(e) => e.preventDefault()} onclick={() => mark('**')}><b>B</b></button>
      <button class="btn plain icon" title={t('Italic (Ctrl+I)')} aria-label={t('Italic')} onmousedown={(e) => e.preventDefault()} onclick={() => mark('*')}><i>I</i></button>
      <button class="btn plain icon" title={t('Link (Ctrl+K)')} aria-label={t('Link')} onmousedown={(e) => e.preventDefault()} onclick={openLink}><Icon name="link" size={14} /></button>
      <button class="btn plain small" onmousedown={(e) => e.preventDefault()} onclick={openNumber}><Icon name="hash" size={13} /> {t('Insert number')}</button>
    </div>

    {#if linking}
      <div class="pop" role="group" aria-label={t('Add a link')}>
        <span class="label">{t('Link to a web address')}</span>
        <form class="row" onsubmit={(e) => { e.preventDefault(); addWeb(); }}>
          <input class="field" aria-label={t('Web address')} bind:value={linking.url} />
          <button class="btn" type="submit" disabled={!safeUrl(linking.url.trim())}>{t('Add')}</button>
        </form>
        {#if pages.length}
          <span class="label">{t('or to a page of this workspace')}</span>
          <div class="row">
            <select class="field" aria-label={t('Page')} bind:value={linking.page}>
              <option value="">{t('Pick a dashboard')}</option>
              {#each pages as p (p.id)}<option value={p.id}>{p.title}</option>{/each}
            </select>
            <button class="btn" disabled={!linking.page} onclick={addPage}>{t('Add')}</button>
          </div>
        {/if}
        <button class="link" onclick={() => (linking = null)}>{t('Cancel')}</button>
      </div>
    {/if}

    {#if numbering}
      <div class="pop" role="group" aria-label={t('Insert a number')}>
        {#if measures.length}
          <span class="label">{t('Measure')}</span>
          <select class="field" aria-label={t('Measure')} bind:value={numbering.id}>
            {#each measures as m (m.id)}<option value={m.id}>{m.name}</option>{/each}
          </select>
          <div class="grains" role="radiogroup" aria-label={t('What to show')}>
            {#each [['value', t('Value')], ['compare', t('Change@@noun')], ['period', t('Period')]] as [id, label]}
              <button role="radio" aria-checked={numbering.part === id} class:on={numbering.part === id} onclick={() => numbering && (numbering.part = id as NumPart)}>{label}</button>
            {/each}
          </div>
          <p class="hint">{t("The number follows the dashboard's filters and its grain. A change is against the period before.")}</p>
          <div class="row">
            <button class="btn primary" onclick={addNumber}>{t('Insert')}</button>
            <button class="btn plain" onclick={() => (numbering = null)}>{t('Cancel')}</button>
          </div>
        {:else}
          <p class="hint">{t('A live number shows a named measure. There are none yet: make one in Data, under Measures.')}</p>
          <button class="link" onclick={() => (numbering = null)}>{t('Close')}</button>
        {/if}
      </div>
    {/if}

    <ol class="blocks" aria-label={t('Blocks of the text')}>
      {#each blocks as b, i (i)}
        <li
          class:over={dragFrom !== null && dragFrom !== i}
          ondragover={(e) => e.preventDefault()}
          ondrop={(e) => {
            e.preventDefault();
            if (dragFrom !== null) commit(move(blocks, dragFrom, i));
            dragFrom = null;
          }}>
          <div class="blk">
            <span class="grip" draggable="true" role="button" tabindex="-1" aria-label={t('Drag to move')} ondragstart={() => (dragFrom = i)} ondragend={() => (dragFrom = null)}>
              <Icon name="grip" size={14} />
            </span>
            <button class="kind" title={t('Change the kind of this block')} aria-haspopup="menu" onclick={() => (menu = menu?.at === i && menu.mode === 'convert' ? null : { at: i, mode: 'convert' })}>{SHORT[b.kind]}</button>
            {#if b.kind === 'hr'}
              <hr />
            {:else}
              <textarea
                class="words {b.kind}"
                rows="1"
                bind:this={els[i]}
                use:growing
                aria-label={t('{kind} block', { kind: SHORT[b.kind] })}
                placeholder={HINT[b.kind] ?? ''}
                value={b.text}
                onfocus={() => (current = i)}
                oninput={(e) => typed(i, e.currentTarget.value, e.currentTarget)}
                onkeydown={(e) => onkey(e, i)}></textarea>
            {/if}
            <button class="btn plain icon x" aria-label={t('Remove this block')} title={t('Remove')} onclick={() => commit(remove(blocks, i))}><Icon name="x" size={12} /></button>
          </div>
          {#if menu?.at === i}
            <div class="menu" role="menu" aria-label={menu.mode === 'convert' ? t('Turn this block into') : t('Add a block')}>
              {#each KINDS as k (k.kind)}
                <button role="menuitem" class:on={menu.mode === 'convert' && b.kind === k.kind} onclick={() => pick(k.kind)}>{k.label}</button>
              {/each}
            </div>
          {/if}
          <button class="plus" title={t('Add a block here')} aria-label={t('Add a block after this one')} onclick={() => (menu = { at: i + 1, mode: 'insert' })}><Icon name="plus" size={11} /></button>
          {#if menu?.at === i + 1 && menu.mode === 'insert' && i + 1 === blocks.length}
            <div class="menu" role="menu" aria-label={t('Add a block')}>
              {#each KINDS as k (k.kind)}<button role="menuitem" onclick={() => pick(k.kind)}>{k.label}</button>{/each}
            </div>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}

  <span class="label up">{t('Style')}</span>
  <div class="grains" role="radiogroup" aria-label={t('Style of the text')}>
    {#each [['body', t('Body')], ['lead', t('Lead')], ['note', t('Note')]] as [id, label]}
      <button role="radio" aria-checked={(tile.textStyle ?? 'body') === id} class:on={(tile.textStyle ?? 'body') === id} onclick={() => set({ textStyle: id === 'body' ? undefined : (id as Tile['textStyle']) })}>{label}</button>
    {/each}
  </div>
  <div class="grains" role="radiogroup" aria-label={t('Alignment of the text')}>
    {#each [['start', t('Left')], ['center', t('Centre')]] as [id, label]}
      <button role="radio" aria-checked={(tile.textAlign ?? 'start') === id} class:on={(tile.textAlign ?? 'start') === id} onclick={() => set({ textAlign: id === 'start' ? undefined : 'center' })}>{label}</button>
    {/each}
  </div>
</div>

<style>
  .sec { display: flex; flex-direction: column; gap: 6px; }
  .top { display: flex; align-items: center; justify-content: space-between; }
  .check { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .label.up { margin-top: 6px; }
  .tools { display: flex; align-items: center; gap: 2px; padding: 2px; border-radius: 8px; background: var(--inset); }
  .tools .btn.icon { width: 26px; height: 24px; }
  .tools .btn.small { height: 24px; font-size: 12px; margin-left: auto; }
  .pop { display: flex; flex-direction: column; gap: 6px; padding: 10px; border: 1px solid var(--sep); border-radius: 8px; background: var(--raised); }
  .row { display: flex; gap: 6px; align-items: center; }
  .row .field { flex: 1; min-width: 0; }
  .blocks { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .blocks li { position: relative; }
  .blocks li.over { outline: 1px dashed var(--accent); border-radius: 6px; }
  .blk { display: grid; grid-template-columns: 16px 46px 1fr 22px; align-items: start; gap: 4px; padding: 2px 0; }
  .grip { color: var(--text-3); cursor: grab; padding-top: 6px; display: flex; }
  .kind { height: 24px; margin-top: 2px; padding: 0 4px; border: 1px solid var(--sep); border-radius: 6px; background: var(--surface); font-size: 10px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kind:hover { background: var(--hover); }
  .words { width: 100%; min-height: 28px; resize: none; overflow: hidden; border: 1px solid transparent; border-radius: 6px; background: var(--inset); padding: 5px 8px; font: inherit; font-size: 13px; line-height: 1.4; color: var(--text); }
  .words:focus { outline: none; border-color: var(--focus); }
  .words.h1 { font-size: 16px; font-weight: 600; }
  .words.h2 { font-size: 14px; font-weight: 600; }
  .words.h3 { font-weight: 600; color: var(--text-2); }
  .words.quote { border-left: 3px solid var(--sep-strong); }
  .words.note { background: color-mix(in srgb, var(--accent) 10%, var(--surface)); }
  .words.good { background: color-mix(in srgb, var(--success) 12%, var(--surface)); }
  .words.warn { background: color-mix(in srgb, var(--warn) 14%, var(--surface)); }
  .blk hr { margin: 13px 0; border: 0; border-top: 1px solid var(--sep-strong); }
  .x { width: 22px; height: 22px; margin-top: 3px; opacity: 0.6; }
  .x:hover { opacity: 1; }
  .plus { position: absolute; left: 18px; bottom: -7px; z-index: 1; width: 16px; height: 16px; padding: 0; display: flex; align-items: center; justify-content: center; border: 1px solid var(--sep); border-radius: 8px; background: var(--surface); color: var(--text-2); opacity: 0; }
  .blocks li:hover .plus, .plus:focus-visible { opacity: 1; }
  .menu { display: grid; grid-template-columns: 1fr 1fr; gap: 2px; margin: 2px 0 6px 20px; padding: 4px; border-radius: 8px; background: var(--raised); box-shadow: var(--shadow-float); }
  .menu button { height: 26px; padding: 0 8px; border: 0; border-radius: 6px; background: none; text-align: left; font-size: 12px; color: var(--text); }
  .menu button:hover, .menu button:focus-visible { background: var(--hover); }
  .menu button.on { color: var(--accent-text); font-weight: 600; }
  .grains { display: flex; padding: 2px; border-radius: 8px; background: var(--inset); }
  .grains button { flex: 1; height: 24px; border: 0; border-radius: 6px; background: none; color: var(--text-2); font-size: 11px; padding: 0 4px; }
  .grains button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .rawtext { height: auto; padding: 8px 10px; line-height: 1.4; resize: vertical; font-family: var(--mono); font-size: 12px; }
  .hint { margin: 0; font-size: 11px; line-height: 1.4; color: var(--text-2); }
  .link { align-self: flex-start; border: 0; background: none; padding: 0; font-size: 12px; color: var(--accent-text); cursor: pointer; }
</style>
