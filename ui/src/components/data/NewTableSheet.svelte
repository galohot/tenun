<script lang="ts">
  // Make a new table from other tables, or change how one is made (build 04, V4-D6): append, merge or
  // group. The engine looks at the table as you pick (columns, the first rows, how many rows and, for a
  // merge, how many rows found a match). Nothing is saved until "Make table".
  import { untrack } from 'svelte';
  import { api, type DerivedPreview, type DerivedSpec, type GroupValue } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { t, tn } from '../../lib/i18n';
  import { cell } from '../../lib/format';
  import { appendColumns, guessKey, OPS, pickable, specOf, specProblem, tableNameProblem, type DerivedOp } from '../../lib/derived';
  import { scopeFor } from '../../lib/formulaScope';
  import Modal from '../Modal.svelte';
  import Icon from '../Icon.svelte';
  import FormulaField from './FormulaField.svelte';

  let {
    op: startOp,
    editing,
    onclose,
    ondone,
  }: {
    /** The way to make a new table. Ignored when `editing` is set: the source says it. */
    op: DerivedOp;
    /** The id of the new table source to change. Not set: a new table. */
    editing?: string;
    onclose: () => void;
    /** After the table is made or changed, with the id of its table. */
    ondone?: (table: string) => void;
  } = $props();

  const sources = $derived(app.snap!.workspace.sources);
  // The sheet starts from the source and is then the user's.
  // svelte-ignore state_referenced_locally
  const old = editing ? sources.find((s) => s.id === editing) : undefined;
  const oldSpec = old ? specOf(old) : null;
  // svelte-ignore state_referenced_locally
  const op: DerivedOp = oldSpec?.op ?? startOp;
  const meta = OPS.find((o) => o.op === op)!;

  let name = $state(old?.name ?? '');
  // Append.
  let tables = $state<string[]>(oldSpec?.op === 'append' ? [...oldSpec.tables] : []);
  // Merge.
  let left = $state(oldSpec?.op === 'merge' ? oldSpec.left : '');
  let right = $state(oldSpec?.op === 'merge' ? oldSpec.right : '');
  let on = $state<[string, string][]>(oldSpec?.op === 'merge' ? oldSpec.on.map((p) => [p[0], p[1]] as [string, string]) : [['', '']]);
  let keep = $state<'left' | 'inner'>(oldSpec?.op === 'merge' ? oldSpec.keep : 'left');
  // Group.
  let table = $state(oldSpec?.op === 'group' ? oldSpec.table : '');
  let by = $state<string[]>(oldSpec?.op === 'group' ? [...oldSpec.by] : []);
  let values = $state<GroupValue[]>(oldSpec?.op === 'group' ? oldSpec.values.map((v) => ({ ...v })) : [{ name: '', formula: '' }]);

  const choices = $derived(pickable(app.tables, sources, editing));
  const tableOf = (id: string) => app.tables.find((t) => t.id === id);

  const spec = $derived.by<DerivedSpec>(() => {
    if (op === 'append') return { op, tables: [...tables] };
    if (op === 'merge') return { op, left, right, on: on.map((p) => [p[0], p[1]] as [string, string]), keep };
    return { op, table, by: [...by], values: values.filter((v) => v.name.trim() || v.formula.trim()).map((v) => ({ name: v.name.trim(), formula: v.formula.trim() })) };
  });
  const problem = $derived(specProblem(spec));
  const nameBad = $derived(editing ? null : tableNameProblem(name));

  type Look = { state: 'idle' | 'looking' | 'ok' | 'bad'; preview?: DerivedPreview; error?: string };
  let look = $state<Look>({ state: 'idle' });
  let seq = 0;
  // Ask the engine a moment after the picking stops. A late answer to an older pick is dropped.
  $effect(() => {
    const s = spec;
    const p = problem;
    seq++;
    if (p) {
      look = { state: 'idle' };
      return;
    }
    const mine = seq;
    // The last look stays on screen, dimmed, while the next one comes. Reading it must not make this
    // effect run again when it is written.
    look = { state: 'looking', preview: untrack(() => look.preview) };
    const t = setTimeout(() => {
      api
        .previewDerived($state.snapshot(s) as DerivedSpec)
        .then((preview) => {
          if (mine === seq) look = { state: 'ok', preview };
        })
        .catch((e: Error) => {
          if (mine === seq) look = { state: 'bad', error: e.message };
        });
    }, 300);
    return () => clearTimeout(t);
  });

  let saving = $state(false);
  const valid = $derived(!problem && !nameBad && look.state === 'ok' && !saving);
  const dirty = $derived(JSON.stringify(spec) !== JSON.stringify(oldSpec ?? null) || (!editing && !!name.trim()));
  function close() {
    if (dirty && !confirm(t('Close without making the table?'))) return;
    onclose();
  }

  async function save() {
    if (!valid) return;
    saving = true;
    app.busy = editing ? t('Saving') : t('Making the table');
    try {
      if (editing) {
        app.snap = await api.setDerived(editing, $state.snapshot(spec) as DerivedSpec);
        ondone?.(`${editing}/${old?.name ?? ''}`);
      } else {
        const r = await api.addDerived(name.trim(), $state.snapshot(spec) as DerivedSpec);
        app.snap = r;
        ondone?.(`${r.added}/${name.trim()}`);
      }
      onclose();
    } catch (e) {
      app.error(e);
    } finally {
      saving = false;
      app.busy = null;
    }
  }

  // ---- append --------------------------------------------------------------------------------
  const picked = $derived(tables.map(tableOf).filter((t) => !!t));
  const columns = $derived(appendColumns(picked));
  function toggle(id: string, on: boolean) {
    tables = on ? [...tables, id] : tables.filter((x) => x !== id);
  }

  // ---- merge ---------------------------------------------------------------------------------
  const leftT = $derived(tableOf(left));
  const rightT = $derived(tableOf(right));
  function pickSide(side: 'left' | 'right', id: string) {
    if (side === 'left') left = id;
    else right = id;
    // A first pair from the names, when none is picked yet.
    if (on.length === 1 && !on[0][0] && !on[0][1]) {
      const g = guessKey(tableOf(left), tableOf(right));
      if (g[0]) on = [g];
    }
  }

  // ---- group ---------------------------------------------------------------------------------
  const groupT = $derived(tableOf(table));
  const scope = $derived(groupT ? scopeFor(groupT, app.tables, app.snap!.workspace.relationships, app.linkHealth, 'group') : null);
  function pickTable(id: string) {
    table = id;
    by = by.filter((b) => tableOf(id)?.columns.some((c) => c.name === b));
  }
