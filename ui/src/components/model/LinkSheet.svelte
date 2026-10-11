<script lang="ts">
  // The sheet of a link: its name, what the data says about it (the repeated keys), and Remove.
  // Opened with two or more links that join the same two tables, it asks for a name for each one
  // (V4-D28), because a field and a formula need a way to say which link they mean.
  import { api, type Relationship } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { cleanName, defaultName, kindWords, MAX_NAME, nameProblem, proposeNames } from '../../lib/links';
  import { cell } from '../../lib/format';
  import Modal from '../Modal.svelte';
  import Icon from '../Icon.svelte';

  let { ids, onclose }: { ids: string[]; onclose: () => void } = $props();

  const rels = $derived(app.snap!.workspace.relationships);
  const links = $derived(ids.map((id) => rels.find((r) => r.id === id)).filter((r): r is Relationship => !!r));
  const naming = $derived(links.length > 1);

  // What is typed for each link. A link that has a name starts with it; one that has none starts with the proposal
  // when the sheet asks for names, and empty (the proposal as a hint) when it is just being looked at.
  const start = (): Record<string, string> => {
    const proposed = proposeNames(links, app.tables);
    const out: Record<string, string> = {};
    for (const r of links) out[r.id] = r.name ?? (links.length > 1 ? proposed[r.id] : '');
    return out;
  };
  let typed = $state<Record<string, string>>(start());

  const table = (id: string) => app.table(id);
  const shown = (t: string, c: string) => table(t)?.columns.find((x) => x.name === c)?.label ?? c;

  const problems = $derived(
    Object.fromEntries(links.map((r) => [r.id, nameProblem(typed[r.id] ?? '', r, rels, app.tables, typed)])) as Record<string, string | null>,
  );
  const changed = $derived(links.filter((r) => cleanName(typed[r.id] ?? '') !== (r.name ?? '')));
  const valid = $derived(links.every((r) => !problems[r.id]));

  async function save() {
    if (!valid) return;
    for (const r of changed) {
      if (!(await app.run(null, () => api.renameRelationship(r.id, cleanName(typed[r.id] ?? ''))))) return;
    }
    onclose();
  }

  async function remove(r: Relationship) {
    if (await app.run(null, () => api.removeRelationship(r.id))) {
      if (links.length <= 1) onclose();
    }
  }

  /** "7 (2 rows), 12 (3 rows) and 4 more" */
  function repeated(r: Relationship): string {
    const h = app.linkHealth?.[r.id];
    if (!h?.sample.length) return '';
    const dtype = table(r.to.table)?.columns.find((c) => c.name === r.to.column)?.dtype ?? 'text';
    const parts = h.sample.map((k, i) => {
      const rows = h.sampleRows?.[i];
      return `${cell(k, dtype)}${rows ? ` (${rows} rows)` : ''}`;
    });
    const more = (h.keys ?? parts.length) - parts.length;
    return more > 0 ? `${parts.join(', ')} and ${more} more` : parts.join(', ');
  }
</script>

<Modal title={naming ? 'Name these links' : 'Link'} {onclose} width={540}>
  {#if naming}
    <p class="lead">
      {links.length} links join {table(links[0].from.table)?.label} and {table(links[0].to.table)?.label}. Give each one a name,
      so a tile or a formula can say which one it means, like “Shipped from” and “Sold at”.
    </p>
  {/if}

  {#each links as r (r.id)}
    {@const h = app.linkHealth?.[r.id]}
    <section class="link" data-link={r.id}>
      <label class="stack">
        <span class="label">{naming ? `Name for ${table(r.from.table)?.label}.${shown(r.from.table, r.from.column)}` : 'Name'}</span>
        <input
          class="field"
          class:bad={!!problems[r.id]}
          maxlength={MAX_NAME + 10}
          placeholder={defaultName(r, app.tables)}
          bind:value={typed[r.id]}
          onkeydown={(e) => e.key === 'Enter' && valid && save()} />
        {#if problems[r.id]}<span class="problem">{problems[r.id]}</span>{/if}
      </label>
      <p class="sides">
        <span><b>{table(r.from.table)?.label}</b>.{shown(r.from.table, r.from.column)}</span>
        <span class="arrow">→</span>
        <span><b>{table(r.to.table)?.label}</b>.{shown(r.to.table, r.to.column)}</span>
        <span class="kind">{kindWords(h)}</span>
      </p>
      {#if h?.note}
        <div class="warn" role="note">
          <Icon name="warning" size={14} />
          <div>
            <p>{h.note}</p>
            {#if repeated(r) && !h.unusable}<p class="keys">Repeated {shown(r.to.table, r.to.column)}: {repeated(r)}</p>{/if}
          </div>
        </div>
      {/if}
      <button class="remove" onclick={() => remove(r)}><Icon name="trash" size={12} /> Remove this link</button>
    </section>
  {/each}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!valid || !changed.length} onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  .lead { margin: 0 0 12px; color: var(--text-2); font-size: 13px; line-height: 1.45; }
  .link { padding: 12px 0; }
  .link + .link { border-top: 1px solid var(--sep); }
  .stack { display: flex; flex-direction: column; gap: 6px; }
  .field.bad { box-shadow: inset 0 0 0 1px var(--danger); }
  .problem { font-size: 12px; color: var(--danger); }
  .sides {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 8px;
    margin: 10px 0 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .sides b { color: var(--text); font-weight: 600; }
  .arrow { color: var(--text-3); }
  .kind { margin-left: auto; padding: 1px 8px; border-radius: 10px; background: var(--inset); font-size: 11px; font-weight: 500; }
  .warn {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    padding: 8px 10px;
    border-radius: 8px;
    background: color-mix(in srgb, #c47400 14%, var(--surface));
    font-size: 12px;
    line-height: 1.45;
    color: var(--text);
  }
  .warn :global(svg) { flex: none; margin-top: 2px; color: #c47400; }
  .warn p { margin: 0; }
  .warn .keys { margin-top: 4px; color: var(--text-2); }
  .remove {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-top: 10px;
    padding: 0;
    border: 0;
    background: none;
    font-size: 12px;
    color: var(--text-2);
  }
  .remove:hover { color: var(--danger); }
</style>
