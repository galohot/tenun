<script lang="ts">
  // Settings of this computer. They are not saved in the workspace file (V3-D1).
  import { api, type AlertRule, type AlertState } from '../lib/api';
  import { channelOf, conditionText, stateText } from '../lib/alerts';
  import { compact, timeShort, tileTitle } from '../lib/format';
  import { formatOf } from '../lib/measures';
  import { app } from '../lib/state.svelte';
  import { SHOW_ADVANCED_SWITCH } from '../lib/tiers';
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
  const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
</script>

{#if places}
  <PlaceNames onclose={() => (places = false)} />
{:else}
<Modal title="Settings" width={440} {onclose}>
  <div class="rows">
    {#if SHOW_ADVANCED_SWITCH}
      <label class="row">
        <span class="text">
          <span class="name">Advanced features</span>
          <span class="muted hint">More chart types and options, like Power BI and Tableau. Free. Your dashboards look the same either way.</span>
        </span>
        <input
          type="checkbox"
          role="switch"
          class="switch"
          checked={app.settings?.advanced === true}
          onchange={(e) => app.saveSettings({ advanced: e.currentTarget.checked })} />
      </label>
    {/if}
    <label class="row stack">
      <span class="text">
        <span class="name">Number and date format</span>
        <span class="muted hint">How numbers and dates read in this workspace. Exported and published dashboards use it too. The app stays in English.</span>
      </span>
      <select class="field format" value={app.locale} onchange={(e) => app.setLocale(e.currentTarget.value as 'id' | 'en')}>
        <option value="id">Indonesia (1.234,5 · Rp 1,2 jt)</option>
        <option value="en">English (1,234.5 · $1.2M)</option>
      </select>
    </label>
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
    <div class="row">
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
  .rows { display: flex; flex-direction: column; gap: 14px; }
  .row { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .name { font-weight: 600; }
  .hint { font-size: 12px; line-height: 1.4; }
  .row.stack { flex-direction: column; gap: 8px; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; width: 100%; }
  .sub { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
  .format { width: 100%; height: 30px; }
  .note { margin: 14px 0 0; font-size: 12px; }
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
