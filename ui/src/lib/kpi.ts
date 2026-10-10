// The target line of a number tile: how far the value is from the goal, and for a period that is
// not over yet, whether it is on pace. Pure code with no imports, so the node tests can load it.

export interface TargetInfo {
  /** The value as a share of the target: 0.82 for 82 %. */
  share: number;
  /** The unfinished period only: where the value is heading if the rest of the period goes like the part so far. */
  projected: number | null;
  /** On track, or behind. For a number where lower is better, "on track" means at or under the target. */
  onTrack: boolean;
}

/**
 * `elapsed` is the share of the period that has data (0–1). Below 1 the period is unfinished, and
 * the pace is value ÷ elapsed. Without it (a finished period, or "All data") the value itself is
 * compared. A target of 0 or less has no meaning, so it gives nothing.
 */
export function targetInfo(
  value: number | null,
  target: number | null | undefined,
  elapsed: number | null | undefined,
  better: 'up' | 'down' = 'up',
): TargetInfo | null {
  if (value === null || !Number.isFinite(value) || !target || !Number.isFinite(target) || target <= 0) return null;
  const unfinished = typeof elapsed === 'number' && elapsed > 0 && elapsed < 1;
  const projected = unfinished ? value / (elapsed as number) : null;
  const basis = projected ?? value;
  return { share: value / target, projected, onTrack: better === 'down' ? basis <= target : basis >= target };
}
