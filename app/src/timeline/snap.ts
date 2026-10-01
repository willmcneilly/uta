// The timeline's grid: what clip edits snap to. Pure functions.

import { gridStep } from "../pianoRoll/viewport";

/**
 * Snap to the grid lines on screen, to bars, to beats, or not at all. "grid"
 * follows the zoom, so a clip lands on any line you can see.
 */
export const CLIP_SNAPS = ["grid", "bar", "beat", "off"] as const;
export type ClipSnap = (typeof CLIP_SNAPS)[number];

/** Clips snap to the lines on screen when the app opens, as in Ableton. */
export const DEFAULT_CLIP_SNAP: ClipSnap = "grid";

/** Grid lines are drawn at least this far apart. */
export const MIN_GRID_PIXELS = 12;

/**
 * The spacing of the timeline's grid lines at `pixelsPerTick`: sixteenths,
 * eighths, beats or bars, whichever is the first at least
 * `MIN_GRID_PIXELS` apart, or whole numbers of bars beyond that.
 */
export function timelineGridStep(
  pixelsPerTick: number,
  ticksPerQuarter: number,
  beatsPerBar: number,
): number {
  return gridStep(pixelsPerTick, ticksPerQuarter, beatsPerBar, MIN_GRID_PIXELS);
}

/**
 * The grid's step in ticks, with the timeline zoomed to `pixelsPerTick`.
 * With snapping off it's 1, so positions still land on whole ticks.
 */
export function clipSnapStep(
  snap: ClipSnap,
  ticksPerQuarter: number,
  beatsPerBar: number,
  pixelsPerTick: number,
): number {
  switch (snap) {
    case "grid":
      return timelineGridStep(pixelsPerTick, ticksPerQuarter, beatsPerBar);
    case "bar":
      return ticksPerQuarter * beatsPerBar;
    case "beat":
      return ticksPerQuarter;
    case "off":
      return 1;
  }
}

/** The name of each choice in the Snap menu. */
export const CLIP_SNAP_NAMES: Record<ClipSnap, string> = {
  grid: "Grid",
  bar: "Bar",
  beat: "Beat",
  off: "Off",
};

/**
 * The shortest a clip can be drawn or resized to with grid step `step`: one
 * step, or a sixteenth with snapping off, so it never gets too thin to grab.
 */
export function minClipLength(step: number, ticksPerQuarter: number): number {
  return Math.max(step, ticksPerQuarter / 4);
}
