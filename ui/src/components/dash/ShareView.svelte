<script lang="ts">
  // Part-to-whole without a pie: one 100 % strip for the shape, then the exact
  // shares as a ranked list. Colours follow the category, never its rank.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { key, type Colors } from '../../lib/chart';
  import { cell, compact, full } from '../../lib/format';

  let {
    res,
    tile,
    colors,
    selected,
    onpick,
  }: { res: QueryResult; tile: Tile; colors: Colors; selected: Cell[] | null; onpick?: (v: Cell) => void } = $props();

  let h = $state(240);

  const items = $derived.by(() => {
    const mi = res.columns.findIndex((c) => c.role === 'measure');
    const rows = res.rows
      .map((r) => ({ raw: r[0], value: typeof r[mi] === 'number' ? (r[mi] as number) : 0 }))
      .filter((r) => r.value > 0)
      .sort((a, b) => b.value - a.value);
    const total = rows.reduce((s, r) => s + r.value, 0);
    // As many rows as fit; the rest become one "Other" row.
    const fit = Math.max(3, Math.min(8, Math.floor((h - 64) / 36)));
    const kept = rows.length > fit ? rows.slice(0, fit - 1) : rows;
    const rest = rows.slice(kept.length);
    const byName = [...kept].sort((a, b) => String(a.raw).localeCompare(String(b.raw), undefined, { numeric: true }));
    const colorOf = new Map(byName.map((r, i) => [key(r.raw), colors.series[i % colors.series.length]]));
    const list = kept.map((r) => ({
      raw: r.raw as Cell,
      other: false,
      label: cell(r.raw, res.columns[0].dtype, tile.dims[0]?.grain),
      value: r.value,
      share: total ? r.value / total : 0,
      color: colorOf.get(key(r.raw))!,
    }));
    if (rest.length) {
      const v = rest.reduce((s, r) => s + r.value, 0);
      list.push({ raw: null, other: true, label: `Other (${rest.length})`, value: v, share: total ? v / total : 0, color: colors.text2 });
    }
    const top = Math.max(...list.map((r) => r.share), 0.0001);
    return { list, total, top };
  });
  // Rows grow to use the tile's height when there are only a few of them.
  const rowH = $derived(Math.max(34, Math.min(48, (h - 64) / Math.max(items.list.length, 1))));

  const isSel = (v: Cell, other: boolean) => !selected || (!other && selected.some((s) => key(s) === key(v)));
  const pct = (x: number) => (x >= 0.995 ? '100%' : x < 0.001 ? '<0.1%' : `${(x * 100).toFixed(x < 0.1 ? 1 : 0)}%`);
</script>

<div class="share" bind:clientHeight={h}>
  <div class="strip" role="img" aria-label="Share of the total">
    {#each items.list as it}
      <span
        style:flex-grow={it.share}
        style:background={it.color}
        style:opacity={isSel(it.raw, it.other) ? 1 : 0.25}
        title="{it.label}: {pct(it.share)}"></span>
    {/each}
  </div>
  <ol>
    {#each items.list as it}
      <li class:dim={!isSel(it.raw, it.other)}>
        <button
          style:height="{rowH}px"
          disabled={it.other || !onpick}
          onclick={() => onpick?.(it.raw)}
          title="{it.label}: {full(it.value)}">
          <i style:background={it.color}></i>
          <span class="name">{it.label}</span>
          <span class="pct">{pct(it.share)}</span>
          <span class="val">{compact(it.value)}</span>
          <span class="track"><span style:width="{(it.share / items.top) * 100}%" style:background={it.color}></span></span>
        </button>
      </li>
    {/each}
  </ol>
  <div class="total"><span>Total</span><span>{full(items.total)}</span></div>
</div>

<style>
  .share { height: 100%; display: flex; flex-direction: column; gap: 14px; min-height: 0; }
  .strip { display: flex; gap: 2px; height: 10px; flex: none; border-radius: 5px; overflow: hidden; }
  .strip span { min-width: 2px; flex-basis: 0; transition: opacity 200ms; }
  ol { list-style: none; margin: 0 -6px; padding: 0; overflow: hidden; display: flex; flex-direction: column; }
  li { transition: opacity 200ms; }
  .total {
    display: flex;
    justify-content: space-between;
    margin-top: auto;
    padding-top: 8px;
    border-top: 1px solid var(--sep);
    font-size: 12px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  li.dim { opacity: 0.4; }
  button {
    width: 100%;
    display: grid;
    grid-template-columns: 8px 1fr auto 52px;
    grid-template-rows: auto 4px;
    align-content: center;
    align-items: center;
    column-gap: 10px;
    row-gap: 5px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    font-size: 12px;
  }
  button:not(:disabled):hover { background: var(--window); }
  button:disabled { cursor: default; }
  i { width: 8px; height: 8px; border-radius: 2px; }
  .track { grid-column: 2 / 5; height: 4px; border-radius: 2px; background: var(--inset); overflow: hidden; }
  .track span { display: block; height: 100%; border-radius: 2px; opacity: 0.85; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pct { font-weight: 600; font-variant-numeric: tabular-nums; }
  .val { text-align: right; color: var(--text-2); font-variant-numeric: tabular-nums; }
</style>
