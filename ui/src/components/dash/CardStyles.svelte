<script lang="ts">
  // The card styles of the workspace (build 08, V4-D10): rename, duplicate, delete. A deleted style is copied into
  // every card that used it first, so no card changes its look.
  import type { CardStyle, KpiInfo, Tile } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { resolveCard, withoutStyle } from '../../lib/card';
  import Icon from '../Icon.svelte';
  import Modal from '../Modal.svelte';
  import CardView from './card/CardView.svelte';

  const SAMPLE: KpiInfo = { value: 128400, period: 'This month', versus: 'last month', prev: 117800, delta: 0.09, partial: false, bars: [] };
  const PLAIN: Tile = { id: 'sample', type: 'kpi', title: 'Revenue', x: 0, y: 0, w: 3, h: 2, measures: [], dims: [] };
  /** The thumbnails draw this number: the tile being edited, or a made-up one when the list opens from the dashboard. */
  let { kpi = SAMPLE, tile = PLAIN, onclose }: { kpi?: KpiInfo; tile?: Tile; onclose: () => void } = $props();

  const styles = $derived(app.snap?.workspace.cardStyles ?? []);
  const cards = $derived((app.snap?.workspace.dashboards ?? []).flatMap((d) => d.tiles.filter((t) => t.type === 'kpi' && t.card?.style)));
  const uses = (id: string) => cards.filter((t) => t.card?.style === id).length;
  let deleting = $state<string | null>(null);

  function rename(s: CardStyle, name: string) {
    const n = name.trim();
    if (!n || n === s.name) return;
    app.saveCardStyles(styles.map((x) => (x.id === s.id ? { ...x, name: n } : x)));
  }
  function duplicate(s: CardStyle) {
    const id = `cs-${Math.random().toString(36).slice(2, 10)}`;
    const at = styles.indexOf(s);
    const list = [...styles];
    list.splice(at + 1, 0, { id, name: `${s.name} (copy)`, card: JSON.parse(JSON.stringify(s.card)) });
    app.saveCardStyles(list);
  }
  function remove(s: CardStyle) {
    deleting = null;
    // Every card that used the style keeps its look: the style's fields go into the card, under its own.
    let changed = false;
    for (const d of app.snap?.workspace.dashboards ?? []) {
      for (const t of d.tiles) {
        if (t.card?.style === s.id) {
          t.card = withoutStyle(t.card, s);
          changed = true;
        }
      }
    }
    if (changed) app.saveDashboards();
    app.saveCardStyles(styles.filter((x) => x.id !== s.id));
  }
</script>

<Modal title="Card styles" width={520} {onclose}>
  {#if !styles.length}
    <p class="empty">No card styles yet. Make one from a number tile: Edit tile › Card › Save as card style.</p>
  {:else}
    <ul class="list">
      {#each styles as s (s.id)}
        {@const n = uses(s.id)}
        <li>
          <span class="thumb ground-{s.card.ground ?? 'plain'}" aria-hidden="true">
            <span class="inner"><CardView {kpi} {tile} card={resolveCard({ style: s.id }, styles, !!tile.target)} title={tile.title || 'Revenue'} /></span>
          </span>
          <div class="info">
            <input class="field" aria-label="Name of the style" value={s.name} maxlength="60" onchange={(e) => rename(s, e.currentTarget.value)} />
            <span class="muted">{n === 0 ? 'Not used' : n === 1 ? 'Used by 1 card' : `Used by ${n} cards`}</span>
            {#if deleting === s.id}
              <div class="confirm" role="alert">
                <span>{n ? `Delete it? The ${n === 1 ? 'card keeps its' : `${n} cards keep their`} look.` : 'Delete it?'}</span>
                <button class="btn danger" onclick={() => remove(s)}>Delete</button>
                <button class="btn plain" onclick={() => (deleting = null)}>Cancel</button>
              </div>
            {:else}
              <div class="acts">
                <button class="btn plain" onclick={() => duplicate(s)}>Duplicate</button>
                <button class="btn plain danger" onclick={() => (deleting = s.id)}><Icon name="trash" size={14} /> Delete</button>
              </div>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</Modal>

<style>
  .empty { margin: 0; font-size: 13px; color: var(--text-2); }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 12px; }
  li { display: grid; grid-template-columns: 160px 1fr; gap: 12px; align-items: start; }
  .thumb { display: block; width: 160px; aspect-ratio: 2 / 1; position: relative; overflow: hidden; border-radius: 6px; }
  .inner { position: absolute; left: 0; top: 0; width: 200%; height: 200%; transform: scale(0.5); transform-origin: 0 0; padding: 12px 14px 10px; box-sizing: border-box; }
  .ground-plain { background: var(--surface); box-shadow: inset 0 0 0 1px var(--sep); }
  .ground-tint { background: var(--tint); }
  .ground-accent { background: var(--accent); }
  .info { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .muted { font-size: 12px; color: var(--text-2); }
  .acts, .confirm { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; font-size: 12px; }
</style>
