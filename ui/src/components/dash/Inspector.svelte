<script lang="ts">
  import { api, DATE_PARTS, type Agg, type Grain, type Measure, type NumFormat, type QueryResult, type Tile, type TileType } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { aggNames, tileTitle } from '../../lib/format';
  import { stackable } from '../../lib/charts/math';
  import { advancedLocked, SHOW_ADVANCED_SWITCH } from '../../lib/tiers';
  import { defaultDim, defaultMeasure, dtypeOf, MAX_CHART_MEASURES, PIVOT_MAX_MEASURES, PIVOT_MAX_ROWS, reachableFrom, type DashFilter } from '../../lib/tiles';
  import Icon, { type IconName } from '../Icon.svelte';
  import FieldPicker from './FieldPicker.svelte';
  import Suggestions from './Suggestions.svelte';
  import { normalisePicture } from '../../lib/image';

  let {
    tile,
    current = null,
    filters = [],
    onchange,
    onremove,
    onclose,
  }: {
    tile: Tile;
    /** What the tile shows now, for the previews under "Also try". */
    current?: { res: QueryResult; key: string } | null;
    filters?: DashFilter[];
    onchange: (t: Tile) => void;
    onremove: () => void;
    onclose: () => void;
  } = $props();

  const types: { id: TileType; icon: IconName; label: string }[] = [
    { id: 'bar', icon: 'bar', label: 'Bar' },
    { id: 'line', icon: 'line', label: 'Line' },
    { id: 'area', icon: 'area', label: 'Area' },
    { id: 'share', icon: 'share', label: 'Share' },
    { id: 'kpi', icon: 'kpi', label: 'Number' },
    { id: 'table', icon: 'table', label: 'Table' },
    { id: 'scatter', icon: 'scatter', label: 'Scatter' },
    { id: 'histogram', icon: 'histogram', label: 'Histogram' },
    { id: 'waterfall', icon: 'waterfall', label: 'Waterfall' },
    { id: 'pivot', icon: 'pivot', label: 'Pivot' },
    { id: 'text', icon: 'text', label: 'Text' },
    { id: 'image', icon: 'image', label: 'Image' },
  ];
  const stacks: { id: NonNullable<Tile['stack']>; label: string }[] = [
    { id: 'stacked', label: 'Stacked' },
    { id: 'side', label: 'Side by side' },
    { id: 'percent', label: '100 %' },
  ];
  const grains: Grain[] = ['year', 'quarter', 'month', 'week', 'day'];
  const partNames: Record<string, string> = { month_of_year: 'Month of year', weekday: 'Weekday', hour: 'Hour' };

  const TOP_PRESETS = [5, 10, 20];
  const sorts: { id: string; label: string; sort?: Tile['sort'] }[] = [
    { id: 'auto', label: 'Automatic' },
    { id: 'value-desc', label: 'Biggest first', sort: { by: 'value', desc: true } },
    { id: 'value-asc', label: 'Smallest first', sort: { by: 'value', desc: false } },
    { id: 'label-asc', label: 'A → Z', sort: { by: 'label', desc: false } },
    { id: 'label-desc', label: 'Z → A', sort: { by: 'label', desc: true } },
  ];

  // How a value is written. Each value has its own (V3-D9). Unset fields keep the defaults.
  const CURRENCIES = ['IDR', 'USD', 'EUR', 'SGD', 'MZN'];
  const DECIMALS = [0, 1, 2, 3, 4];
  const tableLike = $derived(tile.type === 'table' || tile.type === 'pivot');

  function setFormat(i: number, patch: Partial<NumFormat>) {
    const formats = tile.measures.map((_, j) => tile.formats?.[j] ?? null);
    const next: NumFormat = { ...(formats[i] ?? {}), ...patch };
    for (const k of Object.keys(next) as (keyof NumFormat)[]) if (next[k] === undefined) delete next[k];
    formats[i] = Object.keys(next).length ? next : null;
    set({ formats: formats.some(Boolean) ? formats : undefined });
  }

  /** Removes a value and its format together, so the two lists stay in step. */
  function removeMeasure(i: number) {
    const formats = (tile.formats ?? []).filter((_, j) => j !== i);
    set({ measures: tile.measures.filter((_, j) => j !== i), formats: formats.some(Boolean) ? formats : undefined });
  }

  const m = $derived(tile.measures[0]);
  const rels = $derived(app.snap!.workspace.relationships);
  const reach = $derived(m ? reachableFrom(m.table, rels) : null);
  /** The ways a value can be added up: a number column can be summed, any other column only counted. */
  const aggsOf = (x: Measure | undefined): Agg[] =>
    !x || x.column === null
      ? ['count']
      : dtypeOf(app.tables, x.table, x.column) === 'number'
        ? ['sum', 'avg', 'min', 'max', 'count', 'count_distinct']
        : ['count', 'count_distinct', 'min', 'max'];
  const aggs = $derived(aggsOf(m));

  function set(patch: Partial<Tile>) {
    onchange({ ...tile, ...patch });
  }

  // "Top N" keeps the biggest groups and shows the rest as one Other bar.
  let customTop = $state(false);
  // The tile prop is a new object after every change, so reset only when the id changes.
  let lastId = '';
  $effect(() => {
    if (tile.id !== lastId) {
      lastId = tile.id;
      customTop = false;
    }
  });
  const topKind = $derived(tile.type === 'bar' || tile.type === 'table' || tile.type === 'share' || tile.type === 'donut' || tile.type === 'pivot');
  const topChoice = $derived(!tile.top ? 'all' : customTop || !TOP_PRESETS.includes(tile.top.n) ? 'custom' : String(tile.top.n));

  function setTop(choice: string) {
    customTop = choice === 'custom';
    if (choice === 'all') set({ top: undefined });
    else if (choice === 'custom') set({ top: { ...(tile.top ?? {}), n: tile.top?.n ?? 15 } });
    else set({ top: { ...(tile.top ?? {}), n: Number(choice) } });
  }

  function setTopN(raw: string) {
    const n = Math.round(Number(raw));
    if (Number.isFinite(n) && n >= 1) set({ top: { ...tile.top, n: Math.min(n, 1000) } });
  }

  const sortId = $derived(
    !tile.sort
      ? 'auto'
      : tile.sort.by === 'manual'
        ? 'manual'
        : `${tile.sort.by}-${(tile.sort.desc ?? tile.sort.by === 'value') ? 'desc' : 'asc'}`,
  );

  function setDim(i: number, table: string, column: string) {
    const dims = [...tile.dims];
    dims[i] = defaultDim(app.tables, table, column);
    const isDate = dims[0] && ['date', 'datetime'].includes(dtypeOf(app.tables, dims[0].table, dims[0].column) ?? '');
    const patch: Partial<Tile> = { dims };
    // A first group-by picks a sensible chart for the field.
    if (i === 0 && !tile.dims.length && tile.type !== 'table' && tile.type !== 'pivot' && !isScatter && tile.type !== 'waterfall') {
      patch.type = isDate ? 'line' : 'bar';
      patch.horizontal = !isDate;
    }
    set(patch);
  }

  function removeDim(i: number) {
    set({ dims: tile.dims.filter((_, j) => j !== i) });
  }

  // A table takes any number of values; a bar, line or area takes up to four, and so does a pivot.
  const isPivot = $derived(tile.type === 'pivot');
  const manyValues = $derived(tile.type === 'table' || isPivot || tile.type === 'bar' || tile.type === 'line' || tile.type === 'area');
  const isScatter = $derived(tile.type === 'scatter');
  const isHistogram = $derived(tile.type === 'histogram');
  const isText = $derived(tile.type === 'text');
  const isImage = $derived(tile.type === 'image');
  // A note and a picture have no fields to pick.
  const noFields = $derived(isText || isImage);
  const maxValues = $derived(tile.type === 'table' ? Infinity : isPivot ? PIVOT_MAX_MEASURES : MAX_CHART_MEASURES);
  const showSplit = $derived(tile.type === 'bar' || tile.type === 'line' || tile.type === 'area' || tile.type === 'table' || isScatter);
  const stackAble = $derived(stackable(tile.measures.slice(0, MAX_CHART_MEASURES).map((x) => x.agg)));
  // Stacking needs more than one series: a split, or several values.
  const hasSeries = $derived((tile.type === 'bar' || tile.type === 'area') && !!tile.measures.length && (!!tile.dims[1] || tile.measures.length > 1));
  const medianOn = $derived(!!tile.lines?.some((l) => l.kind === 'median'));

  function setMedian(on: boolean) {
    const rest = (tile.lines ?? []).filter((l) => l.kind !== 'median');
    const lines = on ? [...rest, { kind: 'median' as const }] : rest;
    set({ lines: lines.length ? lines : undefined });
  }

  /** A bin width of 0 or less, or an empty box, means the engine picks one. */
  function setBinWidth(raw: string) {
    const w = Number(raw);
    set({ bins: raw.trim() !== '' && Number.isFinite(w) && w > 0 ? { width: w } : undefined });
  }
  const dimTypes = $derived(tile.dims.map((d) => dtypeOf(app.tables, d.table, d.column)));

  /** A target of 0 or less, or an empty box, clears the target. */
  function setTarget(raw: string) {
    const v = Number(raw.replace(/\s/g, ''));
    set({ target: raw.trim() !== '' && Number.isFinite(v) && v > 0 ? { value: v } : undefined });
  }

  // The picture of an image tile. The desktop app asks the system; a browser asks the page.
  const MAX_IMAGE = 1_048_576;
  let imageError = $state('');
  let fileInput: HTMLInputElement | undefined = $state();

  function readBlob(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result));
      r.onerror = () => reject(new Error('That file could not be read.'));
      r.readAsDataURL(blob);
    });
  }

  /** Checks the size and the kind in words, then keeps the picture in the tile. */
  async function takePicture(blob: Blob) {
    imageError = '';
    if (blob.size > MAX_IMAGE) {
      imageError = 'That picture is too big. Pick one under 1 MB.';
      return;
    }
    // The first bytes decide, not the file name: a name that ends in .png can hold anything.
    const data = normalisePicture(await readBlob(blob));
    if (!data) {
      imageError = 'Pick a PNG, JPEG or WebP picture.';
      return;
    }
    set({ image: { fit: 'contain', alt: '', ...tile.image, data } });
  }

  async function choosePicture() {
    imageError = '';
    try {
      if (app.health?.native) {
        const { paths } = await api.pick({ kind: 'image' });
        if (paths[0]) await takePicture(await api.readImage(paths[0]));
      } else {
        fileInput?.click();
      }
    } catch (e) {
      imageError = e instanceof Error ? e.message : String(e);
    }
  }

  async function onFile(e: Event & { currentTarget: HTMLInputElement }) {
    const f = e.currentTarget.files?.[0];
    e.currentTarget.value = '';
    if (f) await takePicture(f).catch((err) => (imageError = String(err.message ?? err)));
  }
