// The piano roll's grid: which note lengths edits snap to. Pure functions.

/** The grids you can pick, as fractions of a whole note, or no snapping. */
export const SNAPS = ["1/4", "1/8", "1/16", "1/32", "off"] as const;
export type Snap = (typeof SNAPS)[number];

/** Snapping is on at 1/16 when the piano roll opens (RFC-002, open question 4). */
export const DEFAULT_SNAP: Snap = "1/16";

/**
 * The grid's step in ticks. With snapping off it's 1, so positions still land
 * on whole ticks.
 */
export function snapStep(snap: Snap, ticksPerQuarter: number): number {
  if (snap === "off") return 1;
  const division = Number(snap.slice(2));
  return (ticksPerQuarter * 4) / division;
}

/** The grid line nearest `ticks`. */
export function snapNearest(ticks: number, step: number): number {
  return Math.round(ticks / step) * step;
}

/** The grid line at or before `ticks`. */
export function snapDown(ticks: number, step: number): number {
  return Math.floor(ticks / step) * step;
}