</script>

<Modal title={editing ? t('New table {name}', { name: old?.name ?? '' }) : meta.title} width={680} onclose={close}>
  <form
    id="derived-form"
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}>
    <p class="intro"><Icon name={meta.icon} size={15} /> {meta.words}</p>

    {#if op === 'append'}
      <fieldset>
        <legend class="label">{t('Tables, in the order their rows come')}</legend>
        <div class="picks">
          {#each choices as tb (tb.id)}
            <label class="pick"><input type="checkbox" checked={tables.includes(tb.id)} onchange={(e) => toggle(tb.id, e.currentTarget.checked)} /> {tb.label} <span class="dim">{t('{n} columns', { n: tb.columns.length })}</span></label>
          {/each}
        </div>
      </fieldset>
      {#if picked.length >= 2}
        <div class="cols" role="table" aria-label={t('The columns of the new table')}>
          <div class="crow head" role="row"><span role="columnheader">{t('Column')}</span><span role="columnheader">{t('Blank in')}</span></div>
          <div class="crow" role="row"><span role="cell"><strong>source</strong></span><span role="cell" class="dim">{t('The name of the table each row came from')}</span></div>
          {#each columns as c (c.name)}
            <div class="crow" role="row">
              <span role="cell">{c.name}{#if c.types.length > 1}<em class="warn" title={t('The tables hold different kinds of values here, so the column becomes text.')}> {t('mixed, read as text')}</em>{/if}</span>
              <span role="cell" class:dim={!c.missing.length}>{c.missing.length ? c.missing.join(', ') : '–'}</span>
            </div>
          {/each}
        </div>
      {/if}
    {:else if op === 'merge'}
      <div class="two">
        <label class="row">
          <span class="label">{t('Left table: every row of it stays')}</span>
          <select class="field" value={left} onchange={(e) => pickSide('left', e.currentTarget.value)}>
            <option value="" disabled>{t('Pick a table')}</option>
            {#each choices as tb (tb.id)}<option value={tb.id}>{tb.label}</option>{/each}
          </select>
        </label>
        <label class="row">
          <span class="label">{t('Right table: its columns come beside')}</span>
          <select class="field" value={right} onchange={(e) => pickSide('right', e.currentTarget.value)}>
            <option value="" disabled>{t('Pick a table')}</option>
            {#each choices as tb (tb.id)}<option value={tb.id}>{tb.label}</option>{/each}
          </select>
        </label>
      </div>
      <fieldset>
        <legend class="label">{t('Match rows where')}</legend>
        {#each on as pair, i (i)}
          <div class="pair">
            <select class="field" aria-label={t('Column of the left table')} bind:value={pair[0]}>
              <option value="" disabled>{leftT ? t('A column') : t('Pick the left table')}</option>
              {#each leftT?.columns ?? [] as c (c.name)}<option value={c.name}>{c.label ?? c.name}</option>{/each}
            </select>
            <span class="eq">=</span>
            <select class="field" aria-label={t('Column of the right table')} bind:value={pair[1]}>
              <option value="" disabled>{rightT ? t('A column') : t('Pick the right table')}</option>
              {#each rightT?.columns ?? [] as c (c.name)}<option value={c.name}>{c.label ?? c.name}</option>{/each}
            </select>
            <button type="button" class="btn plain icon" aria-label={t('Remove this pair')} disabled={on.length < 2} onclick={() => (on = on.filter((_, j) => j !== i))}><Icon name="x" size={13} /></button>
          </div>
        {/each}
        <button type="button" class="btn plain add" onclick={() => (on = [...on, ['', '']])}><Icon name="plus" size={13} /> {t('Add another pair')}</button>
      </fieldset>
      <fieldset class="seg" aria-label={t('Which rows to keep')}>
        <legend class="label">{t('Keep')}</legend>
        <label><input type="radio" name="keep" value="left" bind:group={keep} /> {t('All rows of the left table')}</label>
        <label><input type="radio" name="keep" value="inner" bind:group={keep} /> {t('Only rows that match')}</label>
      </fieldset>
      <p class="note">{t('When the right table has a key twice, the first row of it is used, so no row of the left table is counted twice.')}</p>
    {:else}
      <label class="row">
        <span class="label">{t('Table')}</span>
        <select class="field" value={table} onchange={(e) => pickTable(e.currentTarget.value)}>
          <option value="" disabled>{t('Pick a table')}</option>
          {#each choices as tb (tb.id)}<option value={tb.id}>{tb.label}</option>{/each}
        </select>
      </label>
      {#if groupT}
        <fieldset>
          <legend class="label">{t('Group by')}</legend>
          <div class="picks">
            {#each groupT.columns as c (c.name)}
              <label class="pick"><input type="checkbox" checked={by.includes(c.name)} onchange={(e) => (by = e.currentTarget.checked ? [...by, c.name] : by.filter((x) => x !== c.name))} /> {c.label ?? c.name}</label>
            {/each}
          </div>
        </fieldset>
        <fieldset>
          <legend class="label">{t('Values')}</legend>
          {#each values as v, i (i)}
            <div class="value">
              <input class="field vname" aria-label={t('Name of value {n}', { n: i + 1 })} placeholder={t('Total')} bind:value={v.name} />
              <div class="vformula">
                <FormulaField bind:value={v.formula} {scope} label={t('Formula of value {n}', { n: i + 1 })} placeholder={'SUM([amount])'} />
              </div>
              <button type="button" class="btn plain icon" aria-label={t('Remove this value')} onclick={() => (values = values.filter((_, j) => j !== i))}><Icon name="x" size={13} /></button>
            </div>
          {/each}
          <button type="button" class="btn plain add" onclick={() => (values = [...values, { name: '', formula: '' }])}><Icon name="plus" size={13} /> {t('Add a value')}</button>
        </fieldset>
      {/if}
    {/if}

    <label class="row">
      <span class="label">{t('Name of the new table')}</span>
      <input class="field" bind:value={name} placeholder={op === 'group' ? t('Sales by province') : op === 'merge' ? t('Sales with stores') : t('All targets')} autocomplete="off" disabled={!!editing} aria-invalid={!!(name.trim() && nameBad)} />
    </label>
    {#if editing}<p class="note">{t('The name stays, so the links and tiles that use the table keep working.')}</p>{:else if name.trim() && nameBad}<p class="bad">{nameBad}</p>{/if}

    <section class="look" aria-live="polite" aria-label={t('A look at the new table')}>
      {#if problem}
        <p class="note">{problem}</p>
      {:else if look.state === 'bad'}
        <p class="bad"><Icon name="warning" size={13} /> {look.error}</p>
      {:else if look.preview}
        <p class="count" class:stale={look.state === 'looking'}>{tn('{n} row, {k} columns', '{n} rows, {k} columns', look.preview.total, { n: look.preview.total.toLocaleString('en-US'), k: look.preview.columns.length })}</p>
        {#each look.preview.notes ?? [] as n (n)}<p class="note">{n}</p>{/each}
        <div class="grid" style:--cols={look.preview.columns.length}>
          <table>
            <thead><tr>{#each look.preview.columns as c (c.name)}<th>{c.name}</th>{/each}</tr></thead>
            <tbody>
              {#each look.preview.rows as r, i (i)}
                <tr>{#each r as v, j (j)}<td class:blank={v === null || v === ''}>{v === null || v === '' ? t('blank') : cell(v, look.preview.columns[j]?.dtype ?? 'text', null, false)}</td>{/each}</tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else if look.state === 'looking'}
        <p class="note">{t('Looking…')}</p>
      {/if}
    </section>
  </form>
  {#snippet footer()}
    <button type="button" class="btn" onclick={close}>{t('Cancel')}</button>
    <button type="submit" form="derived-form" class="btn primary" disabled={!valid}>{editing ? t('Save') : t('Make table')}</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .intro { display: flex; gap: 8px; align-items: center; margin: 0; color: var(--text-2); font-size: 13px; }
  .intro :global(svg) { color: var(--accent-text); flex: none; }
  fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .row { display: flex; flex-direction: column; gap: 5px; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .picks { display: flex; flex-wrap: wrap; gap: 4px 14px; max-height: 160px; overflow: auto; }
  .pick { display: inline-flex; gap: 6px; align-items: center; font-size: 13px; }
  .dim { color: var(--text-2); }
  .cols { border: 1px solid var(--sep); border-radius: 8px; max-height: 200px; overflow: auto; font-size: 12px; }
  .crow { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr); gap: 10px; padding: 5px 10px; border-top: 1px solid var(--sep); }
  .crow.head { border-top: 0; font-weight: 600; color: var(--text-2); position: sticky; top: 0; background: var(--surface); }
  .warn { font-style: normal; color: var(--warning-text, var(--text-2)); }
  .pair { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto; gap: 6px; align-items: center; }
  .eq { color: var(--text-2); }
  .add { align-self: flex-start; display: inline-flex; gap: 4px; align-items: center; }
  .seg { flex-direction: row; flex-wrap: wrap; gap: 6px 16px; align-items: center; font-size: 13px; }
  .seg legend { width: 100%; margin-bottom: 2px; }
  .seg label { display: inline-flex; gap: 6px; align-items: center; }
  .value { display: grid; grid-template-columns: 150px minmax(0, 1fr) auto; gap: 6px; align-items: start; }
  .vformula { min-width: 0; }
  .note { margin: 0; color: var(--text-2); font-size: 12px; }
  .bad { display: flex; gap: 6px; align-items: flex-start; margin: 0; color: var(--danger); font-size: 12px; }
  .look { display: flex; flex-direction: column; gap: 6px; min-height: 40px; }
  .count { margin: 0; font-size: 12px; font-weight: 600; }
  .count.stale { opacity: 0.6; }
  .grid { overflow: auto; max-height: 180px; border: 1px solid var(--sep); border-radius: 8px; }
  table { border-collapse: collapse; font-size: 12px; width: max-content; min-width: 100%; }
  th, td { padding: 4px 10px; text-align: left; white-space: nowrap; border-bottom: 1px solid var(--sep); max-width: 220px; overflow: hidden; text-overflow: ellipsis; }
  th { position: sticky; top: 0; background: var(--surface); font-weight: 600; }
  td { font-variant-numeric: tabular-nums; }
  td.blank { color: var(--text-2); font-style: italic; }
  @media (max-width: 560px) {
    .two { grid-template-columns: 1fr; }
    .value { grid-template-columns: 1fr auto; }
    .vformula { grid-column: 1 / -1; grid-row: 2; }
  }
</style>
