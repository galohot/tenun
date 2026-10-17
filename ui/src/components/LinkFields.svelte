<script lang="ts">
  // The fields of a web link, shared by the Add sheet and Link settings: the address, a key, and
  // after a preview the list of records and the way to the next pages.
  import type { KeyPlace } from '../lib/api';
  import type { LinkForm } from '../lib/linkform.svelte';
  import { EVERY_CHOICES, PAGES_CHOICES, listLabel, recordChoices, type PagesBy } from '../lib/sources';
  import { app } from '../lib/state.svelte';
  import Icon from './Icon.svelte';
  import { t, tn } from '../lib/i18n';

  let { f }: { f: LinkForm } = $props();

  const look = $derived(f.look);
  const pasted = (e: Event) => {
    f.addressEdited();
    // A link pasted with a key in it is cleaned at once. Typing waits for the field to lose focus.
    const how = (e as InputEvent).inputType;
    if (how === 'insertFromPaste' || how === 'insertFromDrop') f.tidy();
  };
</script>

<label class="stack first">
  <span class="label">{f.locked ? t('Link') : t('A link to CSV or JSON data: a published Google Sheet, a CSV file, or an API')}</span>
  <input
    class="field"
    type="url"
    bind:value={f.url}
    placeholder="https://"
    autocomplete="off"
    required
    readonly={f.locked}
    oninput={pasted}
    onchange={() => f.tidy()} />
  {#if f.problem}<span class="help bad">{f.problem}</span>{/if}
</label>
{#if !f.locked}
  <p class="note">
    <Icon name="info" size={14} />
    {t('In Google Sheets, open File, then Share, then Publish to the web. Pick the sheet, choose Comma-separated values (.csv), press Publish and copy the link.')}
  </p>
  <div class="grid2">
    <label class="stack">
      <span class="label">{t('Name')}</span>
      <input class="field" bind:value={f.name} placeholder={t('Optional')} />
    </label>
    <label class="stack">
      <span class="label">{t('Refresh')}</span>
      <select class="field" bind:value={f.every}>
        {#each EVERY_CHOICES as c}<option value={c.value}>{c.label}</option>{/each}
      </select>
    </label>
  </div>
{/if}

<details class="key" bind:open={f.keyOpen}>
  <summary>
    {t('Key (optional)')}
    {#if f.hasKey}<span class="chip">{f.stored && !f.keyValue ? t('saved') : t('set')}</span>{/if}
  </summary>
  <div class="keybody">
    {#if f.moved}
      <p class="moved"><Icon name="info" size={14} /> {f.moved}</p>
    {/if}
    <div class="grid2 flat">
      <label class="stack">
        <span class="label">{t('Send as')}</span>
        <select class="field" value={f.place} onchange={(e) => f.setPlace(e.currentTarget.value as KeyPlace)}>
          <option value="header">{t('Header')}</option>
          <option value="query">{t('Link parameter')}</option>
        </select>
      </label>
      <label class="stack">
        <span class="label">{t('Name')}</span>
        <input class="field" bind:value={f.keyName} autocomplete="off" spellcheck="false" />
      </label>
    </div>
    <label class="stack">
      <span class="label">{t('Value')}</span>
      <input
        class="field"
        type="password"
        bind:value={f.keyValue}
        autocomplete="new-password"
        oninput={(e) => {
          // A value typed after Remove key means a new key, not a removal.
          if (e.currentTarget.value) f.removeKey = false;
        }}
        placeholder={f.stored && !f.removeKey ? t('Saved. Leave empty to keep it.') : ''} />
    </label>
    {#if f.stored}
      <div class="remove">
        {#if f.removeKey}
          <span class="help">{t('The key is removed when you save.')}</span>
          <button type="button" class="btn" onclick={() => (f.removeKey = false)}>{t('Keep the key')}</button>
        {:else}
          <button type="button" class="btn" onclick={() => f.removeStored()}>{t('Remove key')}</button>
        {/if}
      </div>
    {/if}
    <p class="note">
      <Icon name="info" size={14} />
      {t('Tenun keeps the key in its own settings on this computer, not in the workspace file. Someone who opens your workspace adds their own key.')}
    </p>
  </div>
</details>

<div class="look">
  <button type="button" class="btn" disabled={!f.url.trim() || !!f.problem || f.looking} onclick={() => f.preview()}>
    {f.looking ? t('Looking…') : t('Preview')}
  </button>
  {#if f.lookError}<span class="help bad">{f.lookError}</span>{/if}
</div>

{#if look}
  {#if look.format === 'json'}
    <div class="grid2">
      <label class="stack">
        <span class="label">{t('Records')}</span>
        <select class="field" value={f.chosen} onchange={(e) => f.setRecords(e.currentTarget.value)}>
          {#each recordChoices(look, f.chosen) as l (l.path)}
            <option value={l.path}>{listLabel(l, app.locale)}</option>
          {/each}
        </select>
      </label>
      <label class="stack">
        <span class="label">{t('More pages')}</span>
        <select class="field" value={f.pages.by} onchange={(e) => f.setPages(e.currentTarget.value as PagesBy)}>
          {#each PAGES_CHOICES as c}<option value={c.value}>{c.label}</option>{/each}
        </select>
      </label>
    </div>
    {#if f.pages.by === 'field' || f.pages.by === 'cursor' || f.pages.by === 'page'}
      <div class="grid2">
        {#if f.pages.by !== 'page'}
          <label class="stack">
            <span class="label">{f.pages.by === 'field' ? t('Field with the next link') : t('Field with the cursor')}</span>
            <input class="field" bind:value={f.pages.path} oninput={() => (f.pagesTouched = true)} autocomplete="off" spellcheck="false" placeholder={f.pages.by === 'field' ? 'paging.next' : 'next_cursor'} />
          </label>
        {/if}
        {#if f.pages.by !== 'field'}
          <label class="stack">
            <span class="label">{t('Link parameter')}</span>
            <input class="field" bind:value={f.pages.param} oninput={() => (f.pagesTouched = true)} autocomplete="off" spellcheck="false" placeholder={f.pages.by === 'page' ? 'page' : 'cursor'} />
          </label>
        {/if}
      </div>
    {/if}
  {/if}
  <div class="wrap">
    <table>
      <thead>
        <tr>{#each look.columns as c}<th>{c}</th>{/each}</tr>
      </thead>
      <tbody>
        {#each look.rows as r}
          <tr>{#each r as v}<td>{v}</td>{/each}</tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="muted small">
    {#if look.format === 'json'}
      {@const n = look.total.toLocaleString(app.locale)}
      {look.total > look.rows.length
        ? tn('{n} row on the first page. The first {shown} are shown.', '{n} rows on the first page. The first {shown} are shown.', look.total, { n, shown: look.rows.length })
        : tn('{n} row on the first page.', '{n} rows on the first page.', look.total, { n })}
      {#if look.pages_hint}{t('Tenun found a way to the next pages.')}{/if}
    {:else}
      {tn('The first {n} row is shown.', 'The first {n} rows are shown.', look.rows.length)}
    {/if}
  </p>
{/if}

<style>
  .stack { display: flex; flex-direction: column; gap: 4px; margin-top: 12px; }
  .stack.first { margin-top: 0; }
  .help { color: var(--text-2); font-size: 12px; }
  .help.bad { color: var(--danger); }
  .grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 12px; margin-top: 12px; }
  .grid2.flat { margin-top: 0; }
  .note { display: flex; gap: 8px; margin: 14px 0 0; color: var(--text-2); font-size: 12px; }
  .note :global(svg) { margin-top: 1px; flex: none; }
  .key { margin-top: 14px; border-top: 1px solid var(--sep); padding-top: 10px; }
  .key summary { cursor: pointer; font-weight: 500; display: flex; align-items: center; gap: 8px; }
  .chip { font-size: 11px; font-weight: 500; padding: 1px 8px; border-radius: 10px; background: var(--selected); color: var(--text-2); }
  .keybody { margin-top: 10px; }
  .moved { display: flex; gap: 8px; margin: 0 0 4px; padding: 8px 12px; border-radius: var(--r-card); background: var(--selected); font-size: 12px; }
  .moved :global(svg) { margin-top: 1px; flex: none; color: var(--accent-text); }
  .remove { display: flex; align-items: center; gap: 10px; margin-top: 10px; }
  .look { display: flex; align-items: center; gap: 10px; margin-top: 14px; }
  .wrap { overflow: auto; border-radius: var(--r-card); background: var(--inset); max-height: 190px; margin-top: 12px; }
  table { border-collapse: collapse; font-size: 12px; min-width: 100%; }
  th { text-align: left; font-weight: 600; padding: 6px 12px; border-bottom: 1px solid var(--sep); white-space: nowrap; }
  td { padding: 5px 12px; border-bottom: 1px solid var(--sep); white-space: nowrap; max-width: 220px; overflow: hidden; text-overflow: ellipsis; }
  .small { font-size: 12px; margin: 8px 0 0; }
</style>
