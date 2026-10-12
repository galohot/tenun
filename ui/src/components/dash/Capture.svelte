<script lang="ts">
  // The dashboard drawn once, off screen, to be made into a picture. It uses the same tile
  // drawing as the app and the exported file, laid out on the 12-column grid.
  import type { QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { partialNote } from '../../lib/run';
  import { tileTitle, type Named } from '../../lib/format';
  import { COLS, EMPTY_RESULT, isStatic } from '../../lib/tiles';
  import TileBody from './TileBody.svelte';
  import { cardGround } from '../../lib/card';

  let {
    title,
    subtitle,
    filters,
    tiles,
    results,
    tables,
    colors,
    width,
  }: {
    title: string;
    subtitle: string;
    filters: string[];
    tiles: Tile[];
    results: Record<string, QueryResult | null | undefined>;
    tables: Named[];
    colors: Colors;
    width: number;
  } = $props();

  const ROW = 76;
  const GAP = 12;
  const PAD = 24;

  const inner = $derived(width - PAD * 2);
  const colW = $derived((inner - GAP * (COLS - 1)) / COLS);
  // A tile with nothing to show yet (no fields chosen) is left out.
  const shown = $derived(tiles.filter((t) => isStatic(t) || results[t.id]));
  const placed = $derived(
    shown.map((t) => ({
      t,
      left: t.x * (colW + GAP),
      top: t.y * (ROW + GAP),
      w: t.w * colW + (t.w - 1) * GAP,
      h: t.h * ROW + (t.h - 1) * GAP,
    })),
  );
  const height = $derived(Math.max(0, ...placed.map((p) => p.top + p.h)));
</script>

<div class="page" style:width="{width}px" style:padding="{PAD}px">
  <header>
    <h1>{title}</h1>
    <p>{subtitle}</p>
    {#if filters.length}<p class="filters">Filtered by: {filters.join(' · ')}</p>{/if}
  </header>
  <div class="grid" style:height="{height}px">
    {#each placed as p (p.t.id)}
      {@const res = results[p.t.id] ?? EMPTY_RESULT}
      {@const name = tileTitle(p.t, tables, res.parts)}
      {@const ground = cardGround(p.t)}
      <article class="tile" style:transform="translate({p.left}px, {p.top}px)" style:width="{p.w}px" style:height="{p.h}px" style:background={ground ? `var(--${ground})` : undefined}>
        <!-- A number card fills its tile with its ground and draws its own title (build 08). -->
        {#if name && !ground}<h3>{name}</h3>{/if}
        <div class="body" class:bare={!name || !!ground}>
          <TileBody tile={p.t} {res} {colors} {tables} selected={null} title={name} />
        </div>
        {#if p.t.type !== 'kpi' && !isStatic(p.t) && partialNote(res, p.t.dims[0]?.grain)}
          <p class="note">{partialNote(res, p.t.dims[0]?.grain)}</p>
        {/if}
      </article>
    {/each}
  </div>
</div>

<style>
  .page { background: var(--window); color: var(--text); font-family: var(--font); box-sizing: border-box; }
  header { margin-bottom: 16px; }
  h1 { font-size: 22px; }
  header p { margin: 4px 0 0; color: var(--text-2); font-size: 12px; }
  .grid { position: relative; }
  .tile {
    position: absolute;
    top: 0;
    left: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-card);
    background: var(--surface);
    box-shadow: var(--shadow-tile);
    overflow: hidden;
  }
  h3 { padding: 10px 14px 0; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .body { flex: 1; min-height: 0; padding: 6px 14px 10px; }
  .body.bare { padding-top: 12px; }
  .note { margin: 0; padding: 0 14px 8px; font-size: 11px; color: var(--text-2); }
</style>
