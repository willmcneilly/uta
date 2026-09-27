// Where the piano roll is looking, and the maths between ticks, pitches and
// pixels. All sizes are CSS pixels. Pure functions, so they're easy to test.

/** The keyboard's width, down the left. */
export const KEYBOARD_WIDTH = 56;
/** The bar and beat ruler's height, across the top. */
export const RULER_HEIGHT = 24;
/** MIDI notes 0 to 127. */
export const PITCH_COUNT = 128;

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
  /** How far the notes are scrolled down, in pixels from pitch 127's top edge. */
  scrollY: number;
  pixelsPerTick: number;
  /** The height of one pitch's row. */
  keyHeight: number;
}

/** The area the notes are drawn in: right of the keyboard, under the ruler. */
export function noteArea(view: Viewport): { x: number; y: number; width: number; height: number } {
  return {
    x: KEYBOARD_WIDTH,
    y: RULER_HEIGHT,
    width: Math.max(0, view.width - KEYBOARD_WIDTH),
    height: Math.max(0, view.height - RULER_HEIGHT),
  };
}

export function tickToX(view: Viewport, ticks: number): number {
  return KEYBOARD_WIDTH + (ticks - view.scrollTicks) * view.pixelsPerTick;
}

export function xToTick(view: Viewport, x: number): number {
  return view.scrollTicks + (x - KEYBOARD_WIDTH) / view.pixelsPerTick;
}

/** The top edge of `pitch`'s row. Higher pitches are further up. */
export function pitchToY(view: Viewport, pitch: number): number {
  return RULER_HEIGHT + (PITCH_COUNT - 1 - pitch) * view.keyHeight - view.scrollY;
}

/** The pitch whose row contains `y`. */
export function yToPitch(view: Viewport, y: number): number {
  return PITCH_COUNT - 1 - Math.floor((y - RULER_HEIGHT + view.scrollY) / view.keyHeight);
}

/** The ticks in view: from `start` (inclusive) to `end` (exclusive). */
export function visibleTicks(view: Viewport): { start: number; end: number } {
  const start = view.scrollTicks;
  return { start, end: start + noteArea(view).width / view.pixelsPerTick };
}

/** The lowest and highest pitches with any part of their row in view. */
export function visiblePitches(view: Viewport): { low: number; high: number } {
  const area = noteArea(view);
  const high = yToPitch(view, area.y);
  // The bottom edge itself belongs to the row below, which isn't in view.
  const low = yToPitch(view, area.y + area.height - 1e-6);
  return { low: Math.max(0, low), high: Math.min(PITCH_COUNT - 1, high) };
}

/**
 * Keeps the view inside the piano roll: zoom within its limits, no scrolling
 * above pitch 127 or below pitch 0, and horizontally from 0 to `contentTicks`.
 */
export function clampViewport(view: Viewport, contentTicks: number): Viewport {
  const pixelsPerTick = clamp(view.pixelsPerTick, MIN_PIXELS_PER_TICK, MAX_PIXELS_PER_TICK);
  const keyHeight = clamp(view.keyHeight, MIN_KEY_HEIGHT, MAX_KEY_HEIGHT);
  const area = noteArea(view);
  const maxScrollTicks = Math.max(0, contentTicks - area.width / pixelsPerTick);
  const maxScrollY = Math.max(0, PITCH_COUNT * keyHeight - area.height);
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
  return { ...view, pixelsPerTick, scrollTicks: anchor - (x - KEYBOARD_WIDTH) / pixelsPerTick };
}

/** Zooms pitch by `factor`, keeping the point under `y` where it is. */
export function zoomPitch(view: Viewport, factor: number, y: number): Viewport {
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
