<script lang="ts">
  // How a number is written (formats §2): one editor for a tile's value, a measure, a parameter and a column.
  // The common choices show at once; the rest sit under "More". A format code is for Advanced.
  import type { NumFormat } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { t, tn } from '../lib/i18n';
  import { getDigits } from '../lib/locale';
  import { codeProblem, COMMON_CURRENCIES, currencyParts, CURRENCY_CODES, writeNumber } from '../lib/numfmt';

  let {
    value,
    onchange,
    sample = 1234567.891,
    shortDefault = true,
    advanced = false,
    percent = true,
    blank = false,
    label = t('Format'),
  }: {
    /** The format as it applies now (a tile's: its measure's with its own over it). */
    value: NumFormat | null;
    /** A change to some fields; `undefined` clears a field. */
    onchange: (patch: Partial<NumFormat>) => void;
    /** The number the preview writes. */
    sample?: number;
    /** "Automatic" size means short (charts, number tiles) or full (tables). */
    shortDefault?: boolean;
    /** Show the format code. */
    advanced?: boolean;
    percent?: boolean;
    /** Offer a text for empty cells (tables, columns). */
    blank?: boolean;
    label?: string;
  } = $props();

  const f = $derived<NumFormat>(value ?? {});
  const loc = $derived(app.locale);
  const write = (n: number, short: boolean, g: NumFormat = f) => writeNumber(n, loc, g, getDigits(), short).text;

  const others = CURRENCY_CODES.filter((c) => !COMMON_CURRENCIES.includes(c)).sort();
  const symbolOf = (c: string) => currencyParts(c, loc).join('').trim();

  const digitsValue = $derived(f.sig ? `s${f.sig}` : f.decimals !== undefined ? `d${f.decimals}` : 'auto');
  function setDigitsChoice(v: string) {
    if (v === 'auto') onchange({ decimals: undefined, sig: undefined });
    else if (v[0] === 'd') onchange({ decimals: Number(v.slice(1)), sig: undefined });
    else onchange({ sig: Number(v.slice(1)), decimals: undefined });
  }

  const sizeValue = $derived(f.scale ? String(f.scale) : f.compact === undefined ? 'auto' : f.compact ? 'short' : 'full');
  function setSize(v: string) {
    if (v === 'auto') onchange({ compact: undefined, scale: undefined });
    else if (v === 'short') onchange({ compact: true, scale: undefined });
    else if (v === 'full') onchange({ compact: false, scale: undefined });
    else onchange({ scale: Number(v) as NumFormat['scale'], compact: undefined });
  }
  const UNIT_NAMES: [number, string][] = [[3, t('Thousands')], [6, t('Millions')], [9, t('Billions')], [12, t('Trillions')]];

  const NEGATIVES: { id: NonNullable<NumFormat['negative']>; red: boolean }[] = [
    { id: 'minus', red: false },
    { id: 'parens', red: false },
    { id: 'red', red: true },
    { id: 'parens_red', red: true },
  ];

  const text = (v: string) => (v === '' ? undefined : v.slice(0, 16));
  let more = $state(false);
  const moreSet = $derived(!!(f.iso || f.duration || f.zero || f.blank || f.code || (f.percent && percent)));
  const problem = $derived(f.code ? codeProblem(f.code) : null);
  const preview = $derived(
    f.code && !problem
      ? [write(sample, false), write(-sample, false), write(0, false)]
      : [write(sample, shortDefault), write(sample, !shortDefault), write(-1234.5, false), write(0, false)],
  );
</script>

