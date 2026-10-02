// Editing clips with the pointer: what's under it, and where a drag puts a
// clip. Pure functions; the component turns their results into commands.

import type { ClipView, TrackView } from "../backend";
import { snapDown, snapNearest } from "../pianoRoll/snap";
import { type TimelineViewport, tickToX, yToTrack } from "./viewport";

/** Which part of a clip the pointer is on: its body moves it, its right edge resizes it. */
export type ClipPart = "body" | "end";

export interface ClipHit {
  clip: ClipView;
  /** The index of the clip's track in the order. */
  track: number;
  part: ClipPart;
}

/** How far into a clip from its right edge the pointer resizes it rather than moving it. */
export const EDGE_PIXELS = 6;

/**
 * The clip under `x`, `y`, and which part of it. Each track's clips are in
 * drawing order (by start, then ID), so where clips overlap, the one that
 * starts later is on top, and wins.
 */
export function hitClip(
  view: TimelineViewport,
  tracks: readonly TrackView[],
  x: number,
  y: number,
): ClipHit | null {
  const track = yToTrack(view, y);
  const clips = tracks[track]?.clips;
  if (!clips) return null;
  for (let i = clips.length - 1; i >= 0; i--) {
    const clip = clips[i];
    const left = tickToX(view, clip.start);
    // As drawn: never narrower than a pixel.
    const width = Math.max(1, clip.length * view.pixelsPerTick);
    if (x < left || x >= left + width) continue;
    // A narrow clip keeps two thirds to move it by.
    const edge = Math.min(EDGE_PIXELS, width / 3);
    return { clip, track, part: x >= left + width - edge ? "end" : "body" };
  }
  return null;
}

/** A clip's place: its track's index in the order, where it starts and how long it is, in ticks. */
export interface Span {
  track: number;
  start: number;
  length: number;
}

/**
 * Where a move puts a clip that was at `from` when the pointer went down at
 * `pressTick` on track `pressTrack`, with the pointer now at `tick` on
 * `track`. Its start snaps to multiples of `step` (1 with snapping off), and
 * it stays on one of the `trackCount` tracks, at or after the song's start.
 */
export function moveClip(
  from: Span,
  pressTick: number,
  pressTrack: number,
  tick: number,
  track: number,
  step: number,
  trackCount: number,
): Span {
  return {
    track: clamp(from.track + track - pressTrack, 0, trackCount - 1),
    start: Math.max(0, snapNearest(from.start + tick - pressTick, step)),
    length: from.length,
  };
}

/**
 * Where dragging the right edge of a clip that was at `from` puts it, with
 * the pointer moved from `pressTick` to `tick`. Its end snaps to multiples of
 * `step`, and it's never shorter than `minLength`.
 */
export function resizeClip(
  from: Span,
  pressTick: number,
  tick: number,
  step: number,
  minLength: number,
): Span {
  const end = snapNearest(from.start + from.length + tick - pressTick, step);
  return { ...from, length: Math.max(minLength, end - from.start) };
}

/**
 * The clip drawn by dragging across empty space on `track` from `pressTick`
 * to `tick`, either way: from the grid line at or before the earlier to the
 * one at or after the later, and at least `minLength` long.
 */
export function drawnClip(
  track: number,
  pressTick: number,
  tick: number,
  step: number,
  minLength: number,
): Span {
  const start = Math.max(0, snapDown(Math.min(pressTick, tick), step));
  const end = Math.ceil(Math.max(pressTick, tick) / step) * step;
  return { track, start, length: Math.max(minLength, end - start) };
}

/** A one-bar clip at the grid line at or before `tick`: a double-click on empty space. */
export function oneBarClip(track: number, tick: number, step: number, bar: number): Span {
  return { track, start: Math.max(0, snapDown(tick, step)), length: bar };
}

export function sameSpan(a: Span, b: Span): boolean {
  return a.track === b.track && a.start === b.start && a.length === b.length;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** A loop region in whole bars: the bar it starts at, counting from 0, and how many long. */
export interface LoopBars {
  startBar: number;
  bars: number;
}

/**
 * The loop region a drag on the ruler sets, from the tick it was pressed at
 * to the tick it's at now, either way round. Both ends snap to the nearest
 * bar line, and it's always at least a bar long, never before the song's
 * start.
 */
export function rulerLoop(pressTick: number, nowTick: number, bar: number): LoopBars {
  const from = Math.max(0, Math.round(pressTick / bar));
  const to = Math.max(0, Math.round(nowTick / bar));
  if (to === from) {
    // Less than half a bar either way: a bar in the direction of the drag.
    const back = nowTick < pressTick && from > 0;
    return { startBar: back ? from - 1 : from, bars: 1 };
  }
  return { startBar: Math.min(from, to), bars: Math.abs(to - from) };
}

/** Where a click on the ruler puts the play start: the nearest step of the grid, never before 0. */
export function rulerClick(tick: number, step: number): number {
  return Math.max(0, snapNearest(tick, step));
}
