<script lang="ts">
  // A cohort grid (Advanced): one row for each group of customers (or any entity) that started in the
  // same period, one column for each period since the start. Each cell is the share of the cohort that
  // was active then, shaded on the one sequential ramp with the percent printed. A cell the cohort has
  // not reached yet is empty. The engine did the counting; this only lays the grid out.
  import type { QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { cohortGrid, cohortRange, periodWord } from '../../lib/charts/cohort';
  import { heatStep, inkFor } from '../../lib/charts/heat';
  import { cell, getLocale, numberFormatter } from '../../lib/format';
  import { t } from '../../lib/i18n';

  let { res, tile, colors }: { res: QueryResult; tile: Tile; colors: Colors } = $props();

  const loc = $derived(getLocale());
  const grid = $derived(cohortGrid(res.rows));
  const range = $derived(cohortRange(grid));
  const ramp = $derived(colors.ramp.filter(Boolean));
  const steps = $derived(ramp.length);
  const percent = $derived(res.columns[2]?.unit === 'percent');
  const fmt = $derived(numberFormatter(tile.formats?.[1] ?? null, loc, res.columns[2]?.unit));
  const count = $derived(numberFormatter(null, loc));
  const grain = $derived(tile.dims[0]?.grain);
  const word = $derived(periodWord(grain));
  const shade = (v: number | null) => (v === null || !range || !steps ? null : ramp[heatStep(v, range.lo, range.hi, steps)]);
  const text = (v: number) => (percent ? fmt.long(v) : fmt.short(v));
  const full = (v: number) => (percent ? fmt.long(v) : fmt.tip(v));
  const compact = $derived(grid.columns > 12);
  const what = $derived(percent ? t('were active') : (res.columns[2]?.name ?? t('total')).toLowerCase());
</script>

<div class="cohort">
  <div class="scroll">
    <table aria-label={t('Cohorts by {period} since the start', { period: word.toLowerCase() })}>
      <thead>
        <tr>
          <th class="corner" scope="col">{t('Cohort')}</th>
          <th class="size" scope="col">{t('Size')}</th>
          {#each Array.from({ length: grid.columns }, (_, k) => k) as k}
            <th class="period" scope="col" title="{word} {k}">{k}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each grid.rows as r}
          <tr>
            <th class="rowhead" scope="row">{cell(r.key, 'date', grain)}</th>
            <td class="sz">{count.short(r.size)}</td>
            {#each r.cells as v, k}
              {@const bg = shade(v)}
              <td
                class:empty={v === null}
                class:small={compact}
                style:background={bg ?? undefined}
                style:color={bg ? inkFor(bg) : undefined}
                title={v === null ? t('Not reached yet') : `${cell(r.key, 'date', grain)}, ${word.toLowerCase()} ${k}: ${full(v)} ${percent ? what : ''}`.trim()}>
                {v === null ? '' : text(v)}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="note">{t('{period}s since the start.', { period: word })} {percent ? t('Each cell is the share of the cohort that was active.') : t('Each cell is the {what} of the cohort in that {period}.', { what, period: word.toLowerCase() })}</p>
</div>

<style>
  .cohort { height: 100%; display: flex; flex-direction: column; gap: 4px; }
  .scroll { flex: 1; min-height: 0; overflow: auto; margin: 0 -14px; padding: 0 14px; }
  table { border-collapse: separate; border-spacing: 2px; font-size: 12px; }
  th { font-weight: 600; text-align: left; white-space: nowrap; }
  thead th { position: sticky; top: 0; background: var(--surface); z-index: 1; }
  .corner, .size { color: var(--text-2); font-weight: 500; }
  .size, .sz { text-align: right; padding-right: 8px; font-variant-numeric: tabular-nums; }
  .sz { color: var(--text-2); }
  .period { text-align: center; color: var(--text-2); font-weight: 500; font-variant-numeric: tabular-nums; }
  .rowhead { position: sticky; left: 0; background: var(--surface); padding-right: 8px; font-weight: 500; }
  td { min-width: 46px; height: 26px; padding: 0 6px; text-align: center; font-variant-numeric: tabular-nums; border-radius: 4px; white-space: nowrap; }
  td.small { min-width: 38px; padding: 0 3px; font-size: 11px; }
  td.empty { background: transparent; }
  .note { flex: none; margin: 0; font-size: 11px; color: var(--text-2); }
</style>
