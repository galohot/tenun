<script lang="ts">
  // A KPI card (build 08, V4-D10): the parts of the card in order, top to bottom, on its ground. A part the tile cannot
  // fill takes no room (`drawable`); parts that do not fit the height drop (`fitParts`) and a "…" says so. A trend that
  // is the last part sits beside the words in a wide tile, as the number tile always had it. Every colour is a token
  // picked for the ground (`INKS`), so the contrast test covers it.
  import type { KpiInfo, Tile } from '../../../lib/api';
  import { compact, measureLabel, tip, toneFor, type Named } from '../../../lib/format';
  import { drawable, fitParts, INKS, ruleFor, ruleWords, sparkBeside, valuePx, type ResolvedCard } from '../../../lib/card';
  import type { Parts } from '../../../lib/measures';
  import Compare from './Compare.svelte';
  import RuleChip from './RuleChip.svelte';
  import Spark from './Spark.svelte';
  import Target from './Target.svelte';
  import { t, tn } from '../../../lib/i18n';

  let {
    kpi,
    tile,
    card,
    title = '',
    parts,
    tables = [],
  }: { kpi: KpiInfo; tile: Tile; card: ResolvedCard; title?: string; parts?: Parts; tables?: Named[] } = $props();

  let w = $state(300);
  let h = $state(120);
  const fmt = $derived(tile.formats?.[0] ?? null);
  const can = $derived(
    drawable(card.parts, { target: (tile.target?.value ?? 0) > 0, versus: !!kpi.versus, bars: kpi.bars.length, measures: tile.measures.length }),
  );
  const valueText = $derived(kpi.value === null ? '–' : compact(kpi.value, fmt));
  const fit = $derived(fitParts(can, h, card.size, w, valueText.length));
  const beside = $derived(sparkBeside(fit.shown, w));
  // Beside a trend the words sit at the bottom, as on the classic tile; a title that comes first stays on top.
  const head = $derived(beside && fit.shown[0]?.kind === 'title');
  const stack = $derived((beside ? fit.shown.slice(0, -1) : fit.shown).slice(head ? 1 : 0));
  const side = $derived(beside ? fit.shown[fit.shown.length - 1] : null);
  const ink = $derived(INKS[card.ground]);
  const inks = $derived(
    `--ink: var(--${ink.text}); --ink-2: var(--${ink.muted}); --ink-good: var(--${ink.good}); --ink-bad: var(--${ink.bad}); --ink-warn: var(--${ink.warn}); --ink-mark: var(--${ink.mark}); --ink-soft: var(--${ink.soft});`,
  );
  const rule = $derived(ruleFor(card.rules, kpi.value));
  // On the accent ground the value keeps the on-accent colour: the rule's word says it.
  // A rule colours the number; without one, a format that marks the value does (red negatives, [Red], [Green]).
  const formatTone = $derived(kpi.value === null ? null : toneFor(kpi.value, fmt));
  const valueInk = $derived(card.ground === 'accent' ? undefined : rule ? `var(--ink-${rule.tone})` : formatTone ? `var(--ink-${formatTone})` : undefined);
  const size = $derived(valuePx(card.size, w, valueText.length));
  const extra = (i: number) => {
    const v = kpi.extras?.[i - 1] ?? null;
    return { v, text: v === null ? '–' : compact(v, tile.formats?.[i] ?? null), tip: v === null ? '' : tip(v, tile.formats?.[i] ?? null) };
  };
  const extraName = (i: number, label?: string) => label?.trim() || (tile.measures[i] ? measureLabel(tile.measures[i], tables) : '');
</script>

<div class="card" class:center={card.align === 'center'} style={inks} bind:clientWidth={w} bind:clientHeight={h}>
  {#if head}<div class="title" title={title}>{title}</div>{/if}
  <div class="main" class:beside>
    <div class="stack">
      {#each stack as p, i (i)}
        {#if p.kind === 'title'}
          <div class="title" title={title}>{title}</div>
        {:else if p.kind === 'value'}
          <div class="valuerow">
            <span class="value" style:font-size="{size}px" style:color={valueInk} title={kpi.value === null ? '' : tip(kpi.value, fmt)}>
              {valueText}
            </span>
            {#if rule}<RuleChip tone={rule.tone} words={ruleWords(rule, (v) => compact(v, fmt), tile.target?.value)} />{/if}
          </div>
        {:else if p.kind === 'period'}
          <div class="period">{kpi.partial ? t('{period} · so far', { period: kpi.period }) : kpi.period}</div>
        {:else if p.kind === 'compare'}
          <Compare {kpi} better={tile.better ?? 'up'} />
        {:else if p.kind === 'target'}
          <Target {kpi} {tile} {parts} narrow={w < 200} />
        {:else if p.kind === 'spark'}
          <Spark {kpi} kind={p.style ?? 'bars'} {fmt} />
        {:else if p.kind === 'extra'}
          {@const e = extra(p.measure)}
          <div class="extra" title={e.tip}>
            <span class="name">{extraName(p.measure, p.label)}</span>
            <span class="num">{e.text}</span>
          </div>
        {:else if p.kind === 'note'}
          <div class="note" title={p.text}>{p.text}</div>
        {:else if p.kind === 'divider'}
          <hr />
        {/if}
      {/each}
    </div>
    {#if side?.kind === 'spark'}<Spark {kpi} kind={side.style ?? 'bars'} {fmt} />{/if}
  </div>
  {#if fit.dropped}
    <span class="more" title={tn('One part does not fit. Make the tile taller to see it.', '{n} parts do not fit. Make the tile taller to see them.', fit.dropped)}>…</span>
  {/if}
</div>

<style>
  .card { position: relative; height: 100%; display: flex; flex-direction: column; min-height: 0; color: var(--ink); }
  .main { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .main.beside { flex-direction: row; align-items: flex-end; gap: 16px; }
  .stack { display: flex; flex-direction: column; min-width: 0; flex: 1; min-height: 0; }
  .beside .stack { flex: none; max-width: 100%; justify-content: flex-end; }
  .center .stack { align-items: center; text-align: center; justify-content: center; }
  .title { font-size: 13px; font-weight: 600; line-height: 20px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .valuerow { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px; margin: 2px 0 6px; }
  .center .valuerow { justify-content: center; }
  .value { font-weight: 600; letter-spacing: -0.025em; line-height: 1; white-space: nowrap; font-variant-numeric: proportional-nums; }
  .period { font-size: 12px; font-weight: 500; line-height: 17px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .extra { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; font-size: 12px; line-height: 20px; max-width: 100%; }
  .center .extra { justify-content: center; }
  .extra .name { color: var(--ink-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .extra .num { font-weight: 600; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .note { font-size: 12px; line-height: 17px; color: var(--ink-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  hr { width: 100%; margin: 5px 0; border: 0; border-top: 1px solid var(--ink-soft); }
  .stack > :global(.spark) { margin-top: 4px; }
  .more { position: absolute; right: 0; bottom: -4px; font-size: 13px; line-height: 1; color: var(--ink-2); cursor: default; }
</style>
