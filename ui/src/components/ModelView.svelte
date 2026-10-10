<script lang="ts">
  import { SvelteFlow, Background, Controls, ConnectionMode, type Node, type Edge, type Connection } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { untrack } from 'svelte';
  import { api, type FieldRef, type Suggestion } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import TableNode from './model/TableNode.svelte';
  import RelEdge from './model/RelEdge.svelte';
  import Icon from './Icon.svelte';

  const nodeTypes = { table: TableNode };
  const edgeTypes = { rel: RelEdge };

  let suggestions = $state<Suggestion[]>([]);
  let dismissed = $state<string[]>([]);
  let looking = $state(false);
  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);

  const ws = $derived(app.snap!.workspace);
  const sk = (s: Suggestion) => `${s.from.table}.${s.from.column}>${s.to.table}.${s.to.column}`;

  /** Positions: saved ones first; new tables go in a column per source. */
  function layout(): Record<string, { x: number; y: number }> {
    const saved = ws.layout?.model ?? {};
    const out: Record<string, { x: number; y: number }> = {};
    const bySource = new Map<string, number>();
    const srcIndex = new Map(ws.sources.map((s, i) => [s.id, i]));
    for (const t of app.tables) {
      if (saved[t.id]) {
        out[t.id] = saved[t.id];
        continue;
      }
      const col = srcIndex.get(t.source_id) ?? 0;
      const y = bySource.get(t.source_id) ?? 0;
      out[t.id] = { x: col * 300, y };
      bySource.set(t.source_id, y + 64 + t.columns.length * 24 + 40);
    }
    return out;
  }

  // Rebuild nodes when the tables change; keep positions the user dragged.
  $effect(() => {
    const pos = layout();
    const names = new Map(ws.sources.map((s) => [s.id, s.name]));
    const linked = new Map<string, Set<string>>();
    for (const r of ws.relationships) {
      for (const f of [r.from, r.to]) {
        if (!linked.has(f.table)) linked.set(f.table, new Set());
        linked.get(f.table)!.add(f.column);
      }
    }
    const current = new Map(untrack(() => nodes).map((n) => [n.id, n.position]));
    nodes = app.tables.map((t) => ({
      id: t.id,
      type: 'table',
      position: current.get(t.id) ?? pos[t.id],
      data: { table: t, source: names.get(t.source_id) ?? '', linked: linked.get(t.id) ?? new Set() },
      dragHandle: 'header',
    }));
  });

  /** Which side of each card a line leaves from: facing sides, or both left when stacked. */
  function sides(a: string, b: string): [string, string] {
    const pa = nodes.find((n) => n.id === a)?.position.x ?? 0;
    const pb = nodes.find((n) => n.id === b)?.position.x ?? 0;
    if (Math.abs(pa - pb) < 240) return ['L', 'L'];
    return pa < pb ? ['R', 'L'] : ['L', 'R'];
  }

  function edgeFor(id: string, from: FieldRef, to: FieldRef, data: Record<string, unknown>): Edge {
    const [sa, sb] = sides(from.table, to.table);
    return {
      id,
      type: 'rel',
      source: from.table,
      sourceHandle: `${sa}:${from.column}`,
      target: to.table,
      targetHandle: `${sb}:${to.column}`,
      data,
      selectable: !data.suggestion,
    };
  }

  $effect(() => {
    void nodes;
    const rels = ws.relationships.map((r) =>
      edgeFor(r.id, r.from, r.to, { onremove: () => app.run(null, () => api.removeRelationship(r.id)) }),
    );
    const sugg = suggestions
      .filter((s) => !dismissed.includes(sk(s)))
      .map((s) =>
        edgeFor(`s:${sk(s)}`, s.from, s.to, {
          suggestion: true,
          reason: s.reason,
          onaccept: () => accept(s),
          ondismiss: () => (dismissed = [...dismissed, sk(s)]),
        }),
      );
    edges = [...rels, ...sugg];
  });

  async function findSuggestions() {
    looking = true;
    try {
      suggestions = await api.suggest();
      if (!suggestions.length) app.toast('No more likely relationships found.');
    } catch (e) {
      app.error(e);
    } finally {
      looking = false;
    }
  }

  async function accept(s: Suggestion) {
    if (await app.run(null, () => api.addRelationship(s.from, s.to))) {
      suggestions = suggestions.filter((x) => sk(x) !== sk(s));
    }
  }

  async function onconnect(c: Connection) {
    const col = (h?: string | null) => (h ?? '').replace(/^[LR]:/, '');
    if (!c.sourceHandle || !c.targetHandle || c.source === c.target) return;
    await app.run(null, () =>
      api.addRelationship({ table: c.source, column: col(c.sourceHandle) }, { table: c.target, column: col(c.targetHandle) }),
    );
  }

  function savePositions() {
    const model: Record<string, { x: number; y: number }> = {};
    for (const n of nodes) model[n.id] = { x: Math.round(n.position.x), y: Math.round(n.position.y) };
    const layoutVal = { ...(ws.layout ?? {}), model };
    app.snap!.workspace.layout = layoutVal;
    api.putLayout(layoutVal).catch((e) => app.error(e));
  }

  // First visit with no relationships: look for suggestions straight away.
  let asked = false;
  $effect(() => {
    if (!asked && app.tables.length > 1) {
      asked = true;
      if (!ws.relationships.length || app.tables.length > ws.relationships.length + 1) findSuggestions();
    }
  });
