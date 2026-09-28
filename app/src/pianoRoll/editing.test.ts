import { describe, expect, it } from "vitest";
import type { NoteView } from "../backend";
import { type Drag, EDGE_PIXELS, dragNote, hitTest, sameNote } from "./editing";
import type { PlacedNote } from "./notes";
import { KEYBOARD_WIDTH, RULER_HEIGHT, type Viewport, pitchToY, tickToX } from "./viewport";

// 0.1 px a tick: a quarter note is 96 px, a sixteenth 24 px. Pitch 127's row
// is at the top.
const view: Viewport = {
  width: KEYBOARD_WIDTH + 800,
  height: RULER_HEIGHT + 480,
  scrollTicks: 0,
  scrollY: 0,
  pixelsPerTick: 0.1,
  keyHeight: 12,
};

const placed = (id: string, pitch: number, start: number, length: number): PlacedNote => ({
  id,
  pitch,
  velocity: 100,
  start,
  length,
  outside: false,
});

/** The middle of `pitch`'s row. */
const rowY = (pitch: number) => pitchToY(view, pitch) + view.keyHeight / 2;

describe("hit-testing", () => {
  const long = placed("long", 120, 960, 960); // 96 px wide, from x = 152
  const notes = [long];

  it("finds the note under the pointer", () => {
    expect(hitTest(view, notes, tickToX(view, 1440), rowY(120))?.note.id).toBe("long");
  });

  it("misses beside, above and below a note", () => {
    expect(hitTest(view, notes, tickToX(view, 959), rowY(120))).toBeNull();
    expect(hitTest(view, notes, tickToX(view, 1920), rowY(120))).toBeNull();
    expect(hitTest(view, notes, tickToX(view, 1440), rowY(121))).toBeNull();
    expect(hitTest(view, notes, tickToX(view, 1440), rowY(119))).toBeNull();
  });

  it("resizes near the ends and moves in between", () => {
    const left = tickToX(view, 960);
    const right = tickToX(view, 1920);
    const y = rowY(120);
    expect(hitTest(view, notes, left, y)?.part).toBe("start");
    expect(hitTest(view, notes, left + EDGE_PIXELS - 0.5, y)?.part).toBe("start");
    expect(hitTest(view, notes, left + EDGE_PIXELS, y)?.part).toBe("body");
    expect(hitTest(view, notes, right - EDGE_PIXELS - 0.5, y)?.part).toBe("body");
    expect(hitTest(view, notes, right - EDGE_PIXELS, y)?.part).toBe("end");
    expect(hitTest(view, notes, right - 0.5, y)?.part).toBe("end");
  });

  it("keeps the middle third of a narrow note for moving it", () => {
    // 9 px wide: 3 px at each end.
    const narrow = [placed("narrow", 120, 0, 90)];
    const left = tickToX(view, 0);
    expect(hitTest(view, narrow, left + 2, rowY(120))?.part).toBe("start");
    expect(hitTest(view, narrow, left + 4.5, rowY(120))?.part).toBe("body");
    expect(hitTest(view, narrow, left + 7, rowY(120))?.part).toBe("end");
  });

  it("picks the note drawn on top where notes overlap", () => {
    const under = placed("under", 120, 0, 1920);
    const over = placed("over", 120, 960, 480);
    expect(hitTest(view, [under, over], tickToX(view, 1200), rowY(120))?.note.id).toBe("over");
    expect(hitTest(view, [under, over], tickToX(view, 480), rowY(120))?.note.id).toBe("under");
  });

  it("can grab a note too short to be a pixel wide", () => {
    const tiny = [placed("tiny", 120, 960, 1)];
    expect(hitTest(view, tiny, tickToX(view, 960) + 0.5, rowY(120))?.note.id).toBe("tiny");
  });
});

describe("dragging a note", () => {
  const from: NoteView = { id: "n", pitch: 60, velocity: 90, start: 960, length: 480 };
  const drag = (kind: Drag["kind"], tick = 1000, pitch = 60): Drag => ({ kind, from, tick, pitch });

  it("moves in time, snapping the start to the nearest line", () => {
    expect(dragNote(drag("move"), 1000 + 100, 60, 240, 0)).toEqual({ ...from, start: 960 });
    expect(dragNote(drag("move"), 1000 + 130, 60, 240, 0)).toEqual({ ...from, start: 1200 });
    expect(dragNote(drag("move"), 1000 - 500, 60, 240, 0)).toEqual({ ...from, start: 480 });
  });

  it("moves in pitch, within MIDI's 0 to 127", () => {
    expect(dragNote(drag("move"), 1000, 67, 240, 0)).toEqual({ ...from, pitch: 67 });
    expect(dragNote(drag("move"), 1000, 200, 240, 0).pitch).toBe(127);
    expect(dragNote(drag("move"), 1000, -40, 240, 0).pitch).toBe(0);
  });

  it("never moves before the clip's start", () => {
    expect(dragNote(drag("move"), 1000 - 5000, 60, 240, 0).start).toBe(0);
    // A clip starting at bar 2: the note's start stays from the clip's start.
    expect(dragNote(drag("move"), 1000 - 5000, 60, 240, 3840).start).toBe(0);
    expect(dragNote(drag("move"), 1000 + 240, 60, 240, 3840).start).toBe(1200);
  });

  it("moves by whole ticks with snapping off", () => {
    expect(dragNote(drag("move"), 1000 + 100.4, 60, 1, 0)).toEqual({ ...from, start: 1060 });
  });

  it("resizes its end, keeping its start, never shorter than a step", () => {
    expect(dragNote(drag("end"), 1000 + 250, 60, 240, 0)).toEqual({ ...from, length: 720 });
    expect(dragNote(drag("end"), 1000 - 2000, 60, 240, 0)).toEqual({ ...from, length: 240 });
    expect(dragNote(drag("end"), 1000 - 2000, 60, 1, 0)).toEqual({ ...from, length: 1 });
  });

  it("resizes its start, keeping its end, never shorter than a step", () => {
    expect(dragNote(drag("start"), 1000 - 250, 60, 240, 0)).toEqual({
      ...from,
      start: 720,
      length: 720,
    });
    expect(dragNote(drag("start"), 1000 + 2000, 60, 240, 0)).toEqual({
      ...from,
      start: 1200,
      length: 240,
    });
    expect(dragNote(drag("start"), 1000 - 5000, 60, 240, 0)).toEqual({
      ...from,
      start: 0,
      length: 1440,
    });
  });

  it("ignores pitch while resizing", () => {
    expect(dragNote(drag("end"), 1000, 72, 240, 0).pitch).toBe(60);
    expect(dragNote(drag("start"), 1000, 72, 240, 0).pitch).toBe(60);
  });

  it("drags out a drawn note's length like its end", () => {
    const drawn: Drag = { kind: "draw", from: { ...from, length: 240 }, tick: 1000, pitch: 60 };
    expect(dragNote(drawn, 1000 + 500, 60, 240, 0).length).toBe(720);
    expect(dragNote(drawn, 1000 - 500, 60, 240, 0).length).toBe(240);
  });

  it("compares every value of two notes", () => {
    expect(sameNote(from, { ...from })).toBe(true);
    const changes = [{ id: "m" }, { pitch: 61 }, { velocity: 1 }, { start: 0 }, { length: 1 }];
    for (const change of changes) {
      expect(sameNote(from, { ...from, ...change })).toBe(false);
    }
  });
});
