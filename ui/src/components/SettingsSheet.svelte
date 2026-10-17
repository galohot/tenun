<script lang="ts">
  import { DIGITS, type Digits } from '../lib/numfmt';
  // Settings of this computer. They are not saved in the workspace file (V3-D1).
  import { api, type AlertRule, type AlertState } from '../lib/api';
  import { channelOf, conditionText, stateText } from '../lib/alerts';
  import { compact, dateTimeMedium, timeShort, tileTitle } from '../lib/format';
  import { formatOf } from '../lib/measures';
  import { t } from '../lib/i18n';
  import { app } from '../lib/state.svelte';
  import { MORE_CHARTS, SHOW_ADVANCED_SWITCH } from '../lib/tiers';
  import Modal from './Modal.svelte';
  import PlaceNames from './PlaceNames.svelte';

  let { onclose }: { onclose: () => void } = $props();
  /** The list of place names a map reads (build 05). */
  let places = $state(false);
  /** The workspace's calendar (v5 build 04). */
  const cal = $derived(app.snap?.workspace.calendar ?? {});
  /** Alerts (v5 build 12): the rules are in the workspace; their state is this computer's. */
  const rules = $derived(app.snap?.workspace.alerts ?? []);
  let states = $state<Record<string, AlertState>>({});
  $effect(() => {
    if (rules.length) api.alertStates().then((s) => (states = s)).catch(() => {});
  });
  function placeOf(r: AlertRule) {
    const d = app.dashboards.find((x) => x.id === r.dashboard);
    const tl = d?.tiles.find((x) => x.id === r.tile);
    const fmt = (n: number) => compact(n, tl ? formatOf(tl, r.measure ?? 0) : null);
    const name = tl && (r.measure ?? 0) === 0 && tl.title ? tl.title : r.name || (tl ? tileTitle(tl, app.tables) : t('A removed tile'));
    return { page: d?.title || t('Untitled'), name, fmt };
  }
  async function turn(r: AlertRule, on: boolean) {
    try {
      app.snap = await api.setAlerts(rules.map((x) => (x.id === r.id ? { ...x, on } : x)));
      states = await api.alertCheck();
    } catch (e) {
      app.error(e);
    }
  }
  const LOCALES: { id: 'id' | 'en'; label: string; sample: string }[] = [
    { id: 'id', label: 'Indonesia', sample: '1.234,5 · Rp 1,2 jt' },
    { id: 'en', label: 'English', sample: '1,234.5 · $1.2M' },
  ];
  const THEMES: { id: 'system' | 'light' | 'dark'; label: string }[] = [
    { id: 'system', label: t('System') },
    { id: 'light', label: t('Light') },
    { id: 'dark', label: t('Dark') },
  ];
  const MONTHS = [t('January'), t('February'), t('March'), t('April'), t('May'), t('June'), t('July'), t('August'), t('September'), t('October'), t('November'), t('December')];
  /** The language of the screens (0.7 build 02): this computer's, like the appearance. Names read in their own language. */
  const LANGS: { id: 'en' | 'id'; label: string }[] = [
    { id: 'en', label: 'English' },
    { id: 'id', label: 'Bahasa Indonesia' },
  ];
  /** "Check now" (0.7 build 01): the answer comes back on the bar. */
  let checking = $state(false);
  async function checkNow() {
    checking = true;
    try {
      app.update = await api.checkUpdate();
      if (!app.update.available && !app.update.error) app.toast(t('Tenun is up to date.'));
      else if (app.update.error) app.toast(t('Tenun could not check for updates: {why}', { why: app.update.error }), 'error');
    } catch (e) {
      app.error(e);
    } finally {
      checking = false;
    }
  }
</script>

