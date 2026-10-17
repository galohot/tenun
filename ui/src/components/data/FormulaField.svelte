<script lang="ts">
  // A field for a formula: monospace, with the problem underlined and a list of names and functions
  // that fits what you are typing (build 02). Calculated columns use it now; measures and new
  // tables (builds 03 and 04) use the same field with their own scope.
  import { tick } from 'svelte';
  import type { FormulaError } from '../../lib/api';
  import { apply, cut, type Item, type Scope, suggest } from '../../lib/formula';
  import { typeIcon } from '../../lib/ui';
  import { t } from '../../lib/i18n';
  import Icon, { type IconName } from '../Icon.svelte';

  let {
    value = $bindable(''),
    scope = null,
    error = null,
    placeholder = '',
    label,
    id,
    oninput,
  }: {
    value: string;
    /** What the formula can read. Not set: the field writes plain text with no list (raw SQL). */
    scope?: Scope | null;
    /** The problem to underline. `at` and `len` are UTF-16 units, as the engine sends them. */
    error?: FormulaError | null;
    placeholder?: string;
    /** The name for screen readers. */
    label: string;
    id?: string;
    oninput?: (value: string) => void;
  } = $props();

  const uid = $props.id();
  const listId = `ff-list-${uid}`;

  let ta = $state<HTMLTextAreaElement>();
  let caret = $state(0);
  let focused = $state(false);
  let dismissed = $state(false);
  let active = $state(0);
  let rect = $state<{ left: number; top: number; width: number; above: boolean } | null>(null);

  const completion = $derived(scope && focused && !dismissed ? suggest(value, caret, scope) : null);
  const open = $derived(!!completion);
  const parts = $derived(cut(value, error));

  // The first row is chosen again when what is typed changes.
  let lastQuery = '';
  $effect(() => {
    const q = completion ? `${completion.from}:${completion.query}` : '';
    if (q !== lastQuery) {
      lastQuery = q;
      active = 0;
    }
  });

  // The chosen row stays in view when the list scrolls.
  $effect(() => {
    if (!open) return;
    document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: 'nearest' });
  });

  // The list floats at the field, so no sheet clips it.
  $effect(() => {
    if (!open || !ta) {
      rect = null;
      return;
    }
    void value;
    const r = ta.getBoundingClientRect();
    const below = innerHeight - r.bottom;
    rect = { left: r.left, top: below < 260 && r.top > below ? r.top - 4 : r.bottom + 4, width: r.width, above: below < 260 && r.top > below };
  });

  function sync() {
    if (ta) caret = ta.selectionStart ?? 0;
  }

  function typed() {
    dismissed = false;
    sync();
    oninput?.(value);
  }

  async function choose(item: Item) {
    if (!completion) return;
    const r = apply(value, completion, item);
    value = r.text;
    oninput?.(value);
    dismissed = false;
    await tick();
    ta?.focus();
    ta?.setSelectionRange(r.caret, r.caret);
    caret = r.caret;
  }

  function keydown(e: KeyboardEvent) {
    if (!completion) return;
    const n = completion.items.length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = (active + 1) % n;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active - 1 + n) % n;
    } else if ((e.key === 'Enter' || e.key === 'Tab') && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      choose(completion.items[active]);
    } else if (e.key === 'Escape') {
      // Close the list only; the sheet around the field stays.
      e.preventDefault();
      e.stopPropagation();
      dismissed = true;
    }
  }

  const iconOf = (it: Item): IconName => (it.kind === 'function' ? 'fx' : it.kind === 'column' && it.dtype ? typeIcon[it.dtype] : it.kind === 'link' ? 'link' : 'table');
</script>

