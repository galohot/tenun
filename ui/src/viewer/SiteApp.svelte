<script lang="ts">
  // An exported site (build 13, V4-D15): the site's navigation (`SiteFrame`, as the preview in the app draws it) around
  // the page the address names. One file routes by the address (`#/p/<page>`), so back and forward work; a folder opens
  // another file. A click on a mark of a tile with an action opens its page kept to the clicked value, from what the
  // file holds (the first 50 values); past them the page opens as it is, and says why (V4-D16). On tenun.cloud (build 14)
  // each page says when the site was last published.
  import { onMount } from 'svelte';
  import type { Cell, Dashboard, Tile } from '../lib/api';
  import type { ExportPayload, SitePayload } from '../lib/export';
  import { key } from '../lib/chart';
  import { filterText } from '../lib/filterText';
  import { navOf, normaliseSite } from '../lib/site';
  import { hashOf, parseHash, type Route } from '../lib/siteview';
  import SiteFrame from '../components/site/SiteFrame.svelte';
  import Viewer from './Viewer.svelte';
  import { t } from '../lib/i18n';

  let { data, hosted = null }: { data: SitePayload; hosted?: { updated: string; source: string; every: number | null; updates?: string | null } | null } = $props();

  // The pages as the navigation needs them: a title and whether the navigation shows it.
  const hidden = $derived(new Set(data.hidden ?? []));
  const dashboards = $derived(data.order.map((id) => ({ id, title: data.titles[id] ?? '', refresh: 0, tiles: [], ...(hidden.has(id) ? { hidden: true } : {}) }) as Dashboard));
  const site = $derived(normaliseSite(data.site, dashboards, data.workspace));
  const nav = $derived(navOf(site, dashboards));
  const home = $derived(site.home ?? data.order[0]);
  /** A folder holds one page; its file is the page. */
  const folder = $derived(!!data.files);
  const here = $derived(data.page);
  const ids = $derived(new Set(data.order));

  // svelte-ignore state_referenced_locally
  let route = $state<Route>(folder ? { page: here ?? home } : parseHash(location.hash, data.order, home));
  onMount(() => {
    const onhash = () => (route = folder ? { page: here ?? home, ...drillOnly() } : parseHash(location.hash, data.order, home));
    addEventListener('hashchange', onhash);
    // A folder's file names its drill-through in the address too.
    if (folder) route = { page: here ?? home, ...drillOnly() };
    return () => removeEventListener('hashchange', onhash);
  });
  /** In a folder, the address holds only a drill-through: `#/d/<tile>/<key>`. */
  function drillOnly(): Pick<Route, 'drill'> {
    const m = /^#\/d\/([^/]+)\/(.*)$/.exec(location.hash);
    return m ? { drill: { tile: decodeURIComponent(m[1]), key: decodeURIComponent(m[2]) } } : {};
  }

  function go(id: string) {
    if (folder) {
      const file = data.files?.[id];
      if (file) location.href = file;
      return;
    }
    location.hash = hashOf({ page: id });
  }

  /** What the route shows: a page, or the page a drill-through opened for a value. */
  const shown = $derived.by((): { payload: ExportPayload | undefined; from?: string; note?: string } => {
    const d = route.drill ? data.drill[route.drill.tile] : undefined;
    if (route.drill && d) {
      const fromPage = route.drill.tile.split('/')[0];
      const at = d.values[route.drill.key];
      if (at) return { payload: at, from: fromPage };
      const field = data.pages[fromPage]?.tiles.find((x) => `${fromPage}/${x.id}` === route.drill!.tile)?.dims[0]?.column ?? t('the field');
      return {
        payload: data.pages[d.page],
        from: fromPage,
        note: t('This page is not kept to the value you clicked: the file holds the pages of the first 50 values of {field}.', { field }),
      };
    }
    return { payload: data.pages[route.page] };
  });
  const current = $derived(route.drill ? (data.drill[route.drill.tile]?.page ?? route.page) : route.page);

  function drill(t: Tile, v: Cell): boolean {
    const tile = `${route.drill ? current : route.page}/${t.id}`;
    const d = data.drill[tile];
    if (!d) return false;
    const r: Route = { page: d.page, drill: { tile, key: key(v) } };
    if (folder) {
      const file = data.files?.[tile.split('/')[0]];
      location.href = `${file ?? ''}#/d/${encodeURIComponent(tile)}/${encodeURIComponent(key(v))}`;
      route = { page: here ?? home, ...drillOnly() };
    } else location.hash = hashOf(r);
    return true;
  }
  const back = () => history.back();

  // Every-page filters offline (v5 build 02): a value picked in such a filter tile follows the reader to every page.
  let everyPick = $state<{ source: string; key: string; value: Cell } | null>(null);
  function pickChange(tile: string, value: Cell | null) {
    const source = `${current}/${tile}`;
    if (!data.every?.[source]) return;
    everyPick = value === null ? null : { source, key: key(value), value };
  }
  const everyAt = $derived.by(() => {
    if (!everyPick) return null;
    const e = data.every?.[everyPick.source];
    if (!e || e.page === current) return null;
    const d = data.pages[e.page]?.tiles.find((x) => x.id === e.tile)?.dims[0];
    // The words of the app's chip: "Region: Java".
    const label = d
      ? filterText({ table: d.table, column: d.column, grain: null, op: 'in', values: [everyPick.value], fromTile: '', ...(d.via ? { via: d.via } : {}) }, data.pages[e.page]?.tables ?? [])
      : String(everyPick.value ?? t('(blank)'));
    return { label, results: e.values[everyPick.key]?.[current], onclear: () => (everyPick = null) };
  });
  /** The page the every-page value was picked on starts with it picked. */
  const startPick = $derived(everyPick && data.every?.[everyPick.source]?.page === current ? { tile: data.every[everyPick.source].tile, value: everyPick.value } : null);
</script>

<SiteFrame {site} {nav} {current} onnavigate={go}>
  {#if shown.payload}
    {#key `${current}|${route.drill?.tile ?? ''}|${route.drill?.key ?? ''}`}
      <Viewer
        data={shown.payload}
        {hosted}
        pages={ids}
        onpage={go}
        ondrill={drill}
        drillFrom={shown.from ? { title: data.titles[shown.from] || t('the page before'), back } : null}
        note={shown.note ?? (everyAt && !everyAt.results ? t('This page is not kept to the value picked for every page: the file holds the first 50 values.') : undefined)}
        {everyAt}
        {startPick}
        useHash={false}
        onpickchange={pickChange} />
    {/key}
  {:else}
    <p class="missing">{t('This page is not in the file.')}</p>
  {/if}
</SiteFrame>

<style>
  .missing { padding: 40px; text-align: center; color: var(--text-2); }
</style>