{#if places}
  <PlaceNames onclose={() => (places = false)} />
{:else}
<Modal title={t('Settings')} width={520} flush {onclose}>
  <div class="rows">
    {#if SHOW_ADVANCED_SWITCH}
      <label class="row">
        <span class="text">
          <span class="name">{t('Advanced features')} <span class="badge">{t('Free')}</span></span>
          <span class="muted hint">{t('Measures, calculated columns, drill-down, saved views, forecasts, alerts and {n} more chart types. Your dashboards look the same either way.', { n: MORE_CHARTS.length })}</span>
        </span>
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={app.settings?.advanced === true}
          onchange={(e) => app.saveSettings({ advanced: e.currentTarget.checked })} />
      </label>
    {/if}
    <div class="row stack">
      <span class="text">
        <span class="name">{t('Number and date format')} <span class="where">{t('· saved in this workspace')}</span></span>
        <span class="muted hint">{t('How numbers and dates read in this workspace. Exported and published dashboards use it too, and read in its language. The language of the app is set under Language.')}</span>
      </span>
      <div class="choices" role="radiogroup" aria-label={t('Number and date format')}>
        {#each LOCALES as l (l.id)}
          <button type="button" role="radio" aria-checked={app.locale === l.id} class:on={app.locale === l.id} onclick={() => app.setLocale(l.id)}>
            <b>{l.label}</b>
            <span class="num">{l.sample}</span>
          </button>
        {/each}
      </div>
      <label class="sub digits">
        <span class="muted">{t('Digits')}</span>
        <select class="field" value={app.digits ?? ''} onchange={(e) => app.setDigits((e.currentTarget.value || null) as Digits | null)}>
          <option value="">{t('As the language: {sample}', { sample: app.locale === 'id' ? '1.234.567,5' : '1,234,567.5' })}</option>
          {#each DIGITS as d (d.id)}<option value={d.id}>{d.example}</option>{/each}
        </select>
      </label>
    </div>
    {#if app.settings?.advanced || (cal.yearStart ?? 1) !== 1 || (cal.weekStart ?? 1) !== 1}
      <div class="row stack">
        <span class="text">
          <span class="name">{t('Year and week')}</span>
          <span class="muted hint">{t('The month this workspace\'s year starts in, and the day its week starts on. Years, quarters, weeks, year to date and "this year" follow them. A year that starts in April 2026 is FY 2027.')}</span>
        </span>
        <div class="pair">
          <label class="sub">
            <span class="muted">{t('Year starts in')}</span>
            <select class="field" value={String(cal.yearStart ?? 1)} disabled={!app.settings?.advanced} onchange={(e) => app.setCalendar({ ...cal, yearStart: Number(e.currentTarget.value) })}>
              {#each MONTHS as m, i (m)}<option value={String(i + 1)}>{m}</option>{/each}
            </select>
          </label>
          <label class="sub">
            <span class="muted">{t('Week starts on')}</span>
            <select class="field" value={String(cal.weekStart ?? 1)} disabled={!app.settings?.advanced} onchange={(e) => app.setCalendar({ ...cal, weekStart: Number(e.currentTarget.value) })}>
              <option value="1">{t('Monday')}</option>
              <option value="6">{t('Saturday')}</option>
              <option value="7">{t('Sunday')}</option>
            </select>
          </label>
        </div>
      </div>
    {/if}
    {#if rules.length}
      <div class="row stack">
        <span class="text">
          <span class="name">{t('Alerts')}</span>
          <span class="muted hint">{t('Checked while Tenun is open on this computer, after each reload and once a minute. Edit one from its tile\'s menu.')}</span>
        </span>
        <ul class="alerts">
          {#each rules as r (r.id)}
            {@const p = placeOf(r)}
            {@const line = stateText(r, states[r.id], p.fmt, (iso) => timeShort(iso))}
            <li>
              <span class="text">
                <span class="name">{p.name} {conditionText(r, p.fmt)} → {channelOf(r.channel.kind).name}</span>
                <span class="muted hint">{p.page}</span>
                <span class="hint state {line.tone}">{line.text}</span>
              </span>
              <input type="checkbox" role="switch" class="switch" aria-label={t('On')} checked={r.on} onchange={(e) => turn(r, e.currentTarget.checked)} />
            </li>
          {/each}
        </ul>
      </div>
    {/if}
    {#if app.update?.supported}
      <label class="row">
        <span class="text">
          <span class="name">{t('Check for updates')}</span>
          <span class="muted hint">{t('Looks for a new version once a day. Tenun installs one only when you say so.')}{#if app.update.checked} {t('Last checked {when}.', { when: dateTimeMedium(app.update.checked) })}{/if}</span>
          <span><button type="button" class="btn" disabled={checking} onclick={checkNow}>{checking ? t('Checking…') : t('Check now')}</button></span>
        </span>
        <input type="checkbox" role="switch" class="switch" checked={app.settings?.updates !== false} onchange={(e) => app.saveSettings({ updates: e.currentTarget.checked ? undefined : false })} />
      </label>
    {/if}
    <div class="row center">
      <span class="text">
        <span class="name">{t('Language')}</span>
        <span class="muted hint">{t('The language of the menus and screens. Belongs to this computer.')}</span>
      </span>
      <div class="seg" role="radiogroup" aria-label={t('Language')}>
        {#each LANGS as l (l.id)}
          <button type="button" role="radio" lang={l.id} aria-checked={app.lang === l.id} class:on={app.lang === l.id} onclick={() => app.saveSettings({ language: l.id })}>{l.label}</button>
        {/each}
      </div>
    </div>
    <div class="row center">
      <span class="text">
        <span class="name">{t('Appearance')}</span>
        <span class="muted hint">{t('Belongs to this computer.')}</span>
      </span>
      <div class="seg" role="radiogroup" aria-label={t('Appearance')}>
        {#each THEMES as th (th.id)}
          <button type="button" role="radio" aria-checked={app.themePref === th.id} class:on={app.themePref === th.id} onclick={() => app.setTheme(th.id)}>{th.label}</button>
        {/each}
      </div>
    </div>
    <div class="row center">
      <span class="text">
        <span class="name">{t('Place names for maps')}</span>
        <span class="muted hint">{t('The standard names and codes of countries, provinces and kabupaten/kota that a map reads, with a CSV to start from.')}</span>
      </span>
      <button type="button" class="btn" onclick={() => (places = true)}>{t('Show')}</button>
    </div>
    <div class="row center">
      <span class="text">
        <span class="name">{t('Report a problem')}</span>
        <span class="muted hint">{t('Opens a new issue on GitHub. You see everything before it is sent, and your data never is.')}</span>
      </span>
      <button type="button" class="btn" onclick={() => { onclose(); app.reportOpen = true; }}>{t('Report…')}</button>
    </div>
  </div>
  <p class="muted note">{t('The number and date format, the year and week, and the alerts are saved in your workspace file; an alert\'s address is not. The other settings, the language among them, belong to this computer.')}</p>
</Modal>
{/if}

<style>
  .alerts { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; width: 100%; align-self: stretch; }
  .alerts li { display: flex; gap: 12px; align-items: flex-start; justify-content: space-between; }
  .state.err { color: var(--danger); }
  .state.warn { color: var(--warn, var(--text)); }
  .rows { display: flex; flex-direction: column; }
  .row { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 16px 20px; border-bottom: 1px solid var(--sep); }
  .row.center { align-items: center; }
  .text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .name { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .badge { font-family: var(--mono); font-size: 10px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); border: 1px solid var(--sep-strong); border-radius: 4px; padding: 0 4px; line-height: 15px; background: none; }
  .where { font-size: 11px; font-weight: 400; color: var(--text-2); }
  .hint { font-size: 12px; line-height: 1.45; }
  .row.stack { flex-direction: column; align-items: stretch; gap: 10px; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; width: 100%; }
  .sub { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
  .note { margin: 0; padding: 12px 20px 16px; font-size: 12px; line-height: 1.45; }
  .choices { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .choices button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 10px 12px;
    border: 1px solid var(--sep);
    border-radius: 8px;
    background: var(--surface);
    text-align: left;
    transition: background 120ms var(--spring), border-color 120ms var(--spring);
  }
  .choices button:hover { background: var(--hover); }
  .choices button.on { border-color: var(--accent); background: var(--selected); box-shadow: 0 0 0 1px var(--accent); }
  .choices b { font-size: 12px; font-weight: 600; }
  .choices .num { font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .seg { display: inline-flex; flex: none; padding: 2px; border-radius: 8px; background: var(--inset); }
  .seg button { height: 26px; padding: 0 12px; border: 0; border-radius: 6px; background: none; color: var(--text-2); font-size: 12px; font-weight: 500; transition: background 120ms var(--spring), color 120ms var(--spring); }
  .seg button:hover { color: var(--text); }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .sub select.field {
    height: 32px;
    appearance: none;
    padding: 0 28px 0 10px;
    border: 1px solid var(--sep-strong);
    background: var(--surface) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 20 20' fill='none' stroke='%238a8a8f' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M5 8l5 5 5-5'/%3E%3C/svg%3E") no-repeat right 9px center / 12px 12px;
  }
  .sub select.field:disabled { opacity: 0.55; }
  .switch {
    appearance: none;
    flex: none;
    width: 36px;
    height: 20px;
    margin: 2px 0 0;
    border-radius: 10px;
    background: var(--inset);
    border: 1px solid var(--sep-strong);
    position: relative;
    cursor: pointer;
    transition: background 150ms var(--spring);
  }
  .switch::after {
    content: '';
    position: absolute;
    top: 1px;
    left: 1px;
    width: 16px;
    height: 16px;
    border-radius: 8px;
    background: var(--surface);
    box-shadow: 0 1px 2px rgb(0 0 0 / 25%);
    transition: transform 150ms var(--spring);
  }
  .switch:checked { background: var(--accent); border-color: transparent; }
  .switch:checked::after { transform: translateX(16px); }
</style>
