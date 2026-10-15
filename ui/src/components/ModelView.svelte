<script lang="ts">
  import { SvelteFlow, Background, Controls, ConnectionMode, type Node, type Edge, type Connection } from '@xyflow/svelte';
  import '@xyflow/svelte/dist/style.css';
  import { untrack } from 'svelte';
  import { api, type FieldRef, type Relationship, type Suggestion } from '../lib/api';
  import { app } from '../lib/state.svelte';
  import { advancedOn } from '../lib/tiers';
  import { firstLinks, kindWords, linkLabel, siblings, unnamedPairs } from '../lib/links';
  import { go } from '../lib/state.svelte';
  import { canMarkLinkTable, notLinkTablesOf, suggestLinkTables } from '../lib/linktables';
  import { isEmptySetup, withLinkTable, withNotLinkTable } from '../lib/prep';
  import TableNode from './model/TableNode.svelte';
  import RelEdge from './model/RelEdge.svelte';
  import LinkSheet from './model/LinkSheet.svelte';
  import HierarchySheet from './model/HierarchySheet.svelte';
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
  /** Tables the person said are not link tables. The file keeps it (V4-D124). */
  const notLinkTables = $derived(notLinkTablesOf(ws.tables));
  const linkSuggestions = $derived(suggestLinkTables(app.tables, ws.relationships, app.linkHealth, app.linkTables, notLinkTables));

  /** Marks a table as a link table, or takes the mark away (V4-D112). */
  async function markLinkTable(id: string, on: boolean) {
    const next = withLinkTable(ws.tables?.[id], on);
    await app.run(null, () => api.setTableSetup(id, isEmptySetup(next) ? null : next));
  }

  /** "Not a link table": the suggestion goes, and stays gone after the file is opened again. */
  async function notLinkTable(id: string) {
    await app.run(null, () => api.setTableSetup(id, withNotLinkTable(ws.tables?.[id])));
  }
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
      nextY.set(col, y + 72 + t.columns.length * 28 + 48);
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

  /** The relationship picked on the canvas or in the panel: its line and its two columns light up. */
  const picked = $derived(edges.find((e) => e.selected && !e.id.startsWith('s:'))?.id ?? null);
  const pickedRel = $derived(ws.relationships.find((r) => r.id === picked) ?? null);

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
      data: {
        table: t,
        source: names.get(t.source_id) ?? '',
        linked: linked.get(t.id) ?? new Set(),
        hot: new Set([pickedRel?.from, pickedRel?.to].filter((f): f is FieldRef => !!f && f.table === t.id).map((f) => f.column)),
        warned: warned.get(t.id) ?? new Set(),
        linkTable: app.linkTables.has(t.id),
        suggestion: linkSuggestions.find((s) => s.table === t.id)?.why ?? null,
        canMark: canMarkLinkTable(t.id, ws.relationships),
        onmark: () => markLinkTable(t.id, true),
        onunmark: () => markLinkTable(t.id, false),
        ondismiss: () => notLinkTable(t.id),
      },
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

  function edgeFor(id: string, from: FieldRef, to: FieldRef, data: Record<string, unknown>, selected = false): Edge {
    const [sa, sb] = sides(from.table, to.table);
    return {
      selected,
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
    // A rebuild keeps the line that is picked.
    const was = new Set(untrack(() => edges).filter((e) => e.selected).map((e) => e.id));
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
        note: h?.note ?? app.snap?.link_clashes?.[r.id],
        unusable: h?.unusable,
        needsName: toName.has(r.id),
      }, was.has(r.id));
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
        const first = firstLinks(a.id, b.id, ws.relationships, app.linkHealth, app.linkTables);
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
  /** The hierarchies sheet (v5 build 01, Advanced). */
  let hierarchies = $state(false);

  /** Picks a relationship from the panel: its line is selected on the canvas, as a click on it does. */
  function pick(id: string) {
    edges = edges.map((e) => ({ ...e, selected: e.id === id && e.id !== picked }));
  }
  /** Opens the sheet of a link; two unnamed links between the same tables open together, to be named. */
  function openRel(r: Relationship) {
    const toName = new Set(unnamedPairs(ws.relationships).flat().map((x) => x.id));
    sheet = toName.has(r.id) ? siblings(r, ws.relationships).map((x) => x.id) : [r.id];
  }
  const tableName = (id: string) => app.table(id)?.label ?? id;
  const shownSuggestions = $derived(suggestions.filter((s) => !dismissed.includes(sk(s))));
