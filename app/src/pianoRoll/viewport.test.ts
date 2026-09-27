import { describe, expect, it } from "vitest";
import {
  KEYBOARD_WIDTH,
  MAX_KEY_HEIGHT,
  MIN_PIXELS_PER_TICK,
  RULER_HEIGHT,
  type Viewport,
  clampViewport,
  gridStep,
  pitchToY,
  tickToX,
  visiblePitches,
  visibleTicks,
  xToTick,
  yToPitch,
  zoomPitch,
  zoomTime,
} from "./viewport";

const view = (overrides: Partial<Viewport> = {}): Viewport => ({
  width: KEYBOARD_WIDTH + 800,
  height: RULER_HEIGHT + 240,
  scrollTicks: 0,
  scrollY: 0,
  pixelsPerTick: 0.1,
  keyHeight: 12,
  ...overrides,
});

describe("ticks and pixels", () => {
  it("puts tick 0 at the keyboard's edge and scales by pixels per tick", () => {
    expect(tickToX(view(), 0)).toBe(KEYBOARD_WIDTH);
    expect(tickToX(view(), 960)).toBe(KEYBOARD_WIDTH + 96);
    expect(tickToX(view({ scrollTicks: 960 }), 960)).toBe(KEYBOARD_WIDTH);
    expect(tickToX(view({ scrollTicks: 960 }), 0)).toBe(KEYBOARD_WIDTH - 96);
  });

  it("turns pixels back into the same ticks", () => {
    const v = view({ scrollTicks: 1234, pixelsPerTick: 0.37 });
    for (const ticks of [0, 1, 960, 7680, 61_440]) {
      expect(xToTick(v, tickToX(v, ticks))).toBeCloseTo(ticks, 9);
    }
  });

  it("puts pitch 127 at the top, under the ruler, and lower pitches further down", () => {
    expect(pitchToY(view(), 127)).toBe(RULER_HEIGHT);
    expect(pitchToY(view(), 126)).toBe(RULER_HEIGHT + 12);
    expect(pitchToY(view(), 0)).toBe(RULER_HEIGHT + 127 * 12);
    expect(pitchToY(view({ scrollY: 12 }), 126)).toBe(RULER_HEIGHT);
  });

  it("finds the pitch whose row holds a point", () => {
    const v = view({ scrollY: 500 });
    for (const pitch of [0, 59, 60, 127]) {
      const top = pitchToY(v, pitch);
      expect(yToPitch(v, top)).toBe(pitch);
      expect(yToPitch(v, top + 11.9)).toBe(pitch);
    }
  });
});

describe("what's in view", () => {
  it("covers the note area's width in ticks", () => {
    expect(visibleTicks(view({ scrollTicks: 480 }))).toEqual({ start: 480, end: 480 + 8000 });
  });

  it("covers every row that shows, even partly", () => {
    // 240 px of 12 px rows from the top: pitches 127 down to 108.
    expect(visiblePitches(view())).toEqual({ low: 108, high: 127 });
    // Scrolled half a row: 21 rows partly show.
    expect(visiblePitches(view({ scrollY: 6 }))).toEqual({ low: 107, high: 127 });
    // At the bottom.
    expect(visiblePitches(view({ scrollY: 128 * 12 - 240 }))).toEqual({ low: 0, high: 19 });
  });
});

describe("clampViewport", () => {
  it("stops scrolling at the edges", () => {
    const content = 16 * 3840;
    const past = clampViewport(view({ scrollTicks: 1e9, scrollY: 1e9 }), content);
    expect(past.scrollTicks).toBe(content - 8000);
    expect(past.scrollY).toBe(128 * 12 - 240);
    const before = clampViewport(view({ scrollTicks: -50, scrollY: -50 }), content);
    expect([before.scrollTicks, before.scrollY]).toEqual([0, 0]);
  });

  it("doesn't scroll at all when everything fits", () => {
    expect(clampViewport(view({ scrollTicks: 100 }), 4000).scrollTicks).toBe(0);
  });

  it("keeps the zoom within its limits", () => {
    const clamped = clampViewport(view({ pixelsPerTick: 0, keyHeight: 1000 }), 1e6);
    expect(clamped.pixelsPerTick).toBe(MIN_PIXELS_PER_TICK);
    expect(clamped.keyHeight).toBe(MAX_KEY_HEIGHT);
  });
});

describe("zooming", () => {
  it("keeps the tick under the pointer in place", () => {
    const v = view({ scrollTicks: 3000 });
    const x = KEYBOARD_WIDTH + 300;
    const before = xToTick(v, x);
    const zoomed = zoomTime(v, 2, x);
    expect(zoomed.pixelsPerTick).toBe(0.2);
    expect(xToTick(zoomed, x)).toBeCloseTo(before, 9);
  });

  it("keeps the pitch under the pointer in place", () => {
    const v = view({ scrollY: 600 });
    const y = RULER_HEIGHT + 100;
    const zoomed = zoomPitch(v, 1.5, y);
    expect(zoomed.keyHeight).toBe(18);
    expect(yToPitch(zoomed, y)).toBe(yToPitch(v, y));
  });
});

describe("gridStep", () => {
  it("picks the finest of sixteenths, eighths, beats and bars that's far enough apart", () => {
    expect(gridStep(0.1, 960, 4)).toBe(240); // a sixteenth is 24 px
    expect(gridStep(0.02, 960, 4)).toBe(480); // a sixteenth is 4.8 px, an eighth 9.6
    expect(gridStep(0.01, 960, 4)).toBe(960); // an eighth is 4.8 px, a beat 9.6
    expect(gridStep(0.005, 960, 4)).toBe(3840); // a beat is 4.8 px, a bar 19.2
  });

  it("skips bars when even they're too close", () => {
    // A bar is 1.92 px: every 5th bar is 9.6 px.
    expect(gridStep(0.0005, 960, 4)).toBe(5 * 3840);
  });
});
