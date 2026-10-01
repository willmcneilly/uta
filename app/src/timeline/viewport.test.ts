import { describe, expect, it } from "vitest";
import {
  ADD_TRACK_HEIGHT,
  MAX_PIXELS_PER_TICK,
  MIN_PIXELS_PER_TICK,
  RULER_HEIGHT,
  TRACK_HEIGHT,
  type TimelineViewport,
  clampViewport,
  inTracks,
  tickToX,
  trackToY,
  visibleTicks,
  visibleTracks,
  xToTick,
  yToTrack,
  zoomTime,
} from "./viewport";

// 800 px wide showing 16 bars: a bar is 50 px. 400 px tall: the ruler, then
// about four tracks.
const view: TimelineViewport = {
  width: 800,
  height: 400,
  scrollTicks: 0,
  scrollY: 0,
  pixelsPerTick: 800 / (16 * 3840),
};

describe("timeline viewport", () => {
  it("turns ticks into x and back", () => {
    expect(tickToX(view, 3840)).toBeCloseTo(50);
    const scrolled = { ...view, scrollTicks: 3840 };
    expect(tickToX(scrolled, 3840)).toBe(0);
    expect(xToTick(scrolled, 50)).toBeCloseTo(7680);
  });

  it("puts each track in a row under the ruler, and finds the row under y", () => {
    expect(trackToY(view, 0)).toBe(RULER_HEIGHT);
    expect(trackToY(view, 2)).toBe(RULER_HEIGHT + 2 * TRACK_HEIGHT);
    expect(yToTrack(view, RULER_HEIGHT)).toBe(0);
    expect(yToTrack(view, RULER_HEIGHT + TRACK_HEIGHT - 1)).toBe(0);
    expect(yToTrack(view, RULER_HEIGHT + TRACK_HEIGHT)).toBe(1);
    expect(yToTrack(view, 0)).toBe(-1);
    const scrolled = { ...view, scrollY: 50 };
    expect(trackToY(scrolled, 1)).toBe(RULER_HEIGHT + TRACK_HEIGHT - 50);
    expect(yToTrack(scrolled, RULER_HEIGHT + TRACK_HEIGHT - 50)).toBe(1);
  });

  it("says whether y is on the tracks or the ruler", () => {
    expect(inTracks(view, RULER_HEIGHT - 1)).toBe(false);
    expect(inTracks(view, RULER_HEIGHT)).toBe(true);
    expect(inTracks(view, 400)).toBe(false);
  });

  it("finds the ticks and the tracks in view", () => {
    expect(visibleTicks({ ...view, scrollTicks: 3840 })).toEqual({ start: 3840, end: 17 * 3840 });
    // 376 px of tracks: rows 0 to 3, the last partly.
    expect(visibleTracks(view, 10)).toEqual({ first: 0, last: 3 });
    expect(visibleTracks(view, 2)).toEqual({ first: 0, last: 1 });
    expect(visibleTracks({ ...view, scrollY: TRACK_HEIGHT + 1 }, 10)).toEqual({
      first: 1,
      last: 4,
    });
    const none = visibleTracks(view, 0);
    expect(none.last).toBeLessThan(none.first);
  });

  it("keeps the view inside the song, the tracks and the zoom limits", () => {
    const content = 20 * 3840;
    expect(clampViewport({ ...view, scrollTicks: -100 }, content, 2).scrollTicks).toBe(0);
    // 16 bars in view of 20: it scrolls at most 4 bars.
    expect(clampViewport({ ...view, scrollTicks: 1e9 }, content, 2).scrollTicks).toBeCloseTo(
      4 * 3840,
    );
    expect(clampViewport({ ...view, pixelsPerTick: 10 }, content, 2).pixelsPerTick).toBe(
      MAX_PIXELS_PER_TICK,
    );
    expect(clampViewport({ ...view, pixelsPerTick: 0 }, content, 2).pixelsPerTick).toBe(
      MIN_PIXELS_PER_TICK,
    );
  });

  it("scrolls down as far as the space under the last track, in whole pixels", () => {
    // 6 tracks and the add-track space, in 376 px.
    const most = 6 * TRACK_HEIGHT + ADD_TRACK_HEIGHT - (400 - RULER_HEIGHT);
    expect(clampViewport({ ...view, scrollY: 1e6 }, 1e6, 6).scrollY).toBe(most);
    expect(clampViewport({ ...view, scrollY: 1e6 }, 1e6, 2).scrollY).toBe(0);
    expect(clampViewport({ ...view, scrollY: 10.6 }, 1e6, 6).scrollY).toBe(11);
  });

  it("zooms time around the pointer", () => {
    const zoomed = zoomTime({ ...view, scrollTicks: 3840 }, 2, 200);
    expect(zoomed.pixelsPerTick).toBeCloseTo(view.pixelsPerTick * 2);
    expect(xToTick(zoomed, 200)).toBeCloseTo(xToTick({ ...view, scrollTicks: 3840 }, 200));
  });
});
