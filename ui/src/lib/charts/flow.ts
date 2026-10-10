// The nodes and links of a Sankey chart from the engine's rows `[stage, from, to, value]`. The engine
// folded the small nodes into "Other" and added up the links; this only names the nodes, gives each
// its colour slot and works out the shares for the labels. Pure code with no imports at run time.

const OTHER_KEY = '\u0000other';
const keyOf = (v: unknown): string => (v === null || v === undefined ? '\u0000null' : String(v));

export interface FlowNode {
  /** Unique in the chart: the stage and the value together, as one stage may repeat a name of another. */
  id: string;
  stage: number;
  /** The value as the engine sent it, for the filter a click makes. */
  raw: unknown;
  key: string;
  /** What passes through the node. */
  value: number;
  /** Its share of the stage. */
  share: number;
  /** The palette slot, or null for "Other", which is grey. */
  slot: number | null;
}

export interface FlowLink {
  source: string;
  target: string;
  value: number;
  /** The share of everything that starts in the first stage. */
  share: number;
}

export interface Flow {
  nodes: FlowNode[];
  links: FlowLink[];
  /** The number of stages that have nodes. */
  stages: number;
}

export const nodeId = (stage: number, raw: unknown): string => `${stage}\u0001${keyOf(raw)}`;

/**
 * Nodes are ordered by stage, then biggest first, with "Other" last. Colour slots follow the names in
 * A to Z order, counted over the whole chart, so a node keeps its colour when the numbers change.
 * `name` turns a raw value into the text shown.
 */
export function flowModel(rows: readonly unknown[][], name: (stage: number, raw: unknown) => string): Flow {
  const values = new Map<string, FlowNode>();
  const touch = (stage: number, raw: unknown, v: number, counts: boolean) => {
    const id = nodeId(stage, raw);
    const n = values.get(id) ?? { id, stage, raw, key: keyOf(raw), value: 0, share: 0, slot: null };
    if (counts) n.value += v;
    values.set(id, n);
  };
  const flows: { stage: number; from: unknown; to: unknown; v: number }[] = [];
  for (const r of rows) {
    const [stage, from, to, v] = [Number(r[0]), r[1], r[2], r[3]];
    if (!Number.isFinite(stage) || typeof v !== 'number' || !(v > 0)) continue;
    flows.push({ stage, from, to, v });
    // What leaves a first-stage node, what arrives at every other.
    touch(stage, from, v, stage === 0);
    touch(stage + 1, to, v, true);
  }
  const stages = values.size ? Math.max(...[...values.values()].map((n) => n.stage)) + 1 : 0;
  const totals = Array.from({ length: stages }, (_, s) => [...values.values()].filter((n) => n.stage === s).reduce((t, n) => t + n.value, 0));
  const nodes = [...values.values()].sort(
    (a, b) =>
      a.stage - b.stage ||
      Number(a.key === OTHER_KEY) - Number(b.key === OTHER_KEY) ||
      b.value - a.value ||
      name(a.stage, a.raw).localeCompare(name(b.stage, b.raw), undefined, { numeric: true }),
  );
  for (const n of nodes) n.share = totals[n.stage] > 0 ? n.value / totals[n.stage] : 0;
  [...nodes]
    .filter((n) => n.key !== OTHER_KEY)
    .sort((a, b) => name(a.stage, a.raw).localeCompare(name(b.stage, b.raw), undefined, { numeric: true }) || a.stage - b.stage)
    .forEach((n, i) => (n.slot = i));
  const top = totals[0] || 1;
  return {
    nodes,
    links: flows.map((f) => ({ source: nodeId(f.stage, f.from), target: nodeId(f.stage + 1, f.to), value: f.v, share: f.v / top })),
    stages,
  };
}
