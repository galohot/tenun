<script lang="ts">
  // Hierarchies (v5 build 01): fields from the top down, such as Region › Store › Product, that a chart can drill
  // down. Each lower field must reach the one above it through the model; the engine checks that when they are saved
  // and says which pair does not.
  import { api, type FieldRef, type Hierarchy } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import Modal from '../Modal.svelte';
  import Icon from '../Icon.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const copy = (): Hierarchy[] => structuredClone($state.snapshot(app.snap?.workspace.hierarchies ?? []) as Hierarchy[]);
  let list = $state<Hierarchy[]>(copy());
  let saving = $state(false);

  const SEP = '\u0000';
  const keyOf = (f: FieldRef) => `${f.table}${SEP}${f.column}`;
  const fields = $derived(
    app.tables.flatMap((t) =>
      t.columns.filter((c) => !c.hidden).map((c) => ({ value: `${t.id}${SEP}${c.name}`, label: `${t.label} · ${c.label ?? c.name}` })),
    ),
  );
  const fieldOf = (value: string): FieldRef => {
    const [table, column] = value.split(SEP);
    return { table, column };
  };
  const first = () => fieldOf(fields[0]?.value ?? SEP);

  function add() {
    list.push({ id: '', name: `Hierarchy ${list.length + 1}`, levels: [first(), first()] });
  }
  function move(h: Hierarchy, from: number, to: number) {
    if (to < 0 || to >= h.levels.length) return;
    const [l] = h.levels.splice(from, 1);
    h.levels.splice(to, 0, l);
  }
  const changed = $derived(JSON.stringify(list) !== JSON.stringify(app.snap?.workspace.hierarchies ?? []));

  async function save() {
    saving = true;
    if (await app.run(null, () => api.putHierarchies($state.snapshot(list) as Hierarchy[]))) onclose();
    saving = false;
  }
  function close() {
    if (changed && !confirm('Close without saving the hierarchies?')) return;
    onclose();
  }
</script>

<Modal title="Hierarchies" width={600} onclose={close}>
  <p class="intro">
    A hierarchy lists fields from the top down, such as Region › Store. A chart whose first field is in it can drill down
    through it: choose it under <b>Drill down</b> in the tile's Advanced options. Dates drill by year, quarter, month and
    day without a hierarchy.
  </p>

  {#each list as h, hi (hi)}
    <section class="h">
      <div class="row">
        <input class="field" aria-label="Name of the hierarchy" maxlength="60" bind:value={h.name} />
        <button class="btn plain icon" aria-label="Remove {h.name}" onclick={() => list.splice(hi, 1)}><Icon name="trash" size={14} /></button>
      </div>
      <ol>
        {#each h.levels as l, li (li)}
          <li>
            <span class="n">{li + 1}</span>
            <select class="field" aria-label="Level {li + 1} of {h.name}" value={keyOf(l)} onchange={(e) => (h.levels[li] = fieldOf(e.currentTarget.value))}>
              {#each fields as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
            </select>
            <button class="btn plain icon" aria-label="Move level {li + 1} up" disabled={li === 0} onclick={() => move(h, li, li - 1)}><Icon name="chevronDown" size={12} class="flip" /></button>
            <button class="btn plain icon" aria-label="Move level {li + 1} down" disabled={li === h.levels.length - 1} onclick={() => move(h, li, li + 1)}><Icon name="chevronDown" size={12} /></button>
            <button class="btn plain icon" aria-label="Remove level {li + 1}" disabled={h.levels.length <= 2} onclick={() => h.levels.splice(li, 1)}><Icon name="x" size={12} /></button>
          </li>
        {/each}
      </ol>
      {#if h.levels.length < 6}
        <button class="link" onclick={() => h.levels.push(first())}>Add a level</button>
      {/if}
    </section>
  {:else}
    <p class="muted">No hierarchies yet.</p>
  {/each}
  <button class="btn" onclick={add} disabled={!fields.length}><Icon name="plus" size={14} /> New hierarchy</button>

  {#snippet footer()}
    <button class="btn plain" onclick={close}>Cancel</button>
    <button class="btn primary" disabled={!changed || saving} onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  .intro { margin: 0 0 14px; font-size: 13px; line-height: 1.45; color: var(--text-2); }
  .h { margin-bottom: 14px; padding: 10px 12px; border-radius: 10px; background: var(--inset); }
  .row { display: grid; grid-template-columns: 1fr 28px; gap: 6px; margin-bottom: 8px; }
  ol { list-style: none; margin: 0 0 6px; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  li { display: grid; grid-template-columns: 18px 1fr 26px 26px 26px; gap: 6px; align-items: center; }
  .n { font-size: 12px; color: var(--text-2); text-align: right; }
  li :global(.flip) { transform: rotate(180deg); }
  .link { border: 0; background: none; padding: 0; font-size: 12px; color: var(--accent-text); cursor: pointer; }
  .muted { color: var(--text-2); font-size: 13px; }
</style>
