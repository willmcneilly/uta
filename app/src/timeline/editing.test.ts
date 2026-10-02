import { describe, expect, it } from "vitest";
import type { ClipView } from "../backend";
import { trackView } from "../pianoRoll/testing";
import {
  EDGE_PIXELS,
  type Span,
  drawnClip,
  hitClip,
  moveClips,
  oneBarClip,
  resizeClip,
  rulerClick,
  rulerLoop,
} from "./editing";
import { RULER_HEIGHT, TRACK_HEIGHT, type TimelineViewport, tickToX, trackToY } from "./viewport";

const BAR = 3840;
const BEAT = 960;

// A bar is 100 px.
const view: TimelineViewport = {
  width: 1600,
  height: 400,
  scrollTicks: 0,
  scrollY: 0,
  pixelsPerTick: 100 / BAR,
};

const clip = (id: string, start: number, length: number): ClipView => ({
  id,
  start,
  length,
  notes: [],
});

/** The middle of track `track`'s row, at `tick`. */
const at = (tick: number, track: number) =>
  [tickToX(view, tick), trackToY(view, track) + TRACK_HEIGHT / 2] as const;

describe("hitting clips", () => {
  const tracks = [
    // Two overlapping clips: the one starting later is drawn on top.
    trackView("t1", "Synth 1", [clip("a", 0, 2 * BAR), clip("b", BAR, 2 * BAR)]),
    trackView("t2", "Synth 2", [clip("c", 4 * BAR, BAR)]),
  ];

  it("finds the clip under the pointer, on its own track", () => {
    expect(hitClip(view, tracks, ...at(BAR / 2, 0))).toMatchObject({ clip: { id: "a" }, track: 0 });
    expect(hitClip(view, tracks, ...at(4.5 * BAR, 1))).toMatchObject({
      clip: { id: "c" },
      track: 1,
    });
    expect(hitClip(view, tracks, ...at(4.5 * BAR, 0))).toBeNull();
    expect(hitClip(view, tracks, ...at(0.5 * BAR, 1))).toBeNull();
  });

  it("picks the later clip where two overlap, as it's drawn on top", () => {
    expect(hitClip(view, tracks, ...at(1.5 * BAR, 0))?.clip.id).toBe("b");
    // Past b's reach, a is still there.
    expect(hitClip(view, tracks, ...at(0.9 * BAR, 0))?.clip.id).toBe("a");
  });

  it("finds nothing above the first track or below the last", () => {
    expect(hitClip(view, tracks, tickToX(view, BAR / 2), RULER_HEIGHT - 1)).toBeNull();
    expect(hitClip(view, tracks, ...at(4.5 * BAR, 2))).toBeNull();
  });

  it("resizes from near the right edge and moves from the rest", () => {
    const [, y] = at(0, 1);
    const right = tickToX(view, 5 * BAR);
    expect(hitClip(view, tracks, right - 1, y)?.part).toBe("end");
    expect(hitClip(view, tracks, right - EDGE_PIXELS, y)?.part).toBe("end");
    expect(hitClip(view, tracks, right - EDGE_PIXELS - 1, y)?.part).toBe("body");
    expect(hitClip(view, tracks, right, y)).toBeNull();
  });

  it("keeps most of a narrow clip for moving it", () => {
    // 9 px wide: the right 3 px resize it.
    const narrow = [trackView("t1", "Synth 1", [clip("n", 0, (9 / 100) * BAR)])];
    const [, y] = at(0, 0);
    expect(hitClip(view, narrow, 5, y)?.part).toBe("body");
    expect(hitClip(view, narrow, 6.5, y)?.part).toBe("end");
  });

  it("uses the drawn edge when scrolled", () => {
    const scrolled = { ...view, scrollTicks: BAR, scrollY: 40 };
    const y = trackToY(scrolled, 1) + 10;
    expect(hitClip(scrolled, tracks, tickToX(scrolled, 4.5 * BAR), y)?.clip.id).toBe("c");
  });
});

