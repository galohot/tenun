<script lang="ts">
  // A calendar heat map (Advanced): one square for each day of the last 12 months with data. Weeks are
  // columns and weekdays are rows (Monday first in Indonesian, Sunday first in English). Squares are
  // shaded on the one sequential ramp, and a line marks the edge of each month. A day with no data is
  // empty. The engine sent a value for each day; this only lays them out.
  import type { Cell, QueryResult, Tile } from '../../lib/api';
  import type { Colors } from '../../lib/chart';
  import { calendarModel, weekdayOfRow } from '../../lib/charts/calendar';
  import { weekStartSet } from '../../lib/locale';
  import { heatRange, heatStep, inkFor } from '../../lib/charts/heat';
  import { cell, getLocale, numberFormatter, partName } from '../../lib/format';

  let {
    res,
    tile,
    colors,
    selected,
    onpick,
  }: {
    res: QueryResult;
    tile: Tile;
    colors: Colors;
    selected: Cell[] | null;
    onpick?: (v: Cell) => void;
  } = $props();

  const loc = $derived(getLocale());
  // The workspace's week start when one is set (v5 build 04); otherwise Sunday first for English, Monday for Indonesian.
  const mondayFirst = $derived(weekStartSet() ?? loc !== 'en');
  const cal = $derived(calendarModel(res.rows, mondayFirst));
  const unit = $derived(res.columns[1]?.unit);
  const fmt = $derived(numberFormatter(tile.formats?.[0], loc, unit));
  const range = $derived(heatRange(cal.cells.map((c) => c.value)));
  const ramp = $derived(colors.ramp.filter(Boolean));
  const steps = $derived(ramp.length);
  const shade = (v: number | null) => (v === null || !range || !steps ? null : ramp[heatStep(v, range.lo, range.hi, steps)]);
  const key = (v: Cell) => (v === null ? '\u0000null' : String(v));
  const isOn = (day: string) => selected === null || selected.some((s) => key(s) === day);

  let w = $state(600);
  let h = $state(200);
  const LEFT = 30;
  const TOP = 18;
  const LEGEND = 22;
  // A square is as wide as the weeks allow. A tall tile makes it taller than wide (up to twice) so the
  // grid fills the tile; a short one makes it shorter.
  const sx = $derived(Math.max(6, Math.min(24, Math.floor((w - LEFT - 4) / Math.max(cal.weeks, 1)))));
  const sy = $derived(Math.max(6, Math.min(Math.floor((h - TOP - LEGEND) / 7), sx * 2, 30)));
  const size = $derived(Math.min(sx, sy));
  const pad = 2;
  const mx = (c: number) => LEFT + c * sx;
  const my = (r: number) => TOP + r * sy;
  const dayName = (day: string) => cell(day, 'date', 'day');
  // Seven rows, but only Monday, Wednesday and Friday are named.
  const rowNames = $derived(
    Array.from({ length: 7 }, (_, r) => ({ r, n: weekdayOfRow(r, mondayFirst) })).filter((x) => x.n === 1 || x.n === 3 || x.n === 5),
  );
  const compact = $derived(size < 11);
  const outline = (m: (typeof cal.months)[number]) => m.outline.map(([x, y]) => `${mx(x)},${my(y)}`).join(' ');
  // Month names need room: only the first one carries the year, and a name that would touch the one
  // before it is left out. The width of a name is estimated from its letters.
  const nameOf = (m: (typeof cal.months)[number], i: number) => partName(m.month + 1, 'month_of_year', loc) + (i === 0 ? ` ${m.year}` : '');
  const monthLabels = $derived.by(() => {
    const shown: { m: (typeof cal.months)[number]; text: string }[] = [];
    let edge = -Infinity;
    cal.months.forEach((m, i) => {
      const text = nameOf(m, i);
      const x = mx(m.week);
      if (x < edge + 6) return;
      shown.push({ m, text });
      edge = x + text.length * 6.4;
    });
    return shown;
  });
</script>

<div class="cal" bind:clientWidth={w} bind:clientHeight={h}>
  <svg width={w} height={h} role="img" aria-label="Calendar of {res.columns[1]?.name ?? 'values'} by day">
    {#each monthLabels as l}
      <text class="mname" x={mx(l.m.week)} y={TOP - 6}>{l.text}</text>
    {/each}
    {#each rowNames as x}
      <text class="dname" x={LEFT - 6} y={my(x.r) + sy / 2 + 3.5} text-anchor="end">{partName(x.n, 'weekday', loc)}</text>
    {/each}
    {#each cal.cells as d (d.key)}
      {@const bg = shade(d.value)}
      {@const on = isOn(d.key)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <g class:pick={!!onpick && d.value !== null} opacity={on ? 1 : 0.3} onclick={onpick && d.value !== null ? () => onpick?.(d.key) : undefined}>
        <rect
          x={mx(d.week) + pad / 2}
          y={my(d.row) + pad / 2}
          width={sx - pad}
          height={sy - pad}
          rx="2"
          fill={bg ?? 'var(--inset)'}>
          <title>{dayName(d.key)}: {d.value === null ? 'No data' : fmt.tip(d.value)}</title>
        </rect>
        {#if !compact && sx >= 20 && sy >= 20 && bg}
          <text class="dnum" x={mx(d.week) + sx / 2} y={my(d.row) + sy / 2 + 3} text-anchor="middle" fill={inkFor(bg)}>{d.day}</text>
        {/if}
      </g>
    {/each}
    {#each cal.months as m}
      <polygon class="month" points={outline(m)} />
    {/each}
    {#if range}
      {@const lx = LEFT}
      {@const ly = h - 14}
      <text class="lg" x={lx} y={ly + 9}>{fmt.short(range.lo)}</text>
      {#each ramp as c, i}
        <rect x={lx + 36 + i * 14} y={ly} width="12" height="10" rx="2" fill={c} />
      {/each}
      <text class="lg" x={lx + 40 + ramp.length * 14} y={ly + 9}>{fmt.short(range.hi)}</text>
    {/if}
  </svg>
</div>

<style>
  .cal { height: 100%; min-height: 0; }
  svg { display: block; }
  .mname, .dname, .lg { font-size: 11px; fill: var(--text-2); font-family: inherit; }
  .dnum { font-size: 9px; font-variant-numeric: tabular-nums; pointer-events: none; }
  .month { fill: none; stroke: var(--text-2); stroke-width: 1; stroke-opacity: 0.4; pointer-events: none; stroke-linejoin: round; }
  g.pick { cursor: pointer; }
  g.pick:hover rect { stroke: var(--text); stroke-width: 1.5; }
</style>