</script>

{#snippet formatRow(i: number)}
  {@const f = tile.formats?.[i] ?? null}
  <div class="fmt" role="group" aria-label="Format of this value">
    <span class="label">Format</span>
    <div class="pair">
      <select
        class="field"
        aria-label="Currency"
        value={f?.currency ?? ''}
        onchange={(e) => setFormat(i, { currency: e.currentTarget.value || undefined })}>
        <option value="">No currency</option>
        {#each CURRENCIES as c}<option value={c}>{c}</option>{/each}
      </select>
      <select
        class="field"
        aria-label="Decimals"
        value={f?.decimals === undefined ? 'auto' : String(f.decimals)}
        onchange={(e) => setFormat(i, { decimals: e.currentTarget.value === 'auto' ? undefined : Number(e.currentTarget.value) })}>
        <option value="auto">Auto decimals</option>
        {#each DECIMALS as d}<option value={String(d)}>{d} {d === 1 ? 'decimal' : 'decimals'}</option>{/each}
      </select>
    </div>
    <label class="check">
      <input type="checkbox" checked={f?.compact ?? !tableLike} onchange={(e) => setFormat(i, { compact: e.currentTarget.checked })} />
      Short ({app.locale === 'id' ? '1,2 jt' : '1.2M'})
    </label>
  </div>
{/snippet}

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
      {#if SHOW_ADVANCED_SWITCH}
        <button class="btn small" onclick={() => (app.settingsOpen = true)}>Open Settings</button>
      {/if}
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
    {#if tile.type === 'kpi' || tile.type === 'waterfall'}
      <div class="better" role="radiogroup" aria-label="Which change is good">
        <span class="label">A rise is</span>
        <button role="radio" aria-checked={(tile.better ?? 'up') === 'up'} class:on={(tile.better ?? 'up') === 'up'} onclick={() => set({ better: 'up' })}>good</button>
        <button role="radio" aria-checked={tile.better === 'down'} class:on={tile.better === 'down'} onclick={() => set({ better: 'down' })}>bad</button>
      </div>
    {/if}
    {#if tile.type === 'kpi'}
      <div class="target">
        <span class="label">Target (optional)</span>
        <input
          class="field"
          type="number"
          min="0"
          step="any"
          placeholder="No target"
          aria-label="Target for this number"
          value={tile.target?.value ?? ''}
          onchange={(e) => setTarget(e.currentTarget.value)} />
      </div>
    {/if}
    {#if tile.type === 'bar'}
      <label class="check">
        <input type="checkbox" checked={!!tile.horizontal} onchange={(e) => set({ horizontal: e.currentTarget.checked })} />
        Horizontal bars
      </label>
    {/if}
  </div>

  {#if isText}
    <div class="sec">
      <span class="label">Text</span>
      <textarea
        class="field text"
        rows="8"
        placeholder="# A heading&#10;Some **bold** words, a [link](https://example.com)&#10;- a list item"
        aria-label="Text of this tile"
        value={tile.text ?? ''}
        oninput={(e) => set({ text: e.currentTarget.value })}></textarea>
      <p class="hint tight">Headings start with # or ##, lists with -. Use **bold**, *italic* and [links](https://…). Anything else shows as it is.</p>
    </div>
  {/if}

  {#if isImage}
    <div class="sec">
      <span class="label">Picture</span>
      <button class="btn" onclick={choosePicture}><Icon name="image" /> {tile.image?.data ? 'Change picture' : 'Choose a picture'}</button>
      <input bind:this={fileInput} class="hidden" type="file" accept="image/png,image/jpeg,image/webp" onchange={onFile} />
      <p class="hint tight">PNG, JPEG or WebP, up to 1 MB.</p>
      {#if imageError}<p class="error" role="alert"><Icon name="warning" size={13} /> {imageError}</p>{/if}
      {#if tile.image?.data}
        <span class="label up">Description</span>
        <input
          class="field"
          placeholder="What the picture shows"
          aria-label="Description of the picture, for screen readers"
          value={tile.image.alt}
          oninput={(e) => set({ image: { ...tile.image!, alt: e.currentTarget.value } })} />
        <div class="grains stack" role="radiogroup" aria-label="How the picture fits">
          {#each [['contain', 'Show all'], ['cover', 'Fill the tile']] as [id, label]}
            <button
              role="radio"
              aria-checked={(tile.image.fit ?? 'contain') === id}
              class:on={(tile.image.fit ?? 'contain') === id}
              onclick={() => set({ image: { ...tile.image!, fit: id as 'contain' | 'cover' } })}>
              {label}
            </button>
          {/each}
        </div>
        <button class="btn plain flush" onclick={() => set({ image: undefined })}><Icon name="trash" /> Remove picture</button>
      {/if}
    </div>
  {/if}

  {#if !noFields}
  <div class="sec">
    <span class="label">{isScatter ? 'Across (X)' : isHistogram ? 'Number column' : 'Value'}</span>
    <FieldPicker
      mode="measure"
      table={m?.table ?? null}
      column={m?.column ?? null}
      placeholder={isHistogram ? 'A column of numbers' : 'What to measure'}
      numbersOnly={isHistogram}
      onchange={(t, c) => set({ measures: [defaultMeasure(app.tables, t, c), ...tile.measures.slice(1)] })} />
    {#if m}
      <!-- A histogram counts every row of the column, so there is nothing to add up. -->
      {#if !isHistogram}
        <select class="field" value={m.agg} onchange={(e) => set({ measures: [{ ...m, agg: e.currentTarget.value as Agg }, ...tile.measures.slice(1)] })}>
          {#each aggs as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
      {/if}
      {@render formatRow(0)}
    {/if}
    {#if isScatter && m}
      <span class="label up">Up (Y)</span>
      <FieldPicker
        mode="measure"
        table={tile.measures[1]?.table ?? null}
        column={tile.measures[1]?.column ?? null}
        placeholder="What to measure"
        reachable={reach}
        onchange={(t, c) => set({ measures: [m, defaultMeasure(app.tables, t, c)] })}
        onclear={() => removeMeasure(1)} />
      {#if tile.measures[1]}
        <select class="field" value={tile.measures[1].agg} onchange={(e) => set({ measures: [m, { ...tile.measures[1], agg: e.currentTarget.value as Agg }] })}>
          {#each aggsOf(tile.measures[1]) as a}<option value={a}>{aggNames[a]}</option>{/each}
        </select>
        {@render formatRow(1)}
      {/if}
    {/if}
    {#if manyValues}
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
          onclear={() => removeMeasure(k + 1)} />
        {@render formatRow(k + 1)}
      {/each}
      {#if m && tile.measures.length < maxValues}
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

  {#if !isHistogram}
  <div class="sec">
      <span class="label">{tile.type === 'kpi' ? 'Period (a date, for the comparison)' : tile.type === 'share' || tile.type === 'donut' ? 'Split into' : isScatter ? 'Dot for each' : isPivot ? 'Rows' : 'Group by'}</span>
      <FieldPicker
        mode="dim"
        table={tile.dims[0]?.table ?? null}
        column={tile.dims[0]?.column ?? null}
        placeholder={tile.type === 'kpi' ? 'A date, for the trend' : isScatter ? 'One dot for each of these' : 'Category or date'}
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
        <!-- Parts of the date repeat every year, week or day. A number tile compares periods, so it has none. -->
        {#if tile.type !== 'kpi'}
          <div class="grains parts" role="radiogroup" aria-label="Part of the date">
            {#each DATE_PARTS.filter((g) => g !== 'hour' || dimTypes[0] === 'datetime') as g}
              <button role="radio" aria-checked={tile.dims[0].grain === g} class:on={tile.dims[0].grain === g} onclick={() => set({ dims: [{ ...tile.dims[0], grain: g }, ...tile.dims.slice(1)] })}>
                {partNames[g]}
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  {#if showSplit && tile.dims[0] && (tile.type === 'table' || isScatter || tile.measures.length < 2)}
    <div class="sec">
      <span class="label">{tile.type === 'table' ? 'Then by' : isScatter ? 'Colour by (optional)' : 'Split by (optional)'}</span>
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

  {#if showSplit && tile.dims[0] && tile.type !== 'table' && !isScatter && tile.measures.length > 1}
    <p class="hint">Split by is off while a tile shows several values.</p>
  {/if}

  {#if isPivot}
    {#if tile.dims[0]}
      <div class="sec">
        <span class="label">Then by (optional)</span>
        {#each tile.dims.slice(1, PIVOT_MAX_ROWS) as d, k}
          <FieldPicker mode="dim" table={d.table} column={d.column} reachable={reach} onchange={(t, c) => c && setDim(k + 1, t, c)} onclear={() => removeDim(k + 1)} />
        {/each}
        {#if tile.dims.length < PIVOT_MAX_ROWS}
          <FieldPicker mode="dim" table={null} column={null} placeholder="Another row field" reachable={reach} onchange={(t, c) => c && setDim(tile.dims.length, t, c)} />
        {/if}
      </div>
    {/if}
    <div class="sec">
      <span class="label">Columns by</span>
      <FieldPicker
        mode="dim"
        table={tile.cols?.table ?? null}
        column={tile.cols?.column ?? null}
        placeholder="The field across the top"
        reachable={reach}
        onchange={(t, c) => c && set({ cols: defaultDim(app.tables, t, c) })}
        onclear={() => set({ cols: undefined })} />
      {#if tile.cols && ['date', 'datetime'].includes(dtypeOf(app.tables, tile.cols.table, tile.cols.column) ?? '')}
        <div class="grains" role="radiogroup" aria-label="Date grouping of the columns">
          {#each grains as g}
            <button role="radio" aria-checked={tile.cols.grain === g} class:on={tile.cols.grain === g} onclick={() => set({ cols: { ...tile.cols!, grain: g } })}>{g}</button>
          {/each}
        </div>
      {/if}
      <label class="check">
        <input type="checkbox" checked={tile.totals !== false} onchange={(e) => set({ totals: e.currentTarget.checked ? undefined : false })} />
        Totals
      </label>
      <p class="hint tight">The rows are the fields above. Up to 30 columns are shown.</p>
    </div>
  {/if}

  {#if tile.type === 'table' && tile.measures.length}
    <div class="sec">
      <label class="check first">
        <input type="checkbox" checked={!!tile.totals} onchange={(e) => set({ totals: e.currentTarget.checked ? true : undefined })} />
        Total row
      </label>
    </div>
  {/if}

  {#if hasSeries}
    <div class="sec">
      <span class="label">Several series</span>
      {#if stackAble}
        <div class="grains stack" role="radiogroup" aria-label="How the series are drawn">
          {#each stacks as o}
            <button
              role="radio"
              aria-checked={(tile.stack ?? 'stacked') === o.id}
              class:on={(tile.stack ?? 'stacked') === o.id}
              onclick={() => set({ stack: o.id === 'stacked' ? undefined : o.id })}>
              {o.label}
            </button>
          {/each}
        </div>
      {:else}
        <p class="hint tight">Averages, lowest and highest values cannot be stacked, so they sit side by side.</p>
      {/if}
    </div>
  {/if}

  {#if isScatter}
    <div class="sec">
      <label class="check first">
        <input type="checkbox" checked={medianOn} onchange={(e) => setMedian(e.currentTarget.checked)} />
        Median lines
      </label>
    </div>
  {/if}

  {#if isHistogram}
    <div class="sec">
      <span class="label">Bin width</span>
      <input
        class="field"
        type="number"
        min="0"
        step="any"
        placeholder="auto"
        aria-label="Bin width, or empty for automatic"
        value={tile.bins?.width ?? ''}
        onchange={(e) => setBinWidth(e.currentTarget.value)} />
      <p class="hint tight">Leave it empty and Tenun picks a width that suits the data.</p>
    </div>
  {/if}

  {#if topKind && tile.dims[0] && dimTypes[0] !== 'date' && dimTypes[0] !== 'datetime'}
    <div class="sec">
      <span class="label">Top</span>
      <select class="field" value={topChoice} onchange={(e) => setTop(e.currentTarget.value)}>
        <option value="all">All</option>
        {#each TOP_PRESETS as n}<option value={String(n)}>Top {n}</option>{/each}
        <option value="custom">Custom…</option>
      </select>
      {#if topChoice === 'custom'}
        <input
          class="field"
          type="number"
          min="1"
          max="1000"
          aria-label="How many to keep"
          value={tile.top?.n ?? 15}
          onchange={(e) => setTopN(e.currentTarget.value)} />
      {/if}
      {#if tile.top}
        <label class="check">
          <input type="checkbox" checked={tile.top.other !== false} onchange={(e) => set({ top: { ...tile.top!, other: e.currentTarget.checked ? undefined : false } })} />
          Show the rest as Other
        </label>
      {/if}
    </div>
  {/if}

  {#if (tile.type === 'bar' || tile.type === 'table' || tile.type === 'waterfall') && tile.dims[0]}
    <div class="sec">
      <span class="label">Sort</span>
      <select
        class="field"
        value={sortId}
        onchange={(e) => set({ sort: sorts.find((x) => x.id === e.currentTarget.value)?.sort })}>
        {#each sorts as o}<option value={o.id}>{o.label}</option>{/each}
        {#if sortId === 'manual'}<option value="manual" disabled>Manual order</option>{/if}
      </select>
    </div>
  {/if}

  <Suggestions {tile} {current} {filters} onapply={onchange} />
  {/if}

  <div class="sec">
    <span class="label">Title</span>
    <input
      class="field"
      placeholder={noFields ? 'Optional' : tileTitle({ ...tile, title: '' }, app.tables)}
      value={tile.title}
      oninput={(e) => set({ title: e.currentTarget.value })} />
  </div>

  <div class="foot">
    <button class="btn danger" onclick={onremove}><Icon name="trash" /> Remove tile</button>
  </div>
</aside>

<style>
  .insp {
    width: 332px;
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
  .btn.flush { align-self: flex-start; padding-left: 0; }
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
  .grains.parts button { text-transform: none; }
  .grains button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .fmt { display: flex; flex-direction: column; gap: 4px; margin: 6px 0 10px; }
  .fmt .label { font-size: 11px; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .pair select.field { min-width: 0; }
  .hint { margin: -8px 0 0; font-size: 11px; color: var(--text-2); }
  .hint.tight { margin: 0; line-height: 1.4; }
  .label.up { margin-top: 6px; }
  .check.first { margin-top: 0; }
  .grains.stack button { text-transform: none; padding: 0 4px; }
  .foot { margin-top: auto; padding-top: 8px; }
  .target { display: flex; flex-direction: column; gap: 6px; margin-top: 8px; }
  textarea.field { height: auto; padding: 8px 10px; line-height: 1.4; resize: vertical; font-family: inherit; }
  .hidden { display: none; }
  .error { display: flex; gap: 6px; margin: 0; color: var(--danger); font-size: 12px; }
  .error :global(svg) { flex: none; margin-top: 1px; }
</style>
