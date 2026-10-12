<script lang="ts">
  // Match fields (build 09, V4-D11): tiles copied from another workspace read tables this one may not have. Each table
  // they read gets a table here, and each of its columns a column, guessed first (`guessMatches`). A field left
  // unmatched pastes as it is, and its tile says what to pick.
  import type { Table } from '../../lib/api';
  import { guessColumns, guessMatches, unmatched, type Clip, type Matches } from '../../lib/clipboard';
  import { typeWords } from '../../lib/formula';
  import Modal from '../Modal.svelte';

  let { clip, tables, onpaste, onclose }: { clip: Clip; tables: Table[]; onpaste: (m: Matches) => void; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let matches = $state<Matches>(guessMatches(clip, tables));
  const left = $derived(unmatched(clip, matches));
  const n = $derived(clip.tiles.length);

  function pickTable(id: string, to: string) {
    const ct = clip.tables.find((t) => t.id === id)!;
    const t = tables.find((x) => x.id === to);
    matches[id] = { to: t ? t.id : null, columns: t ? guessColumns(ct, t) : Object.fromEntries(ct.columns.map((c) => [c.name, null])) };
  }
</script>

<Modal title="Match fields" width={560} {onclose}>
  <p class="intro">
    {n === 1 ? 'The tile comes' : `The ${n} tiles come`} from another workspace. Pick the table and the columns here that hold the same data.
  </p>
  {#each clip.tables as ct (ct.id)}
    {@const m = matches[ct.id]}
    {@const to = tables.find((t) => t.id === m?.to)}
    <section>
      <label class="row head">
        <span class="from"><b>{ct.label}</b></span>
        <select class="field" aria-label="Table for {ct.label}" value={m?.to ?? ''} onchange={(e) => pickTable(ct.id, e.currentTarget.value)}>
          <option value="">Not here (leave unlinked)</option>
          {#each tables as t (t.id)}<option value={t.id}>{t.label}</option>{/each}
        </select>
      </label>
      {#if to}
        {#each ct.columns as c (c.name)}
          <label class="row">
            <span class="from">{c.name} <span class="muted">{typeWords(c.dtype)}</span></span>
            <select class="field" aria-label="Column for {ct.label} {c.name}" value={m.columns[c.name] ?? ''} onchange={(e) => (matches[ct.id].columns[c.name] = e.currentTarget.value || null)}>
              <option value="">Not here</option>
              {#each to.columns as x (x.name)}<option value={x.name}>{x.name}{x.dtype !== c.dtype ? ` (${typeWords(x.dtype)})` : ''}</option>{/each}
            </select>
          </label>
        {/each}
      {/if}
    </section>
  {/each}
  {#if left.length}
    <p class="warn" role="note">
      {left.length === 1 ? 'One field has' : `${left.length} fields have`} no match. A tile that reads {left.length === 1 ? 'it' : 'one'} pastes unlinked: open it to pick the field.
    </p>
  {/if}
  {#snippet footer()}
    <button class="btn plain" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={() => onpaste($state.snapshot(matches))}>Paste {n === 1 ? 'the tile' : `${n} tiles`}</button>
  {/snippet}
</Modal>

<style>
  .intro { margin: 0 0 12px; font-size: 13px; line-height: 1.45; color: var(--text-2); }
  section { padding: 10px 0; border-top: 1px solid var(--sep); display: flex; flex-direction: column; gap: 6px; }
  .row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-items: center; font-size: 13px; }
  .row.head { margin-bottom: 2px; }
  .from { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row:not(.head) .from { padding-left: 12px; }
  .muted { color: var(--text-2); font-size: 12px; }
  select.field { height: 30px; min-width: 0; }
  .warn { margin: 12px 0 0; font-size: 12px; line-height: 1.4; color: var(--text); }
</style>
