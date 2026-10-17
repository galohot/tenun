<script lang="ts">
  // An alert on a number tile (v5 build 12, V5-D15): below, above or outside a number, told to Slack, Teams, Discord,
  // Telegram or any web address. The app and `tenun agent` check it after each reload and once a minute, with the page
  // as it opens (never a reader's clicks). The address stays in this computer's secrets, not in the .tenun file.
  import { onMount } from 'svelte';
  import { api, type AlertRule, type AlertState, type ChannelKind, type Dashboard, type Tile } from '../../lib/api';
  import { alertProblem, CHANNELS, channelOf, newAlertId, stateText } from '../../lib/alerts';
  import { compact, measureLabel, timeShort } from '../../lib/format';
  import { formatOf } from '../../lib/measures';
  import { app } from '../../lib/state.svelte';
  import Modal from '../Modal.svelte';
  import { parts, t } from '../../lib/i18n';

  let { dash, tile, title, onclose }: { dash: Dashboard; tile: Tile; title: string; onclose: () => void } = $props();

  const all = $derived(app.snap?.workspace.alerts ?? []);
  // svelte-ignore state_referenced_locally
  const saved: AlertRule | undefined = all.find((a) => a.dashboard === dash.id && a.tile === tile.id);

  // svelte-ignore state_referenced_locally
  let measure = $state(saved?.measure ?? 0);
  // svelte-ignore state_referenced_locally
  let op = $state<AlertRule['op']>(saved?.op ?? 'below');
  // svelte-ignore state_referenced_locally
  let value = $state<number | null>(saved?.value ?? null);
  // svelte-ignore state_referenced_locally
  let to = $state<number | null>(saved?.to ?? null);
  // svelte-ignore state_referenced_locally
  let kind = $state<ChannelKind>(saved?.channel.kind ?? 'slack');
  // svelte-ignore state_referenced_locally
  let back = $state(saved?.back ?? true);
  // svelte-ignore state_referenced_locally
  let on = $state(saved?.on ?? true);
  let url = $state('');
  let token = $state('');
  let chat = $state('');
  let st = $state<AlertState | undefined>(undefined);
  let busy = $state<string | null>(null);

  const fmt = $derived((n: number) => compact(n, formatOf(tile, measure)));
  const names = $derived(tile.measures.map((m) => measureLabel(m, app.tables)));
  /** The address this computer keeps is for the kind the rule was saved with. */
  const hasAddress = $derived(!!st?.hasAddress && saved?.channel.kind === kind);
  const problem = $derived(alertProblem({ op, value, to, kind, url, token, chat, hasAddress }));
  const channel = $derived(channelOf(kind));

  onMount(async () => {
    if (!saved) return;
    try {
      st = (await api.alertStates())[saved.id];
    } catch {
      /* the sheet works without the state */
    }
  });

  /** Saves the rule, then its address when one was typed, then checks it once. */
  async function save(): Promise<string | null> {
    if (problem) return null;
    busy = t('Saving…');
    try {
      const id = saved?.id ?? newAlertId();
      const rule: AlertRule = {
        id,
        dashboard: dash.id,
        tile: tile.id,
        ...(measure ? { measure } : {}),
        op,
        value: value!,
        ...(op === 'outside' ? { to: to! } : {}),
        channel: { kind, secret: '' },
        on,
        back,
        // A message names the value with the tile's title, or else with these words.
        name: tile.title && measure === 0 ? '' : names[measure] ?? '',
      };
      app.snap = await api.setAlerts([...all.filter((a) => a.id !== id), rule]);
      const typed = channel.needs === 'url' ? url.trim() : token.trim() || chat.trim();
      if (typed) await api.alertAddress({ id, kind, url, token, chat });
      url = token = chat = '';
      st = (await api.alertCheck())[id];
      return id;
    } catch (e) {
      app.error(e);
      return null;
    } finally {
      busy = null;
    }
  }

  async function done() {
    if (await save()) {
      app.toast(t('Alert saved'));
      onclose();
    }
  }

  async function test() {
    const id = await save();
    if (!id) return;
    busy = t('Sending…');
    try {
      await api.alertTest(id);
      app.toast(t('Test sent'));
    } catch (e) {
      app.error(e);
    } finally {
      busy = null;
    }
  }

  async function remove() {
    if (!saved || !confirm(t('Remove this alert?'))) return;
    try {
      app.snap = await api.setAlerts(all.filter((a) => a.id !== saved.id));
      app.toast(t('Alert removed'));
      onclose();
    } catch (e) {
      app.error(e);
    }
  }

  const shownName = $derived(measure === 0 && tile.title ? title : names[measure]);
  const line = $derived(saved ? stateText({ on }, st, fmt, (iso) => timeShort(iso)) : null);
</script>

