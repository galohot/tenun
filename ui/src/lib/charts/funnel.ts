// The numbers under a funnel: how wide each bar is, what share of the stage before reached the next,
// and what share of the first stage reached the last. This is arithmetic on the values on screen
// (like the waterfall steps), so it lives in the UI. Pure code with no imports at run time.

export interface FunnelShape {
  /** Bar width as a share of the biggest bar, 0 to 1. A stage with no value is 0. */
  widths: number[];
  /** Stage `i` as a share of stage `i - 1`. The first stage has none, nor has a stage after an empty one. */
  steps: (number | null)[];
  /** The last stage as a share of the first; null when the first is empty. */
  overall: number | null;
}

export function funnelShape(values: readonly (number | null)[]): FunnelShape {
  const v = values.map((x) => (typeof x === 'number' && Number.isFinite(x) ? Math.max(0, x) : 0));
  const top = Math.max(0, ...v);
  return {
    widths: v.map((x) => (top > 0 ? x / top : 0)),
    steps: v.map((x, i) => (i === 0 || v[i - 1] <= 0 ? null : x / v[i - 1])),
    overall: v.length > 1 && v[0] > 0 ? v[v.length - 1] / v[0] : null,
  };
}
