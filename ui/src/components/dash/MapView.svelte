<script lang="ts">
  // A map tile (build 06): places coloured by a value, or points sized by one. The engine has read every place
  // (geo.rs); this draws the shapes it named, a legend of the range, the credit line, and the names that are not
  // on the map. A click on a place filters the other tiles by it.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import type { EChartsCoreOption } from 'echarts/core';
  import { buildFilled, buildPoints, rampOf, STEPS, type Ramp } from '../../lib/charts/map';
  import { outlineFor, shapesFor, type Shapes } from '../../lib/geoassets';
  import { cell, measureLabel, type Named } from '../../lib/format';
  import { t, tn } from '../../lib/i18n';
  import ChartView from './ChartView.svelte';
  import Icon from '../Icon.svelte';

  let {
    tile,
    res,
    colors,
    selected,
    tables = [],
    onpick,
    onretry,
    onplaces,
  }: {
    tile: Tile;
    res: QueryResult;
    colors: Colors;
    /** What a click picked: the names of a place in the data, or its code (an exported file). */
    selected: Cell[] | null;
    tables?: Named[];
    /** A click on a place: its code. */
    onpick?: (code: Cell) => void;
    /** Another country's map was downloaded: ask again. */
    onretry?: () => void;
    /** Open the list of place names (in the app). */
    onplaces?: () => void;
  } = $props();

  const map = $derived(tile.map ?? { level: 'province' as const });
  const points = $derived(map.style === 'points');
  const country = $derived((res.geo?.country ?? map.country ?? 'ID').toUpperCase());
  const level = $derived(res.geo?.level ?? map.level);

  let shapes = $state<Shapes | null>(null);
  let failed = $state('');
  let downloading = $state(false);

  // Load the shapes of the level; another country's are downloaded once, then the tile asks again.
  $effect(() => {
    const [lv, c, pts, dl] = [level, country, points, !!res.geo?.download];
    failed = '';
    downloading = dl;
    const want = pts ? outlineFor(map.country) : shapesFor(lv, c);
    want
      .then((s) => {
        shapes = s;
        if (dl) {
          downloading = false;
          onretry?.();
        }
      })
      .catch((e: Error) => {
        downloading = false;
        failed = e.message;
      });
  });

  const fmt = $derived(tile.formats?.[0] ?? null);
  const format = (v: number) => cell(v, 'number', null, false, fmt);
  const valueName = $derived(tile.measures[0] ? measureLabel(tile.measures[0], tables) : t('Rows'));

  // The places picked: a code, or the names of a place in the data.
  const pickedCodes = $derived.by(() => {
    if (!selected?.length) return null;
    const codes = res.geo?.codes ?? [];
    const raw = res.geo?.raw ?? [];
    const out = new Set<string>();
    codes.forEach((c, i) => {
      if (!c) return;
      if (selected.some((s) => s === c || raw[i]?.some((r) => r === s))) out.add(c);
    });
    return [...out];
  });

  const built = $derived.by((): { option: EChartsCoreOption; ramp: Ramp | null } | null => {
    if (!shapes) return null;
    if (points) return { option: buildPoints({ res, colors, outline: shapes.shapes, mapName: shapes.mapName, format, valueName }), ramp: null };
    return buildFilled({ res, colors, shapes: shapes.shapes, mapName: shapes.mapName, borders: shapes.borders, selected: pickedCodes, format });
  });
  const ramp = $derived(built?.ramp ?? (points ? null : rampOf([])));
  const steps = $derived(ramp?.kind === 'diverging' ? colors.div : colors.ramp);

  const unmatched = $derived(res.geo?.unmatched ?? []);
  const unmatchedCount = $derived(res.geo?.unmatchedCount ?? unmatched.length);
  let showUnmatched = $state(false);

  const countryName: Record<string, string> = { ID: 'Indonesia' };
</script>

