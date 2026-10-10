<script lang="ts">
  // Publish a dashboard to tenun.cloud: connect once, choose who can see it and
  // how often it updates, publish. The data is built on this computer; only the
  // results travel (V2-D1).
  import { untrack } from 'svelte';
  import { api, type Dashboard, type PublishSettings } from '../../lib/api';
  import { dateTimeMedium } from '../../lib/format';
  import { app } from '../../lib/state.svelte';
  import Icon from '../Icon.svelte';
  import Modal from '../Modal.svelte';

  let { dash, onclose }: { dash: Dashboard; onclose: () => void } = $props();

  const defaults: PublishSettings = { access: 'link', invites: [], every: 'manual', source: '' };
  // The form starts from the saved settings; edits are written back by save().
  let ps = $state<PublishSettings>(untrack(() => ({ ...defaults, ...(dash.publish ?? {}) })));
  let invitesText = $state(untrack(() => (dash.publish?.invites ?? []).join('\n')));
  let busy = $state<string | null>(null);

  const cloud = $derived(app.cloud);
  // Fresh usage numbers each time the sheet opens.
  $effect(() => {
    api.cloud().then((c) => (app.cloud = c)).catch(() => {});
  });
  const published = $derived(!!dash.publish?.url);

  /** Keeps the settings on the dashboard (saved in the .tenun file). */
  function save() {
    const invites = invitesText
      .split(/[\s,;]+/)
      .map((e) => e.trim().toLowerCase())
      .filter((e) => e.includes('@'));
    const next: PublishSettings = { ...(dash.publish ?? {}), access: ps.access, invites, every: ps.every, source: ps.source.trim() };
    dash.publish = next;
    app.saveDashboards();
    return next;
  }

  async function connect() {
    try {
      const r = await api.cloudConnect(!!app.health?.native);
      app.connecting = { code: r.user_code, url: r.url };
      if (!app.health?.native) window.open(r.url, '_blank', 'noopener');
    } catch (e) {
      app.error(e);
    }
  }

  async function publish() {
    save();
    busy = 'Publishing…';
    try {
      await new Promise((r) => setTimeout(r, 400)); // let the settings save first
      const r = await api.publish(dash.id);
      app.snap = r.snapshot;
      app.toast('Published');
    } catch (e) {
      app.error(e);
      app.refresh();
    } finally {
      busy = null;
    }
  }

  async function unpublish() {
    if (!confirm('Remove this dashboard from tenun.cloud? Its link stops working.')) return;
    busy = 'Removing…';
    try {
      app.snap = await api.unpublish(dash.id);
      ps = { ...defaults };
      invitesText = '';
      app.toast('Removed from tenun.cloud');
    } catch (e) {
      app.error(e);
    } finally {
      busy = null;
    }
  }

  async function copy() {
    if (!dash.publish?.url) return;
    await navigator.clipboard.writeText(dash.publish.url);
    app.toast('Link copied');
  }

  const when = (iso?: string) => {
    if (!iso) return '';
    const s = (Date.now() - Date.parse(iso)) / 1000;
    if (s < 90) return 'just now';
    if (s < 5400) return `${Math.round(s / 60)} minutes ago`;
    return dateTimeMedium(iso);
  };

  const everyText: Record<PublishSettings['every'], string> = {
    manual: 'Only when you click Publish',
    change: 'When the data changes (at most hourly)',
    hour: 'Every hour',
    day: 'Every day',
  };
</script>

