<script lang="ts">
  import type { Agg, Grain, Tile, TileType } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { aggNames, tileTitle } from '../../lib/format';
  import { advancedLocked } from '../../lib/tiers';
  import { defaultDim, defaultMeasure, dtypeOf, reachableFrom } from '../../lib/tiles';
  import Icon, { type IconName } from '../Icon.svelte';
  import FieldPicker from './FieldPicker.svelte';

  let {
    tile,
    onchange,
    onremove,
    onclose,
  }: { tile: Tile; onchange: (t: Tile) => void; onremove: () => void; onclose: () => void } = $props();

  const types: { id: TileType; icon: IconName; label: string }[] = [
    { id: 'bar', icon: 'bar', label: 'Bar' },
    { id: 'line', icon: 'line', label: 'Line' },
    { id: 'area', icon: 'area', label: 'Area' },
    { id: 'share', icon: 'share', label: 'Share' },
    { id: 'kpi', icon: 'kpi', label: 'Number' },
    { id: 'table', icon: 'table', label: 'Table' },
  ];
  const grains: Grain[] = ['year', 'quarter', 'month', 'week', 'day'];

  const m = $derived(tile.measures[0]);
  const rels = $derived(app.snap!.workspace.relationships);
  const reach = $derived(m ? reachableFrom(m.table, rels) : null);
  const mType = $derived(m ? dtypeOf(app.tables, m.table, m.column) : undefined);
  const aggs = $derived<Agg[]>(
    !m || m.column === null ? ['count'] : mType === 'number' ? ['sum', 'avg', 'min', 'max', 'count', 'count_distinct'] : ['count', 'count_distinct', 'min', 'max'],
  );

  function set(patch: Partial<Tile>) {
    onchange({ ...tile, ...patch });
  }

  function setDim(i: number, table: string, column: string) {
    const dims = [...tile.dims];
    dims[i] = defaultDim(app.tables, table, column);
    const isDate = dims[0] && ['date', 'datetime'].includes(dtypeOf(app.tables, dims[0].table, dims[0].column) ?? '');
    const patch: Partial<Tile> = { dims };
    // A first group-by picks a sensible chart for the field.
    if (i === 0 && !tile.dims.length && tile.type !== 'table') {
      patch.type = isDate ? 'line' : 'bar';
      patch.horizontal = !isDate;
    }
    set(patch);
  }

  function removeDim(i: number) {
    set({ dims: tile.dims.filter((_, j) => j !== i) });
  }

  const showSplit = $derived(tile.type === 'bar' || tile.type === 'line' || tile.type === 'area' || tile.type === 'table');
  const dimTypes = $derived(tile.dims.map((d) => dtypeOf(app.tables, d.table, d.column)));
</script>

