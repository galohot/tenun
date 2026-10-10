<script lang="ts">
  // A bullet chart (Advanced): one thin ink bar for each group, a tick for its target, and up to three
  // muted bands behind (poor, fair, good). It replaces a gauge. Every bar sits on one scale. The words
  // say the same thing the picture does: "82 % of target".
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import { at, bandsOf, bulletDomain, bulletTicks, shareOfTarget } from '../../lib/charts/bullet';
  import { cell, measureLabel, numberFormatter, getLocale, pctOfTarget, type Named } from '../../lib/format';

  let {
    res,
    tile,
    selected,
    tables = [],
    onpick,
  }: {
    res: QueryResult;
    tile: Tile;
    selected: Cell[] | null;
    tables?: Named[];
    onpick?: (v: Cell) => void;
  } = $props();

  const nd = $derived(res.columns.filter((c) => c.role === 'dimension').length);
  const hasGroup = $derived(nd > 0);
  // The target is the second value of a row, or one number for every row.
  const rows = $derived(
    res.rows.map((r) => ({
      raw: r[0],
      value: typeof r[nd] === 'number' ? (r[nd] as number) : null,
      target: res.columns.length > nd + 1 ? (typeof r[nd + 1] === 'number' ? (r[nd + 1] as number) : null) : (tile.target?.value ?? null),
    })),
  );
  const bands = $derived(bandsOf(tile.lines));
  const domain = $derived(bulletDomain([...rows.flatMap((r) => [r.value, r.target]), ...bands.flatMap((b) => [b.from, b.to])]));
  const fmt = $derived(numberFormatter(tile.formats?.[0], getLocale(), res.columns[nd]?.unit));
  const tfmt = $derived(numberFormatter(tile.formats?.[1] ?? tile.formats?.[0], getLocale()));
  const pos = (v: number) => `${at(v, domain) * 100}%`;
  const zero = $derived(at(0, domain) * 100);
  const name = (v: Cell) => cell(v, res.columns[0]?.dtype ?? 'text', tile.dims[0]?.grain);
  const key = (v: Cell) => (v === null ? '\u0000null' : String(v));
  const dim = (v: Cell) => hasGroup && selected !== null && !selected.some((s) => key(s) === key(v));
  const valueName = $derived(tile.measures[0] ? measureLabel(tile.measures[0], tables) : (res.columns[nd]?.name ?? ''));
  const ticks = $derived(bulletTicks(domain));

  /** The words for a row, which also name it for a screen reader. */
  function words(r: (typeof rows)[number]): string {
    const share = shareOfTarget(r.value, r.target);
    const v = r.value === null ? 'no value' : fmt.long(r.value);
    if (r.target === null) return v;
    return share === null ? `${v}, target ${tfmt.long(r.target)}` : `${v}, ${pctOfTarget(share)} of target`;
  }
  const bandWords = $derived(bands.map((b) => `${b.label || 'Band'} ${tfmt.short(b.from)} to ${tfmt.short(b.to)}`).join('; '));
</script>

<div class="bullet">
  <ol aria-label={valueName}>
    {#each rows as r}
      {@const share = shareOfTarget(r.value, r.target)}
      <li class:dim={dim(r.raw)}>
        {#if hasGroup}
          {#if onpick}
            <button class="name pick" onclick={() => onpick?.(r.raw)} title="Filter the other tiles to {name(r.raw)}">{name(r.raw)}</button>
          {:else}
            <div class="name" title={name(r.raw)}>{name(r.raw)}</div>
          {/if}
        {/if}
        <div class="track" role="img" aria-label="{hasGroup ? name(r.raw) + ': ' : ''}{words(r)}">
          {#each bands as b, i}
            <span class="band" style:left={pos(b.from)} style:width="{(at(b.to, domain) - at(b.from, domain)) * 100}%" style:--shade={0.1 + 0.05 * (bands.length - 1 - i)} title={b.label}></span>
          {/each}
          <span class="zero" style:left="{zero}%"></span>
          {#if r.value !== null}
            <span class="bar" style:left="{Math.min(zero, at(r.value, domain) * 100)}%" style:width="{Math.abs(at(r.value, domain) * 100 - zero)}%"></span>
          {/if}
          {#if r.target !== null}
            <span class="tick" style:left={pos(r.target)} title="Target {tfmt.long(r.target)}"></span>
          {/if}
        </div>
        <span class="words">
          <b>{r.value === null ? '–' : fmt.short(r.value)}</b>
          {#if share !== null}<span class="share">{pctOfTarget(share)} of target</span>{/if}
        </span>
      </li>
    {/each}
  </ol>
  <div class="axis" aria-hidden="true">
    {#if hasGroup}<span class="spacer"></span>{/if}
    <span class="scale">
      {#each ticks as t, i}<span style:left="{(i / (ticks.length - 1)) * 100}%">{fmt.short(t)}</span>{/each}
    </span>
    <span class="spacer end"></span>
  </div>
  {#if bands.length}<p class="note">{bandWords}</p>{/if}
</div>

<style>
  .bullet { height: 100%; display: flex; flex-direction: column; gap: 4px; min-height: 0; }
  ol { flex: 1; min-height: 0; margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; justify-content: center; gap: 10px; overflow: auto; }
  li { display: grid; grid-template-columns: minmax(70px, 24%) 1fr minmax(92px, auto); align-items: center; gap: 10px; font-size: 12px; }
  li:not(:has(.name)) { grid-template-columns: 1fr minmax(92px, auto); }
  li.dim { opacity: 0.4; }
  .name { border: 0; background: none; padding: 0; text-align: left; font-size: 12px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name.pick { cursor: pointer; }
  .name.pick:hover { text-decoration: underline; }
  .track { position: relative; height: 22px; }
  .band { position: absolute; top: 0; bottom: 0; background: var(--text); opacity: var(--shade); }
  .zero { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--sep-strong); }
  .bar { position: absolute; top: 8px; height: 6px; background: var(--text); border-radius: 1px; }
  .tick { position: absolute; top: 3px; height: 16px; width: 3px; margin-left: -1.5px; background: var(--text); border-radius: 1px; box-shadow: 0 0 0 1.5px var(--surface); }
  .words { display: flex; flex-direction: column; align-items: flex-end; line-height: 1.25; font-variant-numeric: tabular-nums; }
  .share { color: var(--text-2); font-size: 11px; white-space: nowrap; }
  .axis { flex: none; display: grid; grid-template-columns: minmax(70px, 24%) 1fr minmax(92px, auto); gap: 10px; font-size: 11px; color: var(--text-2); }
  .axis:not(:has(.spacer + .scale)) { grid-template-columns: 1fr minmax(92px, auto); }
  .scale { position: relative; height: 14px; font-variant-numeric: tabular-nums; }
  .scale span { position: absolute; transform: translateX(-50%); }
  .scale span:first-child { transform: none; }
  .scale span:last-child { transform: translateX(-100%); }
  .note { flex: none; margin: 0; font-size: 11px; color: var(--text-2); }
</style>
