// The maths behind the heat map: which step of the ramp a value falls on, and which ink reads on it.
// Pure functions with no imports at run time, so the node tests can load this file on its own.

/** The lowest and highest number of a grid of cells; null when no cell has a number. */
export function heatRange(values: readonly (number | null | undefined)[]): { lo: number; hi: number } | null {
  const nums = values.filter((v): v is number => typeof v === 'number' && Number.isFinite(v));
  if (!nums.length) return null;
  return { lo: Math.min(...nums), hi: Math.max(...nums) };
}

/**
 * The step of the ramp, 0 to `steps - 1`, for a value between `lo` and `hi`. The steps are equal
 * slices of the range, so the same colour always means the same size of number. When every cell has
 * the same number there is nothing to compare, and it takes the middle step.
 */
export function heatStep(v: number, lo: number, hi: number, steps: number): number {
  if (!(hi > lo)) return Math.floor((steps - 1) / 2);
  const at = Math.floor(((v - lo) / (hi - lo)) * steps);
  return Math.max(0, Math.min(steps - 1, at));
}

function luminance(hex: string): number {
  const h = hex.trim().replace('#', '');
  const full = h.length === 3 ? [...h].map((c) => c + c).join('') : h;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The WCAG contrast ratio of two #rrggbb colours. */
export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/** Ink for the number printed on a cell: the one of the two with the higher contrast. */
export function inkFor(bg: string, light = '#ffffff', dark = '#1c1c1e'): string {
  return contrast(light, bg) >= contrast(dark, bg) ? light : dark;
}
