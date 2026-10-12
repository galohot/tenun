<script lang="ts">
  // The "Card" section of a number tile (build 08, V4-D10): presets, the parts in their order, size, alignment, ground,
  // rules, and card styles. Every edit writes the field it changes; on a card with a style that field then overrides
  // the style's, and a dot says so. "Classic" is no card at all: the number tile as it always was.
  import type { CardPart, CardRule, CardSpec, CardStyle, KpiInfo, Tile } from '../../lib/api';
  import { app } from '../../lib/state.svelte';
  import { measureLabel } from '../../lib/format';
  import { defaultMeasure } from '../../lib/tiles';
  import { namedMeasure, targetUnit } from '../../lib/measures';
  import { asStyle, overrides, PRESETS, presetOf, resolveCard, styleOf, type CardField, type CardKind, type ResolvedCard } from '../../lib/card';
  import Icon from '../Icon.svelte';
  import FieldPicker from './FieldPicker.svelte';
  import KpiView from './KpiView.svelte';
  import CardView from './card/CardView.svelte';
  import CardStyles from './CardStyles.svelte';
  import type { Colors } from '../../lib/chart';

  let {
    tile,
    kpi,
    title,
    colors,
    reach = null,
    base = null,
    set,
  }: {
    tile: Tile;
    /** What the tile shows now, for the thumbnails. A made-up month when the tile has no result yet. */
    kpi: KpiInfo | null;
    title: string;
    colors: Colors;
    reach?: Set<string> | null;
    base?: string | null;
    set: (patch: Partial<Tile>) => void;
  } = $props();

  const SAMPLE: KpiInfo = {
    value: 128400,
    period: 'This month',
    versus: 'last month',
    prev: 117800,
    delta: 0.09,
    partial: false,
    bars: [62, 70, 66, 81, 77, 90, 86, 99, 94, 108, 118, 128].map((v, i) => ({ label: `Month ${i + 1}`, value: v * 1000 })),
  };
  const data = $derived(kpi ?? SAMPLE);
  const styles = $derived(app.snap?.workspace.cardStyles ?? []);
  const withTarget = $derived((tile.target?.value ?? 0) > 0);
  const card = $derived(resolveCard(tile.card, styles, withTarget));
  const style = $derived(styleOf(tile.card, styles));
  const over = $derived(new Set<CardField>(overrides(tile.card, styles)));
  const preset = $derived(tile.card ? (style ? null : presetOf(card, withTarget)) : 'classic');
  const unit = $derived(targetUnit(tile, 0));
  const typed = (v: number) => Math.round(v * unit.scale * 1e8) / 1e8;
  const read = (raw: string) => Number(raw.replace(/\s/g, '')) / unit.scale;

  const NAMES: Record<CardKind, string> = {
    title: 'Title',
    value: 'Value',
    period: 'Period',
    compare: 'Change',
    target: 'Target',
    spark: 'Trend',
    extra: 'Another value',
    note: 'Note',
    divider: 'Line',
  };
  /** Parts a card has once at most. */
  const ONCE: CardKind[] = ['title', 'value', 'period', 'compare', 'target', 'spark'];

  function setCard(patch: Partial<CardSpec>) {
    set({ card: { ...(tile.card ?? {}), ...patch } });
  }
  const setParts = (parts: CardPart[]) => setCard({ parts });
  const setRules = (rules: CardRule[]) => setCard({ rules });

  function pickPreset(id: string) {
    const p = PRESETS.find((x) => x.id === id);
    if (!p) return;
    // Classic is the number tile as it always was: no card.
    set({ card: id === 'classic' ? undefined : { ...p.card(withTarget) } });
  }
  const thumbCard = (id: string): ResolvedCard => resolveCard(PRESETS.find((x) => x.id === id)?.card(withTarget), [], withTarget);

  // Parts: drag to order, the eye hides a part and keeps its settings.
  let dragFrom = $state<number | null>(null);
  function move(from: number, to: number) {
    if (to < 0 || to >= card.parts.length || from === to) return;
    const list = [...card.parts];
    const [p] = list.splice(from, 1);
    list.splice(to, 0, p);
    setParts(list);
  }
  const update = (i: number, patch: Partial<CardPart>) => setParts(card.parts.map((p, j) => (j === i ? ({ ...p, ...patch } as CardPart) : p)));
  const remove = (i: number) => setParts(card.parts.filter((_, j) => j !== i));
  function toggle(i: number) {
    const { hidden, ...rest } = card.parts[i];
    setParts(card.parts.map((p, j) => (j === i ? (hidden ? (rest as CardPart) : ({ ...rest, hidden: true } as CardPart)) : p)));
  }
  const has = (k: CardKind) => card.parts.some((p) => p.kind === k);
  let adding = $state('');
  function addPart(kind: CardKind) {
    adding = '';
    const used = new Set(card.parts.flatMap((p) => (p.kind === 'extra' ? [p.measure] : [])));
    const free = tile.measures.findIndex((_, i) => i >= 1 && !used.has(i));
    const part: CardPart =
      kind === 'extra'
        ? { kind, measure: free >= 1 ? free : tile.measures.length }
        : kind === 'note'
          ? { kind, text: '' }
          : kind === 'spark'
            ? { kind, style: 'bars' }
            : ({ kind } as CardPart);
    setParts([...card.parts, part]);
  }
  /** A value the tile does not have yet: it is added to the tile, and the part shows it. */
  function addValue(i: number, m: Tile['measures'][number]) {
    const at = tile.measures.length;
    set({ measures: [...tile.measures, m], card: { ...(tile.card ?? {}), parts: card.parts.map((p, j) => (j === i ? ({ ...p, measure: at } as CardPart) : p)) } });
  }
  const namedList = $derived(app.snap?.workspace.measures ?? []);

  // Rules: the first that matches colours the value and adds its words.
  const addRule = () => setRules([...card.rules, { op: '<', value: tile.target?.value ?? 0, tone: 'bad' }]);
  const setRule = (i: number, patch: Partial<CardRule>) => setRules(card.rules.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const dropRule = (i: number) => setRules(card.rules.filter((_, j) => j !== i));

  // Card styles.
  let naming = $state<string | null>(null);
  let managing = $state(false);
  async function saveStyles(list: CardStyle[]) {
    await app.saveCardStyles(list);
  }
  async function saveAsStyle() {
    const name = (naming ?? '').trim();
    if (!name) return;
    const id = `cs-${Math.random().toString(36).slice(2, 10)}`;
    await saveStyles([...styles, { id, name, card: asStyle(card) }]);
    naming = null;
    set({ card: { style: id } });
  }
  const applyStyle = (id: string) => set({ card: { style: id } });
  async function updateStyle() {
    if (!style) return;
    await saveStyles(styles.map((s) => (s.id === style.id ? { ...s, card: asStyle(card) } : s)));
    set({ card: { style: style.id } });
  }
  const detach = () => set({ card: asStyle(card) });
  const reset = () => style && set({ card: { style: style.id } });
</script>

{#snippet dot(f: CardField)}
  {#if over.has(f)}<span class="dot" title="Set on this card. The style says otherwise."></span>{/if}
{/snippet}

<div class="sec cardsec">
  <span class="label">Card</span>
  <div class="presets" role="radiogroup" aria-label="Card presets">
    {#each PRESETS as p (p.id)}
      <button role="radio" aria-checked={preset === p.id} class:on={preset === p.id} title={p.name} onclick={() => pickPreset(p.id)}>
        <span class="thumb" aria-hidden="true">
          <span class="inner ground-{p.id === 'classic' ? 'plain' : thumbCard(p.id).ground}">
            {#if p.id === 'classic'}
              <KpiView kpi={data} {tile} {colors} />
            {:else}
              <CardView kpi={data} {tile} card={thumbCard(p.id)} {title} />
            {/if}
          </span>
        </span>
        <span class="name">{p.name}</span>
      </button>
    {/each}
  </div>

  <span class="label up">Card style</span>
  {#if style}
    <div class="stylerow">
      <span class="stylename" title={style.name}>{style.name}</span>
      {#if over.size}<button class="link" onclick={reset}>Reset to style</button>{/if}
    </div>
    <div class="row">
      <button class="btn" onclick={updateStyle} disabled={!over.size}>Update style from this card</button>
      <button class="btn plain" onclick={detach}>Detach</button>
    </div>
  {/if}
  {#if styles.length}
    <select class="field" aria-label="Apply a card style" value="" onchange={(e) => e.currentTarget.value && applyStyle(e.currentTarget.value)}>
      <option value="">{style ? 'Apply another style…' : 'Apply a style…'}</option>
      {#each styles.filter((s) => s.id !== style?.id) as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
    </select>
  {/if}
  {#if naming === null}
    <div class="row links">
      <button class="link" onclick={() => (naming = '')}>Save as card style…</button>
      {#if styles.length}<button class="link" onclick={() => (managing = true)}>Manage card styles…</button>{/if}
    </div>
  {:else}
    <form class="row" onsubmit={(e) => { e.preventDefault(); saveAsStyle(); }}>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" placeholder="Name of the style" aria-label="Name of the card style" bind:value={naming} autofocus maxlength="60" />
      <button class="btn primary" type="submit" disabled={!naming.trim()}>Save</button>
      <button class="btn plain" type="button" onclick={() => (naming = null)}>Cancel</button>
    </form>
  {/if}

  <span class="label up">Parts {@render dot('parts')}</span>
  <ul class="order" aria-label="Parts of the card, top to bottom">
    {#each card.parts as p, i (i)}
      <li
        class:over={dragFrom !== null && dragFrom !== i}
        class:off={p.hidden}
        draggable="true"
        ondragstart={() => (dragFrom = i)}
        ondragover={(e) => e.preventDefault()}
        ondrop={(e) => {
          e.preventDefault();
          if (dragFrom !== null) move(dragFrom, i);
          dragFrom = null;
        }}
        ondragend={() => (dragFrom = null)}>
        <div class="head">
          <Icon name="grip" size={14} />
          <span class="pname">{NAMES[p.kind]}</span>
          <button class="btn plain icon" aria-label="{p.hidden ? 'Show' : 'Hide'} {NAMES[p.kind]}" aria-pressed={!p.hidden} title={p.hidden ? 'Show' : 'Hide'} onclick={() => toggle(i)}>
            <Icon name={p.hidden ? 'eyeOff' : 'eye'} size={13} />
          </button>
          <button class="btn plain icon" aria-label="Move {NAMES[p.kind]} up" disabled={i === 0} onclick={() => move(i, i - 1)}><Icon name="chevronDown" size={12} class="flip" /></button>
          <button class="btn plain icon" aria-label="Move {NAMES[p.kind]} down" disabled={i === card.parts.length - 1} onclick={() => move(i, i + 1)}><Icon name="chevronDown" size={12} /></button>
          {#if p.kind !== 'value'}
            <button class="btn plain icon" aria-label="Remove {NAMES[p.kind]}" title="Remove" onclick={() => remove(i)}><Icon name="x" size={12} /></button>
          {/if}
        </div>
        {#if p.kind === 'spark'}
          <div class="grains stack" role="radiogroup" aria-label="Trend as">
            {#each [['bars', 'Columns'], ['line', 'Line']] as [id, label]}
              <button role="radio" aria-checked={(p.style ?? 'bars') === id} class:on={(p.style ?? 'bars') === id} onclick={() => update(i, { style: id as 'bars' | 'line' })}>{label}</button>
            {/each}
          </div>
        {:else if p.kind === 'target' && !withTarget}
          <p class="hint tight">Set a target above to show this part.</p>
        {:else if p.kind === 'note'}
          <input class="field" placeholder="One line of text" aria-label="Text of the note" maxlength="140" value={p.text} onchange={(e) => update(i, { text: e.currentTarget.value })} />
        {:else if p.kind === 'extra'}
          {#if p.measure >= 1 && p.measure < tile.measures.length}
            <select class="field" aria-label="Which value" value={String(p.measure)} onchange={(e) => update(i, { measure: Number(e.currentTarget.value) })}>
              {#each tile.measures as m, k (k)}
                {#if k >= 1}<option value={String(k)}>{measureLabel(m, app.tables)}</option>{/if}
              {/each}
            </select>
            <input class="field" placeholder="Label (optional)" aria-label="Label of the value" maxlength="40" value={p.label ?? ''} onchange={(e) => update(i, { label: e.currentTarget.value.trim() || undefined })} />
          {:else}
            <FieldPicker
              mode="measure"
              table={null}
              column={null}
              placeholder="Pick the value to show"
              reachable={reach}
              {base}
              measures={namedList}
              onnamed={(id) => addValue(i, namedMeasure(id))}
              onchange={(t, c, v) => addValue(i, defaultMeasure(app.tables, t, c, v))} />
          {/if}
        {/if}
      </li>
    {/each}
  </ul>
  <select class="field" aria-label="Add a part" bind:value={adding} onchange={() => adding && addPart(adding as CardKind)}>
    <option value="">Add a part…</option>
    {#each Object.entries(NAMES) as [k, label] (k)}
      <option value={k} disabled={ONCE.includes(k as CardKind) && has(k as CardKind)}>{label}</option>
    {/each}
  </select>

  <span class="label up">Size {@render dot('size')}</span>
  <div class="grains stack" role="radiogroup" aria-label="Size of the value">
    {#each [['s', 'Small'], ['m', 'Medium'], ['l', 'Large']] as [id, label]}
      <button role="radio" aria-checked={card.size === id} class:on={card.size === id} onclick={() => setCard({ size: id as 's' | 'm' | 'l' })}>{label}</button>
    {/each}
  </div>
  <span class="label up">Alignment {@render dot('align')}</span>
  <div class="grains stack" role="radiogroup" aria-label="Alignment">
    {#each [['start', 'Left'], ['center', 'Centre']] as [id, label]}
      <button role="radio" aria-checked={card.align === id} class:on={card.align === id} onclick={() => setCard({ align: id as 'start' | 'center' })}>{label}</button>
    {/each}
  </div>
  <span class="label up">Ground {@render dot('ground')}</span>
  <div class="grains stack" role="radiogroup" aria-label="Ground of the card">
    {#each [['plain', 'Plain'], ['tint', 'Tint'], ['accent', 'Accent']] as [id, label]}
      <button role="radio" aria-checked={card.ground === id} class:on={card.ground === id} onclick={() => setCard({ ground: id as 'plain' | 'tint' | 'accent' })}>{label}</button>
    {/each}
  </div>

  <span class="label up">Rules {@render dot('rules')}</span>
  {#each card.rules as r, i (i)}
    <div class="rule">
      <select class="field" aria-label="When the value is" value={r.op} onchange={(e) => setRule(i, { op: e.currentTarget.value as CardRule['op'] })}>
        <option value="<">Below</option>
        <option value=">">Above</option>
        <option value="between">Between</option>
      </select>
      <span class="withunit">
        <input class="field" type="number" step="any" aria-label="Value{unit.unit ? ` in ${unit.unit}` : ''}" value={typed(r.value)} onchange={(e) => Number.isFinite(read(e.currentTarget.value)) && setRule(i, { value: read(e.currentTarget.value) })} />
        {#if r.op === 'between'}
          <span class="unit">and</span>
          <input class="field" type="number" step="any" aria-label="Upper value{unit.unit ? ` in ${unit.unit}` : ''}" value={r.to === undefined ? '' : typed(r.to)} onchange={(e) => Number.isFinite(read(e.currentTarget.value)) && setRule(i, { to: read(e.currentTarget.value) })} />
        {/if}
        {#if unit.unit}<span class="unit" aria-hidden="true">{unit.unit}</span>{/if}
      </span>
      <select class="field" aria-label="Then the card shows" value={r.tone} onchange={(e) => setRule(i, { tone: e.currentTarget.value as CardRule['tone'] })}>
        <option value="good">Good</option>
        <option value="warn">Warning</option>
        <option value="bad">Bad</option>
      </select>
      <button class="btn plain icon" aria-label="Remove this rule" onclick={() => dropRule(i)}><Icon name="x" size={12} /></button>
    </div>
  {/each}
  <button class="link" onclick={addRule}>Add a rule</button>
  {#if card.rules.length}<p class="hint tight">The first rule the value meets colours it and adds its words, like "Below target".</p>{/if}
</div>

{#if managing}
  <CardStyles kpi={data} {tile} onclose={() => (managing = false)} />
{/if}

<style>
  .sec { display: flex; flex-direction: column; gap: 6px; }
  .label.up { margin-top: 6px; display: flex; align-items: center; gap: 6px; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent-text); }
  .presets { display: grid; grid-template-columns: repeat(2, 1fr); gap: 6px; }
  .presets button { display: flex; flex-direction: column; gap: 4px; padding: 4px; border: 1px solid var(--sep); border-radius: 8px; background: var(--surface); font-size: 11px; color: var(--text-2); }
  .presets button:hover { background: var(--hover); }
  .presets button.on { border-color: var(--accent); color: var(--accent-text); }
  .thumb { display: block; width: 100%; aspect-ratio: 2 / 1; overflow: hidden; border-radius: 5px; pointer-events: none; position: relative; }
  /* The card is laid out at twice the size and drawn at half, so it looks as it does on a dashboard. */
  .inner { position: absolute; left: 0; top: 0; width: 200%; height: 200%; transform: scale(0.5); transform-origin: 0 0; padding: 12px 14px 10px; box-sizing: border-box; }
  .ground-plain { background: var(--surface); box-shadow: inset 0 0 0 1px var(--sep); }
  .ground-tint { background: var(--tint); }
  .ground-accent { background: var(--accent); }
  .name { text-align: left; padding: 0 2px; }
  .stylerow { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 12px; }
  .stylename { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row { display: flex; gap: 6px; align-items: center; }
  .row .field { flex: 1; min-width: 0; }
  .row.links { gap: 14px; }
  .link { align-self: flex-start; border: 0; background: none; padding: 0; font-size: 12px; color: var(--accent-text); cursor: pointer; }
  .link:hover { text-decoration: underline; }
  .order { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .order li { display: flex; flex-direction: column; gap: 4px; padding: 2px 4px 4px; border-radius: 6px; background: var(--inset); cursor: grab; font-size: 12px; }
  .order li.over { outline: 1px dashed var(--accent); }
  .order li.off .pname { color: var(--text-2); text-decoration: line-through; }
  .head { display: flex; align-items: center; gap: 4px; }
  .pname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .order .btn.icon { width: 22px; height: 22px; }
  .order :global(.flip) { transform: rotate(180deg); }
  .grains { display: flex; padding: 2px; border-radius: 8px; background: var(--inset); }
  .order .grains { background: var(--surface); }
  .grains button { flex: 1; height: 24px; border: 0; border-radius: 6px; background: none; color: var(--text-2); font-size: 11px; padding: 0 4px; }
  .grains button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 12%); }
  .order .grains button.on { background: var(--inset); }
  .hint { margin: 0; font-size: 11px; line-height: 1.4; color: var(--text-2); }
  select.field { height: 30px; }
  .rule { display: grid; grid-template-columns: 84px 1fr 84px 22px; gap: 4px; align-items: center; }
  .rule .btn.icon { width: 22px; height: 22px; }
  .withunit { display: flex; align-items: center; gap: 4px; min-width: 0; }
  .withunit .field { flex: 1; min-width: 0; }
  .unit { color: var(--text-2); font-size: 12px; }
</style>
