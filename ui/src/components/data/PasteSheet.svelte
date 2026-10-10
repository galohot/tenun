<script lang="ts">
  // "Paste as a new table": a name, a 5-row preview and a header checkbox. On confirm the engine
  // writes a CSV file in the workspace's data folder and adds it as a normal source.
  import { api, type PastePreview } from '../../lib/api';
  import { app, go } from '../../lib/state.svelte';
  import { pastedName } from '../../lib/sources';
  import Modal from '../Modal.svelte';

  let { text, onclose }: { text: string; onclose: () => void } = $props();

  let name = $state(pastedName(app.snap!.workspace.sources.map((s) => s.name)));
  let header = $state(true);
  let preview = $state<PastePreview | null>(null);
  let problem = $state('');
  const saved = $derived(app.snap!.path !== null);

  $effect(() => {
    const h = header;
    problem = '';
    api
      .pastePreview(text, h)
      .then((p) => (preview = p))
      .catch((e) => {
        preview = null;
        problem = e.message;
      });
  });

  async function create() {
    let added = '';
    const ok = await app.run('Adding the table…', async () => {
      const r = await api.paste(name.trim() || 'Pasted table', text, header);
      added = r.added;
      return r;
    });
    if (!ok) return;
    onclose();
    const t = app.tables.find((x) => x.source_id === added);
    go({ view: 'data', table: t?.id });
    app.toast('Added the pasted table');
  }
</script>

<Modal title="Paste as a new table" {onclose} width={620}>
  {#if !saved}
    <p class="warn">
      Save your workspace first. Tenun keeps a pasted table as a file in a folder next to the workspace file. Use the
      save button in the sidebar, then paste again.
    </p>
  {/if}
  <label class="stack">
    <span class="label">Name</span>
    <input class="field" bind:value={name} onkeydown={(e) => e.key === 'Enter' && saved && preview && create()} />
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={header} />
    <span>First row is the header</span>
  </label>
  {#if problem}
    <p class="warn">{problem}</p>
  {:else if preview}
    <div class="wrap">
      <table>
        <thead>
          <tr>{#each preview.columns as c}<th>{c}</th>{/each}</tr>
        </thead>
        <tbody>
          {#each preview.rows as r}
            <tr>{#each r as v}<td>{v}</td>{/each}</tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="muted small">
      {preview.total.toLocaleString(app.locale)}
      {preview.total === 1 ? 'row' : 'rows'}{preview.total > preview.rows.length ? `. The first ${preview.rows.length} are shown.` : '.'}
      The table is saved as a CSV file, so you can edit it later.
    </p>
  {:else}
    <p class="muted">Reading…</p>
  {/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!saved || !preview || !!app.busy} onclick={create}>{app.busy ?? 'Add table'}</button>
  {/snippet}
</Modal>

<style>
  .stack { display: flex; flex-direction: column; gap: 4px; margin-bottom: 10px; }
  .check { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .warn { margin: 0 0 12px; padding: 8px 12px; border-radius: var(--r-card); background: var(--selected); font-size: 12px; }
  .wrap { overflow: auto; border-radius: var(--r-card); background: var(--inset); max-height: 220px; }
  table { border-collapse: collapse; font-size: 12px; min-width: 100%; }
  th { text-align: left; font-weight: 600; padding: 6px 12px; border-bottom: 1px solid var(--sep); white-space: nowrap; }
  td { padding: 5px 12px; border-bottom: 1px solid var(--sep); white-space: nowrap; max-width: 240px; overflow: hidden; text-overflow: ellipsis; }
  .small { font-size: 12px; margin: 8px 0 0; }
</style>
