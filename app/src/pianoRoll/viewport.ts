// Where the piano roll is looking, and the maths between ticks, rows,
// pitches and pixels. All sizes are CSS pixels. Pure functions, so they're
// easy to test. The fixed sizes come from DESIGN.md.

import { spacing } from "../design/tokens";

/** The keyboard's width, down the left. */
export const KEYBOARD_WIDTH = spacing.keyboardWidth;
/** The bar and beat ruler's height, across the top. */
export const RULER_HEIGHT = spacing.rulerHeight;
/** The velocity lane's height, along the bottom, under the notes. */
export const VELOCITY_LANE_HEIGHT = spacing.velocityLaneHeight;
/** Space above and below the velocity lane's tallest and shortest bars. */
const VELOCITY_LANE_PADDING = 6;
/** MIDI notes 0 to 127. */
export const PITCH_COUNT = 128;
/**
 * The column of drum lane labels, wide enough for "Closed hat" in the label
 * type with room either side (provisional: D-23).
 */
export const LANE_LABELS_WIDTH = 88;

/** One drum lane: a sound of the kit, its name, and the note that plays it. */
export interface Lane {
  name: string;
  pitch: number;
}

/**
 * The piano roll's rows, from the bottom up: one for every pitch beside the
 * keyboard, or one for each sound of a drum kit beside its labels (drum
 * lanes, RFC-006, "In the window"). Row 0 is at the bottom. Moving a note up
 * or down moves it a row, so on a drum track it goes from one sound to the
 * next.
 */
export interface Rows {
  readonly count: number;
  /** The column down the left: the keys, or the lanes' labels. */
  readonly width: number;
  /**
   * The drum lanes, bottom to top, or `null` beside the keyboard. Lanes
   * always share out the height between them, so all of them show without
   * scrolling, and don't zoom.
   */
  readonly lanes: readonly Lane[] | null;
  /** The pitch on `row`. */
  pitch(row: number): number;
  /** The row `pitch` is on, or -1 if no row has it. Rust only lets a drum track have notes on its lanes. */
  row(pitch: number): number;
}

/** Every pitch, beside the keyboard. */
export const KEYBOARD: Rows = {
  count: PITCH_COUNT,
  width: KEYBOARD_WIDTH,
  lanes: null,
  pitch: (row) => row,
  row: (pitch) => pitch,
};

/** A drum kit's lanes, bottom to top, beside their labels. */
export function drumLanes(lanes: readonly Lane[]): Rows {
  return {
    count: lanes.length,
    width: LANE_LABELS_WIDTH,
    lanes,
    pitch: (row) => lanes[row].pitch,
    row: (pitch) => lanes.findIndex((lane) => lane.pitch === pitch),
  };
}

export const MIN_KEY_HEIGHT = 4;
export const MAX_KEY_HEIGHT = 40;
/** At 960 ticks a quarter: a 16-bar loop fits in about 600 px. */
export const MIN_PIXELS_PER_TICK = 0.01;
/** A sixteenth note is 480 px wide. */
export const MAX_PIXELS_PER_TICK = 2;

