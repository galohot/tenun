<script lang="ts">
  // A pivot table: rows down the side, one field across the top, one or more values in each cell.
  // The engine computed every number, totals included. This only lays them out.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { barScale, barSpan, barStyle, goodBad } from '../../lib/cells';
  import { cell, columnHeads, resultDim0, type Named } from '../../lib/format';
  import { effectiveShowAs } from '../../lib/showas';
  import { groupKeys, isExpandable, MAX_PIVOT_ROWS, pivotModel, pivotTree, visibleNodes } from '../../lib/pivot';

  let {
    res,
    tile,
    tables = [],
    selected,
    selectedCol,
    onpick,
    onpickcol,
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

  // How the cells of a value look (build 07). All the cells of one value share a scale; totals have none.
  const kind = $derived(resultDim0(res, tile));
  const condOf = (j: number) => tile.cond?.[j] ?? null;
  const scales = $derived(
    Array.from({ length: m.nValues }, (_, j) =>
      condOf(j)?.kind === 'bars' ? barScale(m.rows.flatMap((r) => r.cells.map((c) => (typeof c?.[j] === 'number' ? (c[j] as number) : null)))) : null,
    ),
  );
  const barOf = (v: Cell | undefined, j: number) => {
    const sc = scales[j];
    return sc && typeof v === 'number' ? barStyle(barSpan(v, sc)) : '';
  };
  const verdictOf = (v: Cell | undefined, j: number) => {
    const cond = condOf(j);
    return cond?.kind === 'goodbad' ? goodBad(typeof v === 'number' ? v : null, cond, effectiveShowAs(tile, j, kind), show(v, j), lineText(j)) : null;
  };
  const lineText = (j: number) => {
    const how = effectiveShowAs(tile, j, kind);
    const t = how === 'change' || how === 'pct_change' ? 0 : (condOf(j)?.threshold ?? 0);
    return cell(t, 'number', null, false, how === 'pct_change' ? { percent: true } : fmtOf(j));
  };

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
  // Expandable groups (Advanced): the first row fields open and close. A closed group shows its subtotal.
  const expandable = $derived(!!tile.expand && isExpandable(m));
  const tree = $derived(expandable ? pivotTree(m) : []);
  let closed = $state<Set<string>>(new Set());
  const shown = $derived(expandable ? visibleNodes(tree, closed) : []);
  function toggle(k: string) {
    const next = new Set(closed);
    if (!next.delete(k)) next.add(k);
    closed = next;
  }
  const rowFields = $derived(Array.from({ length: m.nRows }, (_, i) => i));
  const rowSel = (raw: Cell[]) => selected === null || selected.some((s) => key(s) === key(raw[0]));
  const colSel = (raw: Cell) => selectedCol === null || selectedCol.some((s) => key(s) === key(raw));
</script>

{#snippet dataCells(cells: (Cell[] | null)[], rowTotal: Cell[] | null, bold: boolean)}
  {#each cells as c, ci}
    {#each values as j}
      {@const verdict = verdictOf(c?.[j], j)}
      <td
        class="num"
        class:strong={bold}
        class:dim={!colSel(m.cols[ci].raw)}
        class:empty={blank(c?.[j])}
        class:good={verdict?.tone === 'good'}
        class:bad={verdict?.tone === 'bad'}
        style={barOf(c?.[j], j) || undefined}
        title={verdict ? `${verdict.word} ${lineText(j)}` : undefined}>
        {#if verdict}<span class="glyph" role="img" aria-label={verdict.word.toLowerCase()}>{verdict.glyph}</span>{/if}{show(c?.[j], j)}
      </td>
    {/each}
  {/each}
  {#if hasRowTotals}
    {#each values as j}<td class="num strong">{total(rowTotal, j)}</td>{/each}
  {/if}
{/snippet}

<div class="pivot">
  {#if expandable}
    <div class="tools">
      <button class="link" onclick={() => (closed = new Set())}>Open all</button>
      <button class="link" onclick={() => (closed = new Set(groupKeys(tree)))}>Close all</button>
    </div>
  {/if}
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
        {#if expandable}
          {#each shown as n (n.group ? `g${n.group.key}` : `r${n.row?.key}`)}
            {#if n.group}
              {@const g = n.group}
              <tr class="group" class:dim={!rowSel(g.raw)} class:pick={!!onpick}>
                {#each rowFields as i}
                  <th class="rowhead" class:first={i === 0} scope="row" colspan={i === g.depth ? m.nRows - g.depth : 1} hidden={i > g.depth}>
                    {#if i === g.depth}
                      <button class="twisty" aria-expanded={!closed.has(g.key)} aria-label="{closed.has(g.key) ? 'Open' : 'Close'} {rowLabel(g.raw[g.depth], g.depth)}" onclick={() => toggle(g.key)}>
                        <span class="chev" class:shut={closed.has(g.key)} aria-hidden="true">▾</span>
                      </button>
                      {#if onpick && g.depth === 0}
                        <button class="name" onclick={() => onpick?.(g.raw[0])} title="Filter the other tiles to this row">{rowLabel(g.raw[g.depth], g.depth)}</button>
                      {:else}
                        {rowLabel(g.raw[g.depth], g.depth)}
                      {/if}
                    {/if}
                  </th>
                {/each}
                {@render dataCells(g.cells, g.total, true)}
              </tr>
            {:else if n.row}
              {@const r = n.row}
              <tr class:dim={!rowSel(r.raw)}>
                {#each r.raw as v, i}
                  <th class="rowhead" class:first={i === 0} scope="row">{#if i === m.nRows - 1}{rowLabel(v, i)}{/if}</th>
                {/each}
                {@render dataCells(r.cells, r.total, false)}
              </tr>
            {/if}
          {/each}
        {:else}
        {#each m.rows as r, ri}
          <tr class:dim={!rowSel(r.raw)} class:pick={!!onpick}>
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
            {@render dataCells(r.cells, r.total, false)}
          </tr>
        {/each}
        {/if}
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
  .tools { flex: none; display: flex; gap: 12px; justify-content: flex-end; margin: 0 0 4px; }
  .tools .link { border: 0; background: none; padding: 0; font-size: 11px; color: var(--accent-text); }
  .tools .link:hover { text-decoration: underline; }
  tr.group th, tr.group td { font-weight: 600; background: var(--window); }
  .twisty { border: 0; background: none; padding: 0 4px 0 0; width: 18px; color: var(--text-2); }
  .chev { display: inline-block; transition: transform 0.12s; }
  .chev.shut { transform: rotate(-90deg); }
  .rowhead .name { border: 0; background: none; padding: 0; font-weight: 600; text-align: left; }
  .rowhead .name:hover { text-decoration: underline; }
  table { border-collapse: separate; border-spacing: 0; width: 100%; font-size: 12px; }
  th, td { padding: 5px 12px; border-bottom: 1px solid var(--sep); white-space: nowrap; background: var(--surface); }
  td.num { text-align: right; font-variant-numeric: tabular-nums; }
  td.empty { color: var(--text-3); }
  td.good { color: var(--success); }
  td.bad { color: var(--danger); }
  .glyph { display: inline-block; margin-right: 4px; font-size: 9px; vertical-align: 1px; }
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
