<script lang="ts">
  // The exported dashboard. Everything it shows was computed when the file was
  // made; clicking a bar or a slice switches to the stored filtered results.
  import { onMount } from 'svelte';
  import type { Cell, Tile } from '../lib/api';
  import type { ExportPayload } from '../lib/export';
  import { key, readColors, type Colors } from '../lib/chart';
  import { cell, dimLabel, tileTitle } from '../lib/format';
  import TileBody from '../components/dash/TileBody.svelte';
  import { partialNote } from '../lib/run';

  const data = (window as unknown as { TENUN: ExportPayload }).TENUN;
  const ROW = 76;
  const GAP = 12;

  let dark = $state(matchMedia('(prefers-color-scheme: dark)').matches);
  let colors = $state<Colors | null>(null);
  let width = $state(1000);
  let pick = $state<{ tile: string; value: Cell } | null>(null);

  onMount(() => {
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => (dark = e.matches));
  });

  $effect(() => {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    requestAnimationFrame(() => (colors = readColors()));
  });

  const narrow = $derived(width < 720);
  const colW = $derived((width - GAP * 11) / 12);

  const layout = $derived.by(() => {
    if (!narrow) return data.tiles.map((t) => ({ t, x: t.x * (colW + GAP), y: t.y * (ROW + GAP), w: t.w * colW + (t.w - 1) * GAP, h: t.h * ROW + (t.h - 1) * GAP }));
    let y = 0;
    return [...data.tiles]
      .sort((a, b) => a.y - b.y || a.x - b.x)
      .map((t) => {
        const h = Math.max(2, t.type === 'kpi' ? 2 : 4) * ROW;
        const r = { t, x: 0, y, w: width, h };
        y += h + GAP;
        return r;
      });
  });
  const height = $derived(Math.max(0, ...layout.map((l) => l.y + l.h)));

  function resultFor(t: Tile) {
    if (!pick || pick.tile === t.id) return { res: data.base[t.id], offline: false };
    const r = data.cross[pick.tile]?.[key(pick.value)]?.[t.id];
    return r ? { res: r, offline: false } : { res: data.base[t.id], offline: true };
  }

  function onpick(t: Tile, v: Cell) {
    if (!data.cross[t.id]) return;
    pick = pick && pick.tile === t.id && key(pick.value) === key(v) ? null : { tile: t.id, value: v };
  }

  function label(tileId: string, v: Cell): string {
    const t = data.tiles.find((x) => x.id === tileId)!;
    const d = t.dims[0];
    const dtype = data.tables.find((x) => x.id === d.table)?.columns.find((c) => c.name === d.column)?.dtype ?? 'text';
    const name = dimLabel(d, data.tables);
    return `${name.charAt(0).toUpperCase() + name.slice(1)}: ${cell(v, dtype, d.grain)}`;
  }

  const when = new Date(data.generated).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
</script>

<div class="page">
  <header>
    <div>
      <h1>{data.title}</h1>
      <p>{data.workspace} · exported {when}</p>
    </div>
    <div class="chips">
      {#each data.filters as f}
        <span class="chip static">{label(f.fromTile, f.values[0])}</span>
      {/each}
      {#if pick}
        <span class="chip">
          {label(pick.tile, pick.value)}
          <button aria-label="Clear" onclick={() => (pick = null)}>×</button>
        </span>
      {/if}
    </div>
  </header>

  <div class="grid" bind:clientWidth={width} style:height="{height}px">
    {#if colors}
      {#each layout as l (l.t.id)}
        {@const r = resultFor(l.t)}
        <article class="tile" style:transform="translate({l.x}px, {l.y}px)" style:width="{l.w}px" style:height="{l.h}px">
          <h3>{data.titles?.[l.t.id] ?? tileTitle(l.t, data.tables)}</h3>
          <div class="body">
            {#if r.res}
              <TileBody
                tile={l.t}
                res={r.res}
                {colors}
                selected={pick?.tile === l.t.id ? [pick.value] : null}
                onpick={data.cross[l.t.id] ? (v) => onpick(l.t, v) : undefined} />
            {/if}
          </div>
          {#if r.res && l.t.type !== 'kpi' && partialNote(r.res, l.t.dims[0]?.grain)}
            <p class="note">{partialNote(r.res, l.t.dims[0]?.grain)}</p>
          {/if}
          {#if r.offline}<p class="note">Not filtered in this file</p>{/if}
        </article>
      {/each}
    {/if}
  </div>

  <footer>
    Made with <a href="https://github.com/galohot/tenun" target="_blank" rel="noopener">Tenun</a> · the data in this file is a snapshot.
  </footer>
</div>

<style>
  :global(body) { background: var(--window); }
  .page { max-width: 1440px; margin: 0 auto; padding: 24px; }
  header { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; margin-bottom: 16px; flex-wrap: wrap; }
  h1 { font-size: 22px; }
  header p { margin: 4px 0 0; color: var(--text-2); font-size: 12px; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 4px 0 10px;
    border-radius: 12px;
    background: var(--selected);
    color: var(--accent-text);
    font-size: 12px;
    font-weight: 500;
  }
  .chip.static { padding-right: 10px; background: var(--inset); color: var(--text-2); }
  .chip button { border: 0; background: none; color: inherit; font-size: 15px; line-height: 1; padding: 0 6px; cursor: pointer; }
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
  .note { margin: 0; padding: 0 14px 8px; font-size: 11px; color: var(--text-2); }
  footer { margin-top: 24px; font-size: 12px; color: var(--text-2); text-align: center; }
  footer a { color: var(--accent-text); }
</style>
