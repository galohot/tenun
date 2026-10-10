<script lang="ts">
  // Draws one tile from its result. Used by the app and by exported files.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { buildOption, CHART_TYPES, type Colors } from '../../lib/chart';
  import { barScale, barSpan, barStyle, goodBad } from '../../lib/cells';
  import { cell, columnHeads, full, OTHER_KEY, resultDim0, type Named } from '../../lib/format';
  import { effectiveShowAs } from '../../lib/showas';
  import ChartView from './ChartView.svelte';
  import ImageView from './ImageView.svelte';
  import KpiView from './KpiView.svelte';
  import PivotView from './PivotView.svelte';
  import ShareView from './ShareView.svelte';
  import TextView from './TextView.svelte';

  let {
    tile,
    res,
    colors,
    selected,
    selectedCol = null,
    editing = false,
    tables = [],
    onpick,
    onpickcol,
    onwidths,
  }: {
    tile: Tile;
    res: QueryResult;
    colors: Colors;
    selected: Cell[] | null;
    /** Pivot: the column values picked by a click on a column header. */
    selectedCol?: Cell[] | null;
    /** In the app, an empty note or picture says how to fill it. In an exported file it stays empty. */
    editing?: boolean;
    /** For naming the values of a chart with several. */
    tables?: Named[];
    onpick?: (value: Cell) => void;
    /** Pivot: a click on a column header. */
    onpickcol?: (value: Cell) => void;
    /** Table: the column widths after a drag on a heading, or undefined after a double click (back to automatic). */
    onwidths?: (widths: number[] | undefined) => void;
  } = $props();

  let width = $state(400);
  let height = $state(240);

  const isChart = $derived((CHART_TYPES as readonly string[]).includes(tile.type));
  // A scatter needs two values and a field for its dots, and a waterfall a field to step through.
  // Without them there is nothing to draw, so the tile says what is missing.
  const missing = $derived.by(() => {
    if (tile.type === 'scatter' && (tile.measures.length < 2 || !tile.dims.length)) {
      return 'A scatter chart needs two values and a field to draw one dot for each.';
    }
    if (tile.type === 'waterfall' && !tile.dims.length) return 'A waterfall needs a category or a date to step through.';
    return null;
  });
  const unfinished = $derived(missing !== null);
  const built = $derived(isChart && !unfinished ? buildOption({ res, tile, colors, selected, width, height, tables }) : null);

  // Table: click a header to sort.
  let sortBy = $state<number | null>(null);
  let sortDesc = $state(true);
  const rows = $derived.by(() => {
    if (sortBy === null) return res.rows;
    const i = sortBy;
    // "Other" (the rest outside a top N) stays last whatever the order.
    const isOther = (r: Cell[]) => !!res.other && r[0] === OTHER_KEY;
    const sorted = [...res.rows.filter((r) => !isOther(r))].sort((a, b) => {
      const x = a[i],
        y = b[i];
      if (x === y) return 0;
      if (x === null) return 1;
      if (y === null) return -1;
      const c = typeof x === 'number' && typeof y === 'number' ? x - y : String(x).localeCompare(String(y), undefined, { numeric: true });
      return sortDesc ? -c : c;
    });
    return [...sorted, ...res.rows.filter(isOther)];
  });
  // The format of a value column: the columns are the groups, then the values.
  const groups = $derived(res.columns.filter((c) => c.role === 'dimension').length);
  const formatOf = (i: number) => (res.columns[i]?.role === 'measure' ? (tile.formats?.[i - groups] ?? null) : null);
  // The total row of a table: the engine's own numbers. A value shown another way has no total.
  const totalRow = $derived(tile.type === 'table' && tile.totals && groups > 0 && res.totals ? res.totals.grand : null);
  const totalCell = (i: number) => {
    const j = i - groups;
    const v = totalRow?.[j];
    if (j < 0 || v === null || v === undefined || (!!tile.showAs?.[j] && tile.showAs[j] !== 'value')) return '';
    return cell(v, res.columns[i].dtype, null, false, formatOf(i));
  };
  const heads = $derived(columnHeads(res, tile, tables));
  const showText = (v: Cell, i: number) =>
    v === null && res.columns[i].role === 'measure' ? '–' : cell(v, res.columns[i].dtype, tile.dims[i]?.grain, false, formatOf(i), res.columns[i].unit);

  // How the cells of a value look (build 07). The bars of one value share a scale.
  const condOf = (i: number) => (res.columns[i]?.role === 'measure' ? (tile.cond?.[i - groups] ?? null) : null);
  const scales = $derived(res.columns.map((_, i) => (condOf(i)?.kind === 'bars' ? barScale(res.rows.map((r) => r[i] as number | null)) : null)));
  const kind = $derived(resultDim0(res, tile));
  const verdictOf = (v: Cell, i: number) => {
    const cond = condOf(i);
    return cond?.kind === 'goodbad' ? goodBad(typeof v === 'number' ? v : null, cond, effectiveShowAs(tile, i - groups, kind)) : null;
  };
  /** The line a verdict is read against, for the tooltip: "Above 100". */
  const lineText = (i: number) => {
    const how = effectiveShowAs(tile, i - groups, kind);
    const t = how === 'change' || how === 'pct_change' ? 0 : (condOf(i)?.threshold ?? 0);
    return cell(t, 'number', null, false, how === 'pct_change' ? { percent: true } : formatOf(i));
  };
  const barOf = (v: Cell, i: number) => {
    const sc = scales[i];
    return sc && typeof v === 'number' ? barStyle(barSpan(v, sc)) : '';
  };

  // Find box, and the rows that match it. The find works on the rows loaded, as they read on screen.
  const FIND_FROM = 50;
  let find = $state('');
  const needle = $derived(find.trim().toLowerCase());
  const shownRows = $derived(needle ? rows.filter((r) => r.some((v, i) => showText(v, i).toLowerCase().includes(needle))) : rows);
  const cut = $derived(res.total_rows !== undefined && res.total_rows > res.rows.length);
  const rowsNote = $derived.by(() => {
    const loaded = res.rows.length;
    if (needle) {
      return cut
        ? `${full(shownRows.length)} of the first ${full(loaded)} rows match (${full(res.total_rows!)} in all)`
        : `${full(shownRows.length)} of ${full(loaded)} rows match`;
    }
    if (cut) return `Rows 1–${full(loaded)} of ${full(res.total_rows!)}`;
    return `${full(loaded)} ${loaded === 1 ? 'row' : 'rows'}`;
  });
  const tableTools = $derived(res.rows.length > FIND_FROM || cut);

  // Column widths: dragged live here, kept in the tile after the drop.
  let live = $state<number[] | null>(null);
  const widths = $derived.by(() => {
    const w = live ?? tile.widths;
    return w && w.length === res.columns.length ? w : null;
  });
  const MIN_COL = 48;
  function startResize(e: PointerEvent, i: number) {
    e.preventDefault();
    const grip = e.currentTarget as HTMLElement;
    const start = Array.from(grip.closest('tr')!.children, (el) => Math.round((el as HTMLElement).getBoundingClientRect().width));
    const x0 = e.clientX;
    const next = (x: number) => start.map((w, j) => (j === i ? Math.max(MIN_COL, Math.round(w + x - x0)) : w));
    grip.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (live = next(ev.clientX));
    const up = (ev: PointerEvent) => {
      grip.removeEventListener('pointermove', move);
      grip.removeEventListener('pointerup', up);
      grip.removeEventListener('pointercancel', up);
      const done = next(ev.clientX);
      // Only a real drag changes the widths; a click on the grip changes nothing.
      if (done[i] === start[i]) return void (live = null);
      live = done;
      onwidths?.(done);
    };
    grip.addEventListener('pointermove', move);
    grip.addEventListener('pointerup', up);
    grip.addEventListener('pointercancel', up);
  }
  function resetWidths() {
    live = null;
    onwidths?.(undefined);
  }
  // A new width saved in the tile (or removed) replaces the live one.
  $effect(() => {
    void tile.widths;
    live = null;
  });
  const note = $derived(built?.note ?? null);
  const selKeys = $derived(new Set((selected ?? []).map((v) => String(v))));
