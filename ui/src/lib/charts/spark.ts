// A trend line in a table cell: 80 × 20 px, no axis, the last point as a dot. Each line has its own
// scale, from its lowest to its highest value, so every row shows its own shape. Pure code with no
// imports at run time.

export const SPARK_W = 80;
export const SPARK_H = 20;
const PAD = 3;

export interface Spark {
  /** The path of the line. A gap in the data breaks it into pieces. Empty when there is nothing to draw. */
  d: string;
  /** The last period that has a value, for the dot. */
  last: { x: number; y: number } | null;
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const r1 = (n: number) => Math.round(n * 10) / 10;

export function sparkPath(values: readonly (number | null)[], w = SPARK_W, h = SPARK_H): Spark {
  const nums = values.filter(finite);
  if (!nums.length) return { d: '', last: null };
  const lo = Math.min(...nums);
  const hi = Math.max(...nums);
  const x = (i: number) => (values.length < 2 ? w / 2 : PAD + (i * (w - 2 * PAD)) / (values.length - 1));
  // Every value the same: a flat line in the middle.
  const y = (v: number) => (hi > lo ? PAD + ((hi - v) / (hi - lo)) * (h - 2 * PAD) : h / 2);
  let d = '';
  let pen = false;
  let last: Spark['last'] = null;
  values.forEach((v, i) => {
    if (!finite(v)) {
      pen = false;
      return;
    }
    d += `${pen ? 'L' : 'M'}${r1(x(i))} ${r1(y(v))}`;
    pen = true;
    last = { x: r1(x(i)), y: r1(y(v)) };
  });
  return { d, last };
}

/** The key of a table row in `res.sparks`: the values of its groups joined by a unit separator. The engine builds the same key. */
export function sparkKey(groups: readonly unknown[]): string {
  return groups.map((v) => (v === null || v === undefined ? '\u0000null' : String(v))).join('\u001f');
}
