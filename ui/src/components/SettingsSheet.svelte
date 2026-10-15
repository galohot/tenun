<script lang="ts">
  // Settings of this computer. They are not saved in the workspace file (V3-D1).
  import { api, type AlertRule, type AlertState } from '../lib/api';
  import { channelOf, conditionText, stateText } from '../lib/alerts';
  import { compact, timeShort, tileTitle } from '../lib/format';
  import { formatOf } from '../lib/measures';
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
    const t = d?.tiles.find((x) => x.id === r.tile);
    const fmt = (n: number) => compact(n, t ? formatOf(t, r.measure ?? 0) : null);
    const name = t && (r.measure ?? 0) === 0 && t.title ? t.title : r.name || (t ? tileTitle(t, app.tables) : 'A removed tile');
    return { page: d?.title || 'Untitled', name, fmt };
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
    { id: 'system', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];
  const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
</script>

{#if places}
  <PlaceNames onclose={() => (places = false)} />
{:else}
<Modal title="Settings" width={520} flush {onclose}>
  <div class="rows">
    {#if SHOW_ADVANCED_SWITCH}
      <label class="row">
        <span class="text">
          <span class="name">Advanced features <span class="badge">Free</span></span>
          <span class="muted hint">Measures, calculated columns, drill-down, saved views, forecasts, alerts and {MORE_CHARTS.length} more chart types. Your dashboards look the same either way.</span>
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
        <span class="name">Number and date format <span class="where">· saved in this workspace</span></span>
        <span class="muted hint">How numbers and dates read in this workspace. Exported and published dashboards use it too. The app stays in English.</span>
      </span>
      <div class="choices" role="radiogroup" aria-label="Number and date format">
        {#each LOCALES as l (l.id)}
          <button type="button" role="radio" aria-checked={app.locale === l.id} class:on={app.locale === l.id} onclick={() => app.setLocale(l.id)}>
            <b>{l.label}</b>
            <span class="num">{l.sample}</span>
          </button>
        {/each}
      </div>
    </div>
    {#if app.settings?.advanced || (cal.yearStart ?? 1) !== 1 || (cal.weekStart ?? 1) !== 1}
      <div class="row stack">
        <span class="text">
          <span class="name">Year and week</span>
          <span class="muted hint">The month this workspace's year starts in, and the day its week starts on. Years, quarters, weeks, year to date and "this year" follow them. A year that starts in April 2026 is FY 2027.</span>
        </span>
        <div class="pair">
          <label class="sub">
            <span class="muted">Year starts in</span>
            <select class="field" value={String(cal.yearStart ?? 1)} disabled={!app.settings?.advanced} onchange={(e) => app.setCalendar({ ...cal, yearStart: Number(e.currentTarget.value) })}>
              {#each MONTHS as m, i (m)}<option value={String(i + 1)}>{m}</option>{/each}
            </select>
          </label>
          <label class="sub">
            <span class="muted">Week starts on</span>
            <select class="field" value={String(cal.weekStart ?? 1)} disabled={!app.settings?.advanced} onchange={(e) => app.setCalendar({ ...cal, weekStart: Number(e.currentTarget.value) })}>
              <option value="1">Monday</option>
              <option value="6">Saturday</option>
              <option value="7">Sunday</option>
            </select>
          </label>
        </div>
      </div>
    {/if}
    {#if rules.length}
      <div class="row stack">
        <span class="text">
          <span class="name">Alerts</span>
          <span class="muted hint">Checked while Tenun is open on this computer, after each reload and once a minute. Edit one from its tile's menu.</span>
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
              <input type="checkbox" role="switch" class="switch" aria-label="On" checked={r.on} onchange={(e) => turn(r, e.currentTarget.checked)} />
            </li>
          {/each}
        </ul>
      </div>
    {/if}
    <div class="row center">
      <span class="text">
        <span class="name">Appearance</span>
        <span class="muted hint">Belongs to this computer.</span>
      </span>
      <div class="seg" role="radiogroup" aria-label="Appearance">
        {#each THEMES as t (t.id)}
          <button type="button" role="radio" aria-checked={app.themePref === t.id} class:on={app.themePref === t.id} onclick={() => app.setTheme(t.id)}>{t.label}</button>
        {/each}
      </div>
    </div>
    <div class="row center">
      <span class="text">
        <span class="name">Place names for maps</span>
        <span class="muted hint">The standard names and codes of countries, provinces and kabupaten/kota that a map reads, with a CSV to start from.</span>
      </span>
      <button type="button" class="btn" onclick={() => (places = true)}>Show</button>
    </div>
  </div>
  <p class="muted note">The number and date format, the year and week, and the alerts are saved in your workspace file; an alert's address is not. The other settings belong to this computer.</p>
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
