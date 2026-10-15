<script lang="ts">
  // Add a measure, or change one (build 03). A measure is a formula of totals with a name, such as
  // SUM([Sales].[amount]) / SUM([Targets].[target]). It may add up tables from different sources, and
  // each table is totalled on its own, so a target is never counted once for each sale.
  // The engine checks the formula as you type: the problem is underlined, or the type and the grand
  // total show. Nothing is saved until the formula is good.
  import { api, MeasureRefused, type FormulaCheck, type FormulaError, type NamedMeasure, type NumFormat } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { cell } from '../../lib/format';
  import { typeWords } from '../../lib/formula';
  import { measureScope } from '../../lib/formulaScope';
  import { formulaProblem, nameProblem, tilesWith } from '../../lib/measures';
  import Modal from '../Modal.svelte';
  import Icon from '../Icon.svelte';
  import FormatEditor from '../FormatEditor.svelte';
  import FormulaField from './FormulaField.svelte';

  let {
    editing,
    onclose,
  }: {
    /** The id of the measure to change. Not set: a new measure. */
    editing?: string;
    onclose: () => void;
  } = $props();

  const all = $derived(app.snap!.workspace.measures ?? []);
  // The sheet starts from the measure and is then the user's.
  // svelte-ignore state_referenced_locally
  const old = all.find((m) => m.id === editing);
  // svelte-ignore state_referenced_locally
  let name = $state(old?.name ?? '');
  // svelte-ignore state_referenced_locally
  let formula = $state(old?.formula ?? '');
  // svelte-ignore state_referenced_locally
  let description = $state(old?.description ?? '');
  // svelte-ignore state_referenced_locally
  let fmt = $state<NumFormat>({ ...(old?.format ?? {}) });

  type Check = { state: 'idle' | 'checking' | 'ok' | 'bad'; reply?: FormulaCheck };
  let check = $state<Check>({ state: 'idle' });
  /** The engine refused the save: the same kind of answer as a check, and which field it is about. */
  let refused = $state<{ field: 'name' | 'formula' | 'list'; error: FormulaError } | null>(null);
  let saving = $state(false);
  let seq = 0;

  const scope = $derived(measureScope(app.tables, app.snap!.workspace.relationships));
  const nameBad = $derived((name.trim() ? nameProblem(name, all, editing) : null) ?? (refused?.field === 'name' ? refused.error.message : null));
  const formulaBad = $derived(formula.trim() ? formulaProblem(formula) : null);
  const error = $derived<FormulaError | null>(refused?.field === 'formula' ? refused.error : check.state === 'bad' ? (check.reply?.error ?? null) : null);
  const valid = $derived(!nameProblem(name, all, editing) && !formulaProblem(formula) && check.state === 'ok' && !refused && !saving);
  /** Something was typed that is not saved: closing asks first (V4-D58). */
  const dirty = $derived(
    name.trim() !== (old?.name ?? '') ||
      formula.trim() !== (old?.formula ?? '') ||
      description.trim() !== (old?.description ?? '') ||
      JSON.stringify(fmt) !== JSON.stringify(old?.format ?? {}),
  );
  function close() {
    if (dirty && !confirm('Close without saving what you typed?')) return;
    onclose();
  }
  const used = $derived(editing ? tilesWith(app.dashboards, editing).length : 0);
  const readsTables = $derived((check.reply?.uses ?? []).map((id) => app.table(id)?.label ?? id));

  // Ask the engine a moment after the typing stops. A late answer to an older formula is dropped.
  $effect(() => {
    const f = formula;
    if (!f.trim() || formulaProblem(f)) {
      // An answer that was asked for before the field was emptied must not fill the panel.
      seq++;
      check = { state: 'idle' };
      return;
    }
    check = { state: 'checking' };
    const mine = ++seq;
    const t = setTimeout(() => {
      api
        .checkFormula({ formula: f, kind: 'measure' })
        .then((reply) => {
          if (mine === seq) check = { state: reply.ok ? 'ok' : 'bad', reply };
        })
        .catch((e: Error) => {
          if (mine === seq) check = { state: 'bad', reply: { ok: false, error: { message: e.message, at: 0, len: 0 } } };
        });
    }, 280);
    return () => clearTimeout(t);
  });

  // ---- the format ----------------------------------------------------------------------------
  function setFmt(patch: Partial<NumFormat>) {
    const next: NumFormat = { ...fmt, ...patch };
    for (const k of Object.keys(next) as (keyof NumFormat)[]) if (next[k] === undefined || next[k] === false || next[k] === '') delete next[k];
    fmt = next;
    refused = null;
  }
  const hasFormat = $derived(Object.keys(fmt).length > 0);

  const total = $derived(check.reply?.preview?.[0]);
  const shown = $derived.by(() => {
    const v = total;
    if (v === null || v === undefined || v === '') return 'blank';
    if (check.reply?.dtype === 'number' && typeof v === 'number') return cell(v, 'number', null, false, hasFormat ? fmt : null);
    return String(v);
  });

  async function save() {
    if (!valid) return;
    const m: NamedMeasure = { id: old?.id ?? '', name: name.trim().replace(/\s+/g, ' '), formula: formula.trim() };
    if (hasFormat) m.format = fmt;
    if (description.trim()) m.description = description.trim();
    const list = [...all];
    const at = old ? list.findIndex((x) => x.id === old.id) : -1;
    if (at < 0) list.push(m);
    else list[at] = m;
    saving = true;
    app.busy = 'Saving';
    try {
      const r = await api.setMeasures(list);
      if ('saved' in r) {
        app.snap = r.saved;
        onclose();
      }
    } catch (e) {
      if (e instanceof MeasureRefused) refused = { field: e.field, error: e.error };
      else app.error(e);
    } finally {
      saving = false;
      app.busy = null;
    }
  }
