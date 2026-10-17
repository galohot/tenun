<script lang="ts">
  // Publish a dashboard to tenun.cloud: connect once, choose who can see it and
  // how often it updates, publish. The data is built on this computer; only the
  // results travel (V2-D1). When the workspace is a site, the whole site can be
  // published instead, with the same settings, at its own link (build 14).
  import { untrack } from 'svelte';
  import { api, type Dashboard, type PublishSettings } from '../../lib/api';
  import { dateTimeMedium } from '../../lib/format';
  import { CLOUD_SITE_PAGES, normaliseSite } from '../../lib/site';
  import { app } from '../../lib/state.svelte';
  import { advancedOn } from '../../lib/tiers';
  import Icon from '../Icon.svelte';
  import Modal from '../Modal.svelte';
  import { parts, t, tn } from '../../lib/i18n';

  let { dash, onclose }: { dash: Dashboard; onclose: () => void } = $props();

  const site = $derived(normaliseSite(app.snap?.workspace.site, app.dashboards, app.snap?.workspace.name));
  /** What Publish sends: the whole site (the first choice when the workspace is a site) or this page. */
  let target = $state<'site' | 'page'>(untrack(() => (site.on ? 'site' : 'page')));
  const forSite = $derived(target === 'site' && site.on);
  const current = $derived(forSite ? site.publish : dash.publish);
  /** Every page goes with the site, hidden ones too. */
  const pageCount = $derived(app.dashboards.length);
  const tooBig = $derived(forSite && pageCount > CLOUD_SITE_PAGES);

  const defaults: PublishSettings = { access: 'link', invites: [], every: 'manual', source: '' };
  // The form starts from the saved settings; edits are written back by save().
  let ps = $state<PublishSettings>(untrack(() => ({ ...defaults, ...(current ?? {}) })));
  let invitesText = $state(untrack(() => (current?.invites ?? []).join('\n')));
  let busy = $state<string | null>(null);
  /** A new password, typed here; it is never kept in the form or the file. */
  let password = $state('');
  /** A password counts characters, as tenun.cloud does (an emoji is one). */
  const passwordOk = $derived([...password].length >= 4 && [...password].length <= 200);
  const hasPassword = $derived(!!current?.pw_key);
  /** "Anyone with the password" with no password set and none typed: Publish waits. */
  const needsPassword = $derived(ps.access === 'password' && !hasPassword && !passwordOk);

  /** Shows the settings of the other choice. */
  function choose(t: 'site' | 'page') {
    target = t;
    const from = t === 'site' ? site.publish : dash.publish;
    ps = { ...defaults, ...(from ?? {}) };
    invitesText = (from?.invites ?? []).join('\n');
    password = '';
  }

  const cloud = $derived(app.cloud);
  // Fresh usage numbers each time the sheet opens.
  $effect(() => {
    api.cloud().then((c) => (app.cloud = c)).catch(() => {});
  });
  const published = $derived(!!current?.url);

  /** Keeps the settings on the dashboard, or in the site settings (saved in the .tenun file). */
  function save() {
    const invites = invitesText
      .split(/[\s,;]+/)
      .map((e) => e.trim().toLowerCase())
      .filter((e) => e.includes('@'));
    const next: PublishSettings = {
      ...(current ?? {}),
      access: ps.access,
      invites,
      embed: !!ps.embed,
      updates: ps.updates ?? 'off',
      every: ps.every,
      source: ps.source.trim(),
    };
    if (forSite) {
      app.saveSite({ ...site, publish: next });
    } else {
      dash.publish = next;
      app.saveDashboards();
    }
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

  /** Keeps the typed password on this computer; it goes to tenun.cloud with the next publish. */
  async function setPassword(quiet = false) {
    if (!passwordOk) return;
    save();
    await new Promise((r) => setTimeout(r, 400)); // let the settings save first
    app.snap = await api.cloudPassword(forSite ? { site: true } : { dashboard: dash.id }, password);
    password = '';
    if (!quiet) app.toast(published ? t('Password set. Publish to use it.') : t('Password set'));
  }

  async function publish() {
    busy = t('Publishing…');
    try {
      if (ps.access === 'password' && password) await setPassword(true);
      save();
      await new Promise((r) => setTimeout(r, 400)); // let the settings save first
      const r = forSite ? await api.publishSite() : await api.publish(dash.id);
      app.snap = r.snapshot;
      app.toast(forSite ? t('Site published') : t('Published'));
    } catch (e) {
      app.error(e);
      app.refresh();
    } finally {
      busy = null;
    }
  }

  async function unpublish() {
    if (!confirm(forSite ? t('Remove this site from tenun.cloud? Its link stops working.') : t('Remove this dashboard from tenun.cloud? Its link stops working.'))) return;
    busy = t('Removing…');
    try {
      app.snap = forSite ? await api.unpublishSite() : await api.unpublish(dash.id);
      ps = { ...defaults };
      invitesText = '';
      app.toast(t('Removed from tenun.cloud'));
    } catch (e) {
      app.error(e);
    } finally {
      busy = null;
    }
  }

  async function copy() {
    if (!current?.url) return;
    await navigator.clipboard.writeText(current.url);
    app.toast(t('Link copied'));
  }

  /** The code another site puts in its page to show this one. */
  const iframe = $derived(current?.url ? `<iframe src="${current.url}" width="100%" height="720" style="border:0"></iframe>` : '');
  async function copyEmbed() {
    await navigator.clipboard.writeText(iframe);
    app.toast(t('Code copied'));
  }

  const when = (iso?: string) => {
    if (!iso) return '';
    const s = (Date.now() - Date.parse(iso)) / 1000;
    if (s < 90) return t('just now');
    if (s < 5400) return t('{n} minutes ago', { n: Math.round(s / 60) });
    return dateTimeMedium(iso);
  };

  const everyText: Record<PublishSettings['every'], string> = {
    manual: t('Only when you click Publish'),
    change: t('When the data changes (at most hourly)'),
    hour: t('Every hour'),
    day: t('Every day'),
  };
</script>

<Modal title={t('Publish to tenun.cloud')} {onclose} width={560}>
  {#if !cloud?.connected}
    <div class="connect">
      <Icon name="globe" size={28} />
      {#if app.connecting}
        <h3>{t('Approve in your browser')}</h3>
        <p class="muted">{t('A page opened at tenun.cloud. Sign in and approve this code:')}</p>
        <p class="code">{app.connecting.code}</p>
        <p class="muted small">{#each parts(t('Not open? {link}. This window updates by itself.')) as p}{#if p.slot === 'link'}<a href={app.connecting.url} target="_blank" rel="noopener">{t('Open it here')}</a>{:else}{p.text}{/if}{/each}</p>
      {:else}
        <h3>{t('Share dashboards online')}</h3>
        <p class="muted">
          {t('tenun.cloud gives this dashboard a private link. Tenun builds it on this computer and sends only the results — not your files, and never your database passwords.')}
        </p>
        <button class="btn primary" onclick={connect}>{t('Connect to tenun.cloud')}</button>
        <p class="muted small">{t('Free: up to 3 published dashboards and sites.')}</p>
      {/if}
    </div>
  {:else}
    {#if site.on}
      <div class="targets" role="radiogroup" aria-label={t('What to publish')}>
        <label class="target" class:on={target === 'site'}>
          <input type="radio" name="target" checked={target === 'site'} onchange={() => choose('site')} />
          <span>
            <b>{t('Publish the whole site')}</b>
            <span class="muted small">{site.title} · {tn('{n} page, with its navigation', '{n} pages, with its navigation', pageCount)}{site.publish?.url ? ` · ${t('published')}` : ''}</span>
          </span>
        </label>
        <label class="target" class:on={target === 'page'}>
          <input type="radio" name="target" checked={target === 'page'} onchange={() => choose('page')} />
          <span>
            <b>{t('Publish this page')}</b>
            <span class="muted small">{t('{page} on its own', { page: dash.title || t('Untitled') })}{dash.publish?.url ? ` · ${t('published')}` : ''}</span>
          </span>
        </label>
      </div>
    {/if}
    {#if current?.url}
      <div class="status">
        <Icon name="globe" />
        <div>
          <a href={current.url} target="_blank" rel="noopener">{current.url.replace(/^https?:\/\//, '')}</a>
          <p class="muted small">{t('Published {when}', { when: when(current.last) })}</p>
        </div>
        <button class="btn" onclick={copy}><Icon name="link" size={14} /> {t('Copy link')}</button>
      </div>
    {/if}
    {#if tooBig}
      <p class="err"><Icon name="warning" size={14} /> {t('A site on tenun.cloud holds up to {max} pages; this one has {n}. Leave some pages out of the workspace, or export the site instead.', { max: CLOUD_SITE_PAGES, n: pageCount })}</p>
    {:else if current?.error}
      <p class="err"><Icon name="warning" size={14} /> {current.error}</p>
    {/if}

    <div class="sec">
      <span class="label">{t('Who can see it')}</span>
      <label class="radio"><input type="radio" bind:group={ps.access} value="link" onchange={save} /> {t('Anyone with the link')}</label>
      <label class="radio"><input type="radio" bind:group={ps.access} value="password" onchange={save} /> {t('Anyone with the link and the password')}</label>
      {#if ps.access === 'password'}
        <div class="pw">
          <input
            class="field"
            type="password"
            aria-label={t('Password')}
            placeholder={hasPassword ? t('Type a new password to change it') : t('Password, 4 characters or more')}
            autocomplete="new-password"
            maxlength="200"
            bind:value={password}
            onkeydown={(e) => e.key === 'Enter' && setPassword().catch((err) => app.error(err))}
          />
          <button class="btn" onclick={() => setPassword().catch((err) => app.error(err))} disabled={!passwordOk || !!busy}>{hasPassword ? t('Change') : t('Set password')}</button>
        </div>
        <p class="muted small">
          {hasPassword ? `${t('A password is set.')} ` : ''}{t('Tenun keeps it on this computer, not in the .tenun file. tenun.cloud keeps only a hash of it (a form it cannot turn back into the password). Give the password to the people who may see it.')}
        </p>
      {/if}
      <label class="radio"><input type="radio" bind:group={ps.access} value="invited" onchange={save} /> {t('Only people I invite (they sign in with Google)')}</label>
      {#if ps.access === 'invited'}
        <textarea class="field area" rows="3" placeholder={t('Email addresses, one per line')} bind:value={invitesText} onblur={save}></textarea>
      {/if}
    </div>

    {#if advancedOn() || ps.embed}
      <div class="sec">
        <span class="label">{t('Embed')}</span>
        <label class="radio">
          <input
            type="checkbox"
            checked={!!ps.embed && ps.access !== 'invited'}
            disabled={ps.access === 'invited'}
            onchange={(e) => {
              ps.embed = e.currentTarget.checked;
              save();
            }}
          />
          {t('Allow embedding in other sites')}
        </label>
        {#if ps.access === 'invited'}
          <p class="muted small">{forSite ? t('A site for invited people cannot be embedded: their Google sign-in does not work inside another site.') : t('A dashboard for invited people cannot be embedded: their Google sign-in does not work inside another site.')}</p>
        {:else if ps.embed}
          {#if iframe}
            <div class="embed">
              <code>{iframe}</code>
              <button class="btn" onclick={copyEmbed}>{t('Copy code')}</button>
            </div>
            {#if ps.access === 'password'}<p class="muted small">{t('Inside the other site, people type the password first.')}</p>{/if}
          {:else}
            <p class="muted small">{t("Publish first. The code to put in the other site's page shows here.")}</p>
          {/if}
        {/if}
      </div>
    {/if}

    {#if advancedOn() || (ps.updates ?? 'off') !== 'off'}
      <div class="sec">
        <span class="label">{t('Email updates')}</span>
        <select class="field" value={ps.updates ?? 'off'} onchange={(e) => ((ps.updates = e.currentTarget.value as PublishSettings['updates']), save())}>
          <option value="off">{t('Off')}</option>
          <option value="each">{t('After each update')}</option>
          <option value="daily">{t('Daily')}</option>
          <option value="weekly">{t('Weekly, on Monday')}</option>
        </select>
        {#if (ps.updates ?? 'off') !== 'off'}
          <p class="muted small">
            {ps.access === 'invited'
              ? t('The values of the number tiles and the link go to you, and to invited people who ask for them on the page: at most one email a day for each person, each with a link to stop them.')
              : t('The values of the number tiles and the link go to you: at most one email a day for each person, each with a link to stop them.')}{cloud?.emailUpdates
              ? ''
              : ` ${t('tenun.cloud does not send emails yet. Your choice is kept, and the emails start when it does.')}`}
          </p>
        {/if}
      </div>
    {/if}

    <div class="sec">
      <span class="label">{t('Keep it updated')}</span>
      <select class="field" bind:value={ps.every} onchange={save}>
        {#each Object.entries(everyText) as [k, label]}<option value={k}>{label}</option>{/each}
      </select>
      {#if ps.every !== 'manual'}
        <p class="muted small">
          {#each parts(t('Updates run while Tenun is open on this computer. To keep updating when it is closed, run {command} on a computer that stays on.')) as p}{#if p.slot === 'command'}<code>tenun agent "{app.snap?.path ?? 'your.tenun'}"</code>{:else}{p.text}{/if}{/each}
        </p>
      {/if}
    </div>

    <p class="muted small who">
      {t('Signed in as {email}', { email: cloud.email ?? '' })}{cloud.usage ? ` · ${t('{used} of {limit} free dashboards and sites used', { used: cloud.usage.dashboards, limit: cloud.usage.limit })}` : ''}.
      {t('Updates are labelled “from {source}”.', { source: ps.source || cloud.source })}
      <button class="linkbtn" onclick={async () => { await api.cloudDisconnect(); app.cloud = await api.cloud(); }}>{t('Disconnect')}</button>
    </p>
  {/if}

  {#snippet footer()}
    {#if published}
      <button class="btn danger" onclick={unpublish} disabled={!!busy}>{t('Remove')}</button>
    {/if}
    <span class="spacer"></span>
    <button class="btn" onclick={onclose}>{t('Close')}</button>
    {#if cloud?.connected}
      <button class="btn primary" onclick={publish} disabled={!!busy || tooBig || needsPassword} title={needsPassword ? t('Type a password first') : undefined}>{busy ?? (published ? t('Publish now') : forSite ? t('Publish site') : t('Publish'))}</button>
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
  .targets { display: flex; flex-direction: column; gap: 6px; margin-bottom: 14px; }
  .target { display: flex; gap: 10px; align-items: flex-start; padding: 10px 12px; border: 1px solid var(--sep); border-radius: var(--r-card); cursor: pointer; }
  .target.on { border-color: var(--accent); background: var(--selected); }
  .target input { margin-top: 3px; }
  .target > span { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .target b { font-size: 13px; }
  .radio { display: flex; gap: 8px; align-items: center; }
  .pw { display: flex; gap: 6px; margin-left: 24px; }
  .pw .field { flex: 1; min-width: 0; }
  .sec > .small { margin: 0 0 0 24px; }
  .embed { display: flex; gap: 8px; align-items: center; }
  .embed code { flex: 1; min-width: 0; overflow-wrap: anywhere; padding: 6px 8px; }
  .area { height: auto; padding: 8px 10px; resize: vertical; font-family: inherit; }
  select.field { height: 30px; }
  code { font-family: var(--mono); font-size: 11px; background: var(--inset); padding: 1px 4px; border-radius: 4px; }
  .who { margin: 0; }
  .linkbtn { border: 0; background: none; color: var(--accent-text); padding: 0; font-size: 12px; }
  .spacer { flex: 1; }
</style>
