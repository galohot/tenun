<script lang="ts">
  import { SvelteFlow, Background, Controls, ConnectionMode, type Node, type Edge, type Connection } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { untrack } from 'svelte';
  import { api, type FieldRef, type Relationship, type Suggestion } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { firstLinks, kindWords, linkLabel, siblings, unnamedPairs } from '../lib/links';
  import TableNode from './model/TableNode.svelte';
  import RelEdge from './model/RelEdge.svelte';
  import LinkSheet from './model/LinkSheet.svelte';
  import Icon from './Icon.svelte';

  const nodeTypes = { table: TableNode };
  const edgeTypes = { rel: RelEdge };

  let suggestions = $state<Suggestion[]>([]);
  let dismissed = $state<string[]>([]);
  let looking = $state(false);
  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);
  /** The links whose sheet is open: one to look at it, several to name them (V4-D28). */
  let sheet = $state<string[] | null>(null);

  const ws = $derived(app.snap!.workspace);
  const sk = (s: Suggestion) => `${s.from.table}.${s.from.column}>${s.to.table}.${s.to.column}`;

  /**
   * Positions: saved ones first. Unplaced tables are laid out by their links:
   * tables that point at others (the "many" side, like order lines) on the
   * left, the tables they point at to their right, so lines stay short and
   * their buttons never sit on top of each other. Unlinked tables go last.
   */
  function layout(links: { from: FieldRef; to: FieldRef }[] = []): Record<string, { x: number; y: number }> {
    const saved = ws.layout?.model ?? {};
    const level = new Map<string, number>(app.tables.map((t) => [t.id, 0]));
    const linked = new Set(links.flatMap((l) => [l.from.table, l.to.table]));
    for (let pass = 0; pass < app.tables.length; pass++) {
      let moved = false;
      for (const l of links) {
        const want = (level.get(l.from.table) ?? 0) + 1;
        if ((level.get(l.to.table) ?? 0) < want && want < app.tables.length) {
          level.set(l.to.table, want);
          moved = true;
        }
      }
      if (!moved) break;
    }
    const last = Math.max(0, ...[...level.entries()].filter(([t]) => linked.has(t)).map(([, v]) => v));
    const out: Record<string, { x: number; y: number }> = {};
    const nextY = new Map<number, number>();
    for (const t of app.tables) {
      if (saved[t.id]) {
        out[t.id] = saved[t.id];
        continue;
      }
      const col = linked.has(t.id) ? level.get(t.id)! : last + 1;
      const y = nextY.get(col) ?? 0;
      out[t.id] = { x: col * 400, y };
      nextY.set(col, y + 64 + t.columns.length * 24 + 48);
    }
    return out;
  }

  /** Bumped to re-fit the view after an automatic layout. */
  let fitKey = $state(0);

  /** Lays out again once suggestions arrive, unless the person already arranged the tables. */
  function relayout() {
    if (Object.keys(ws.layout?.model ?? {}).length) return;
    const pos = layout([...ws.relationships, ...suggestions]);
    nodes = nodes.map((n) => ({ ...n, position: pos[n.id] ?? n.position }));
    fitKey++;
  }

  // Rebuild nodes when the tables change; keep positions the user dragged.
  $effect(() => {
    const pos = layout(ws.relationships);
    const names = new Map(ws.sources.map((s) => [s.id, s.name]));
    const linked = new Map<string, Set<string>>();
    // The columns that repeat values: the keys of a link with a warning.
    const warned = new Map<string, Set<string>>();
    const mark = (f: FieldRef) => {
      if (!warned.has(f.table)) warned.set(f.table, new Set());
      warned.get(f.table)!.add(f.column);
    };
    for (const r of ws.relationships) {
      for (const f of [r.from, r.to]) {
        if (!linked.has(f.table)) linked.set(f.table, new Set());
        linked.get(f.table)!.add(f.column);
      }
      const h = app.linkHealth?.[r.id];
      if (h && h.duplicates > 0) mark(r.to);
      if (h?.unusable) mark(r.from);
    }
    const current = new Map(untrack(() => nodes).map((n) => [n.id, n.position]));
    nodes = app.tables.map((t) => ({
      id: t.id,
      type: 'table',
      position: current.get(t.id) ?? pos[t.id],
      data: { table: t, source: names.get(t.source_id) ?? '', linked: linked.get(t.id) ?? new Set(), warned: warned.get(t.id) ?? new Set() },
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
    const toName = new Set(unnamedPairs(ws.relationships).flat().map((r) => r.id));
    const rels = ws.relationships.map((r) => {
      const h = app.linkHealth?.[r.id];
      // Two lines between the same tables end in the same place; their labels sit one above the other.
      const pair = siblings(r, ws.relationships);
      const dy = pair.length > 1 ? (pair.findIndex((x) => x.id === r.id) - (pair.length - 1) / 2) * 30 : 0;
      return edgeFor(r.id, r.from, r.to, {
        dy,
        onremove: () => app.run(null, () => api.removeRelationship(r.id)),
        onopen: () => (sheet = toName.has(r.id) ? siblings(r, ws.relationships).map((x) => x.id) : [r.id]),
        name: r.name,
        kind: kindWords(h),
        note: h?.note,
        unusable: h?.unusable,
        needsName: toName.has(r.id),
      });
    });
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
      relayout();
      if (!suggestions.length) app.toast('No more likely relationships found.');
    } catch (e) {
      app.error(e);
    } finally {
      looking = false;
    }
  }

  /** Adds a link. A second link between the same two tables opens the sheet that asks for names. */
  async function addLink(a: FieldRef, b: FieldRef): Promise<boolean> {
    const before = new Set(ws.relationships.map((r) => r.id));
    if (!(await app.run(null, () => api.addRelationship(a, b)))) return false;
    const added = ws.relationships.find((r: Relationship) => !before.has(r.id));
    const group = added ? siblings(added, ws.relationships) : [];
    if (group.length > 1) sheet = group.map((r) => r.id);
    return true;
  }

  async function accept(s: Suggestion) {
    if (await addLink(s.from, s.to)) {
      suggestions = suggestions.filter((x) => sk(x) !== sk(s));
    }
  }

  async function onconnect(c: Connection) {
    const col = (h?: string | null) => (h ?? '').replace(/^[LR]:/, '');
    if (!c.sourceHandle || !c.targetHandle || c.source === c.target) return;
    await addLink({ table: c.source, column: col(c.sourceHandle) }, { table: c.target, column: col(c.targetHandle) });
  }

  /** Tables that can be reached two ways at the same length: the field list offers them once for each way. */
  const ways = $derived.by(() => {
    const out: string[] = [];
    for (const a of app.tables) {
      for (const b of app.tables) {
        if (a.id === b.id) continue;
        const first = firstLinks(a.id, b.id, ws.relationships, app.linkHealth);
        if (first.length < 2) continue;
        const names = first.map((id) => ws.relationships.find((r) => r.id === id)).filter((r): r is Relationship => !!r).map((r) => linkLabel(r, app.tables));
        out.push(`${a.label} reaches ${b.label} ${first.length === 2 ? 'two ways' : `${first.length} ways`}: ${names.join(' and ')}.`);
      }
    }
    return out;
  });

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
  <span class="hint muted">Drag from a column to a column in another table to relate them. Click the label of a line to open it.</span>
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
    {#key fitKey}
    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      {edgeTypes}
      fitView
      fitViewOptions={{ padding: 0.12, maxZoom: 1.25 }}
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
    {/key}
    <div class="legend">
      <span><i class="solid"></i> relationship (the dot marks the side with unique values)</span>
      <span><i class="warn-line"></i> a column repeats values</span>
      <span><i class="dash"></i> suggestion</span>
    </div>
    {#if ways.length}
      <div class="ways" role="note">
        <strong>More than one way</strong>
        {#each ways.slice(0, 3) as w (w)}<span>{w}</span>{/each}
        {#if ways.length > 3}<span>and {ways.length - 3} more.</span>{/if}
        <span class="muted">The field list offers these tables once for each way.</span>
      </div>
    {/if}
  </div>
{/if}

{#if sheet}
  <LinkSheet ids={sheet} onclose={() => (sheet = null)} />
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
  .legend i.warn-line { border-top-color: #c47400; }
  .ways {
    position: absolute;
    left: 72px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-width: 340px;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--shadow-tile);
    font-size: 11px;
    line-height: 1.4;
    color: var(--text);
  }
  .ways strong { font-weight: 600; }
  .ways .muted { color: var(--text-2); }
  .empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; }
  .empty :global(svg) { color: var(--accent-text); }
  .empty p { margin: 0 0 8px; }
  @media (max-width: 760px) {
    .hint { display: none; }
    /* A narrow screen has no room for two boxes at the bottom: the note about two ways goes to the top. */
    .legend { display: none; }
    .ways { left: 12px; right: 12px; top: 12px; bottom: auto; max-width: none; }
  }
</style>
