// The timeline's grid: what clip edits snap to. Pure functions.

/** Bars, beats, or no snapping. */
export const CLIP_SNAPS = ["bar", "beat", "off"] as const;
export type ClipSnap = (typeof CLIP_SNAPS)[number];

/** Clips snap to bars when the app opens (RFC-003, "Clips on the timeline"). */
export const DEFAULT_CLIP_SNAP: ClipSnap = "bar";

/** The grid's step in ticks. With snapping off it's 1, so positions still land on whole ticks. */
export function clipSnapStep(snap: ClipSnap, ticksPerQuarter: number, beatsPerBar: number): number {
  switch (snap) {
    case "bar":
      return ticksPerQuarter * beatsPerBar;
    case "beat":
      return ticksPerQuarter;
    case "off":
      return 1;
  }
}

/**
 * The shortest a clip can be drawn or resized to with grid step `step`: one
 * step, or a sixteenth with snapping off, so it never gets too thin to grab.
 */
export function minClipLength(step: number, ticksPerQuarter: number): number {
  return Math.max(step, ticksPerQuarter / 4);
}