<Modal title={t('Alert me')} width={500} {onclose}>
  <p class="intro">
    {#each parts(t("Tenun sends a message when {value} on {page} goes below, above or outside the numbers you set. It reads the page as it opens: its saved filters and its default view, never someone's clicks.")) as p}{#if p.slot === 'value'}<b>{shownName}</b>{:else if p.slot === 'page'}<b>{dash.title || t('Untitled')}</b>{:else}{p.text}{/if}{/each}
  </p>

  {#if tile.measures.length > 1}
    <label class="row">
      <span class="label">{t('Value')}</span>
      <select class="field" bind:value={measure}>
        {#each names as n, i (i)}<option value={i}>{n}</option>{/each}
      </select>
    </label>
  {/if}

  <div class="row">
    <span class="label">{t('When it is')}</span>
    <div class="cond">
      <select class="field op" bind:value={op} aria-label={t('Condition')}>
        <option value="below">{t('below')}</option>
        <option value="above">{t('above')}</option>
        <option value="outside">{t('outside')}</option>
      </select>
      <input class="field num" type="number" step="any" bind:value aria-label={op === 'outside' ? t('Bottom') : t('Number')} placeholder={t('Number')} />
      {#if op === 'outside'}
        <span class="muted">{t('to')}</span>
        <input class="field num" type="number" step="any" bind:value={to} aria-label={t('Top@@edge')} placeholder={t('Number')} />
      {/if}
    </div>
  </div>

  <div class="row">
    <span class="label">{t('Send to')}</span>
    <select class="field" bind:value={kind}>
      {#each CHANNELS as c (c.kind)}<option value={c.kind}>{c.name}</option>{/each}
    </select>
  </div>
  <div class="row sub">
    {#if channel.needs === 'url'}
      <input
        class="field"
        type="password"
        autocomplete="off"
        aria-label={t('Web address')}
        placeholder={hasAddress ? t('An address is set on this computer. Paste a new one to change it.') : 'https://…'}
        bind:value={url} />
    {:else}
      <input class="field" type="password" autocomplete="off" aria-label={t('Bot token')} placeholder={hasAddress ? t('A token is set on this computer. Type a new one to change it.') : t('Bot token, 123456789:AA…')} bind:value={token} />
      <input class="field" autocomplete="off" aria-label={t('Chat id')} placeholder={t('Chat id, -1001234567890 or @channel')} bind:value={chat} />
    {/if}
    <p class="hint">{t('{hint} It is kept on this computer only, not in the .tenun file.', { hint: channel.hint })}</p>
  </div>

  <label class="check"><input type="checkbox" bind:checked={back} /> {t('Also tell me when it is back to normal')}</label>
  <label class="check"><input type="checkbox" bind:checked={on} /> {t('On')}</label>

  {#if line}
    <p class="state {line.tone}">{line.text}</p>
  {/if}
  <p class="hint">
    {#each parts(t("The message holds the value, its period and the tile's name, nothing else. If the alert holds when you save it, the first message goes at once. Tenun checks while it is open; {agent} checks on a computer that stays on.")) as p}{#if p.slot === 'agent'}<code>tenun agent</code>{:else}{p.text}{/if}{/each}
  </p>
  {#if problem && (value !== null || url || token)}
    <p class="err">{problem}</p>
  {/if}

  {#snippet footer()}
    {#if saved}<button class="btn danger" onclick={remove} disabled={!!busy}>{t('Remove')}</button>{/if}
    <span class="spacer"></span>
    <button class="btn" onclick={test} disabled={!!busy || !!problem}>{t('Send a test')}</button>
    <button class="btn plain" onclick={onclose}>{t('Cancel')}</button>
    <button class="btn primary" onclick={done} disabled={!!busy || !!problem}>{busy ?? t('Save')}</button>
  {/snippet}
</Modal>

<style>
  .intro { margin: 0 0 14px; font-size: 13px; color: var(--text-2); }
  .row { display: flex; flex-direction: column; gap: 6px; margin-bottom: 12px; }
  .row.sub { margin-top: -4px; }
  .label { font-size: 12px; color: var(--text-2); }
  .cond { display: flex; gap: 6px; align-items: center; }
  .cond .op { width: 110px; flex: none; }
  .cond .num { flex: 1; min-width: 0; }
  select.field { height: 30px; }
  .check { display: flex; gap: 8px; align-items: center; font-size: 13px; margin-bottom: 6px; }
  .hint { margin: 0; font-size: 11px; color: var(--text-2); }
  .state { margin: 10px 0 6px; font-size: 12px; }
  .state.ok { color: var(--good, var(--text-2)); }
  .state.warn { color: var(--warn, var(--text)); }
  .state.err, .err { color: var(--danger); font-size: 12px; }
  .err { margin: 8px 0 0; }
  .muted { color: var(--text-2); font-size: 12px; }
  code { font-family: var(--mono); font-size: 11px; background: var(--inset); padding: 1px 4px; border-radius: 4px; }
  .spacer { flex: 1; }
</style>
