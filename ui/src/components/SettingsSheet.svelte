<script lang="ts">
  // Settings of this computer. They are not saved in the workspace file (V3-D1).
  import { app } from '../lib/state.svelte';
  import { SHOW_ADVANCED_SWITCH } from '../lib/tiers';
  import Modal from './Modal.svelte';
  import PlaceNames from './PlaceNames.svelte';

  let { onclose }: { onclose: () => void } = $props();
  /** The list of place names a map reads (build 05). */
  let places = $state(false);
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
    <div class="row">
      <span class="text">
        <span class="name">Place names for maps</span>
        <span class="muted hint">The standard names and codes of countries, provinces and kabupaten/kota that a map reads, with a CSV to start from.</span>
      </span>
      <button type="button" class="btn" onclick={() => (places = true)}>Show</button>
    </div>
  </div>
  <p class="muted note">The number and date format is saved in your workspace file. The other settings belong to this computer.</p>
</Modal>
{/if}

<style>
  .rows { display: flex; flex-direction: column; gap: 14px; }
  .row { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .name { font-weight: 600; }
  .hint { font-size: 12px; line-height: 1.4; }
  .row.stack { flex-direction: column; gap: 8px; }
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