<Modal title="Publish to tenun.cloud" {onclose} width={560}>
  {#if !cloud?.connected}
    <div class="connect">
      <Icon name="globe" size={28} />
      {#if app.connecting}
        <h3>Approve in your browser</h3>
        <p class="muted">A page opened at tenun.cloud. Sign in and approve this code:</p>
        <p class="code">{app.connecting.code}</p>
        <p class="muted small">Not open? <a href={app.connecting.url} target="_blank" rel="noopener">Open it here</a>. This window updates by itself.</p>
      {:else}
        <h3>Share dashboards online</h3>
        <p class="muted">
          tenun.cloud gives this dashboard a private link. Tenun builds it on this computer and sends only the
          results — not your files, and never your database passwords.
        </p>
        <button class="btn primary" onclick={connect}>Connect to tenun.cloud</button>
        <p class="muted small">Free: up to 3 published dashboards.</p>
      {/if}
    </div>
  {:else}
    {#if dash.publish?.url}
      <div class="status">
        <Icon name="globe" />
        <div>
          <a href={dash.publish.url} target="_blank" rel="noopener">{dash.publish.url.replace(/^https?:\/\//, '')}</a>
          <p class="muted small">Published {when(dash.publish.last)}</p>
        </div>
        <button class="btn" onclick={copy}><Icon name="link" size={14} /> Copy link</button>
      </div>
    {/if}
    {#if dash.publish?.error}
      <p class="err"><Icon name="warning" size={14} /> {dash.publish.error}</p>
    {/if}

    <div class="sec">
      <span class="label">Who can see it</span>
      <label class="radio"><input type="radio" bind:group={ps.access} value="link" onchange={save} /> Anyone with the link</label>
      <label class="radio"><input type="radio" bind:group={ps.access} value="invited" onchange={save} /> Only people I invite (they sign in with Google)</label>
      {#if ps.access === 'invited'}
        <textarea class="field area" rows="3" placeholder="Email addresses, one per line" bind:value={invitesText} onblur={save}></textarea>
      {/if}
    </div>

    <div class="sec">
      <span class="label">Keep it updated</span>
      <select class="field" bind:value={ps.every} onchange={save}>
        {#each Object.entries(everyText) as [k, t]}<option value={k}>{t}</option>{/each}
      </select>
      {#if ps.every !== 'manual'}
        <p class="muted small">
          Updates run while Tenun is open on this computer. To keep updating when it is closed, run
          <code>tenun agent "{app.snap?.path ?? 'your.tenun'}"</code> on a computer that stays on.
        </p>
      {/if}
    </div>

    <p class="muted small who">
      Signed in as {cloud.email}{cloud.usage ? ` · ${cloud.usage.dashboards} of ${cloud.usage.limit} free dashboards used` : ''}.
      Updates are labelled “from {ps.source || cloud.source}”.
      <button class="linkbtn" onclick={async () => { await api.cloudDisconnect(); app.cloud = await api.cloud(); }}>Disconnect</button>
    </p>
  {/if}

  {#snippet footer()}
    {#if published}
      <button class="btn danger" onclick={unpublish} disabled={!!busy}>Remove</button>
    {/if}
    <span class="spacer"></span>
    <button class="btn" onclick={onclose}>Close</button>
    {#if cloud?.connected}
      <button class="btn primary" onclick={publish} disabled={!!busy}>{busy ?? (published ? 'Publish now' : 'Publish')}</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .connect { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 8px; padding: 8px 12px 4px; }
  .connect :global(svg) { color: var(--accent-text); }
  .connect h3 { font-size: 15px; }
  .connect p { margin: 0; max-width: 420px; }
  .code { font-size: 26px; font-weight: 600; letter-spacing: 0.12em; margin: 6px 0 !important; }
  .small { font-size: 12px; }
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--r-card);
    background: var(--window);
    margin-bottom: 14px;
  }
  .status :global(svg) { color: var(--accent-text); }
  .status > div { flex: 1; min-width: 0; }
  .status a { color: var(--accent-text); font-weight: 500; word-break: break-all; }
  .status p { margin: 2px 0 0; }
  .err { display: flex; gap: 6px; color: var(--danger); font-size: 12px; margin: 0 0 12px; }
  .sec { display: flex; flex-direction: column; gap: 6px; margin-bottom: 16px; }
  .radio { display: flex; gap: 8px; align-items: center; }
  .area { height: auto; padding: 8px 10px; resize: vertical; font-family: inherit; }
  select.field { height: 30px; }
  code { font-family: var(--mono); font-size: 11px; background: var(--inset); padding: 1px 4px; border-radius: 4px; }
  .who { margin: 0; }
  .linkbtn { border: 0; background: none; color: var(--accent-text); padding: 0; font-size: 12px; }
  .spacer { flex: 1; }
</style>