export interface Viewport {
  /** The whole piano roll's size, keyboard and ruler included. */
  width: number;
  height: number;
  /** The tick at the left edge of the notes. */
  scrollTicks: number;
  /** How far the notes are scrolled down, in pixels from the top row's top edge. */
  scrollY: number;
  pixelsPerTick: number;
  /** The height of one row. */
  keyHeight: number;
  /** What each row is: a pitch beside the keyboard, or a drum lane. */
  rows: Rows;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * The area the notes are drawn in: right of the keyboard (or the lanes'
 * labels), between the ruler and the velocity lane.
 */
export function noteArea(view: Viewport): Rect {
  const left = view.rows.width;
  return {
    x: left,
    y: RULER_HEIGHT,
    width: Math.max(0, view.width - left),
    height: Math.max(0, view.height - RULER_HEIGHT - VELOCITY_LANE_HEIGHT),
  };
}

/** The velocity lane: under the notes, as wide as them. */
export function velocityLane(view: Viewport): Rect {
  const notes = noteArea(view);
  const y = notes.y + notes.height;
  return { x: notes.x, y, width: notes.width, height: Math.max(0, view.height - y) };
}

/** Whether `x`, `y` is inside `rect`. */
export function inRect(rect: Rect, x: number, y: number): boolean {
  return x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height;
}

/** The top of a velocity bar: 127 near the lane's top, 0 near its bottom. */
export function velocityToY(view: Viewport, velocity: number): number {
  const lane = velocityLane(view);
  const bottom = lane.y + lane.height - VELOCITY_LANE_PADDING;
  const range = Math.max(0, lane.height - 2 * VELOCITY_LANE_PADDING);
  return bottom - (velocity / 127) * range;
}

/** How many velocity steps a pointer movement of one pixel up the lane is. */
export function velocityPerPixel(view: Viewport): number {
  return 127 / Math.max(1, velocityLane(view).height - 2 * VELOCITY_LANE_PADDING);
}

export function tickToX(view: Viewport, ticks: number): number {
  return view.rows.width + (ticks - view.scrollTicks) * view.pixelsPerTick;
}

export function xToTick(view: Viewport, x: number): number {
  return view.scrollTicks + (x - view.rows.width) / view.pixelsPerTick;
}

/** The top edge of `row`. Higher rows are further up. */
export function rowToY(view: Viewport, row: number): number {
  return RULER_HEIGHT + (view.rows.count - 1 - row) * view.keyHeight - view.scrollY;
}

/** The row that contains `y`, which may be past the top or bottom row. */
export function yToRow(view: Viewport, y: number): number {
  return view.rows.count - 1 - Math.floor((y - RULER_HEIGHT + view.scrollY) / view.keyHeight);
}

/** The top edge of `pitch`'s row. */
export function pitchToY(view: Viewport, pitch: number): number {
  return rowToY(view, view.rows.row(pitch));
}

/** The pitch of the row that contains `y`, or of the nearest row if it's past them. */
export function yToPitch(view: Viewport, y: number): number {
  return view.rows.pitch(clamp(yToRow(view, y), 0, view.rows.count - 1));
}

/** The ticks in view: from `start` (inclusive) to `end` (exclusive). */
export function visibleTicks(view: Viewport): { start: number; end: number } {
  const start = view.scrollTicks;
  return { start, end: start + noteArea(view).width / view.pixelsPerTick };
}

/** The lowest and highest rows with any part of them in view. */
export function visibleRows(view: Viewport): { low: number; high: number } {
  const area = noteArea(view);
  const high = yToRow(view, area.y);
  // The bottom edge itself belongs to the row below, which isn't in view.
  const low = yToRow(view, area.y + area.height - 1e-6);
  return { low: Math.max(0, low), high: Math.min(view.rows.count - 1, high) };
}

/**
 * Keeps the view inside the piano roll: zoom within its limits, no scrolling
 * above the top row or below the bottom one, and horizontally from 0 to
 * `contentTicks`. Drum lanes share out the height, so all of them show.
 */
export function clampViewport(view: Viewport, contentTicks: number): Viewport {
  const pixelsPerTick = clamp(view.pixelsPerTick, MIN_PIXELS_PER_TICK, MAX_PIXELS_PER_TICK);
  const area = noteArea(view);
  const { count, lanes } = view.rows;
  const keyHeight = lanes
    ? Math.max(1, area.height) / count
    : clamp(view.keyHeight, MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
  const maxScrollTicks = Math.max(0, contentTicks - area.width / pixelsPerTick);
  const maxScrollY = Math.max(0, count * keyHeight - area.height);
  return {
    ...view,
    pixelsPerTick,
    keyHeight,
    scrollTicks: clamp(view.scrollTicks, 0, maxScrollTicks),
    scrollY: clamp(view.scrollY, 0, maxScrollY),
  };
}

/** Zooms time by `factor`, keeping the tick under `x` where it is. */
export function zoomTime(view: Viewport, factor: number, x: number): Viewport {
  const anchor = xToTick(view, x);
  const pixelsPerTick = clamp(
    view.pixelsPerTick * factor,
    MIN_PIXELS_PER_TICK,
    MAX_PIXELS_PER_TICK,
  );
  return { ...view, pixelsPerTick, scrollTicks: anchor - (x - view.rows.width) / pixelsPerTick };
}

/**
 * Zooms pitch by `factor`, keeping the point under `y` where it is. Drum
 * lanes always fit the height, so they don't zoom.
 */
export function zoomPitch(view: Viewport, factor: number, y: number): Viewport {
  if (view.rows.lanes) return view;
  const keyHeight = clamp(view.keyHeight * factor, MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
  // How far down the whole keyboard `y` is, as a fraction of a row.
  const rows = (y - RULER_HEIGHT + view.scrollY) / view.keyHeight;
  return { ...view, keyHeight, scrollY: rows * keyHeight - (y - RULER_HEIGHT) };
}

/**
 * The spacing of the finest grid lines worth drawing: sixteenths, eighths,
 * beats or bars, whichever is the first at least `minPixels` apart. Beyond
 * that, whole numbers of bars.
 */
export function gridStep(
  pixelsPerTick: number,
  ticksPerQuarter: number,
  beatsPerBar: number,
  minPixels = 8,
): number {
  const bar = ticksPerQuarter * beatsPerBar;
  for (const step of [ticksPerQuarter / 4, ticksPerQuarter / 2, ticksPerQuarter, bar]) {
    if (step * pixelsPerTick >= minPixels) return step;
  }
  return bar * Math.ceil(minPixels / (bar * pixelsPerTick));
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
