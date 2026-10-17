<script lang="ts">
  // "Report a problem" (0.7 build 05). Everything that would leave this computer is on the sheet; nothing
  // goes until the user presses Submit on GitHub. The error's paths, links and values are taken out first.
  import { api } from '../lib/api';
  import { t } from '../lib/i18n';
  import { reportText, reportUrl, scrub, systemOf } from '../lib/report';
  import { app } from '../lib/state.svelte';
  import Modal from './Modal.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let what = $state('');
  let withError = $state(true);
  const error = scrub(app.lastError);
  const report = $derived({
    what,
    version: app.health?.version ?? '?',
    system: systemOf(navigator.userAgent, !!app.health?.native),
    error: withError ? error : '',
  });

  async function open() {
    const { url, cut } = reportUrl(report);
    try {
      if (app.health?.native) await api.openReport(url);
      else window.open(url, '_blank', 'noopener');
      if (cut) app.toast(t('Your text was too long for one link, so its end was cut. Paste the rest on GitHub.'));
      onclose();
    } catch (e) {
      app.error(e);
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(reportText(report));
      app.toast(t('Copied. Paste it in a new issue at github.com/galohot/tenun, or send it to the person who gave you Tenun.'));
      onclose();
    } catch (e) {
      app.error(e);
    }
  }
</script>

<Modal title={t('Report a problem')} subtitle={t('Opens a new issue on GitHub with this filled in. You see it all before you send it.')} width={520} {onclose}>
  <div class="form">
    <label class="sub">
      <span>{t('What happened')}</span>
      <textarea class="field" rows="5" bind:value={what} placeholder={t('What you did, what you expected, and what Tenun did instead.')}></textarea>
    </label>
    <dl class="facts">
      <dt>{t('Tenun version')}</dt><dd class="num">{report.version}</dd>
      <dt>{t('System')}</dt><dd>{report.system}</dd>
      <dt>{t('Last error')}</dt>
      <dd>
        {#if error}
          <label class="check"><input type="checkbox" bind:checked={withError} /> <span class:off={!withError}>{error}</span></label>
        {:else}
          <span class="muted">{t('None')}</span>
        {/if}
      </dd>
    </dl>
    <p class="muted note">{t('Never sent: your data, file names, paths, links or passwords. Paths and values in the error are taken out.')}</p>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={copy}>{t('Copy instead')}</button>
    <button class="btn primary" onclick={open}>{t('Open on GitHub')}</button>
  {/snippet}
</Modal>

<style>
  .form { display: flex; flex-direction: column; gap: 14px; }
  .sub { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
  textarea.field { resize: vertical; min-height: 96px; padding: 8px 10px; font-size: 13px; line-height: 1.45; }
  .facts { display: grid; grid-template-columns: max-content 1fr; gap: 6px 14px; margin: 0; font-size: 12px; }
  .facts dt { font-family: var(--mono); font-size: 10.5px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); padding-top: 1px; }
  .facts dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
  .check { display: flex; align-items: flex-start; gap: 6px; }
  .check input { margin-top: 2px; }
  .off { color: var(--text-3); text-decoration: line-through; }
  .note { margin: 0; font-size: 12px; line-height: 1.45; }
</style>
