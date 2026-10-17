<script lang="ts">
  // Link settings: where the records sit, how the pages follow each other, and the key of a web
  // link. The address is fixed. Saving fetches the link again.
  import { api, type Source } from '../../lib/api';
  import { LinkForm } from '../../lib/linkform.svelte';
  import { app } from '../../lib/state.svelte';
  import { t } from '../../lib/i18n';
  import LinkFields from '../LinkFields.svelte';
  import Modal from '../Modal.svelte';

  let { source, onclose }: { source: Source; onclose: () => void } = $props();

  // The sheet opens for one link and is thrown away on close, so the first value is the one it needs.
  // svelte-ignore state_referenced_locally
  const f = new LinkForm(source, !!app.snap?.link_keys?.includes(source.id));
  let problem = $state('');

  // The lists in the answer are shown at once, so the Records choice is there to change.
  void f.preview();

  async function save(e: SubmitEvent) {
    e.preventDefault();
    problem = '';
    const ok = await app.run(t('Fetching the link…'), () => api.setLink(source.id, f.change()));
    if (!ok) return;
    // The settings are saved even when the fetch fails. The error shows here and beside the source.
    problem = app.snap?.errors[source.id] ?? '';
    if (problem) return;
    onclose();
    app.toast(t('Saved the link settings'));
  }
</script>

<Modal title={t('Link settings')} {onclose} width={560}>
  <form id="linkset" onsubmit={save}>
    <LinkFields {f} />
  </form>
  {#if problem}
    <p class="bad">{t('Saved, but the link did not load. {problem}', { problem })}</p>
  {/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{problem ? t('Close') : t('Cancel')}</button>
    <button class="btn primary" type="submit" form="linkset" disabled={!!app.busy}>{app.busy ?? t('Save')}</button>
  {/snippet}
</Modal>

<style>
  .bad { margin: 12px 0 0; color: var(--danger); font-size: 12px; }
</style>
