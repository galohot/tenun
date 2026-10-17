<script lang="ts">
  import type { Column, DateFormat, NumFormat, TableSetup } from '../../lib/api';
  import { DATE_FORMATS, TYPE_CHOICES, labelProblem, withColumn } from '../../lib/prep';
  import { datePattern, patternProblem } from '../../lib/locale';
  import { app } from '../../lib/state.svelte';
  import { t } from '../../lib/i18n';
  import Icon from '../Icon.svelte';
  import FormatEditor from '../FormatEditor.svelte';

  let {
    column,
    setup,
    canRetype,
    table,
    x,
    y,
    onsave,
    onclose,
    onedit,
    ondelete,
  }: {
    column: Column;
    setup: TableSetup | undefined;
    /** The data is local, so the column can be read as another type. */
    canRetype: boolean;
    /** The table of the column, so a name that another column answers to can be refused. */
    table: { label: string; columns: Pick<Column, 'name' | 'label'>[] };
    x: number;
    y: number;
    onsave: (next: TableSetup) => void;
    onclose: () => void;
    /** A calculated column, with Advanced on: change its formula. */
    onedit?: (name: string) => void;
    /** A calculated column, with Advanced on: remove it. */
    ondelete?: (name: string) => void;
  } = $props();

  type Mode = 'main' | 'rename' | 'type' | 'description' | 'format' | 'dates';
  let mode = $state<Mode>('main');
  const mine = $derived(setup?.columns?.[column.name]);
  // The forms start from the current text. The menu closes after a save, so it never needs to follow.
  // svelte-ignore state_referenced_locally
  let label = $state(column.label ?? '');
  // svelte-ignore state_referenced_locally
  let description = $state(column.description ?? '');
  let root: HTMLDivElement;
  let winW = $state(innerWidth);
  // The menu stays in the window; the number format is wider than the other pages.
  const left = $derived(Math.max(8, Math.min(x, winW - (mode === 'format' ? 348 : 268))));

  const nameProblem = $derived(labelProblem(label, column, table));
  const isDate = (t: string) => t === 'date' || t === 'datetime';
  const format = $derived<DateFormat>(mine?.date_format ?? 'auto');

  function outside(e: MouseEvent) {
    if (root && !root.contains(e.target as Node)) onclose();
  }

  function patch(p: Parameters<typeof withColumn>[2], close = true) {
    onsave(withColumn(setup, column.name, p));
    if (close) onclose();
  }

  // The menu closes first, and the column it named is out of the page by then, so the name goes along.
  function edit() {
    const name = column.name;
    onclose();
    onedit?.(name);
  }

  function remove() {
    const name = column.name;
    onclose();
    ondelete?.(name);
  }

  // How the column's numbers and dates are written (formats §2.3). Every change is saved at once.
  function setFormat(p: Partial<NumFormat>) {
    const next: Record<string, unknown> = { ...(mine?.format ?? {}), ...p };
    for (const k of Object.keys(next)) if (next[k] === undefined || next[k] === false || next[k] === '') delete next[k];
    patch({ format: Object.keys(next).length ? (next as NumFormat) : undefined }, false);
  }
  const SAMPLE_DAY = '2026-10-15T14:30:00';
  const PATTERNS = ['d MMM yyyy', 'd MMMM yyyy', 'dd/MM/yyyy', 'MM/dd/yyyy', 'yyyy-MM-dd', 'EEE, d MMM yyyy', 'MMM yyyy'];
  const TIME_PATTERNS = ['d MMM yyyy HH:mm', 'dd/MM/yyyy HH:mm', 'yyyy-MM-dd HH:mm:ss', 'd MMM yyyy h:mm a'];
  // svelte-ignore state_referenced_locally
  let pattern = $state(setup?.columns?.[column.name]?.date_pattern ?? '');
  const ownPattern = $derived(pattern.trim() ? patternProblem(pattern.trim()) : null);

  function pickType(dtype: Column['dtype']) {
    if (dtype === column.dtype) return;
    // A date column asks for its format next, so the menu stays open.
    patch({ dtype }, !isDate(dtype));
  }
</script>