<div class="map">
  {#if failed}
    <p class="msg"><Icon name="warning" size={14} /> {failed}</p>
  {:else if downloading}
    <p class="msg"><Icon name="clock" size={14} /> {t('Downloading the map of {country} once…', { country: countryName[country] ?? country })}</p>
  {:else if !points && !res.rows.length && !unmatchedCount}
    <p class="msg">{t('No places to show.')}</p>
  {:else if built}
    <div class="chart" role="img" aria-label={t('{name} on a map: {n} places', { name: tile.title || valueName, n: res.rows.length })}>
      <ChartView
        option={built.option}
        onitem={onpick && !points ? (_s, i) => {
          // Items past the rows are the shapes with no data: nothing to pick.
          const code = res.rows[i]?.[0];
          if (code !== undefined) onpick(code);
        } : undefined} />
    </div>
  {:else}
    <p class="msg">{t('Loading the map…')}</p>
  {/if}
  <footer>
    {#if ramp && !points && res.rows.length}
      <div class="legend" aria-label={t('Colour scale from {from} to {to}', { from: format(ramp.min), to: format(ramp.max) })}>
        <div class="bar">{#each { length: STEPS } as _, i (i)}<span style:background={steps[i]}></span>{/each}</div>
        <div class="ends"><span>{format(ramp.min)}</span>{#if ramp.kind === 'diverging'}<span>0</span>{/if}<span>{format(ramp.max)}</span></div>
      </div>
      <span class="nodata"><span class="hatch"></span> {t('No data')}</span>
    {/if}
    {#if unmatchedCount}
      <span class="chip-wrap">
        <button type="button" class="chip" aria-expanded={showUnmatched} onclick={() => (showUnmatched = !showUnmatched)}>
          <Icon name="warning" size={11} /> {tn('{n} name not on the map', '{n} names not on the map', unmatchedCount)}
        </button>
        {#if showUnmatched}
          <div class="pop" role="dialog" aria-label={t('Names not on the map')}>
            <ul>
              {#each unmatched as u (u.name)}
                <li><strong>{u.name}</strong>{#if u.hint}<span> {u.hint}</span>{/if}</li>
              {/each}
            </ul>
            {#if unmatchedCount > unmatched.length}<p>{t('And {n} more.', { n: unmatchedCount - unmatched.length })}</p>{/if}
            {#if onplaces}<button type="button" class="linkbtn" onclick={onplaces}>{t('See the place names Tenun reads')}</button>{/if}
          </div>
        {/if}
      </span>
    {/if}
    {#if res.geo?.dropped}
      <span class="muted">{tn('{n} row has no place on the earth', '{n} rows have no place on the earth', res.geo.dropped)}</span>
    {/if}
    {#if shapes}<span class="credit">{shapes.credit}</span>{/if}
  </footer>
</div>

<style>
  .map { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .chart { flex: 1; min-height: 0; }
  .msg { margin: auto; display: flex; gap: 6px; align-items: center; color: var(--text-2); font-size: 13px; padding: 12px; text-align: center; }
  footer { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 12px; padding: 4px 2px 0; font-size: 11px; color: var(--text-2); }
  .legend { display: flex; flex-direction: column; gap: 2px; min-width: 140px; }
  .bar { display: grid; grid-template-columns: repeat(7, 1fr); height: 8px; border-radius: 3px; overflow: hidden; }
  .ends { display: flex; justify-content: space-between; font-variant-numeric: tabular-nums; }
  .nodata { display: inline-flex; align-items: center; gap: 4px; }
  .hatch {
    width: 12px;
    height: 8px;
    border-radius: 2px;
    background: repeating-linear-gradient(45deg, var(--inset) 0 2px, var(--grid-line) 2px 3px);
    border: 1px solid var(--sep);
  }
  .chip-wrap { position: relative; }
  .chip { display: inline-flex; gap: 4px; align-items: center; padding: 1px 8px; border: 1px solid var(--sep); border-radius: 999px; background: var(--surface); color: var(--text-2); font-size: 11px; cursor: pointer; }
  .chip:hover { color: var(--text); }
  .pop {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    z-index: 5;
    width: 280px;
    max-height: 220px;
    overflow: auto;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--raised);
    box-shadow: var(--shadow-float);
    color: var(--text);
    font-size: 12px;
  }
  .pop ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 4px; }
  .pop li span { color: var(--text-2); }
  .pop p { margin: 6px 0 0; color: var(--text-2); }
  .linkbtn { margin-top: 8px; border: 0; background: none; padding: 0; color: var(--accent-text); cursor: pointer; font-size: 12px; }
  .credit { margin-left: auto; }
  .muted { color: var(--text-2); }
  @media (max-width: 700px) {
    .credit { margin-left: 0; width: 100%; }
  }
</style>