<div class="ff" class:bad={!!error && !open}>
  <div class="box">
    <!-- The text again, behind the field, so the problem can be underlined in it. -->
    <div class="mirror" aria-hidden="true">{parts[0]}{#if parts[1] && !open}<span class="err">{parts[1]}</span>{:else}{parts[1]}{/if}{parts[2]}&#8203;</div>
    <textarea
      {id}
      bind:this={ta}
      bind:value
      class="input"
      rows="1"
      spellcheck="false"
      autocomplete="off"
      autocapitalize="off"
      aria-label={label}
      aria-invalid={!!error}
      role="combobox"
      aria-expanded={open}
      aria-controls={open ? listId : undefined}
      aria-autocomplete="list"
      aria-activedescendant={open ? `${listId}-${active}` : undefined}
      {placeholder}
      oninput={typed}
      onkeydown={keydown}
      onkeyup={(e) => !['ArrowDown', 'ArrowUp'].includes(e.key) && sync()}
      onclick={() => {
        dismissed = false;
        sync();
      }}
      onfocus={() => {
        focused = true;
        sync();
      }}
      onblur={() => (focused = false)}></textarea>
  </div>

  {#if open && completion && rect}
    <ul
      class="list"
      id={listId}
      role="listbox"
      aria-label={t('Suggestions')}
      style:left="{rect.left}px"
      style:top="{rect.top}px"
      style:width="{rect.width}px"
      style:transform={rect.above ? 'translateY(-100%)' : undefined}>
      {#each completion.items as it, i (it.kind + it.label + it.insert)}
        <!-- Pressing a row must not take the focus from the field, so the choice is made on mousedown. -->
        <li
          id="{listId}-{i}"
          role="option"
          aria-selected={i === active}
          class:on={i === active}
          onmousedown={(e) => {
            e.preventDefault();
            choose(it);
          }}
          onmouseenter={() => (active = i)}>
          <Icon name={iconOf(it)} size={12} />
          <span class="nm">{it.label}</span>
          <span class="dt" class:sig={it.kind === 'function'}>{it.detail}</span>
          {#if it.hint}<span class="hint">{it.hint}</span>{/if}
        </li>
      {/each}
      <li class="keys" role="presentation" aria-hidden="true">{t('↑ ↓ to move · Enter or Tab to insert · Esc to close')}</li>
    </ul>
  {/if}
</div>

<style>
  .ff { position: relative; }
  .box {
    position: relative;
    border: 1px solid transparent;
    border-radius: var(--r-control);
    background: var(--inset);
  }
  .ff:focus-within .box { border-color: var(--focus); }
  .ff.bad .box { border-color: var(--danger); }
  .mirror,
  .input {
    margin: 0;
    padding: 8px 10px;
    border: 0;
    font: 13px/20px var(--mono);
    letter-spacing: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    tab-size: 2;
  }
  .mirror {
    min-height: 64px;
    color: var(--text);
    pointer-events: none;
    box-sizing: border-box;
  }
  .input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    resize: none;
    overflow: hidden;
    background: transparent;
    color: transparent;
    caret-color: var(--text);
    outline: none;
  }
  .input::placeholder { color: var(--text-3); }
  .input::selection { background: color-mix(in srgb, var(--focus) 32%, transparent); color: transparent; }
  .err {
    text-decoration: underline wavy var(--danger);
    text-decoration-thickness: 1.5px;
    text-underline-offset: 4px;
  }
  .list {
    position: fixed;
    z-index: 70;
    margin: 0;
    padding: 4px;
    list-style: none;
    max-height: 320px;
    overflow: auto;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    font-size: 12px;
  }
  li[role='option'] {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr) auto;
    column-gap: 8px;
    align-items: center;
    padding: 5px 8px;
    border-radius: 5px;
    cursor: pointer;
  }
  li[role='option'] :global(svg) { color: var(--text-2); }
  li.on { background: var(--accent); color: #fff; }
  li.on :global(svg), li.on .dt, li.on .hint { color: #fff; }
  .nm { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dt { color: var(--text-2); white-space: nowrap; }
  .dt.sig { font-family: var(--mono); font-size: 11px; }
  .hint { grid-column: 2 / 4; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .keys {
    position: sticky;
    bottom: -4px;
    margin: 4px -4px -4px;
    padding: 6px 12px;
    background: var(--raised);
    color: var(--text-2);
    font-size: 11px;
    border-top: 1px solid var(--sep);
    border-radius: 0 0 var(--r-card) var(--r-card);
  }
</style>
