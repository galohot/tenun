<script lang="ts">
  // The dashboard drawn once, off screen, to be made into a picture. It uses the same tile
  // drawing as the app and the exported file, laid out on the 12-column grid.
  import type { QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { partialNote } from '../../lib/run';
  import { tileTitle, type Named } from '../../lib/format';
  import { COLS, EMPTY_RESULT, isStatic } from '../../lib/tiles';
  import TileBody from './TileBody.svelte';
  import FlowLayout from './FlowLayout.svelte';
  import { flowOrder } from '../../lib/flow';
  import { cardGround } from '../../lib/card';
  import { t } from '../../lib/i18n';

  let {
    title,
    subtitle,
    filters,
    tiles,
    results,
    tables,
    colors,
    width,
    layout,
    order,
  }: {
    title: string;
    subtitle: string;
    filters: string[];
    tiles: Tile[];
    results: Record<string, QueryResult | null | undefined>;
    tables: Named[];
    colors: Colors;
    width: number;
    /** A story draws as a column in its order (build 11). */
    layout?: 'grid' | 'flow';
    order?: string[];
  } = $props();
  const story = $derived(layout === 'flow');

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

<div class="page" class:paper={story} style:width="{width}px" style:padding="{PAD}px">
  <header>
    <h1>{title}</h1>
    <p>{subtitle}</p>
    {#if filters.length}<p class="filters">{t('Filtered by: {filters}', { filters: filters.join(' · ') })}</p>{/if}
  </header>
  <!-- One tile: on the grid at its place, or in a story's column (build 11). A text may be cut across pages of a PDF. -->
  {#snippet frame(t: Tile, box: { left: number; top: number; w: number; h: number } | null)}
    {@const res = results[t.id] ?? EMPTY_RESULT}
    {@const name = tileTitle(t, tables, res.parts)}
    {@const ground = cardGround(t)}
    <article
      class="tile"
      class:flowtile={!box}
      class:grounded={!!ground && ground !== 'surface'}
      data-tile={t.id}
      data-split={t.type === 'text' ? '1' : undefined}
      style:transform={box ? `translate(${box.left}px, ${box.top}px)` : undefined}
      style:width={box ? `${box.w}px` : undefined}
      style:height={box ? `${box.h}px` : undefined}
      style:background={ground ? `var(--${ground})` : undefined}>
      <!-- A number card fills its tile with its ground and draws its own title (build 08). -->
      {#if name && !ground}<h3>{name}</h3>{/if}
      <div class="body" class:bare={!name || !!ground}>
        <TileBody tile={t} {res} {colors} {tables} selected={null} title={name} />
      </div>
      {#if t.type !== 'kpi' && !isStatic(t) && partialNote(res, t.dims[0]?.grain)}
        <p class="note">{partialNote(res, t.dims[0]?.grain)}</p>
      {/if}
    </article>
  {/snippet}
  {#if story}
    <FlowLayout tiles={flowOrder({ tiles: shown, order })}>
      {#snippet cell(t)}{@render frame(t, null)}{/snippet}
    </FlowLayout>
  {:else}
    <div class="grid" style:height="{height}px">
      {#each placed as p (p.t.id)}
        {@render frame(p.t, p)}
      {/each}
    </div>
  {/if}
</div>

<style>
  .page { background: var(--window); color: var(--text); font-family: var(--font); box-sizing: border-box; }
  header { margin-bottom: 16px; }
  h1 { font-size: 22px; }
  header p { margin: 4px 0 0; color: var(--text-2); font-size: 12px; }
  .grid { position: relative; }
  .page.paper { background: var(--surface); }
  .tile.flowtile { position: relative; width: 100%; height: 100%; background: transparent; box-shadow: none; }
  .tile.flowtile h3 { padding-left: 0; font-size: 15px; }
  .tile.flowtile:not(.grounded) .body { padding-left: 0; padding-right: 0; }
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