<svelte:window bind:innerWidth={winW} onmousedown={outside} onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="menu" class:wide={mode === 'format'} bind:this={root} style:left="{left}px" style:top="{y}px" role="menu">
  {#if mode === 'main'}
    <button role="menuitem" onclick={() => (mode = 'rename')}><Icon name="pencil" size={13} /> {t('Rename')}</button>
    {#if canRetype}
      <button role="menuitem" onclick={() => (mode = 'type')}>
        <Icon name="text" size={13} /> {t('Type')}
        <span class="more">{TYPE_CHOICES.find((c) => c.dtype === column.dtype)?.label}<Icon name="chevronRight" size={12} /></span>
      </button>
    {/if}
    <button role="menuitem" onclick={() => patch({ hidden: !column.hidden })}>
      <Icon name={column.hidden ? 'check' : 'x'} size={13} /> {column.hidden ? t('Show in field lists') : t('Hide from field lists')}
    </button>
    <button role="menuitem" onclick={() => (mode = 'description')}><Icon name="info" size={13} /> {t('Description')}</button>
    {#if column.dtype === 'number'}
      <button role="menuitem" onclick={() => (mode = 'format')}>
        <Icon name="hash" size={13} /> {t('Number format')}
        <span class="more">{mine?.format ? t('Set') : t('Automatic')}<Icon name="chevronRight" size={12} /></span>
      </button>
    {:else if isDate(column.dtype)}
      <button role="menuitem" onclick={() => (mode = 'dates')}>
        <Icon name="calendar" size={13} /> {t('Date format')}
        <span class="more">{mine?.date_pattern ? datePattern(SAMPLE_DAY, mine.date_pattern, app.locale) : t('Automatic')}<Icon name="chevronRight" size={12} /></span>
      </button>
    {/if}
    {#if column.calc && (onedit || ondelete)}
      <div class="rule" role="separator"></div>
      {#if onedit}
        <button role="menuitem" onclick={edit}><Icon name="fx" size={13} /> {t('Edit formula')}</button>
      {/if}
      {#if ondelete}
        <button role="menuitem" class="danger" onclick={remove}><Icon name="trash" size={13} /> {t('Delete column')}</button>
      {/if}
    {/if}
  {:else if mode === 'rename'}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        if (!nameProblem) patch({ label });
      }}>
      <label class="label" for="cm-name">{t('Name shown in Tenun')}</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="cm-name" class="field" bind:value={label} placeholder={column.name} aria-invalid={!!nameProblem} autofocus />
      {#if nameProblem}
        <p class="hint warn" role="alert">{nameProblem}</p>
      {:else}
        <p class="hint">{t('Queries still use "{name}".', { name: column.name })}</p>
      {/if}
      <div class="row">
        {#if column.label}<button type="button" class="btn plain" onclick={() => patch({ label: undefined })}>{t('Reset')}</button>{/if}
        <span class="sp"></span>
        <button type="button" class="btn" onclick={() => (mode = 'main')}>{t('Back')}</button>
        <button class="btn primary" disabled={!!nameProblem}>{t('Save')}</button>
      </div>
    </form>
  {:else if mode === 'description'}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        patch({ description });
      }}>
      <label class="label" for="cm-desc">{t('Shown as a tip in the field lists')}</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="cm-desc" class="field" bind:value={description} placeholder={t('What is in this column?')} autofocus />
      <div class="row">
        {#if column.description}<button type="button" class="btn plain" onclick={() => patch({ description: undefined })}>{t('Remove')}</button>{/if}
        <span class="sp"></span>
        <button type="button" class="btn" onclick={() => (mode = 'main')}>{t('Back')}</button>
        <button class="btn primary">{t('Save')}</button>
      </div>
    </form>
  {:else if mode === 'format'}
    <div class="head"><button class="btn plain icon" aria-label={t('Back')} onclick={() => (mode = 'main')}><Icon name="chevronLeft" size={13} /></button> {t('Number format')}</div>
    <div class="pad">
      <FormatEditor value={mine?.format ?? null} onchange={setFormat} shortDefault={false} blank advanced={!!app.settings?.advanced} label={t('Every tile that adds up this column')} />
      <p class="hint">{t('A sum, an average, a minimum or a maximum of this column is written this way. A tile or a measure can still set its own.')}</p>
      {#if mine?.format}<button type="button" class="btn plain" onclick={() => patch({ format: undefined }, false)}>{t('Back to automatic')}</button>{/if}
    </div>
  {:else if mode === 'dates'}
    <div class="head"><button class="btn plain icon" aria-label={t('Back')} onclick={() => (mode = 'main')}><Icon name="chevronLeft" size={13} /></button> {t('Date format')}</div>
    <button role="menuitemradio" aria-checked={!mine?.date_pattern} class:on={!mine?.date_pattern} onclick={() => { pattern = ''; patch({ date_pattern: undefined }, false); }}>
      <span class="tick">{#if !mine?.date_pattern}<Icon name="check" size={12} />{/if}</span>{t('Automatic')}
    </button>
    {#each column.dtype === 'datetime' ? TIME_PATTERNS : PATTERNS as p (p)}
      <button role="menuitemradio" aria-checked={mine?.date_pattern === p} class:on={mine?.date_pattern === p} onclick={() => { pattern = p; patch({ date_pattern: p }, false); }}>
        <span class="tick">{#if mine?.date_pattern === p}<Icon name="check" size={12} />{/if}</span>{datePattern(SAMPLE_DAY, p, app.locale)}
      </button>
    {/each}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        if (!ownPattern) patch({ date_pattern: pattern.trim() || undefined }, false);
      }}>
      <label class="label" for="cm-pat">{t('Your own pattern')}</label>
      <input id="cm-pat" class="field mono" bind:value={pattern} placeholder="d MMM yyyy" spellcheck="false" autocomplete="off" />
      {#if ownPattern}
        <p class="hint warn" role="alert">{ownPattern}</p>
      {:else}
        <p class="hint">{pattern.trim() ? datePattern(SAMPLE_DAY, pattern.trim(), app.locale) : 'yyyy MM MMM MMMM d dd EEE EEEE HH mm ss · \'text\''}</p>
      {/if}
      <div class="row"><span class="sp"></span><button class="btn primary" disabled={!!ownPattern || !pattern.trim()}>{t('Use')}</button></div>
    </form>
  {:else}
    <div class="head"><button class="btn plain icon" aria-label={t('Back')} onclick={() => (mode = 'main')}><Icon name="chevronLeft" size={13} /></button> {t('Read this column as')}</div>
    {#each TYPE_CHOICES as c (c.dtype)}
      <button role="menuitemradio" aria-checked={column.dtype === c.dtype} class:on={column.dtype === c.dtype} onclick={() => pickType(c.dtype)}>
        <span class="tick">{#if column.dtype === c.dtype}<Icon name="check" size={12} />{/if}</span>{c.label}
      </button>
    {/each}
    {#if isDate(column.dtype) && mine?.dtype}
      <div class="sub">{t('Dates written as text')}</div>
      {#each DATE_FORMATS as f (f.value)}
        <button role="menuitemradio" aria-checked={format === f.value} class:on={format === f.value} onclick={() => patch({ date_format: f.value }, false)}>
          <span class="tick">{#if format === f.value}<Icon name="check" size={12} />{/if}</span>{f.label}
        </button>
      {/each}
    {/if}
    {#if mine?.dtype}
      <button role="menuitem" onclick={() => patch({ dtype: undefined, date_format: undefined })}>{t('Use the type from the file')}</button>
    {/if}
    {#if column.failed}
      <p class="hint warn">{t('Cells that cannot be read become blank.')}</p>
    {/if}
  {/if}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 45;
    min-width: 210px;
    max-width: 260px;
    padding: 6px;
    border-radius: var(--r-card);
    background: var(--raised);
    box-shadow: var(--shadow-float);
    font-size: 12px;
    animation: pop 180ms var(--spring);
    transform-origin: top left;
  }
  button[role^='menuitem'] {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
    font-size: 12px;
  }
  button[role^='menuitem']:hover { background: var(--accent); color: #fff; }
  button[role^='menuitem'] :global(svg) { color: var(--text-2); }
  button[role^='menuitem']:hover :global(svg), button[role^='menuitem']:hover .more { color: #fff; }
  .more { margin-left: auto; display: inline-flex; align-items: center; gap: 2px; color: var(--text-2); }
  .on { font-weight: 600; }
  .rule { height: 1px; margin: 4px 6px; background: var(--sep); }
  button.danger { color: var(--danger); }
  button.danger :global(svg) { color: var(--danger); }
  button.danger:hover, button.danger:hover :global(svg) { color: #fff; }
  .tick { width: 14px; display: inline-grid; place-items: center; }
  .menu.wide { max-width: 340px; width: 340px; }
  .pad { display: flex; flex-direction: column; gap: 8px; padding: 4px; }
  .mono { font-family: var(--mono); }
  .head { display: flex; align-items: center; gap: 4px; padding: 0 4px 4px; font-weight: 600; }
  .sub { padding: 8px 8px 2px; font-size: 11px; font-weight: 600; color: var(--text-2); }
  form { display: flex; flex-direction: column; gap: 6px; padding: 4px; }
  .row { display: flex; align-items: center; gap: 6px; margin-top: 4px; }
  .sp { flex: 1; }
  .hint { margin: 0; font-size: 11px; color: var(--text-2); }
  .warn { padding: 6px 8px 2px; }
  form .warn { padding: 0; color: var(--danger); }
  @keyframes pop { from { opacity: 0; transform: scale(0.98); } }
</style>
