<script lang="ts">
  // A pivot table: rows down the side, one field across the top, one or more values in each cell.
  // The engine computed every number, totals included. This only lays them out.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { cell, columnHeads, type Named } from '../../lib/format';
  import { MAX_PIVOT_ROWS, pivotModel } from '../../lib/pivot';

  let {
    res,
    tile,
    tables = [],
    selected,
    selectedCol,
    onpick,
    onpickcol,
    oncontext,
  }: {
    res: QueryResult;
    tile: Tile;
    /** For the names of the values. */
    tables?: Named[];
    /** Values of the first row field picked by a click on a row header. */
    selected: Cell[] | null;
    /** Values of the column field picked by a click on a column header. */
    selectedCol: Cell[] | null;
    onpick?: (v: Cell) => void;
    onpickcol?: (v: Cell) => void;
    /** A right-click on a row: the value of its first field. */
    oncontext?: (v: Cell, e: MouseEvent) => void;
  } = $props();

  const m = $derived(pivotModel(res, tile));
  const nd = $derived(res.columns.filter((c) => c.role === 'dimension').length);
  const heads = $derived(columnHeads(res, tile, tables));
  const vname = (j: number) => heads[nd + j] ?? '';
  const fmtOf = (j: number) => tile.formats?.[j] ?? null;
  const blank = (v: Cell | undefined) => v === null || v === undefined;
  // A value shown another way (build 06) has no meaningful total.
  const noTotal = (j: number) => !!tile.showAs?.[j] && tile.showAs[j] !== 'value';
  const unitOf = (j: number) => res.columns[nd + j]?.unit;
  const show = (v: Cell | undefined, j: number) => (blank(v) ? '–' : cell(v as Cell, 'number', null, false, fmtOf(j), unitOf(j)));
  const total = (cells: Cell[] | null | undefined, j: number) => (!cells || noTotal(j) || blank(cells[j]) ? '' : cell(cells[j], 'number', null, false, fmtOf(j)));
  const key = (v: Cell) => (v === null ? '\u0000null' : String(v));
  const rowDimName = (i: number) => heads[i] ?? '';
  const colLabel = (v: Cell) => cell(v, res.columns[m.nRows]?.dtype ?? 'text', tile.cols?.grain);
  const rowLabel = (v: Cell, i: number) => cell(v, res.columns[i]?.dtype ?? 'text', tile.dims[i]?.grain);

  const hasRowTotals = $derived(m.hasCols && m.rows.some((r) => r.total));
  // With no row field there is one row, which already is the total.
  const hasTotalRow = $derived(!!m.grand && m.nRows > 0);
  const twoHeads = $derived(m.hasCols && m.nValues > 1);
  const values = $derived(Array.from({ length: m.nValues }, (_, j) => j));
  // A repeated row heading is left blank, so groups read as groups.
  const shownRow = (r: number, i: number) => {
    if (i === m.nRows - 1 || r === 0) return true;
    for (let k = 0; k <= i; k++) if (key(m.rows[r].raw[k]) !== key(m.rows[r - 1].raw[k])) return true;
    return false;
  };
  const rowSel = (raw: Cell[]) => selected === null || selected.some((s) => key(s) === key(raw[0]));
  const colSel = (raw: Cell) => selectedCol === null || selectedCol.some((s) => key(s) === key(raw));
</script>

