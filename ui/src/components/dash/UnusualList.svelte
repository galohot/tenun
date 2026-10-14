<script lang="ts">
  // The unusual values of a chart (v5 build 09), under it: "3 unusual values", which opens to list each with its words.
  // Screen readers read the list; the rings on the chart are only for the eye.
  import type { QueryResult, Tile } from '../../lib/api';
  import { cell, numberFormatter } from '../../lib/format';
  import { unusualCount, unusualWords } from '../../lib/unusual';

  let { res, tile }: { res: QueryResult; tile: Tile } = $props();

  const grain = $derived(tile.dims[0]?.grain);
  const nd = $derived(res.columns.filter((c) => c.role === 'dimension').length);
  const items = $derived(
    (res.unusual ?? []).flatMap((u) => {
      const row = res.rows[u.at];
      if (!row) return [];
      const when = cell(row[0], res.columns[0]?.dtype ?? 'date', grain);
      const series = nd >= 2 ? ` · ${cell(row[1], res.columns[1]?.dtype ?? 'text', tile.dims[1]?.grain)}` : '';
      const value = res.columns[nd + (u.measure ?? 0)];
      const fmt = numberFormatter(tile.formats?.[u.measure ?? 0] ?? null, undefined, value?.unit);
      const v = row[nd + (u.measure ?? 0)];
      return [{ key: `${u.at}:${u.measure ?? 0}`, head: `${when}${series}: ${typeof v === 'number' ? fmt.short(v) : '–'}`, words: unusualWords(u, grain, fmt.short) }];
    }),
  );
</script>

{#if items.length}
  <details class="unusual">
    <summary>{unusualCount(items.length)}</summary>
    <ul>
      {#each items as it (it.key)}
        <li><b>{it.head}.</b> {it.words}</li>
      {/each}
    </ul>
  </details>
{/if}

<style>
  .unusual { font-size: 12px; color: var(--text-2); }
  summary { cursor: pointer; width: max-content; }
  summary:hover { color: var(--text); }
  ul { margin: 4px 0 0; padding-left: 18px; display: flex; flex-direction: column; gap: 2px; }
  b { color: var(--text); font-weight: 600; }
</style>
