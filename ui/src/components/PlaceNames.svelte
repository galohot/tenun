<script lang="ts">
  // The place names a map reads (build 05, V4-D8): the standard name and code of each country, province and
  // kabupaten/kota, the other forms Tenun reads, and a CSV to start a sheet from.
  import { api } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { PLACE_LISTS, placesTable, searchPlaces, type PlaceLevel, type PlaceRow } from '../lib/places';
  import { fileName, toCsv } from '../lib/tabular';
  import { saveBlob } from '../lib/download';
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';
  import { t, tn } from '../lib/i18n';

  let { level: start = 'regency', onclose }: { level?: PlaceLevel; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let level = $state<PlaceLevel>(start);
  let q = $state('');
  let rows = $state<PlaceRow[]>([]);
  let loading = $state(false);
  const list = $derived(PLACE_LISTS.find((l) => l.level === level)!);
  const shown = $derived(searchPlaces(rows, q));

  $effect(() => {
    const l = list;
    loading = true;
    api
      .places(l.level, l.country)
      .then((r) => {
        if (list === l) rows = r.places;
      })
      .catch((e) => app.error(e))
      .finally(() => (loading = false));
  });

  function download() {
    const csv = toCsv(placesTable(shown));
    saveBlob(new Blob([csv], { type: 'text/csv;charset=utf-8' }), fileName(t('Place names - {name}', { name: list.title }), 'csv'));
  }
</script>

<Modal title={t('Place names')} width={640} {onclose}>
  <div class="intro">
    <p>
      {t('A map reads a column of place names or codes. Write them the standard way and every row finds its place. Case, extra spaces and dots do not matter, and "Kab." is "Kabupaten". A bare name that is a kabupaten and a kota, like "Bandung", needs "Kabupaten" or "Kota" in front.')}
    </p>
  </div>
  <div class="bar">
    <div class="seg" role="radiogroup" aria-label={t('Which list')}>
      {#each PLACE_LISTS as l (l.level)}
        <button type="button" role="radio" aria-checked={level === l.level} class:on={level === l.level} onclick={() => (level = l.level)}>{l.title}</button>
      {/each}
    </div>
    <input class="field search" type="search" placeholder={t('Search a name or a code')} bind:value={q} aria-label={t('Search')} />
  </div>
  <p class="codes">{t('Codes: {codes}.', { codes: list.codeWords })}</p>
  <div class="grid" role="table" aria-label={list.title} aria-busy={loading}>
    <div class="row head" role="row"><span role="columnheader">{t('Code')}</span><span role="columnheader">{t('Name')}</span><span role="columnheader">{t('Also read as')}</span></div>
    {#each shown.slice(0, 600) as r (r.code)}
      <div class="row" role="row">
        <span role="cell" class="code">{r.code}</span>
        <span role="cell">{r.name}</span>
        <span role="cell" class="dim">{r.aliases.join(', ')}</span>
      </div>
    {:else}
      <p class="none">{loading ? t('Loading…') : t('No place matches “{q}”.', { q })}</p>
    {/each}
  </div>
  <p class="credit">{t('Boundaries: Natural Earth (public domain) and geoBoundaries (CC BY 3.0 IGO). Codes and names of Indonesia: Kepmendagri 2025, by way of cahyadsn/wilayah (MIT).')}</p>
  {#snippet footer()}
    <span class="count">{tn('{n} place', '{n} places', shown.length, { n: shown.length.toLocaleString('en-US') })}</span>
    <button type="button" class="btn" onclick={download} disabled={!shown.length}><Icon name="export" size={14} /> {t('Download CSV')}</button>
    <button type="button" class="btn primary" onclick={onclose}>{t('Done')}</button>
  {/snippet}
</Modal>

<style>
  .intro p { margin: 0 0 12px; color: var(--text-2); font-size: 13px; line-height: 1.45; }
  .bar { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .seg { display: inline-flex; flex-wrap: wrap; padding: 2px; border-radius: 8px; background: var(--inset); }
  .seg button { border: 0; background: none; padding: 4px 10px; border-radius: 6px; font-size: 12px; color: var(--text-2); }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: var(--shadow-1, 0 1px 2px rgb(0 0 0 / 12%)); font-weight: 600; }
  .search { flex: 1; min-width: 160px; height: 30px; }
  .codes { margin: 8px 0; font-size: 12px; color: var(--text-2); }
  .grid { border: 1px solid var(--sep); border-radius: 8px; max-height: 340px; overflow: auto; font-size: 12px; }
  .row { display: grid; grid-template-columns: 72px minmax(0, 1.2fr) minmax(0, 1fr); gap: 10px; padding: 5px 10px; border-top: 1px solid var(--sep); }
  .row.head { border-top: 0; position: sticky; top: 0; background: var(--surface); font-weight: 600; color: var(--text-2); }
  .code { font-variant-numeric: tabular-nums; font-family: var(--mono, ui-monospace, monospace); }
  .dim { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; }
  .none { margin: 0; padding: 14px; color: var(--text-2); }
  .credit { margin: 10px 0 0; font-size: 11px; color: var(--text-2); }
  .count { margin-right: auto; color: var(--text-2); font-size: 12px; }
  @media (max-width: 520px) {
    .row { grid-template-columns: 64px minmax(0, 1fr); }
    .row > :nth-child(3) { grid-column: 2; }
  }
</style>