describe("moving clips", () => {
  const from = { track: 0, start: BAR, length: 2 * BAR };
  const moveOne = (clip: Span, ...rest: [number, number, number, number, number, number]) =>
    moveClips([clip], clip, ...rest)[0];

  it("moves its start by as far as the pointer moves, snapped to the grid", () => {
    // Pressed half a bar in; moved 1.4 bars right snaps to a bar later.
    expect(moveOne(from, 1.5 * BAR, 0, 2.9 * BAR, 0, BAR, 3)).toEqual({
      track: 0,
      start: 2 * BAR,
      length: 2 * BAR,
    });
    expect(moveOne(from, 1.5 * BAR, 0, 2.2 * BAR, 0, BEAT, 3).start).toBe(BAR + 3 * BEAT);
    // Unsnapped: to the tick.
    expect(moveOne(from, 1.5 * BAR, 0, 1.5 * BAR + 123, 0, 1, 3).start).toBe(BAR + 123);
  });

  it("moves onto another track, keeping to the tracks there are", () => {
    expect(moveOne(from, BAR, 0, BAR, 2, BAR, 3).track).toBe(2);
    expect(moveOne(from, BAR, 0, BAR, 7, BAR, 3).track).toBe(2);
    expect(moveOne({ ...from, track: 1 }, BAR, 1, BAR, -3, BAR, 3).track).toBe(0);
  });

  it("never starts before the song", () => {
    expect(moveOne(from, BAR, 0, -5 * BAR, 0, BAR, 1).start).toBe(0);
  });

  describe("a group", () => {
    const group = [
      { id: "a", track: 1, start: 2 * BAR, length: BAR },
      { id: "b", track: 2, start: 3 * BAR + BEAT, length: BAR },
    ];
    const [a] = group;

    it("moves together, the pressed clip snapped and the rest kept where they are from it", () => {
      expect(moveClips(group, a, 2.5 * BAR, 1, 4.6 * BAR, 0, BAR, 4)).toEqual([
        { id: "a", track: 0, start: 4 * BAR, length: BAR },
        { id: "b", track: 1, start: 5 * BAR + BEAT, length: BAR },
      ]);
    });

    it("stops where its first clip reaches the song's start, or its clips the first or last track", () => {
      const left = moveClips(group, a, 2.5 * BAR, 1, -3 * BAR, 1, BAR, 4);
      expect(left.map((clip) => clip.start)).toEqual([0, BAR + BEAT]);
      expect(moveClips(group, a, 2 * BAR, 1, 2 * BAR, 9, BAR, 4).map((c) => c.track)).toEqual([
        2, 3,
      ]);
      expect(moveClips(group, a, 2 * BAR, 1, 2 * BAR, -4, BAR, 4).map((c) => c.track)).toEqual([
        0, 1,
      ]);
    });
  });
});

describe("resizing a clip", () => {
  const from = { track: 1, start: BAR, length: 2 * BAR };

  it("moves its end by as far as the pointer moves, snapped to the grid", () => {
    expect(resizeClip(from, 3 * BAR, 3.6 * BAR, BAR, BAR)).toEqual({
      track: 1,
      start: BAR,
      length: 3 * BAR,
    });
    expect(resizeClip(from, 3 * BAR, 3.3 * BAR, BEAT, BEAT).length).toBe(2 * BAR + BEAT);
  });

  it("is never shorter than the least it can be", () => {
    expect(resizeClip(from, 3 * BAR, -10 * BAR, BAR, BAR).length).toBe(BAR);
    expect(resizeClip(from, 3 * BAR, -10 * BAR, 1, 240).length).toBe(240);
  });
});

describe("drawing a clip", () => {
  it("spans the grid lines around where the drag went, either way", () => {
    expect(drawnClip(1, 1.2 * BAR, 3.5 * BAR, BAR, BAR)).toEqual({
      track: 1,
      start: BAR,
      length: 3 * BAR,
    });
    expect(drawnClip(1, 3.5 * BAR, 1.2 * BAR, BAR, BAR)).toEqual({
      track: 1,
      start: BAR,
      length: 3 * BAR,
    });
    expect(drawnClip(0, 1.2 * BAR, 1.3 * BAR, BEAT, BEAT)).toEqual({
      track: 0,
      start: BAR,
      length: 2 * BEAT,
    });
  });

  it("is at least the least a clip can be, and never before the song", () => {
    expect(drawnClip(0, BAR, BAR, BAR, BAR).length).toBe(BAR);
    expect(drawnClip(0, 100, 150, 1, 240)).toEqual({ track: 0, start: 100, length: 240 });
    expect(drawnClip(0, BAR, -BAR, BAR, BAR).start).toBe(0);
  });

  it("makes a one-bar clip at the grid line before a double-click", () => {
    expect(oneBarClip(2, 2.7 * BAR, BAR, BAR)).toEqual({ track: 2, start: 2 * BAR, length: BAR });
    expect(oneBarClip(0, 2.7 * BAR, BEAT, BAR)).toEqual({
      track: 0,
      start: 2 * BAR + 2 * BEAT,
      length: BAR,
    });
  });
});

describe("the ruler", () => {
  it("sets the loop between the nearest bar lines to each end of a drag", () => {
    expect(rulerLoop(1.1 * BAR, 3.8 * BAR, BAR)).toEqual({ startBar: 1, bars: 3 });
    // Either way round.
    expect(rulerLoop(3.8 * BAR, 1.1 * BAR, BAR)).toEqual({ startBar: 1, bars: 3 });
  });

  it("makes the loop at least a bar, in the direction of the drag", () => {
    expect(rulerLoop(2 * BAR, 2.3 * BAR, BAR)).toEqual({ startBar: 2, bars: 1 });
    expect(rulerLoop(2 * BAR, 1.8 * BAR, BAR)).toEqual({ startBar: 1, bars: 1 });
  });

  it("never sets the loop before the song's start", () => {
    expect(rulerLoop(0.2 * BAR, -3 * BAR, BAR)).toEqual({ startBar: 0, bars: 1 });
    expect(rulerLoop(2 * BAR, -3 * BAR, BAR)).toEqual({ startBar: 0, bars: 2 });
  });

  it("puts the play start on the nearest grid line to a click, never before 0", () => {
    expect(rulerClick(2.4 * BAR, BAR)).toBe(2 * BAR);
    expect(rulerClick(2.6 * BAR, BAR)).toBe(3 * BAR);
    expect(rulerClick(2 * BAR + 500, BEAT)).toBe(2 * BAR + BEAT);
    expect(rulerClick(1234, 1)).toBe(1234);
    expect(rulerClick(-50, BEAT)).toBe(0);
  });
});