</script>

{#if tile.type === 'text'}
  <TextView text={tile.text ?? ''} {editing} />
{:else if tile.type === 'image'}
  <ImageView image={tile.image} {editing} />
{:else if tile.type === 'kpi' && res.kpi}
  <KpiView kpi={res.kpi} {tile} {colors} />
{:else if unfinished}
  <div class="placeholder">{missing}</div>
{:else if res.rows.length === 0}
  <div class="none">No data for this selection.</div>
{:else if tile.type === 'share' || tile.type === 'donut'}
  <ShareView {res} {tile} {colors} {selected} {onpick} />
{:else if tile.type === 'pivot'}
  <PivotView {res} {tile} {tables} {selected} {selectedCol} {onpick} {onpickcol} />
{:else if tile.type === 'table'}
  <div class="tabletile">
    {#if tableTools}
      <input class="find" type="search" placeholder="Find in this table" aria-label="Find in this table" bind:value={find} />
    {/if}
    <div class="tablewrap">
      <table class:fixed={!!widths} style:width={widths ? `${widths.reduce((a, b) => a + b, 0)}px` : undefined}>
        <thead>
          <tr>
            {#each res.columns as c, i}
              <th class:num={c.dtype === 'number'} style:width={widths ? `${widths[i]}px` : undefined}>
                <button onclick={() => (sortBy === i ? (sortDesc = !sortDesc) : ((sortBy = i), (sortDesc = c.dtype === 'number')))}>
                  {heads[i]}{sortBy === i ? (sortDesc ? ' ↓' : ' ↑') : ''}
                </button>
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <span
                  class="grip"
                  role="separator"
                  aria-orientation="vertical"
                  aria-label="Drag to change the width of {heads[i]}. Double click to reset."
                  title="Drag to resize. Double click to reset."
                  onpointerdown={(e) => startResize(e, i)}
                  ondblclick={resetWidths}></span>
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each shownRows as r}
            <tr
              class:dim={selected !== null && !selKeys.has(String(r[0]))}
              class:pick={!!onpick && tile.dims.length > 0}
              class:other={!!res.other && r[0] === OTHER_KEY}
              onclick={() => tile.dims.length && onpick?.(r[0])}>
              {#each r as v, i}
                {@const verdict = verdictOf(v, i)}
                <td
                  class:num={res.columns[i].dtype === 'number'}
                  class:good={verdict?.tone === 'good'}
                  class:bad={verdict?.tone === 'bad'}
                  style={barOf(v, i) || undefined}
                  title={verdict ? `${verdict.word} ${lineText(i)}` : undefined}>
                  {#if verdict}<span class="glyph" role="img" aria-label={verdict.word.toLowerCase()}>{verdict.glyph}</span>{/if}{showText(v, i)}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
        {#if totalRow}
          <tfoot>
            <tr>
              {#each res.columns as c, i}
                <td class:num={c.dtype === 'number'}>{i === 0 ? 'Total' : totalCell(i)}</td>
              {/each}
            </tr>
          </tfoot>
        {/if}
      </table>
      {#if needle && !shownRows.length}<p class="nomatch">Nothing matches "{find.trim()}".</p>{/if}
    </div>
    {#if tableTools}<p class="rowsnote">{rowsNote}</p>{/if}
  </div>
{:else if built}
  <div class="chart">
    <div class="plot">
      <ChartView
        option={built.option}
        horizontal={!!tile.horizontal && tile.type === 'bar'}
        onresize={(w, h) => ((width = w), (height = h))}
        onpick={onpick && !built.pick
          ? (i) => {
              const v = built.categories[i];
              if (i < built.categories.length && v !== undefined) onpick?.(v);
            }
          : undefined}
        onitem={onpick && built.pick?.by === 'item'
          ? (si, i) => {
              const v = built.pick?.by === 'item' ? built.pick.value(si, i) : undefined;
              if (v !== undefined) onpick?.(v);
            }
          : undefined}
        onx={onpick && built.pick?.by === 'x'
          ? (x) => {
              const v = built.pick?.by === 'x' ? built.pick.value(x) : undefined;
              if (v !== undefined) onpick?.(v);
            }
          : undefined} />
    </div>
    {#if note}<p class="size-note">{note}</p>{/if}
  </div>
{/if}

<style>
  .chart { height: 100%; display: flex; flex-direction: column; }
  .plot { flex: 1; min-height: 0; }
  .size-note { flex: none; margin: 4px 0 0; font-size: 11px; color: var(--text-2); }
  .placeholder {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 0 16px;
    text-align: center;
    font-size: 12px;
    color: var(--text-2);
    border: 1.5px dashed var(--sep-strong);
    border-radius: 8px;
  }
  .none { height: 100%; display: grid; place-items: center; color: var(--text-2); font-size: 12px; }
  .tabletile { height: 100%; display: flex; flex-direction: column; }
  .tablewrap { flex: 1; min-height: 0; overflow: auto; margin: 0 -14px; }
  .find {
    flex: none;
    width: 100%;
    height: 26px;
    margin: 0 0 6px;
    padding: 0 8px;
    border: 1px solid var(--sep-strong);
    border-radius: 6px;
    background: var(--surface);
    font-size: 12px;
    color: var(--text);
  }
  .find:focus { outline: none; border-color: var(--focus); }
  .find::placeholder { color: var(--text-2); }
  .rowsnote, .nomatch { flex: none; margin: 4px 0 0; font-size: 11px; color: var(--text-2); }
  .nomatch { padding: 8px 14px; font-size: 12px; }
  table { border-collapse: collapse; width: 100%; font-size: 12px; }
  table.fixed { table-layout: fixed; min-width: 100%; }
  table.fixed td { overflow: hidden; text-overflow: ellipsis; }
  th { position: sticky; top: 0; background: var(--surface); text-align: left; padding: 0; border-bottom: 1px solid var(--sep); }
  /* A narrow strip on the right edge of a heading that takes a drag. */
  .grip { position: absolute; top: 0; right: 0; bottom: 0; width: 8px; cursor: col-resize; touch-action: none; }
  .grip:hover, .grip:active { background: linear-gradient(to right, transparent 3px, var(--sep-strong) 3px, var(--sep-strong) 5px, transparent 5px); }
  th button {
    display: block;
    width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    border: 0;
    background: none;
    padding: 6px 14px;
    font-weight: 600;
    text-align: inherit;
    white-space: nowrap;
  }
  td { padding: 5px 14px; border-bottom: 1px solid var(--sep); white-space: nowrap; }
  .num, .num button { text-align: right; font-variant-numeric: tabular-nums; }
  tr.pick { cursor: pointer; }
  tr.pick:hover td { background-color: var(--window); }
  td.good { color: var(--success); }
  td.bad { color: var(--danger); }
  .glyph { display: inline-block; margin-right: 4px; font-size: 9px; vertical-align: 1px; }
  tr.dim td { opacity: 0.4; }
  tr.other td { color: var(--text-2); }
  tfoot td { position: sticky; bottom: 0; background: var(--surface); font-weight: 600; border-top: 1px solid var(--sep-strong); border-bottom: 0; }
</style>
