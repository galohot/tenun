<script lang="ts">
  // Add a parameter, or change one (v5 build 05, Advanced). A parameter is a number with a smallest value, a largest
  // value and a step, or a choice from a list. A measure reads it with PARAM("name"); a Top N, a number tile's target
  // and a chart's line can take it. A parameter tile on a dashboard moves it. Renaming it rewrites every PARAM("…").
  import { api, type Cell, type Param, type NumFormat } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { parts, t, tn } from '../../lib/i18n';
  import { cell, numberFormatter } from '../../lib/format';
  import { choicesFrom, MAX_CHOICES, MAX_STEPS, newParam, paramProblem, stepCount } from '../../lib/params';
  import Modal from '../Modal.svelte';
  import Icon from '../Icon.svelte';
  import FormatEditor from '../FormatEditor.svelte';

  let {
    editing,
    onclose,
  }: {
    /** The id of the parameter to change. Not set: a new parameter. */
    editing?: string;
    onclose: () => void;
  } = $props();

  const all = $derived(app.snap!.workspace.params ?? []);
  // The sheet starts from the parameter and is then the user's.
  // svelte-ignore state_referenced_locally
  const old = all.find((p) => p.id === editing);
  // svelte-ignore state_referenced_locally
  let p = $state<Param>(structuredClone($state.snapshot(old) as Param | undefined) ?? newParam(all.length + 1));
  // The percent fields are typed as people say them: 5 for 5 %.
  const pct = $derived(!!p.format?.percent && p.kind !== 'choice');
  const show = (v: number | undefined) => (v === undefined ? '' : String(Math.round((pct ? v * 100 : v) * 1e9) / 1e9));
  const read = (raw: string) => {
    const n = Number(raw.replace(',', '.'));
    return raw.trim() === '' || !Number.isFinite(n) ? undefined : pct ? n / 100 : n;
  };
  // svelte-ignore state_referenced_locally
  let choicesText = $state((old?.choices ?? []).map((c) => String(c)).join('\n'));
  let saving = $state(false);
  let refused = $state<string | null>(null);


  $effect(() => {
    if (p.kind === 'choice') p.choices = choicesFrom(choicesText);
  });
  const problem = $derived(paramProblem($state.snapshot(p) as Param, all));
  const steps = $derived(p.kind === 'choice' ? (p.choices ?? []).length : stepCount(p));
  const fmt = $derived(numberFormatter(p.format ?? null));
  const words = (v: Cell) => (typeof v === 'number' ? fmt.long(v) : cell(v, 'text', null));
  /** Measures that read it: they follow a rename. */
  const readers = $derived(
    old ? (app.snap!.workspace.measures ?? []).filter((m) => m.formula.toLowerCase().includes(`"${old.name.toLowerCase()}"`)).map((m) => m.name) : [],
  );

  function setFmt(patch: Partial<NumFormat>) {
    const next = { ...(p.format ?? {}), ...patch };
    for (const k of Object.keys(next) as (keyof NumFormat)[]) if (next[k] === undefined) delete next[k];
    p.format = Object.keys(next).length ? next : undefined;
  }
  function setKind(kind: 'number' | 'choice') {
    p.kind = kind;
    if (kind === 'choice') {
      p.choices = choicesFrom(choicesText);
      if (!p.choices.some((c) => String(c) === String(p.value))) p.value = p.choices[0] ?? null;
    } else if (typeof p.value !== 'number') p.value = p.min ?? 0;
  }

  // A list from the values of a field: the most common first, at most 50.
  const SEP = '\u0000';
  const fields = $derived(
    app.tables.flatMap((tb) =>
      tb.columns.filter((c) => !c.hidden).map((c) => ({ value: `${tb.id}${SEP}${c.name}`, label: `${tb.label} · ${c.label ?? c.name}` })),
    ),
  );
  let taking = $state(false);
  async function takeFrom(value: string) {
    if (!value) return;
    const [table, column] = value.split(SEP);
    taking = true;
    try {
      const r = await api.query({
        measures: [{ table, column: null, agg: 'count' }],
        dimensions: [{ table, column, grain: null }],
        filters: [],
        sort: [{ by: 1, desc: true }],
        limit: MAX_CHOICES,
      } as never);
      choicesText = r.rows.map((row) => row[0]).filter((v) => v !== null && v !== '').map(String).join('\n');
    } catch (e) {
      app.error(e);
    } finally {
      taking = false;
    }
  }

  const dirty = $derived(JSON.stringify($state.snapshot(p)) !== JSON.stringify(old ?? newParam(all.length + 1)));
  function close() {
    if (dirty && !confirm(t('Close without saving the parameter?'))) return;
    onclose();
  }
  async function save() {
    if (problem) return;
    saving = true;
    refused = null;
    const mine = $state.snapshot(p) as Param;
    const list = old ? all.map((o) => (o.id === old.id ? mine : o)) : [...all, mine];
    try {
      app.snap = await api.setParams(list);
      onclose();
    } catch (e) {
      refused = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }
  async function remove() {
    if (!old || !confirm(t('Delete the parameter {name}? A measure that reads it stops working until it is changed.', { name: old.name }))) return;
    saving = true;
    try {
      app.snap = await api.setParams(all.filter((o) => o.id !== old.id));
      onclose();
    } catch (e) {
      refused = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={old ? t('Parameter {name}', { name: old.name }) : t('New parameter')} width={560} onclose={close}>
  <form
    id="param-form"
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}>
    <label class="row">
      <span class="label">{t('Name')}</span>
      <input class="field" bind:value={p.name} maxlength="60" placeholder={t('Price rise')} autocomplete="off" />
    </label>
    <p class="note">
      {#each parts(t('A measure reads it as {code}, for example {example}.')) as part, i (i)}{#if part.slot === 'code'}<code>PARAM("{p.name.trim() || t('name')}")</code>{:else if part.slot === 'example'}<code>SUM([amount]) * (1 + PARAM("{p.name.trim() || t('name')}"))</code>{:else}{part.text}{/if}{/each}
      {#if readers.length}{readers.length === 1 ? t('A new name is written into the measure {name}.', { name: readers[0] }) : t('A new name is written into {n} measures.', { n: readers.length })}{/if}
    </p>

    <div class="row" role="radiogroup" aria-label={t('Kind')}>
      <span class="label">{t('Kind')}</span>
      <div class="seg">
        <button type="button" role="radio" aria-checked={p.kind !== 'choice'} class:on={p.kind !== 'choice'} onclick={() => setKind('number')}>{t('A number')}</button>
        <button type="button" role="radio" aria-checked={p.kind === 'choice'} class:on={p.kind === 'choice'} onclick={() => setKind('choice')}>{t('A choice from a list')}</button>
      </div>
    </div>

    {#if p.kind !== 'choice'}
      <label class="check">
        <input type="checkbox" checked={!!p.format?.percent} onchange={(e) => setFmt({ percent: e.currentTarget.checked || undefined })} />
        {t('A percent (type 5 for 5 %)')}
      </label>
      <div class="three">
        <label class="row"><span class="label">{t('Smallest')}</span><input class="field" inputmode="decimal" value={show(p.min)} onchange={(e) => (p.min = read(e.currentTarget.value))} /></label>
        <label class="row"><span class="label">{t('Largest')}</span><input class="field" inputmode="decimal" value={show(p.max)} onchange={(e) => (p.max = read(e.currentTarget.value))} /></label>
        <label class="row"><span class="label">{t('Step')}</span><input class="field" inputmode="decimal" value={show(p.step)} onchange={(e) => (p.step = read(e.currentTarget.value))} /></label>
      </div>
      <label class="row">
        <span class="label">{t('Value now')}</span>
        <input class="field short" inputmode="decimal" value={show(typeof p.value === 'number' ? p.value : undefined)} onchange={(e) => (p.value = read(e.currentTarget.value) ?? p.min ?? 0)} />
      </label>
      {#if !problem}
        <p class="note">
          {tn('{n} value, from {min} to {max}.', '{n} values, from {min} to {max}.', steps, { n: steps.toLocaleString(), min: words(p.min ?? 0), max: words(p.max ?? 0) })}
          {steps <= MAX_STEPS ? t('An exported file can move it.') : t('An exported file holds only the value it was saved with (it can hold up to {n}).', { n: MAX_STEPS })}
        </p>
      {/if}
      <FormatEditor value={p.format ?? null} onchange={setFmt} percent={false} sample={typeof p.value === 'number' ? p.value : 1234.5} advanced={!!app.settings?.advanced} />
    {:else}
      <label class="row">
        <span class="label">{t('Values to choose from')} <em>{t('(one per line, at most {n})', { n: MAX_CHOICES })}</em></span>
        <textarea class="field" rows="6" bind:value={choicesText} placeholder={'Jawa\nSumatra\nBali'}></textarea>
      </label>
      <label class="row">
        <span class="label">{t('Or take them from a field')} <em>{t('(the {n} most common values)', { n: MAX_CHOICES })}</em></span>
        <select class="field" disabled={taking || !fields.length} value="" onchange={(e) => takeFrom(e.currentTarget.value)}>
          <option value="">{t('Pick a field…')}</option>
          {#each fields as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
        </select>
      </label>
      {#if (p.choices ?? []).length}
        <label class="row">
          <span class="label">{t('Value now')}</span>
          <select class="field short" value={String(p.value)} onchange={(e) => (p.value = (p.choices ?? []).find((c) => String(c) === e.currentTarget.value) ?? p.value)}>
            {#each p.choices ?? [] as c (String(c))}<option value={String(c)}>{words(c)}</option>{/each}
          </select>
        </label>
      {/if}
    {/if}

    <label class="row">
      <span class="label">{t('Description')} <em>{t('(optional)')}</em></span>
      <input class="field" value={p.description ?? ''} oninput={(e) => (p.description = e.currentTarget.value.trim() ? e.currentTarget.value : undefined)} placeholder={t('What does it change?')} autocomplete="off" />
    </label>

    {#if problem && dirty}<p class="bad"><Icon name="warning" size={13} /> {problem}</p>{/if}
    {#if refused}<p class="bad"><Icon name="warning" size={13} /> {refused}</p>{/if}
  </form>
  {#snippet footer()}
    {#if old}<button type="button" class="btn plain danger-text" onclick={remove} disabled={saving}>{t('Delete')}</button>{/if}
    <span class="grow"></span>
    <button type="button" class="btn" onclick={close}>{t('Cancel')}</button>
    <button type="submit" form="param-form" class="btn primary" disabled={!!problem || saving || (!!old && !dirty)}>{old ? t('Save parameter') : t('Add parameter')}</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 12px; }
  .row { display: flex; flex-direction: column; gap: 5px; }
  .label em { font-style: normal; color: var(--text-2); font-weight: 400; }
  .note { margin: 0; color: var(--text-2); font-size: 12px; line-height: 1.45; }
  .note code { font-size: 11.5px; }
  .bad { display: flex; gap: 6px; align-items: flex-start; margin: 0; color: var(--danger); font-size: 12px; }
  .three { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .three .field { min-width: 0; }
  .short { max-width: 200px; }
  .check { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .seg { display: flex; gap: 4px; }
  .seg button { flex: 1; height: 30px; border: 1px solid var(--sep-strong); border-radius: 8px; background: var(--inset); font-size: 13px; color: var(--text); cursor: pointer; }
  .seg button.on { border-color: var(--accent); background: var(--selected); color: var(--accent-text); font-weight: 600; }
  textarea { resize: vertical; font: inherit; }
  .grow { flex: 1; }
  .danger-text { color: var(--danger); }
</style>