</script>

<header class="toolbar">
  <h2>Model</h2>
  <span class="hint muted">Drag from a column to a column in another table to relate them.</span>
  <span class="spacer"></span>
  <button class="btn" onclick={findSuggestions} disabled={looking || app.tables.length < 2}>
    <Icon name="sparkles" /> {looking ? 'Looking…' : 'Suggest relationships'}
  </button>
</header>

{#if app.tables.length === 0}
  <div class="empty">
    <Icon name="model" size={32} />
    <h2>Nothing to relate yet</h2>
    <p class="muted">Add two or more tables, then link them here.</p>
    <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data</button>
  </div>
{:else}
  <div class="canvas">
    <svg width="0" height="0" style="position:absolute">
      <defs>
        <marker id="tenun-one" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="8" markerHeight="8">
          <circle cx="5" cy="5" r="3.5" fill="var(--surface)" stroke="currentColor" stroke-width="1.6" style="color: var(--text-2)" />
        </marker>
      </defs>
    </svg>
    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      {edgeTypes}
      fitView
      fitViewOptions={{ padding: 0.15, maxZoom: 1 }}
      minZoom={0.3}
      maxZoom={1.6}
      colorMode={app.theme}
      connectionMode={ConnectionMode.Loose}
      connectionRadius={24}
      deleteKey={['Backspace', 'Delete']}
      proOptions={{ hideAttribution: true }}
      onconnect={onconnect}
      ondelete={({ edges: gone }) => {
        for (const e of gone) if (!e.id.startsWith('s:')) app.run(null, () => api.removeRelationship(e.id));
      }}
      onnodedragstop={savePositions}>
      <Background gap={20} size={1} />
      <Controls showLock={false} />
    </SvelteFlow>
    <div class="legend">
      <span><i class="solid"></i> relationship (the dot marks the side with unique values)</span>
      <span><i class="dash"></i> suggestion</span>
    </div>
  </div>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 52px;
    padding: 0 20px;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    flex: none;
  }
  .hint { font-size: 12px; }
  .spacer { flex: 1; }
  .canvas { position: relative; flex: 1; min-height: 0; }
  .canvas :global(.svelte-flow) {
    --xy-background-color: var(--window);
    --xy-background-pattern-color: var(--sep-strong);
    --xy-controls-button-background-color: var(--surface);
    --xy-controls-button-background-color-hover: var(--hover);
    --xy-controls-button-color: var(--text);
    --xy-controls-button-border-color: var(--sep);
    --xy-connectionline-stroke: var(--accent);
    --xy-connectionline-stroke-width: 2;
    --xy-node-background-color: transparent;
    --xy-node-border: none;
    --xy-node-boxshadow-selected: none;
    --xy-edge-stroke: var(--text-2);
  }
  .canvas :global(.svelte-flow__node) { padding: 0; border: 0; background: none; box-shadow: none; }
  .canvas :global(.svelte-flow__controls) { box-shadow: var(--shadow-float); border-radius: 8px; overflow: hidden; }
  .legend {
    position: absolute;
    right: 16px;
    bottom: 16px;
    display: flex;
    gap: 16px;
    padding: 6px 12px;
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow-tile);
    font-size: 11px;
    color: var(--text-2);
  }
  .legend span { display: flex; align-items: center; gap: 6px; }
  .legend i { display: inline-block; width: 18px; height: 0; border-top: 1.6px solid var(--text-2); }
  .legend i.dash { border-top: 1.6px dashed var(--accent); }
  .empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; }
  .empty :global(svg) { color: var(--accent-text); }
  .empty p { margin: 0 0 8px; }
</style>
