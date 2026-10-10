import { describe, expect, it } from "vitest";
import type { NoteView } from "../backend";
import {
  type Drag,
  EDGE_PIXELS,
  dragNote,
  dragVelocities,
  hitTest,
  hitVelocity,
  moveNotes,
  resizeNotes,
  sameNote,
  transposeNotes,
} from "./editing";
import type { PlacedNote } from "./notes";
import {
  KEYBOARD_WIDTH,
  RULER_HEIGHT,
  type Viewport,
  pitchToY,
  tickToX,
  velocityPerPixel,
} from "./viewport";

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

describe("moving several notes", () => {
  const n = (id: string, pitch: number, start: number): NoteView => ({
    id,
    pitch,
    velocity: 100,
    start,
    length: 240,
  });
  const group = [n("a", 60, 480), n("b", 64, 960), n("c", 67, 1920)];
  // Dragging "b" from where it was pressed.
  const drag: Drag = { kind: "move", from: group[1], tick: 1000, pitch: 64 };

  it("moves them all by the dragged note's snapped move", () => {
    // "b" goes from 960 to 1300, and snaps to 1200: 240 later, 2 up.
    expect(moveNotes(group, drag, 1340, 66, 240, 0)).toEqual([
      n("a", 62, 720),
      n("b", 66, 1200),
      n("c", 69, 2160),
    ]);
  });

  it("stops the move where the earliest would pass the clip's start", () => {
    const moved = moveNotes(group, drag, 0, 64, 240, 0);
    expect(moved.map((note) => note.start)).toEqual([0, 480, 1440]);
  });

  it("stops the move where any would go off the keyboard", () => {
    expect(moveNotes(group, drag, 1000, 127, 240, 0).map((note) => note.pitch)).toEqual([
      120, 124, 127,
    ]);
    expect(moveNotes(group, drag, 1000, 0, 240, 0).map((note) => note.pitch)).toEqual([0, 4, 7]);
  });

  it("moves one note as dragging it alone does", () => {
    const one: Drag = { kind: "move", from: group[0], tick: 500, pitch: 60 };
    for (const [tick, pitch] of [
      [900, 62],
      [-500, 60],
      [500, 200],
    ]) {
      expect(moveNotes([group[0]], one, tick, pitch, 240, 3840)).toEqual([
        dragNote(one, tick, pitch, 240, 3840),
      ]);
    }
  });
});

describe("resizing several notes", () => {
  const n = (id: string, start: number, length: number): NoteView => ({
    id,
    pitch: 60,
    velocity: 100,
    start,
    length,
  });
  // "b" is dragged. "c" is shorter than it, and "d" is shorter than a step.
  const group = [n("a", 480, 480), n("b", 1920, 960), n("c", 3840, 360), n("d", 4800, 120)];

  it("changes every length by the dragged note's snapped change, from the end", () => {
    // "b"'s end goes from 2880 to 3130, and snaps to 3120: 240 longer.
    const drag: Drag = { kind: "end", from: group[1], tick: 2870, pitch: 60 };
    expect(resizeNotes(group, drag, 3120, 240, 0)).toEqual([
      n("a", 480, 720),
      n("b", 1920, 1200),
      n("c", 3840, 600),
      n("d", 4800, 360),
    ]);
  });

  it("keeps each note at least a step long, or as long as it was if shorter", () => {
    // 720 shorter: "b" stops at one step, and so does "a"; "c" stops at one
    // step, and "d" stays as it was.
    const drag: Drag = { kind: "end", from: group[1], tick: 2870, pitch: 60 };
    expect(resizeNotes(group, drag, 2150, 240, 0)).toEqual([
      n("a", 480, 240),
      n("b", 1920, 240),
      n("c", 3840, 240),
      n("d", 4800, 120),
    ]);
  });

  it("moves every start by the same amount, from the start, keeping each end", () => {
    // "b"'s start goes from 1920 to 1690, and snaps to 1680: 240 longer.
    const drag: Drag = { kind: "start", from: group[1], tick: 1930, pitch: 60 };
    expect(resizeNotes(group, drag, 1700, 240, 0)).toEqual([
      n("a", 240, 720),
      n("b", 1680, 1200),
      n("c", 3600, 600),
      n("d", 4560, 360),
    ]);
  });

  it("never starts a note before its clip", () => {
    const drag: Drag = { kind: "start", from: group[1], tick: 1930, pitch: 60 };
    // 960 longer: "a" would start at -480.
    const resized = resizeNotes(group, drag, 970, 240, 0);
    expect(resized[0]).toEqual(n("a", 0, 960));
    expect(resized[1]).toEqual(n("b", 960, 1920));
  });

  it("resizes one note as dragging it alone does", () => {
    for (const kind of ["start", "end"] as const) {
      const one: Drag = { kind, from: group[1], tick: 2000, pitch: 60 };
      for (const tick of [100, 1500, 2600, 5000]) {
        expect(resizeNotes([group[1]], one, tick, 240, 0)).toEqual([
          dragNote(one, tick, 60, 240, 0),
        ]);
      }
    }
  });
});

describe("moving notes by pitch", () => {
  const n = (id: string, pitch: number): NoteView => ({
    id,
    pitch,
    velocity: 100,
    start: 0,
    length: 240,
  });
  const chord = [n("a", 60), n("b", 64), n("c", 67)];

  it("moves every note by the same number of semitones", () => {
    expect(transposeNotes(chord, 12)).toEqual([n("a", 72), n("b", 76), n("c", 79)]);
    expect(transposeNotes(chord, -1)).toEqual([n("a", 59), n("b", 63), n("c", 66)]);
  });

  it("stops where any note would go off the keyboard", () => {
    const high = [n("a", 120), n("b", 124)];
    expect(transposeNotes(high, 12).map((note) => note.pitch)).toEqual([123, 127]);
    const low = [n("a", 5), n("b", 9)];
    expect(transposeNotes(low, -12).map((note) => note.pitch)).toEqual([0, 4]);
  });
});

describe("velocity bars", () => {
  const chord = [placed("root", 60, 960, 480), placed("third", 64, 960, 480)];
  const x = tickToX(view, 960);

  it("finds the bar at a note's start, within a few pixels", () => {
    expect(hitVelocity(view, chord, new Set(), x + 3)?.id).toBe("third");
    expect(hitVelocity(view, chord, new Set(), x + 6)).toBeNull();
  });

  it("prefers a selected note where bars overlap", () => {
    expect(hitVelocity(view, chord, new Set(["root"]), x)?.id).toBe("root");
    expect(hitVelocity(view, chord, new Set(["root", "third"]), x)?.id).toBe("third");
  });

  it("changes every note's velocity by the same amount as the pointer moves", () => {
    const notes = [
      { id: "a", pitch: 60, velocity: 100, start: 0, length: 240 },
      { id: "b", pitch: 62, velocity: 40, start: 240, length: 240 },
    ];
    const pixels = 20 / velocityPerPixel(view);
    expect(dragVelocities(view, notes, pixels).map((note) => note.velocity)).toEqual([120, 60]);
    expect(dragVelocities(view, notes, -pixels).map((note) => note.velocity)).toEqual([80, 20]);
  });

  it("keeps velocities from 1 to 127", () => {
    const notes = [{ id: "a", pitch: 60, velocity: 100, start: 0, length: 240 }];
    expect(dragVelocities(view, notes, 1000)[0].velocity).toBe(127);
    expect(dragVelocities(view, notes, -1000)[0].velocity).toBe(1);
  });
});
