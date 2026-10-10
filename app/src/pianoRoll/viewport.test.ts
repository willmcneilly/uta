import { describe, expect, it } from "vitest";
import {
  KEYBOARD,
  KEYBOARD_WIDTH,
  MAX_KEY_HEIGHT,
  MIN_PIXELS_PER_TICK,
  RULER_HEIGHT,
  VELOCITY_LANE_HEIGHT,
  type Viewport,
  clampViewport,
  gridStep,
  noteArea,
  pitchToY,
  tickToX,
  velocityLane,
  velocityPerPixel,
  velocityToY,
  visibleRows,
  visibleTicks,
  xToTick,
  yToPitch,
  zoomPitch,
  zoomTime,
  LANE_LABELS_WIDTH,
  drumLanes,
  rowToY,
  yToRow,
} from "./viewport";

const view = (overrides: Partial<Viewport> = {}): Viewport => ({
  width: KEYBOARD_WIDTH + 800,
  // 240 px of notes.
  height: RULER_HEIGHT + 240 + VELOCITY_LANE_HEIGHT,
  scrollTicks: 0,
  scrollY: 0,
  pixelsPerTick: 0.1,
  keyHeight: 12,
  rows: KEYBOARD,
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
    expect(visibleRows(view())).toEqual({ low: 108, high: 127 });
    // Scrolled half a row: 21 rows partly show.
    expect(visibleRows(view({ scrollY: 6 }))).toEqual({ low: 107, high: 127 });
    // At the bottom.
    expect(visibleRows(view({ scrollY: 128 * 12 - 240 }))).toEqual({ low: 0, high: 19 });
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

describe("the velocity lane", () => {
  it("sits under the notes, as wide as them, to the bottom", () => {
    const v = view();
    expect(noteArea(v)).toEqual({ x: KEYBOARD_WIDTH, y: RULER_HEIGHT, width: 800, height: 240 });
    expect(velocityLane(v)).toEqual({
      x: KEYBOARD_WIDTH,
      y: RULER_HEIGHT + 240,
      width: 800,
      height: VELOCITY_LANE_HEIGHT,
    });
  });

  it("puts louder notes' bars higher, inside the lane", () => {
    const v = view();
    const lane = velocityLane(v);
    const top = velocityToY(v, 127);
    const bottom = velocityToY(v, 0);
    expect(top).toBeGreaterThan(lane.y);
    expect(bottom).toBeLessThan(lane.y + lane.height);
    expect(velocityToY(v, 127 / 2)).toBeCloseTo((top + bottom) / 2, 9);
    // Moving the pointer the lane's usable height covers the whole range.
    expect((bottom - top) * velocityPerPixel(v)).toBeCloseTo(127, 9);
  });
});

describe("drum lanes", () => {
  // As Rust sends a kit, bottom to top: the notes aren't in order.
  const kit = drumLanes([
    { name: "Kick", pitch: 36 },
    { name: "Snare", pitch: 38 },
    { name: "Low tom", pitch: 45 },
    { name: "Closed hat", pitch: 42 },
  ]);
  const lanes = (overrides: Partial<Viewport> = {}) =>
    clampViewport(view({ rows: kit, keyHeight: 12, ...overrides }), 1e6);

  it("map each lane to its sound's note, and back", () => {
    expect([0, 1, 2, 3].map((row) => kit.pitch(row))).toEqual([36, 38, 45, 42]);
    expect(kit.row(42)).toBe(3);
    expect(kit.row(37)).toBe(-1);
    expect(KEYBOARD.row(61)).toBe(61);
  });

  it("share out the height between them, so all show, and don't scroll or zoom", () => {
    const v = lanes({ scrollY: 300 });
    expect(v.keyHeight).toBe(240 / 4);
    expect(v.scrollY).toBe(0);
    expect(visibleRows(v)).toEqual({ low: 0, high: 3 });
    expect(zoomPitch(v, 2, RULER_HEIGHT + 100)).toBe(v);
  });

  it("put the first at the bottom and the last at the top", () => {
    const v = lanes();
    expect(rowToY(v, 3)).toBe(RULER_HEIGHT);
    expect(pitchToY(v, 42)).toBe(RULER_HEIGHT);
    expect(pitchToY(v, 36)).toBe(RULER_HEIGHT + 180);
    expect(yToPitch(v, RULER_HEIGHT + 70)).toBe(45);
    // Past the top and bottom lanes, the nearest.
    expect(yToPitch(v, -100)).toBe(42);
    expect(yToPitch(v, RULER_HEIGHT + 1000)).toBe(36);
    expect(yToRow(v, -100)).toBeGreaterThan(3);
  });

  it("make room for their labels in place of the keyboard", () => {
    const v = lanes();
    expect(noteArea(v).x).toBe(LANE_LABELS_WIDTH);
    expect(tickToX(v, 0)).toBe(LANE_LABELS_WIDTH);
    expect(xToTick(v, LANE_LABELS_WIDTH + 96)).toBe(960);
  });
});
