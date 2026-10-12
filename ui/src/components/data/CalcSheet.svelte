<script lang="ts">
  // Add a column that is computed from the others, or change the formula of one (build 02).
  // The engine checks the formula as you type; the sheet shows the problem underlined, or the type
  // and the first values. Nothing is saved until the formula is good.
  import { api, SetupRefused, type CalcColumn, type FormulaCheck, type FormulaError, type Table, type TableSetup } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { cell } from '../../lib/format';
  import { nameProblem, typeWords } from '../../lib/formula';
  import { scopeFor } from '../../lib/formulaScope';
  import { withCalc } from '../../lib/prep';
  import Modal from '../Modal.svelte';
  import Icon from '../Icon.svelte';
  import FormulaField from './FormulaField.svelte';

  let {
    table,
    setup,
    editing,
    onclose,
  }: {
    table: Table;
    setup: TableSetup | undefined;
    /** The name of the calculated column to change. Not set: a new column. */
    editing?: string;
    onclose: () => void;
  } = $props();

  // The sheet starts from the column and is then the user's.
  // svelte-ignore state_referenced_locally
  const old = setup?.calc?.find((c) => c.name === editing);
  // svelte-ignore state_referenced_locally
  let name = $state(old?.name ?? '');
  // svelte-ignore state_referenced_locally
  let formula = $state(old?.formula ?? '');
  // svelte-ignore state_referenced_locally
  let description = $state(old?.description ?? '');
  // svelte-ignore state_referenced_locally
  let sql = $state(old?.sql ?? false);

  type Check = { state: 'idle' | 'checking' | 'ok' | 'bad'; reply?: FormulaCheck };
  let check = $state<Check>({ state: 'idle' });
  /** The engine refused the save: the same kind of answer as a check. */
  let refused = $state<FormulaError | null>(null);
  let saving = $state(false);
  let seq = 0;

  const scope = $derived(sql ? null : scopeFor(table, app.tables, app.snap!.workspace.relationships, app.linkHealth, 'column', editing));
  const problem = $derived(editing ? null : nameProblem(name, table));
  const error = $derived<FormulaError | null>(refused ?? (check.state === 'bad' ? (check.reply?.error ?? null) : null));
  const valid = $derived(!problem && formula.trim() !== '' && check.state === 'ok' && !refused && !saving);
  const usesToday = $derived(!sql && /\b(TODAY|NOW)\s*\(/i.test(formula));

  // Ask the engine a moment after the typing stops. A late answer to an older formula is dropped.
  $effect(() => {
    const f = formula;
    const isSql = sql;
    if (!f.trim()) {
      // An answer that was asked for before the field was emptied must not fill the panel.
      seq++;
      check = { state: 'idle' };
      return;
    }
    check = { state: 'checking' };
    const mine = ++seq;
    const t = setTimeout(() => {
      api
        .checkFormula({ formula: f, kind: 'column', table: table.id, before: editing, sql: isSql })
        .then((reply) => {
          if (mine === seq) check = { state: reply.ok ? 'ok' : 'bad', reply };
        })
        .catch((e: Error) => {
          if (mine === seq) check = { state: 'bad', reply: { ok: false, error: { message: e.message, at: 0, len: 0 } } };
        });
    }, 280);
    return () => clearTimeout(t);
  });

  const show = (v: unknown) => {
    if (v === null || v === undefined || v === '') return 'blank';
    return check.reply?.dtype === 'number' ? cell(v as number, 'number') : String(v);
  };

  async function save() {
    if (!valid) return;
    const col: CalcColumn = { name: name.trim(), formula: formula.trim() };
    if (sql) col.sql = true;
    if (description.trim()) col.description = description.trim();
    const list = [...(setup?.calc ?? [])];
    const at = editing === undefined ? -1 : list.findIndex((c) => c.name === editing);
    if (at < 0) list.push(col);
    else list[at] = col;
    saving = true;
    app.busy = 'Saving';
    try {
      app.snap = await api.setTableSetup(table.id, withCalc(setup, list));
      onclose();
    } catch (e) {
      if (e instanceof SetupRefused) refused = e.error;
      else app.error(e);
    } finally {
      saving = false;
      app.busy = null;
    }
  }
</script>

<Modal title={editing === undefined ? `Add a column to ${table.label}` : `Formula of ${editing}`} width={560} {onclose}>
  <form
    id="calc-form"
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}>
    <label class="row">
      <span class="label">Name</span>
      <input class="field" bind:value={name} disabled={editing !== undefined} placeholder="Revenue incl. tax" autocomplete="off" />
    </label>
    {#if editing !== undefined}
      <p class="note">The name stays, so the tiles that use it keep working. To show another name, choose Rename in the column menu.</p>
    {:else if problem && name.trim()}
      <p class="bad">{problem}</p>
    {/if}

    <div class="row">
      <div class="head">
        <span class="label" id="calc-formula-label">{sql ? 'SQL expression' : 'Formula'}</span>
        {#if app.settings?.advanced}
          <button type="button" class="linkbtn" onclick={() => (sql = !sql)}>{sql ? 'Use a formula instead' : 'Use SQL instead'}</button>
        {/if}
      </div>
      <FormulaField
        bind:value={formula}
        {scope}
        {error}
        label={sql ? 'SQL expression' : 'Formula'}
        placeholder={sql ? '"Total sales" / 2' : '[amount] * 1.11'}
        oninput={() => (refused = null)} />
      <div class="result" aria-live="polite">
        {#if check.state === 'idle'}
          <p class="note">
            {#if sql}
              One DuckDB expression over the columns of this table. Put a name with spaces in double quotes. It cannot read other tables or files.
            {:else}
              Write it as in Excel. A column is in square brackets, like [amount]. Type [ to see the columns and related tables.
            {/if}
          </p>
        {:else if check.state === 'checking'}
          <p class="note">Checking…</p>
        {:else if error}
          <p class="bad"><Icon name="warning" size={13} /> {error.message}</p>
        {:else if check.reply?.ok}
          <div class="ok">
            <span class="type"><Icon name="check" size={12} /> {check.reply.dtype ? typeWords(check.reply.dtype) : ''}</span>
            <span class="values">
              {#each check.reply.preview ?? [] as v, i (i)}<span class="v" class:blank={v === null || v === ''}>{show(v)}</span>{/each}
            </span>
          </div>
          {#if usesToday}
            <p class="note">This column uses today's date. It is as of the day the table was loaded or refreshed.</p>
          {/if}
        {/if}
      </div>
    </div>

    <label class="row">
      <span class="label">Description <em>(optional)</em></span>
      <input class="field" bind:value={description} placeholder="What does this column hold?" autocomplete="off" />
    </label>
  </form>
  {#snippet footer()}
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button type="submit" form="calc-form" class="btn primary" disabled={!valid}>{editing === undefined ? 'Add column' : 'Save formula'}</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .row { display: flex; flex-direction: column; gap: 5px; }
  .label em { font-style: normal; color: var(--text-2); font-weight: 400; }
  .note { margin: 0; color: var(--text-2); font-size: 12px; }
  .bad { display: flex; gap: 6px; align-items: flex-start; margin: 0; color: var(--danger); font-size: 12px; }
  .bad :global(svg) { margin-top: 1px; }
  .head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .result { min-height: 26px; }
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
  .values { display: flex; flex-wrap: wrap; gap: 4px; min-width: 0; }
  .v {
    padding: 2px 7px;
    border-radius: 5px;
    background: var(--inset);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    max-width: 150px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .v.blank { color: var(--text-2); font-style: italic; }
  .linkbtn {
    border: 0;
    background: none;
    padding: 0;
    color: var(--accent-text);
    font-size: 12px;
    cursor: pointer;
  }
  .linkbtn:hover { text-decoration: underline; }
</style>
