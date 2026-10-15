<script lang="ts">
  // A filter tile: a list of values, a dropdown, a range of numbers or a span of dates. In the app it
  // edits the selection saved in the tile. In an exported file the selection is the one saved, and a
  // list can be clicked (`onpick`) because the file holds the results for each value.
  import type { Cell, DType, QueryResult, Tile, TileFilter } from '../../lib/api';
  import { cell, dimLabel, pretty, type Named } from '../../lib/format';
  import { DATE_PRESETS, keyOf } from '../../lib/filters';
  import { portal } from '../../lib/portal';
  import Icon from '../Icon.svelte';

  let {
    tile,
    res,
    tables = [],
    selected = null,
    onchange,
    onpick,
  }: {
    tile: Tile;
    res: QueryResult;
    tables?: (Named & { columns?: { name: string; dtype: DType }[] })[];
    /** An exported file: the value picked by a click, which replaces the saved selection. */
    selected?: Cell[] | null;
    /** The app: the selection changed. */
    onchange?: (f: TileFilter) => void;
    /** An exported file: a value was clicked. */
    onpick?: (v: Cell) => void;
  } = $props();

  const d = $derived(tile.dims[0]);
  const style = $derived(tile.filter?.style ?? 'list');
  const single = $derived(!!tile.filter?.single);
  const dtype = $derived<DType>(tables.find((t) => t.id === d?.table)?.columns?.find((c) => c.name === d?.column)?.dtype ?? 'text');
  const name = $derived(d ? pretty(dimLabel({ table: d.table, column: d.column, via: d.via }, tables)) : '');
  const editable = $derived(!!onchange);

  const save = (patch: Partial<TileFilter>) => onchange?.({ style, ...tile.filter, ...patch });
  const clear = () => onchange?.({ style, ...(single ? { single } : {}) });

  // ---- list and dropdown ----------------------------------------------------------------------
  let q = $state('');
  let open = $state(false);
  let trigger: HTMLButtonElement | undefined = $state();
  let pop = $state({ left: 0, top: 0, width: 220 });

  const chosen = $derived<Cell[]>(selected ?? tile.filter?.values ?? []);
  const options = $derived(res.rows.map((r) => ({ value: r[0] as Cell, count: typeof r[1] === 'number' ? r[1] : 0 })));
  // A value that is selected but has no rows now (other filters hide it) stays in the list.
  const shown = $derived.by(() => {
    const have = new Set(options.map((o) => keyOf(o.value)));
    const extra = chosen.filter((v) => !have.has(keyOf(v))).map((value) => ({ value, count: 0 }));
    const word = q.trim().toLowerCase();
    return [...extra, ...options].filter((o) => !word || cell(o.value, dtype, null, false, null, null, d).toLowerCase().includes(word));
  });
  const isOn = (v: Cell) => chosen.some((c) => keyOf(c) === keyOf(v));
  const all = $derived(chosen.length === 0);
  const capped = $derived(options.length >= 200);

  function toggle(v: Cell) {
    if (onpick) return onpick(v);
    if (single) return save({ values: isOn(v) ? [] : [v] });
    // Every box reads as ticked when nothing is picked, so unticking one keeps all the others.
    if (all && !capped) return save({ values: options.map((o) => o.value).filter((c) => keyOf(c) !== keyOf(v)) });
    const next = isOn(v) ? chosen.filter((c) => keyOf(c) !== keyOf(v)) : [...chosen, v];
    // Ticking the last one back is the same as no filter.
    if (!capped && options.every((o) => next.some((c) => keyOf(c) === keyOf(o.value)))) return clear();
    save({ values: next });
  }

  const summary = $derived(
    all ? 'All' : chosen.length === 1 ? cell(chosen[0], dtype, null, false, null, null, d) : `${chosen.length} selected`,
  );

  function openPop() {
    if (!trigger) return;
    const r = trigger.getBoundingClientRect();
    const height = 300;
    const below = innerHeight - r.bottom > height + 12 || r.top < height;
    pop = { left: Math.max(8, Math.min(r.left, innerWidth - 248)), top: below ? r.bottom + 4 : Math.max(8, r.top - height - 4), width: Math.max(r.width, 220) };
    open = true;
  }

  // ---- range --------------------------------------------------------------------------------
  const bins = $derived(res.bins);
  const lo = $derived(bins?.start ?? 0);
  const hi = $derived(bins ? bins.start + bins.width * res.rows.length : 0);
  const saved = $derived<[number | null, number | null]>([
    typeof tile.filter?.range?.[0] === 'number' ? tile.filter.range[0] : null,
    typeof tile.filter?.range?.[1] === 'number' ? tile.filter.range[1] : null,
  ]);
  // While a thumb is dragged the bars follow it; the selection is saved when it is let go.
  let drag = $state<[number, number] | null>(null);
  const from = $derived(drag?.[0] ?? saved[0] ?? lo);
  const to = $derived(drag?.[1] ?? saved[1] ?? hi);
  const peak = $derived(Math.max(1, ...res.rows.map((r) => (typeof r[1] === 'number' ? r[1] : 0))));
  const step = $derived(hi > lo ? (hi - lo) / 400 : 1);

  function setRange(a: number | null, b: number | null) {
    drag = null;
    // An end on the end of the data is left open, so new data inside the filter still counts.
    const x = a !== null && a <= lo ? null : a;
    const y = b !== null && b >= hi ? null : b;
    if (x !== null && y !== null && x > y) return save({ range: [y, x] });
    save({ range: [x, y] });
  }
  const typed = (v: string): number | null => (v.trim() === '' || !Number.isFinite(Number(v)) ? null : Number(v));
  const num = (n: number) => cell(Math.round(n * 1e6) / 1e6, 'number', null, true);

  // ---- dates --------------------------------------------------------------------------------
  const preset = $derived(tile.filter?.preset ?? (tile.filter?.range ? 'custom' : ''));
  const first = $derived(typeof res.rows[0]?.[0] === 'string' ? (res.rows[0][0] as string).slice(0, 10) : '');
  const last = $derived(typeof res.rows[0]?.[1] === 'string' ? (res.rows[0][1] as string).slice(0, 10) : '');
  const day = (v: Cell | undefined) => (typeof v === 'string' ? v.slice(0, 10) : '');
  function pickPreset(p: string) {
    if (!p) return onchange?.({ style });
    if (p === 'custom') return save({ preset: 'custom', range: tile.filter?.range ?? [null, null] });
    onchange?.({ style, preset: p });
  }
  function setDays(a: string, b: string) {
    save({ preset: 'custom', range: [a || null, b || null] });
  }