<div class="fe" role="group" aria-label={label}>
  <div class="head">
    <span class="label">{label}</span>
    <span class="preview" aria-label={t('Preview')}>{#each [...new Set(preview)] as p, i (i)}{#if i}<i>·</i>{/if}<span class="num">{p}</span>{/each}</span>
  </div>
  <div class="grid">
    <select class="field" aria-label={t('Currency')} value={f.currency ?? ''} onchange={(e) => onchange({ currency: e.currentTarget.value || undefined })}>
      <option value="">{t('No currency')}</option>
      <optgroup label={t('Common')}>
        {#each COMMON_CURRENCIES as c (c)}<option value={c}>{c} · {symbolOf(c)}</option>{/each}
      </optgroup>
      <optgroup label={t('All currencies')}>
        {#each others as c (c)}<option value={c}>{c} · {symbolOf(c)}</option>{/each}
      </optgroup>
    </select>
    <select class="field" aria-label={t('Decimals')} value={digitsValue} onchange={(e) => setDigitsChoice(e.currentTarget.value)}>
      <option value="auto">{t('Auto decimals')}</option>
      {#each [0, 1, 2, 3, 4, 5, 6] as d (d)}<option value={`d${d}`}>{tn('{n} decimal', '{n} decimals', d)}</option>{/each}
      <optgroup label={t('Significant digits')}>
        {#each [1, 2, 3, 4, 5, 6] as s (s)}<option value={`s${s}`}>{t('{n} significant', { n: s })}</option>{/each}
      </optgroup>
    </select>
    <select class="field" aria-label={t('Size')} value={sizeValue} onchange={(e) => setSize(e.currentTarget.value)}>
      <option value="auto">{shortDefault ? t('Automatic (short)') : t('Automatic (full)')}</option>
      <option value="short">{t('Short ({example})', { example: loc === 'id' ? '1,2 jt' : '1.2M' })}</option>
      <option value="full">{t('Every digit')}</option>
      <optgroup label={t('Always in')}>
        {#each UNIT_NAMES as [p, w] (p)}<option value={String(p)}>{w}</option>{/each}
      </optgroup>
    </select>
    <select class="field" aria-label={t('Negative numbers')} value={f.negative ?? 'minus'} onchange={(e) => onchange({ negative: e.currentTarget.value === 'minus' ? undefined : (e.currentTarget.value as NumFormat['negative']) })}>
      {#each NEGATIVES as n (n.id)}{@const v = write(-1234, false, { ...f, code: undefined, negative: n.id, zero: undefined, prefix: undefined, suffix: undefined })}<option value={n.id}>{n.red ? t('{value} in red', { value: v }) : v}</option>{/each}
    </select>
    <input class="field" aria-label={t('Text before')} placeholder={t('Text before')} maxlength="16" value={f.prefix ?? ''} onchange={(e) => onchange({ prefix: text(e.currentTarget.value) })} />
    <input class="field" aria-label={t('Text after')} placeholder={t('Text after: pcs, kg, °C')} maxlength="16" value={f.suffix ?? ''} onchange={(e) => onchange({ suffix: text(e.currentTarget.value) })} />
  </div>

  <button type="button" class="more" aria-expanded={more || moreSet} onclick={() => (more = !(more || moreSet))}>
    <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M4 2.5 7.5 6 4 9.5" /></svg>
    {t('More: {list}', { list: [t('percent'), t('duration'), t('zero'), ...(blank ? [t('empty')] : []), ...(advanced ? [t('format code')] : [])].join(', ') })}
  </button>
  {#if more || moreSet}
    <div class="rest">
      {#if f.currency}
        <label class="check"><input type="checkbox" checked={!!f.iso} onchange={(e) => onchange({ iso: e.currentTarget.checked || undefined })} /> {t('Show the code: {code} 1,234', { code: f.currency })}</label>
      {/if}
      {#if percent}
        <label class="check"><input type="checkbox" checked={!!f.percent} onchange={(e) => onchange({ percent: e.currentTarget.checked || undefined })} /> {t('A percent (0.58 reads 58%)')}</label>
      {/if}
      <div class="pair">
        <label class="sub"><span>{t('A duration in')}</span>
          <select class="field" value={f.duration ?? ''} onchange={(e) => onchange({ duration: (e.currentTarget.value || undefined) as NumFormat['duration'] })}>
            <option value="">{t('Not a duration')}</option>
            <option value="s">{t('Seconds → 1:02:05')}</option>
            <option value="m">{t('Minutes → 1:02')}</option>
            <option value="h">{t('Hours → 1:02')}</option>
            <option value="d">{t('Days → 26:00')}</option>
          </select>
        </label>
        <label class="sub"><span>{t('Zero reads')}</span>
          <input class="field" placeholder="0" maxlength="16" value={f.zero ?? ''} onchange={(e) => onchange({ zero: text(e.currentTarget.value) })} />
        </label>
        {#if blank}
          <label class="sub"><span>{t('An empty cell reads')}</span>
            <input class="field" placeholder="–" maxlength="16" value={f.blank ?? ''} onchange={(e) => onchange({ blank: text(e.currentTarget.value) })} />
          </label>
        {/if}
      </div>
      {#if advanced}
        <label class="sub code"><span>{t('Format code')} <em>{t("(Excel's codes; it decides the whole text)")}</em></span>
          <input class="field mono" placeholder={'#,##0.00;(#,##0.00);"–"'} value={f.code ?? ''} spellcheck="false" autocomplete="off"
            onchange={(e) => onchange({ code: e.currentTarget.value.trim() || undefined })} />
        </label>
        {#if problem}<p class="problem" role="alert">{t('{problem} The other settings are used until it is fixed.', { problem })}</p>{/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .fe { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .head { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .preview { display: flex; gap: 6px; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 12px; color: var(--text-2); }
  .preview .num { color: var(--text); font-variant-numeric: tabular-nums; }
  .preview i { font-style: normal; color: var(--text-3); }
  select.field {
    height: 30px;
    appearance: none;
    padding: 0 26px 0 9px;
    border: 1px solid var(--sep-strong);
    background: var(--surface) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 20 20' fill='none' stroke='%238a8a8f' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M5 8l5 5 5-5'/%3E%3C/svg%3E") no-repeat right 8px center / 12px 12px;
    font-size: 12px;
    text-overflow: ellipsis;
  }
  select.field:hover { border-color: var(--text-3); }
  input.field { height: 30px; font-size: 12px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .grid > * { min-width: 0; }
  .more { display: inline-flex; align-items: center; gap: 4px; align-self: flex-start; padding: 0; border: 0; background: none; font-size: 12px; color: var(--text-2); }
  .more:hover { color: var(--text); }
  .more svg { width: 10px; height: 10px; fill: none; stroke: currentColor; stroke-width: 1.6; transition: transform var(--dur-fast) var(--ease); }
  .more[aria-expanded='true'] svg { transform: rotate(90deg); }
  .rest { display: flex; flex-direction: column; gap: 8px; padding-left: 14px; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .sub { display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-2); min-width: 0; }
  .sub em { font-style: normal; }
  .code { grid-column: 1 / -1; }
  .mono { font-family: var(--mono); font-size: 12px; }
  .check { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .problem { margin: 0; font-size: 12px; color: var(--danger); }
</style>