<aside class="insp">
  <header>
    <h3>Tile</h3>
    <button class="btn plain icon" aria-label="Close panel" onclick={onclose}><Icon name="x" /></button>
  </header>

  {#if advancedLocked(tile)}
    <!-- V3-D2: the tile still draws; only editing its Advanced parts waits for the switch. -->
    <div class="notice" role="note">
      <Icon name="info" size={14} />
      <p>This tile uses Advanced features. Turn them on in Settings to edit them.</p>
      <button class="btn small" onclick={() => (app.settingsOpen = true)}>Open Settings</button>
    </div>
  {/if}

  <div class="sec">
    <div class="types" role="radiogroup" aria-label="Tile type">
      {#each types as t}
        <button
          role="radio"
          aria-checked={tile.type === t.id}
          class:on={tile.type === t.id || (t.id === 'share' && tile.type === 'donut')}
          title={t.label}
          onclick={() => set({ type: t.id })}>
          <Icon name={t.icon} size={18} />
          <span>{t.label}</span>
        </button>
      {/each}
    </div>
    {#if tile.type === 'kpi'}
      <div class="better" role="radiogroup" aria-label="Which change is good">
        <span class="label">A rise is</span>
        <button role="radio" aria-checked={(tile.better ?? 'up') === 'up'} class:on={(tile.better ?? 'up') === 'up'} onclick={() => set({ better: 'up' })}>good</button>
        <button role="radio" aria-checked={tile.better === 'down'} class:on={tile.better === 'down'} onclick={() => set({ better: 'down' })}>bad</button>
      </div>
    {/if}
    {#if tile.type === 'bar'}
      <label class="check">
        <input type="checkbox" checked={!!tile.horizontal} onchange={(e) => set({ horizontal: e.currentTarget.checked })} />
        Horizontal bars
      </label>
    {/if}
  </div>

  <div class="sec">
    <span class="label">Value</span>
    <FieldPicker
      mode="measure"
      table={m?.table ?? null}
      column={m?.column ?? null}
      placeholder="What to measure"
      onchange={(t, c) => set({ measures: [defaultMeasure(app.tables, t, c), ...tile.measures.slice(1)] })} />
    {#if m}
      <select class="field" value={m.agg} onchange={(e) => set({ measures: [{ ...m, agg: e.currentTarget.value as Agg }, ...tile.measures.slice(1)] })}>
        {#each aggs as a}<option value={a}>{aggNames[a]}</option>{/each}
      </select>
    {/if}
    {#if tile.type === 'table'}
      {#each tile.measures.slice(1) as extra, k}
        <FieldPicker
          mode="measure"
          table={extra.table}
          column={extra.column}
          reachable={reach}
          onchange={(t, c) => {
            const ms = [...tile.measures];
            ms[k + 1] = defaultMeasure(app.tables, t, c);
            set({ measures: ms });
          }}
          onclear={() => set({ measures: tile.measures.filter((_, j) => j !== k + 1) })} />
      {/each}
      {#if m}
        <FieldPicker
          mode="measure"
          table={null}
          column={null}
          placeholder="Add another value"
          reachable={reach}
          onchange={(t, c) => set({ measures: [...tile.measures, defaultMeasure(app.tables, t, c)] })} />
      {/if}
    {/if}
  </div>

    <div class="sec">
      <span class="label">{tile.type === 'kpi' ? 'Period (a date, for the comparison)' : tile.type === 'share' || tile.type === 'donut' ? 'Split into' : 'Group by'}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[0]?.table ?? null}
        column={tile.dims[0]?.column ?? null}
        placeholder={tile.type === 'kpi' ? 'A date, for the trend' : 'Category or date'}
        reachable={reach}
        onchange={(t, c) => c && setDim(0, t, c)}
        onclear={() => removeDim(0)} />
      {#if tile.dims[0] && (dimTypes[0] === 'date' || dimTypes[0] === 'datetime')}
        <div class="grains" role="radiogroup" aria-label="Date grouping">
          {#each grains as g}
            <button role="radio" aria-checked={tile.dims[0].grain === g} class:on={tile.dims[0].grain === g} onclick={() => set({ dims: [{ ...tile.dims[0], grain: g }, ...tile.dims.slice(1)] })}>
              {g}
            </button>
          {/each}
        </div>
      {/if}
    </div>

  {#if showSplit && tile.dims[0]}
    <div class="sec">
      <span class="label">{tile.type === 'table' ? 'Then by' : 'Split by (optional)'}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[1]?.table ?? null}
        column={tile.dims[1]?.column ?? null}
        placeholder={tile.type === 'table' ? 'Another column' : 'Colour by a category'}
        reachable={reach}
        onchange={(t, c) => c && setDim(1, t, c)}
        onclear={() => removeDim(1)} />
    </div>
  {/if}

  <div class="sec">
    <span class="label">Title</span>
    <input
      class="field"
      placeholder={tileTitle({ ...tile, title: '' }, app.tables)}
      value={tile.title}
      oninput={(e) => set({ title: e.currentTarget.value })} />
  </div>

  <div class="foot">
    <button class="btn danger" onclick={onremove}><Icon name="trash" /> Remove tile</button>
  </div>
</aside>

<style>
  .insp {
    width: 300px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 12px 16px 16px;
    border-left: 1px solid var(--sep);
    background: var(--surface);
    overflow: auto;
  }
  header { display: flex; align-items: center; justify-content: space-between; }
  .notice {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 8px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--selected);
    color: var(--text);
    font-size: 12px;
  }
  .notice :global(svg) { color: var(--accent-text); margin-top: 1px; }
  .notice p { margin: 0; line-height: 1.4; }
  .notice .btn { grid-column: 2; justify-self: start; height: 24px; font-size: 12px; }
  .sec { display: flex; flex-direction: column; gap: 6px; }
  .types { display: grid; grid-template-columns: repeat(3, 1fr); gap: 6px; }
  .types button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 4px 6px;
    border: 1px solid var(--sep);
    border-radius: 8px;
    background: var(--surface);
    font-size: 11px;
    color: var(--text-2);
  }
  .types button :global(svg) { color: var(--text-2); }
  .types button:hover { background: var(--hover); }
  .types button.on { border-color: var(--accent); background: var(--selected); color: var(--accent-text); }
  .types button.on :global(svg) { color: var(--accent-text); }
  .better { display: flex; align-items: center; gap: 4px; margin-top: 6px; }
  .better .label { margin-right: 4px; }
  .better button {
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--sep);
    border-radius: 12px;
    background: none;
    font-size: 12px;
    color: var(--text-2);
  }
  .better button.on { border-color: var(--accent); background: var(--selected); color: var(--accent-text); }
  .check { display: flex; align-items: center; gap: 6px; font-size: 12px; margin-top: 4px; }
  select.field { height: 30px; }
  .grains { display: flex; padding: 2px; border-radius: 8px; background: var(--inset); }
  .grains button {
    flex: 1;
    height: 24px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-size: 11px;
    text-transform: capitalize;
  }
  .grains button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .foot { margin-top: auto; padding-top: 8px; }
</style>