</script>

{#snippet picker()}
  <input class="field search" placeholder="Search" aria-label="Search the values" bind:value={q} />
  <div class="rows">
    <label class="opt all">
      <input type="checkbox" checked={all} disabled={!editable && !onpick} onchange={() => (onpick ? selected?.length && onpick(selected[0]) : clear())} />
      <span class="lab">Select all</span>
      <span class="n">{options.reduce((s, o) => s + o.count, 0).toLocaleString()}</span>
    </label>
    {#each shown as o (keyOf(o.value))}
      <label class="opt">
        <input type={single ? 'radio' : 'checkbox'} checked={isOn(o.value) || (all && !single)} disabled={!editable && !onpick} onchange={() => toggle(o.value)} />
        <span class="lab" class:blank={o.value === null || o.value === ''}>{o.value === '' ? '(blank)' : cell(o.value, dtype, null, false, null, null, d)}</span>
        <span class="n">{o.count ? o.count.toLocaleString() : '–'}</span>
      </label>
    {:else}
      <p class="none">Nothing matches.</p>
    {/each}
    {#if capped && !q}<p class="none">Showing the 200 most common values.</p>{/if}
  </div>
{/snippet}

<svelte:window
  onpointerdown={(e) => open && !(e.target as HTMLElement).closest('.drop-pop, .drop') && (open = false)}
  onkeydown={(e) => open && e.key === 'Escape' && (open = false)} />

{#if !d}
  <div class="none center">Pick a field for this filter.</div>
{:else if style === 'list'}
  <div class="list">
    <div class="head">
      <span class="name">{name}</span>
      {#if !all && editable}<button class="link" onclick={clear}>Clear</button>{/if}
    </div>
    {@render picker()}
  </div>
{:else if style === 'dropdown'}
  <div class="dropwrap">
  <button class="drop" bind:this={trigger} onclick={() => (open ? (open = false) : openPop())} aria-expanded={open} aria-label="Filter by {name}">
    <span class="dn">{name}</span>
    <span class="dv" class:all>{summary}</span>
    <Icon name="chevronDown" size={12} />
  </button>
  </div>
  {#if open}
    <div class="drop-pop" use:portal style:left="{pop.left}px" style:top="{pop.top}px" style:width="{pop.width}px">
      {@render picker()}
    </div>
  {/if}
{:else if style === 'range'}
  <div class="range">
    <div class="head">
      <span class="name">{name}</span>
      {#if (saved[0] !== null || saved[1] !== null) && editable}<button class="link" onclick={clear}>Clear</button>{/if}
    </div>
    {#if !bins || !res.rows.length}
      <div class="none">No numbers to filter.</div>
    {:else}
      <div class="bars" aria-hidden="true">
        {#each res.rows as r}
          {@const a = (r[0] as number)}
          {@const inside = a + bins.width > from && a < to}
          <i style:height="{Math.max(4, ((r[1] as number) / peak) * 100)}%" class:out={!inside}></i>
        {/each}
      </div>
      <div class="sliders">
        <input
          type="range"
          min={lo}
          max={hi}
          {step}
          value={from}
          disabled={!editable}
          aria-label="Lowest {name}"
          oninput={(e) => (drag = [Math.min(+e.currentTarget.value, to), to])}
          onchange={(e) => setRange(Math.min(+e.currentTarget.value, to), saved[1])} />
        <input
          type="range"
          min={lo}
          max={hi}
          {step}
          value={to}
          disabled={!editable}
          aria-label="Highest {name}"
          oninput={(e) => (drag = [from, Math.max(+e.currentTarget.value, from)])}
          onchange={(e) => setRange(saved[0], Math.max(+e.currentTarget.value, from))} />
      </div>
      <div class="pair">
        <input
          class="field"
          type="number"
          step="any"
          placeholder={num(lo)}
          aria-label="From"
          disabled={!editable}
          value={saved[0] ?? ''}
          onchange={(e) => setRange(typed(e.currentTarget.value), saved[1])} />
        <span class="muted">to</span>
        <input
          class="field"
          type="number"
          step="any"
          placeholder={num(hi)}
          aria-label="To"
          disabled={!editable}
          value={saved[1] ?? ''}
          onchange={(e) => setRange(saved[0], typed(e.currentTarget.value))} />
      </div>
    {/if}
  </div>
{:else}
  <div class="dates">
    <div class="head"><span class="name">{name}</span></div>
    <select class="field" aria-label="Dates to keep" value={preset} disabled={!editable} onchange={(e) => pickPreset(e.currentTarget.value)}>
      <option value="">All dates</option>
      {#each DATE_PRESETS as p}<option value={p.id}>{p.label}</option>{/each}
      <option value="custom">Custom between</option>
    </select>
    {#if preset === 'custom'}
      <label class="day">
        <span class="muted">From</span>
        <input class="field" type="date" aria-label="First day" min={first} max={last} disabled={!editable} value={day(tile.filter?.range?.[0])} onchange={(e) => setDays(e.currentTarget.value, day(tile.filter?.range?.[1]))} />
      </label>
      <label class="day">
        <span class="muted">To</span>
        <input class="field" type="date" aria-label="Last day" min={first} max={last} disabled={!editable} value={day(tile.filter?.range?.[1])} onchange={(e) => setDays(day(tile.filter?.range?.[0]), e.currentTarget.value)} />
      </label>
    {/if}
    {#if first && last}<p class="muted note">Data from {cell(first, 'date', null, true)} to {cell(last, 'date', null, true)}.</p>{/if}
  </div>
{/if}

<style>
  .center { height: 100%; display: grid; place-items: center; }
  .none { margin: 0; padding: 6px 0; color: var(--text-2); font-size: 12px; }
  .list, .range, .dates { height: 100%; display: flex; flex-direction: column; gap: 6px; min-height: 0; }
  /* The tile's own buttons float over its top right corner. */
  .head { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex: none; min-height: 22px; padding-right: 60px; }
  .name { font-family: var(--mono); font-size: 10.5px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); }
  .link { border: 0; background: none; padding: 0; font-size: 12px; color: var(--accent-text); cursor: pointer; }
  .link:hover { text-decoration: underline; }
  .search { height: 26px; flex: none; }
  .rows { flex: 1; min-height: 0; overflow: auto; margin: 0 -6px; }
  .opt { display: flex; align-items: center; gap: 8px; min-height: 26px; padding: 0 6px; border-radius: 5px; font-size: 12px; cursor: pointer; }
  .opt:hover { background: var(--window); }
  .opt.all { color: var(--text-2); border-bottom: 1px solid var(--sep); border-radius: 0; margin-bottom: 2px; }
  .lab { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lab.blank { color: var(--text-2); font-style: italic; }
  .n { color: var(--text-2); font-variant-numeric: tabular-nums; font-size: 11px; }
  .dropwrap { height: 100%; display: flex; align-items: center; }
  .drop {
    width: 100%;
    height: 40px;
    margin-top: 4px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border: 1px solid var(--sep-strong);
    border-radius: 8px;
    background: var(--inset);
    font-size: 13px;
    text-align: left;
  }
  .drop:hover { border-color: var(--accent); }
  .dn { color: var(--text-2); font-family: var(--mono); font-size: 10.5px; font-weight: 500; text-transform: uppercase; letter-spacing: 0.02em; white-space: nowrap; }
  .dv { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .dv.all { font-weight: 400; color: var(--text-2); }
  .drop-pop {
    position: fixed;
    z-index: 70;
    max-height: 300px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 8px 6px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
  }
  .bars { flex: 1; min-height: 24px; max-height: 70px; display: flex; align-items: flex-end; gap: 2px; }
  .bars i { flex: 1; background: var(--accent); border-radius: 1px; opacity: 0.85; }
  .bars i.out { background: var(--sep-strong); opacity: 0.6; }
  .sliders { position: relative; height: 18px; flex: none; }
  .sliders input {
    position: absolute;
    inset: 0;
    width: 100%;
    margin: 0;
    background: none;
    pointer-events: none;
    appearance: none;
  }
  .sliders input::-webkit-slider-runnable-track { height: 2px; background: var(--sep-strong); }
  .sliders input::-moz-range-track { height: 2px; background: var(--sep-strong); }
  .sliders input::-webkit-slider-thumb { appearance: none; pointer-events: auto; width: 14px; height: 14px; margin-top: -6px; border-radius: 7px; border: 2px solid var(--accent); background: var(--surface); cursor: ew-resize; }
  .sliders input::-moz-range-thumb { pointer-events: auto; width: 10px; height: 10px; border-radius: 7px; border: 2px solid var(--accent); background: var(--surface); cursor: ew-resize; }
  .pair { display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; gap: 6px; flex: none; }
  .pair .field { min-width: 0; height: 28px; }
  .muted { color: var(--text-2); font-size: 12px; }
  .note { margin: 0; font-size: 11px; }
  .dates .field { height: 30px; }
  .day { display: grid; grid-template-columns: 40px 1fr; align-items: center; gap: 6px; flex: none; }
  .day .field { min-width: 0; }
</style>