</script>

<header class="toolbar">
  <nav class="crumbs" aria-label="Breadcrumb">
    <button class="crumb link ws" onclick={() => go({ view: 'data' })} title="Open Data">{ws.name || 'Workspace'}</button>
    <Icon name="chevronRight" size={12} />
    <h2 class="crumb now">Model</h2>
  </nav>
  <span class="hint muted">Drag from a column to a column in another table to relate them. Click the label of a line to open it.</span>
  <span class="spacer"></span>
  {#if advancedOn() || (app.snap?.workspace.hierarchies ?? []).length}
    <button class="btn" aria-label="Hierarchies" title="Hierarchies, such as Region › Store, for drill-down" onclick={() => (hierarchies = true)} disabled={!app.tables.length}>
      <Icon name="drill" /> <span class="bl">Hierarchies</span>
    </button>
  {/if}
  <button class="btn" aria-label={looking ? 'Looking…' : 'Suggest relationships'} title="Find the columns that likely join your tables" onclick={findSuggestions} disabled={looking || app.tables.length < 2}>
    <Icon name="sparkles" /> <span class="bl">{looking ? 'Looking…' : 'Suggest relationships'}</span>
  </button>
</header>

{#if hierarchies}
  <HierarchySheet onclose={() => (hierarchies = false)} />
{/if}

{#if app.tables.length === 0}
  <div class="empty">
    <Icon name="model" size={32} />
    <h2>Nothing to relate yet</h2>
    <p class="muted">Add two or more tables, then link them here.</p>
    <button class="btn primary" onclick={() => (app.addOpen = true)}><Icon name="plus" /> Add data</button>
  </div>
{:else}
  <div class="body">
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
    {#if ways.length}
      <div class="ways" role="note">
        <strong>More than one way</strong>
        {#each ways.slice(0, 3) as w (w)}<span>{w}</span>{/each}
        {#if ways.length > 3}<span>and {ways.length - 3} more.</span>{/if}
        <span class="muted">The field list offers these tables once for each way.</span>
      </div>
    {/if}
  </div>
  <aside class="rels" aria-label="Relationships">
    <div class="rh"><span>Relationships · <span class="num">{ws.relationships.length}</span></span></div>
    <div class="rlist">
      {#each ws.relationships as r (r.id)}
        {@const h = app.linkHealth?.[r.id]}
        {@const note = h?.note ?? app.snap?.link_clashes?.[r.id]}
        <div class="rel" class:on={picked === r.id}>
          <button class="rmain" aria-pressed={picked === r.id} onclick={() => pick(r.id)} title="Show it on the canvas">
            {#if r.name}<span class="rname">{r.name}</span>{/if}
            <span class="path">{tableName(r.from.table)}.{r.from.column} <span class="arrow">→</span> {tableName(r.to.table)}.{r.to.column}</span>
            <span class="state">
              <i class="dot" class:warn={!!note} class:off={h?.unusable}></i>
              <span>{kindWords(h)}</span>
              {#if h?.unusable}<span>· Not used</span>{/if}
            </span>
            {#if note}<span class="note">{note}</span>{/if}
          </button>
          <button class="btn plain icon tiny" title="Open {linkLabel(r, app.tables)}" aria-label="Open {linkLabel(r, app.tables)}" onclick={() => openRel(r)}><Icon name="chevronRight" size={13} /></button>
        </div>
      {:else}
        <p class="empty-note">No relationships yet. Drag from a column to a column in another table, or ask for suggestions.</p>
      {/each}
      {#if shownSuggestions.length}
        <div class="rh sub"><span>Suggested · <span class="num">{shownSuggestions.length}</span></span></div>
        {#each shownSuggestions as sg (sk(sg))}
          <div class="rel sug">
            <div class="rmain static">
              <span class="path">{tableName(sg.from.table)}.{sg.from.column} <span class="arrow">→</span> {tableName(sg.to.table)}.{sg.to.column}</span>
              {#if sg.reason}<span class="note">{sg.reason}</span>{/if}
              <span class="acts">
                <button class="btn primary small" onclick={() => accept(sg)}><Icon name="check" size={12} /> Link</button>
                <button class="btn plain small" onclick={() => (dismissed = [...dismissed, sk(sg)])}>Dismiss</button>
              </span>
            </div>
          </div>
        {/each}
      {/if}
    </div>
    <div class="legend">
      <span><i></i> Relationship · the dot marks the unique side</span>
      <span><i class="warn-line"></i> A column repeats values</span>
      <span><i class="dash"></i> Suggestion</span>
    </div>
  </aside>
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
    height: 48px;
    padding: 0 12px 0 20px;
    border-bottom: 1px solid var(--sep);
    background: var(--surface);
    flex: none;
  }
  .crumbs { display: flex; align-items: center; gap: 6px; min-width: 0; flex: none; color: var(--text-2); }
  .crumbs :global(svg) { flex: none; color: var(--text-3); }
  .crumb { font-size: 13px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .crumb.link { max-width: 180px; border: 0; background: none; padding: 2px 4px; margin: 0 -4px; border-radius: 5px; color: var(--text-2); transition: background 120ms var(--spring), color 120ms var(--spring); }
  .crumb.link:hover { background: var(--hover); color: var(--text); }
  .crumb.now { font-weight: 600; color: var(--text); letter-spacing: 0; }
  .hint { font-size: 12px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .spacer { flex: 1; }
  .body { flex: 1; min-height: 0; display: flex; }
  .rels { width: 300px; flex: none; display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--sep); background: var(--surface); }
  .rh { display: flex; align-items: center; height: 40px; padding: 0 16px; flex: none; font-family: var(--mono); font-size: 10.5px; font-weight: 500; letter-spacing: 0.02em; text-transform: uppercase; color: var(--text-2); border-bottom: 1px solid var(--sep); }
  .rh.sub { margin-top: 8px; border-top: 1px solid var(--sep); }
  .rlist { flex: 1; min-height: 0; overflow: auto; }
  .rel { display: flex; align-items: flex-start; gap: 2px; padding-right: 8px; border-bottom: 1px solid var(--sep); transition: background 120ms var(--spring); }
  .rel:hover { background: var(--hover); }
  .rel.on { background: var(--selected); }
  .rmain { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; padding: 11px 6px 11px 16px; border: 0; background: none; text-align: left; color: inherit; }
  .rmain.static { padding-right: 8px; }
  .rel .tiny { width: 26px; height: 26px; margin-top: 8px; color: var(--text-2); flex: none; }
  .rname { font-weight: 600; font-size: 12px; }
  .path { font: 12px/1.35 var(--mono); color: var(--text); overflow-wrap: anywhere; }
  .arrow { color: var(--text-3); }
  .state { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--text-2); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--success); flex: none; }
  .dot.warn { background: var(--warn); }
  .dot.off { background: var(--text-3); }
  .note { font-size: 11px; color: var(--text-2); line-height: 1.4; }
  .acts { display: flex; gap: 6px; margin-top: 2px; }
  .btn.small { height: 24px; padding: 0 9px; font-size: 12px; }
  .empty-note { margin: 0; padding: 14px 16px; font-size: 12px; color: var(--text-2); line-height: 1.45; }
  .legend { display: flex; flex-direction: column; gap: 6px; padding: 12px 16px; border-top: 1px solid var(--sep); flex: none; font-size: 11px; color: var(--text-2); }
  .legend span { display: flex; align-items: center; gap: 8px; }
  .legend i { display: inline-block; width: 16px; height: 0; flex: none; border-top: 1.6px solid var(--text-2); }
  .legend i.dash { border-top: 1.6px dashed var(--accent); }
  .legend i.warn-line { border-top-color: #c47400; }
  .canvas { position: relative; flex: 1; min-width: 0; min-height: 0; }
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
  .canvas :global(.svelte-flow__controls) { box-shadow: none; border: 1px solid var(--sep); border-radius: 8px; overflow: hidden; background: var(--surface); }
  .canvas :global(.svelte-flow__controls-button) { width: 30px; height: 30px; border-bottom: 1px solid var(--sep); transition: background 120ms var(--spring); }
  .canvas :global(.svelte-flow__controls-button:last-child) { border-bottom: 0; }
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
    .rels { display: none; }
    .toolbar { padding: 0 10px 0 14px; gap: 8px; }
    .crumb.link, .crumbs > :global(svg) { display: none; }
    .bl { display: none; }
    .toolbar .btn { width: 32px; padding: 0; justify-content: center; }
    .ways { left: 12px; right: 12px; top: 12px; bottom: auto; max-width: none; }
  }
</style>
