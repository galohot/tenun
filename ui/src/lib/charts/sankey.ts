// Sankey chart (Advanced): flows between 2 or 3 stages. The engine added up every link and folded the
// small nodes into "Other"; ECharts lays the nodes out. A band takes the colour of the node it starts
// at, from the palette in a fixed order. A click on a node filters by that node's field.

import type { Cell } from '../api';
import { cell, getLocale, numberFormatter, pctShare } from '../format';
import { esc, tooltipBase, type BuildArgs, type Built } from './common';
import { flowModel, type FlowNode } from './flow';
import { t } from '../i18n';

const OTHER_KEY = '\u0000other';

export function buildSankey({ res, tile, colors: c, selected, selectedStage, width, tables = [] }: BuildArgs): Built {
  const loc = getLocale();
  const fmt = numberFormatter(tile.formats?.[0], loc);
  const dtypeOf = (stage: number) => {
    const d = tile.dims[stage];
    return tables.find((t) => t.id === d?.table)?.columns?.find((x) => x.name === d?.column)?.dtype ?? 'text';
  };
  const name = (stage: number, raw: unknown) => cell(raw as Cell, dtypeOf(stage), tile.dims[stage]?.grain, true);
  const flow = flowModel(res.rows, name);
  const keys = new Set((selected ?? []).map((v) => (v === null ? '\u0000null' : String(v))));
  const lit = (n: FlowNode) => !selected || (selectedStage === n.stage && keys.has(n.key));
  const anySel = !!selected && flow.nodes.some(lit);
  const colourOf = (n: FlowNode) => (n.slot === null ? c.text2 : c.series[n.slot % c.series.length]);
  const byId = new Map(flow.nodes.map((n) => [n.id, n]));

  const room = Math.min(190, Math.max(80, width * 0.26));
  const tail = (n: FlowNode) => `  ${fmt.short(n.value)} · ${pctShare(n.share, loc)}`;
  // A long name is cut by itself, so its numbers always stay. A letter is about 6 px wide.
  const clip = (text: string, rest: string) => {
    const fit = Math.floor((room - 14) / 6.2) - rest.length;
    return text.length <= fit ? text : `${text.slice(0, Math.max(3, fit - 1)).trimEnd()}…`;
  };

  const nodes = flow.nodes.map((n) => ({
    name: n.id,
    depth: n.stage,
    value: n.value,
    itemStyle: { color: colourOf(n), opacity: anySel && !lit(n) ? 0.3 : 1 },
    label: { formatter: () => `${clip(name(n.stage, n.raw), tail(n))}${tail(n)}` },
  }));
  const links = flow.links.map((l) => {
    // With a node picked, only the bands that touch it keep their colour.
    const touches = !anySel || lit(byId.get(l.source)!) || lit(byId.get(l.target)!);
    return { source: l.source, target: l.target, value: l.value, lineStyle: { opacity: touches ? 0.4 : 0.08 } };
  });

  const option = {
    animationDuration: 350,
    tooltip: {
      ...tooltipBase(c),
      trigger: 'item',
      formatter: (p: { dataType: string; data: { name?: string; source?: string; target?: string; value: number } }) => {
        if (p.dataType === 'edge') {
          const [a, b] = [byId.get(p.data.source!), byId.get(p.data.target!)];
          const l = flow.links.find((x) => x.source === p.data.source && x.target === p.data.target);
          if (!a || !b) return '';
          return `<b>${esc(name(a.stage, a.raw))} → ${esc(name(b.stage, b.raw))}</b><br/>${esc(fmt.tip(p.data.value))}${l ? ` · ${esc(t('{share} of the total', { share: pctShare(l.share, loc) }))}` : ''}`;
        }
        const n = byId.get(p.data.name ?? '');
        return n ? `<b>${esc(name(n.stage, n.raw))}</b><br/>${esc(fmt.tip(n.value))} · ${esc(t('{share} of its stage', { share: pctShare(n.share, loc) }))}` : '';
      },
    },
    series: [
      {
        type: 'sankey',
        left: 8,
        right: room,
        top: 12,
        bottom: 12,
        nodeAlign: 'justify',
        nodeWidth: 14,
        nodeGap: 8,
        draggable: false,
        layoutIterations: 24,
        emphasis: { focus: 'adjacency' },
        lineStyle: { color: 'source', curveness: 0.5 },
        // A thin ring of the surface colour keeps the names readable where a band runs behind them.
        label: { color: c.text, fontSize: 11, position: 'right', overflow: 'truncate', width: room - 14, textBorderColor: c.surface, textBorderWidth: 4 },
        labelLayout: { hideOverlap: true },
        data: nodes,
        links,
      },
    ],
  };
  const note = res.other ? t('The smallest nodes of a stage are added up as Other.') : null;
  return {
    option,
    categories: [],
    pick: {
      by: 'item',
      // A node is a value of the field of its stage; a band is not a value.
      value: (_s, i, type) => {
        const n = type === 'node' || type === undefined ? flow.nodes[i] : undefined;
        return n && n.key !== OTHER_KEY ? (n.raw as Cell) : undefined;
      },
      stage: (_s, i, type) => (type === 'node' || type === undefined ? flow.nodes[i]?.stage : undefined),
    },
    note,
  };
}