</script>

<Modal title={editing === undefined ? 'New measure' : `Measure ${old?.name ?? ''}`} width={600} onclose={close}>
  <form
    id="measure-form"
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}>
    <label class="row">
      <span class="label">Name</span>
      <input class="field" bind:value={name} placeholder="Achievement" autocomplete="off" aria-invalid={!!nameBad} oninput={() => (refused = null)} />
    </label>
    {#if nameBad}<p class="bad">{nameBad}</p>{/if}
    {#if editing !== undefined && used}
      <p class="note">{used === 1 ? 'A tile uses' : `${used} tiles use`} this measure. They keep using it when the name changes.</p>
    {/if}

    <div class="row">
      <span class="label" id="measure-formula-label">Formula</span>
      <FormulaField
        bind:value={formula}
        {scope}
        {error}
        label="Formula"
        placeholder={'SUM([Sales].[amount]) / SUM([Targets].[target])'}
        oninput={() => (refused = null)} />
      <div class="result" aria-live="polite">
        {#if formulaBad}
          <p class="bad"><Icon name="warning" size={13} /> {formulaBad}</p>
        {:else if check.state === 'idle'}
          <p class="note">
            Add up columns with SUM, AVERAGE, COUNT and the like, then use them in arithmetic. A column is in square brackets with its
            table: [Sales].[amount]. Type [ to see the tables. The parts can come from different files and databases.
          </p>
        {:else if check.state === 'checking'}
          <p class="note">Checking…</p>
        {:else if error}
          <p class="bad"><Icon name="warning" size={13} /> {error.message}</p>
        {:else if check.reply?.ok}
          <div class="ok">
            <span class="type"><Icon name="check" size={12} /> {check.reply.dtype ? typeWords(check.reply.dtype) : ''}</span>
            <span class="total">
              <span class="cap">All data</span>
              <span class="v" class:blank={total === null || total === ''}>{shown}</span>
            </span>
          </div>
          {#if readsTables.length > 1}
            <p class="note">Each of {readsTables.join(' and ')} is added up on its own, then the formula combines them.</p>
          {/if}
        {/if}
      </div>
    </div>

    <div class="row">
      <FormatEditor value={fmt} onchange={setFmt} advanced={!!app.settings?.advanced} label="Format (optional; a tile can use its own)" />
    </div>

    <label class="row">
      <span class="label">Description <em>(optional)</em></span>
      <input class="field" bind:value={description} placeholder="What does this measure tell?" autocomplete="off" oninput={() => (refused = null)} />
    </label>
  </form>
  {#snippet footer()}
    {#if refused?.field === 'list'}<p class="bad foot"><Icon name="warning" size={13} /> {refused.error.message}</p>{/if}
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button type="submit" form="measure-form" class="btn primary" disabled={!valid}>{editing === undefined ? 'Add measure' : 'Save measure'}</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .row { display: flex; flex-direction: column; gap: 5px; }
  .label em { font-style: normal; color: var(--text-2); font-weight: 400; }
  .note { margin: 0; color: var(--text-2); font-size: 12px; }
  .bad { display: flex; gap: 6px; align-items: flex-start; margin: 0; color: var(--danger); font-size: 12px; }
  .bad :global(svg) { margin-top: 1px; }
  .result { min-height: 26px; display: flex; flex-direction: column; gap: 6px; }
  .ok { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .type {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 8px;
    border-radius: 11px;
    background: var(--selected);
    color: var(--accent-text);
    font-size: 12px;
    font-weight: 600;
  }
  .total { display: inline-flex; align-items: center; gap: 6px; min-width: 0; }
  .cap { color: var(--text-2); font-size: 12px; }
  .v {
    padding: 2px 8px;
    border-radius: 5px;
    background: var(--inset);
    font-size: 13px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .v.blank { color: var(--text-2); font-style: italic; font-weight: 400; }
  .foot { margin-right: auto; }
</style>