<div class="pivot">
  <div class="scroll">
    <table>
      <thead>
        <tr>
          {#each Array.from({ length: m.nRows }, (_, i) => i) as i}
            <th class="corner" class:first={i === 0} rowspan={twoHeads ? 2 : 1} scope="col">{rowDimName(i)}</th>
          {/each}
          {#if m.hasCols}
            {#each m.cols as c}
              <th class="colhead" colspan={m.nValues} class:dim={!colSel(c.raw)} scope="colgroup">
                {#if onpickcol}
                  <button onclick={() => onpickcol?.(c.raw)} title="Filter the other tiles to this column">{colLabel(c.raw)}</button>
                {:else}
                  <span>{colLabel(c.raw)}</span>
                {/if}
              </th>
            {/each}
            {#if hasRowTotals}<th class="colhead totalhead" colspan={m.nValues} scope="colgroup">Total</th>{/if}
          {:else}
            {#each values as j}<th class="colhead" scope="col">{vname(j)}</th>{/each}
          {/if}
        </tr>
        {#if twoHeads}
          <tr class="sub">
            {#each [...m.cols, ...(hasRowTotals ? [null] : [])] as c}
              {#each values as j}<th class="valhead" class:dim={c !== null && !colSel(c.raw)} scope="col">{vname(j)}</th>{/each}
            {/each}
          </tr>
        {/if}
      </thead>
      <tbody>
        {#each m.rows as r, ri}
          <tr
            class:dim={!rowSel(r.raw)}
            class:pick={!!onpick}
            oncontextmenu={(e) => {
              if (!oncontext || m.nRows === 0) return;
              e.preventDefault();
              oncontext(r.raw[0], e);
            }}>
            {#each r.raw as v, i}
              <th class="rowhead" class:first={i === 0} scope="row">
                {#if shownRow(ri, i)}
                  {#if onpick && i === 0}
                    <button onclick={() => onpick?.(r.raw[0])} title="Filter the other tiles to this row">{rowLabel(v, i)}</button>
                  {:else}
                    {rowLabel(v, i)}
                  {/if}
                {/if}
              </th>
            {/each}
            {#each r.cells as c, ci}
              {#each values as j}
                <td class="num" class:dim={!colSel(m.cols[ci].raw)} class:empty={blank(c?.[j])}>{show(c?.[j], j)}</td>
              {/each}
            {/each}
            {#if hasRowTotals}
              {#each values as j}<td class="num strong">{total(r.total, j)}</td>{/each}
            {/if}
          </tr>
        {/each}
      </tbody>
      {#if hasTotalRow}
        <tfoot>
          <tr>
            <th class="rowhead first" colspan={Math.max(m.nRows, 1)} scope="row">Total</th>
            {#each m.cols as c, ci}
              {#each values as j}
                <td class="num">{total(m.hasCols ? m.colTotals[ci] : m.grand, j)}</td>
              {/each}
            {/each}
            {#if hasRowTotals}
              {#each values as j}<td class="num">{total(m.grand, j)}</td>{/each}
            {/if}
          </tr>
        </tfoot>
      {/if}
    </table>
  </div>
  {#if m.cut || m.limited}
    <p class="note">
      {#if m.cut}Showing {m.cols.length} of {m.colCount} columns.{/if}
      {#if m.limited}Showing the first {MAX_PIVOT_ROWS.toLocaleString('en-US')} cells. Add a filter to see the rest.{/if}
    </p>
  {/if}
</div>

<style>
  .pivot { height: 100%; display: flex; flex-direction: column; }
  .scroll { flex: 1; min-height: 0; overflow: auto; margin: 0 -14px; }
  .note { flex: none; margin: 4px 0 0; font-size: 11px; color: var(--text-2); }
  table { border-collapse: separate; border-spacing: 0; width: 100%; font-size: 12px; }
  th, td { padding: 5px 12px; border-bottom: 1px solid var(--sep); white-space: nowrap; background: var(--surface); }
  td.num { text-align: right; font-variant-numeric: tabular-nums; }
  td.empty { color: var(--text-3); }
  td.strong, tfoot td { font-weight: 600; }
  thead th { position: sticky; top: 0; z-index: 2; font-weight: 600; text-align: right; padding: 0; }
  thead tr.sub th { top: 28px; font-weight: 500; color: var(--text-2); padding: 5px 12px; }
  .colhead span, .colhead button { display: block; padding: 6px 12px; }
  .colhead button { width: 100%; border: 0; background: none; font-weight: 600; text-align: right; }
  .colhead button:hover { background: var(--window); }
  .colhead:not(:has(button)) { padding: 6px 12px; }
  .corner { text-align: left; padding: 6px 12px; z-index: 3; vertical-align: bottom; }
  .rowhead { text-align: left; font-weight: 500; }
  .rowhead.first, .corner.first { position: sticky; left: 0; z-index: 1; }
  thead .corner.first { z-index: 4; }
  .rowhead button { border: 0; background: none; padding: 0; font-weight: 500; text-align: left; }
  .rowhead button:hover { text-decoration: underline; }
  tfoot th, tfoot td { position: sticky; bottom: 0; z-index: 2; border-top: 1px solid var(--sep-strong); border-bottom: 0; }
  tfoot th.first { z-index: 3; font-weight: 600; }
  tr.dim th, tr.dim td { opacity: 0.4; }
  td.dim, th.dim { opacity: 0.4; }
  tr.pick:hover td, tr.pick:hover th { background: var(--window); }
</style>
