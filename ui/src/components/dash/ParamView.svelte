<script lang="ts">
  // A parameter tile (v5 build 05): one control and the parameter's name. A slider or a box for a number, buttons or a
  // dropdown for a choice. In the app a move saves the value, and every tile that uses it asks again. In an exported
  // file it moves through the steps the file holds; past 21 steps it is fixed at the saved value and says so.
  import type { Cell, Param } from '../../lib/api';
  import { cell, numberFormatter } from '../../lib/format';
  import { controlOf, snap } from '../../lib/params';

  let {
    param,
    value,
    style,
    named = true,
    onchange,
    note = '',
  }: {
    /** Not set: the parameter is gone. */
    param: Param | undefined;
    value: Cell;
    style?: 'slider' | 'input' | 'buttons' | 'dropdown';
    /** The control writes the parameter's name. Not when the tile has a title of its own. */
    named?: boolean;
    /** Not set: the control is fixed. */
    onchange?: (v: Cell) => void;
    /** Why the control is fixed, or what moving it does. */
    note?: string;
  } = $props();

  const control = $derived(param ? controlOf(param, style) : 'slider');
  const name = $derived(param?.name ?? '');
  const fmt = $derived(numberFormatter(param?.format ?? null));
  const words = (v: Cell) => (typeof v === 'number' ? fmt.long(v) : cell(v, 'text', null));

  // The slider moves at once; the value is sent when the hand stops (a short pause, or the release).
  // svelte-ignore state_referenced_locally
  let shown = $state<Cell>(value);
  $effect.pre(() => {
    shown = value;
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  function slide(v: number) {
    if (!param) return;
    shown = snap(param, v);
    clearTimeout(timer);
    timer = setTimeout(() => send(shown), 250);
  }
  function send(v: Cell) {
    clearTimeout(timer);
    if (v !== value) onchange?.(v);
  }
  function typed(raw: string) {
    if (!param) return;
    const n = Number(raw.replace(',', '.'));
    if (raw.trim() === '' || !Number.isFinite(n)) return;
    // A percent is typed as people say it: 5 means 5 %.
    const v = snap(param, param.format?.percent ? n / 100 : n);
    shown = v;
    send(v);
  }
  const fixed = $derived(!onchange);
</script>

<div class="param" class:fixed>
  {#if !param}
    <div class="gone">The parameter of this tile was deleted. Pick another in the tile's options.</div>
  {:else}
    <div class="head">
      <span class="name">{named ? name : ''}</span>
      {#if control === 'slider'}<span class="v" aria-live="polite">{words(shown)}</span>{/if}
    </div>
    {#if control === 'slider'}
      <input
        class="slider"
        type="range"
        min={param.min}
        max={param.max}
        step={param.step}
        value={typeof shown === 'number' ? shown : param.min}
        disabled={fixed}
        aria-label={name}
        aria-valuetext={words(shown)}
        oninput={(e) => slide(Number(e.currentTarget.value))}
        onchange={(e) => {
          slide(Number(e.currentTarget.value));
          send(shown);
        }} />
      <div class="ends" aria-hidden="true"><span>{words(param.min ?? 0)}</span><span>{words(param.max ?? 0)}</span></div>
    {:else if control === 'input'}
      <input
        class="field box"
        type="number"
        aria-label={name}
        disabled={fixed}
        value={typeof shown === 'number' ? Math.round((param.format?.percent ? shown * 100 : shown) * 1e9) / 1e9 : ''}
        step={param.format?.percent ? (param.step ?? 0.01) * 100 : param.step}
        onchange={(e) => typed(e.currentTarget.value)} />
    {:else if control === 'buttons'}
      <div class="seg" role="radiogroup" aria-label={name}>
        {#each param.choices ?? [] as c (String(c))}
          <button role="radio" aria-checked={String(c) === String(shown)} class:on={String(c) === String(shown)} disabled={fixed} onclick={() => send((shown = c))}>{words(c)}</button>
        {/each}
      </div>
    {:else}
      <select class="field drop" aria-label={name} disabled={fixed} value={String(shown)} onchange={(e) => send((shown = (param.choices ?? []).find((c) => String(c) === e.currentTarget.value) ?? shown))}>
        {#each param.choices ?? [] as c (String(c))}<option value={String(c)}>{words(c)}</option>{/each}
      </select>
    {/if}
    {#if note}<p class="note">{note}</p>{/if}
  {/if}
</div>

<style>
  .param { height: 100%; display: flex; flex-direction: column; justify-content: center; gap: 6px; min-height: 0; }
  /* The tile's own buttons float over its top right corner. */
  .head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; padding-right: 60px; }
  .name { font-size: 11px; font-weight: 600; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .v { font-size: 18px; font-weight: 650; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .slider { width: 100%; margin: 4px 0 0; appearance: none; background: none; height: 18px; }
  .slider::-webkit-slider-runnable-track { height: 3px; border-radius: 2px; background: var(--sep-strong); }
  .slider::-moz-range-track { height: 3px; border-radius: 2px; background: var(--sep-strong); }
  .slider::-webkit-slider-thumb { appearance: none; width: 16px; height: 16px; margin-top: -7px; border-radius: 8px; border: 2px solid var(--accent); background: var(--surface); cursor: ew-resize; }
  .slider::-moz-range-thumb { width: 12px; height: 12px; border-radius: 8px; border: 2px solid var(--accent); background: var(--surface); cursor: ew-resize; }
  .slider:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 4px; }
  .fixed .slider::-webkit-slider-thumb { border-color: var(--sep-strong); cursor: default; }
  .fixed .slider::-moz-range-thumb { border-color: var(--sep-strong); cursor: default; }
  .ends { display: flex; justify-content: space-between; font-size: 11px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .box { height: 32px; max-width: 160px; }
  .drop { height: 34px; }
  .seg { display: flex; flex-wrap: wrap; gap: 4px; }
  .seg button { flex: 1 1 auto; min-width: 0; height: 30px; padding: 0 10px; border: 1px solid var(--sep-strong); border-radius: 8px; background: var(--inset); font-size: 13px; color: var(--text); cursor: pointer; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .seg button:hover:not(:disabled) { border-color: var(--accent); }
  .seg button.on { border-color: var(--accent); background: var(--selected); color: var(--accent-text); font-weight: 600; }
  .seg button:disabled { cursor: default; }
  .note { margin: 0; font-size: 11px; color: var(--text-2); }
  .gone { height: 100%; display: grid; place-items: center; text-align: center; color: var(--text-2); font-size: 12px; }
</style>
