// Where the timeline is looking, and the maths between ticks, tracks and
// pixels. All sizes are CSS pixels. Pure functions, so they're easy to test.
// The fixed sizes come from DESIGN.md.

import { spacing } from "../design/tokens";

/** The bar ruler's height, across the top. The track headers leave the same space above them. */
export const RULER_HEIGHT = spacing.rulerHeight;
/** Each track's row, and its header, are this tall, so the two line up. */
export const TRACK_HEIGHT = spacing.trackHeight;
/** The space below the last track, where the headers have **+ Synth** and **+ Drums**. */
export const ADD_TRACK_HEIGHT = spacing.addTrackHeight;

/** At 960 ticks a quarter: 128 bars fit in about 1000 px. */
export const MIN_PIXELS_PER_TICK = 0.002;
/** A beat is 240 px wide. */
export const MAX_PIXELS_PER_TICK = 0.25;

export interface TimelineViewport {
  /** The whole timeline's size, ruler included. */
  width: number;
  height: number;
  /** The tick at the left edge. */
  scrollTicks: number;
  /** How far the tracks are scrolled down, in whole pixels. */
  scrollY: number;
  pixelsPerTick: number;
}

export function tickToX(view: TimelineViewport, ticks: number): number {
  return (ticks - view.scrollTicks) * view.pixelsPerTick;
}

export function xToTick(view: TimelineViewport, x: number): number {
  return view.scrollTicks + x / view.pixelsPerTick;
}

/** The top edge of the track at `index` in the order. */
export function trackToY(view: TimelineViewport, index: number): number {
  return RULER_HEIGHT + index * TRACK_HEIGHT - view.scrollY;
}

/**
 * The index of the track row containing `y`. It's outside `0` to the
 * track count when `y` is above the first track or below the last.
 */
export function yToTrack(view: TimelineViewport, y: number): number {
  return Math.floor((y - RULER_HEIGHT + view.scrollY) / TRACK_HEIGHT);
}

/** Whether `y` is below the ruler, where the tracks are. */
export function inTracks(view: TimelineViewport, y: number): boolean {
  return y >= RULER_HEIGHT && y < view.height;
}

/** The ticks in view: from `start` (inclusive) to `end` (exclusive). */
export function visibleTicks(view: TimelineViewport): { start: number; end: number } {
  const start = view.scrollTicks;
  return { start, end: start + view.width / view.pixelsPerTick };
}

/**
 * The first and last of `trackCount` tracks with any part of their row in
 * view. None are if `last < first`.
 */
export function visibleTracks(
  view: TimelineViewport,
  trackCount: number,
): { first: number; last: number } {
  const first = Math.max(0, yToTrack(view, RULER_HEIGHT));
  // The bottom edge itself belongs to the row below, which isn't in view.
  const last = Math.min(trackCount - 1, yToTrack(view, view.height - 1e-6));
  return { first, last };
}

/**
 * Keeps the view inside the timeline: zoom within its limits, horizontally
 * from 0 to `contentTicks`, and vertically as far as the space under the
 * last of `trackCount` tracks, as the headers scroll.
 */
export function clampViewport(
  view: TimelineViewport,
  contentTicks: number,
  trackCount: number,
): TimelineViewport {
  const pixelsPerTick = clamp(view.pixelsPerTick, MIN_PIXELS_PER_TICK, MAX_PIXELS_PER_TICK);
  const maxScrollTicks = Math.max(0, contentTicks - view.width / pixelsPerTick);
  const content = trackCount * TRACK_HEIGHT + ADD_TRACK_HEIGHT;
  const maxScrollY = Math.max(0, content - (view.height - RULER_HEIGHT));
  return {
    ...view,
    pixelsPerTick,
    scrollTicks: clamp(view.scrollTicks, 0, maxScrollTicks),
    // Whole pixels, as the headers' scrollTop is.
    scrollY: Math.round(clamp(view.scrollY, 0, maxScrollY)),
  };
}

/** Zooms time by `factor`, keeping the tick under `x` where it is. */
export function zoomTime(view: TimelineViewport, factor: number, x: number): TimelineViewport {
  const anchor = xToTick(view, x);
  const pixelsPerTick = clamp(
    view.pixelsPerTick * factor,
    MIN_PIXELS_PER_TICK,
    MAX_PIXELS_PER_TICK,
  );
  return { ...view, pixelsPerTick, scrollTicks: anchor - x / pixelsPerTick };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
