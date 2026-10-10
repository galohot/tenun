<script lang="ts">
  // A heat map (Advanced): the cells of a pivot of one value, each shaded on the one sequential ramp,
  // light to dark. The number is printed in every cell, in the ink that reads best on its shade. The
  // engine computed every number and the order of rows and columns; this only lays them out.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { heatRange, heatStep, inkFor } from '../../lib/charts/heat';
  import { cell, columnHeads, type Named } from '../../lib/format';
  import { pivotModel } from '../../lib/pivot';

  let {
    res,
    tile,
    colors,
    tables = [],
    selected,
    selectedCol,
    onpick,
    onpickcol,
  }: {
    res: QueryResult;
    tile: Tile;
    colors: Colors;
    tables?: Named[];
    /** Values of the row field picked by a click on a row heading. */
    selected: Cell[] | null;
    /** Values of the column field picked by a click on a column heading. */
    selectedCol: Cell[] | null;
    onpick?: (v: Cell) => void;
    onpickcol?: (v: Cell) => void;
  } = $props();

  const m = $derived(pivotModel(res, tile));
  const nd = $derived(res.columns.filter((c) => c.role === 'dimension').length);
  const heads = $derived(columnHeads(res, tile, tables));
  const fmt = $derived(tile.formats?.[0] ?? null);
  const unit = $derived(res.columns[nd]?.unit);
  const key = (v: Cell) => (v === null ? '\u0000null' : String(v));
  const numberAt = (c: Cell[] | null | undefined) => (typeof c?.[0] === 'number' ? (c[0] as number) : null);

  const range = $derived(heatRange(m.rows.flatMap((r) => r.cells.map(numberAt))));
  const ramp = $derived(colors.ramp.filter(Boolean));
  const steps = $derived(ramp.length);
  const shade = (v: number | null) => (v === null || !range || !steps ? null : ramp[heatStep(v, range.lo, range.hi, steps)]);
  // A wide grid prints the short form of a number, so it fits.
  const compactCells = $derived(m.cols.length > 7);
  const show = (v: number) => cell(v, 'number', null, compactCells, fmt, unit);
  const full = (v: number) => cell(v, 'number', null, false, fmt, unit);

  const rowName = $derived(heads[0] ?? '');
  const colName = $derived(heads[1] ?? '');
  const colLabel = (v: Cell) => cell(v, res.columns[1]?.dtype ?? 'text', tile.cols?.grain);
  const rowLabel = (v: Cell) => cell(v, res.columns[0]?.dtype ?? 'text', tile.dims[0]?.grain);
  const rowSel = (raw: Cell[]) => selected === null || selected.some((s) => key(s) === key(raw[0]));
  const colSel = (raw: Cell) => selectedCol === null || selectedCol.some((s) => key(s) === key(raw));
  const valueName = $derived(heads[nd] ?? '');
</script>

<div class="heat">
  <div class="scroll">
    <table aria-label="{valueName} by {rowName} and {colName}">
      <thead>
        <tr>
          <th class="corner" scope="col">{rowName}</th>
          {#each m.cols as c}
            <th class="colhead" class:dim={!colSel(c.raw)} scope="col">
              {#if onpickcol && m.hasCols}
                <button onclick={() => onpickcol?.(c.raw)} title="Filter the other tiles to this column">{colLabel(c.raw)}</button>
              {:else}
                <span>{m.hasCols ? colLabel(c.raw) : valueName}</span>
              {/if}
            </th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each m.rows as r}
          <tr class:dim={!rowSel(r.raw)}>
            <th class="rowhead" scope="row">
              {#if onpick}
                <button onclick={() => onpick?.(r.raw[0])} title="Filter the other tiles to this row">{rowLabel(r.raw[0])}</button>
              {:else}
                <span>{rowLabel(r.raw[0])}</span>
              {/if}
            </th>
            {#each r.cells as c, ci}
              {@const v = numberAt(c)}
              {@const bg = shade(v)}
              <td
                class:empty={v === null}
                class:dim={!colSel(m.cols[ci].raw)}
                style:background={bg ?? undefined}
                style:color={bg ? inkFor(bg) : undefined}
                title={v === null ? 'No data' : `${rowLabel(r.raw[0])} · ${m.hasCols ? colLabel(m.cols[ci].raw) : valueName}: ${full(v)}`}>
                {v === null ? '' : show(v)}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  {#if range}
    <div class="legend" aria-label="Scale from {full(range.lo)} to {full(range.hi)}">
      <span class="end">{show(range.lo)}</span>
      <span class="ramp" style:background="linear-gradient(to right, {ramp.map((c, i) => `${c} ${(i / steps) * 100}% ${((i + 1) / steps) * 100}%`).join(', ')})"></span>
      <span class="end">{show(range.hi)}</span>
    </div>
  {/if}
  {#if m.cut}<p class="cutnote">Showing {m.cols.length} of {m.colCount} columns.</p>{/if}
</div>

<style>
  .heat { height: 100%; display: flex; flex-direction: column; gap: 6px; }
  .scroll { flex: 1; min-height: 0; overflow: auto; margin: 0 -14px; padding: 0 14px; }
  table { border-collapse: separate; border-spacing: 2px; width: 100%; font-size: 12px; }
  th { font-weight: 600; text-align: left; white-space: nowrap; }
  thead th { position: sticky; top: 0; background: var(--surface); z-index: 1; }
  .corner { color: var(--text-2); font-weight: 500; }
  .colhead { text-align: center; }
  .colhead.dim, td.dim, tr.dim th { opacity: 0.4; }
  tr.dim td { opacity: 0.4; }
  th button { border: 0; background: none; padding: 2px 4px; font-weight: 600; border-radius: 4px; }
  th button:hover { background: var(--hover); }
  .rowhead { position: sticky; left: 0; background: var(--surface); max-width: 160px; overflow: hidden; text-overflow: ellipsis; padding-right: 8px; }
  td { min-width: 52px; height: 28px; padding: 0 8px; text-align: center; font-variant-numeric: tabular-nums; border-radius: 4px; white-space: nowrap; }
  td.empty { background: var(--inset); }
  .legend { flex: none; display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--text-2); }
  .ramp { width: 120px; height: 8px; border-radius: 4px; }
  .end { font-variant-numeric: tabular-nums; }
  .cutnote { flex: none; margin: 0; font-size: 11px; color: var(--text-2); }
</style>
