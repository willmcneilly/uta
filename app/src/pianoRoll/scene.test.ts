import { beforeEach, describe, expect, it } from "vitest";
import type { NoteView, ProjectView } from "../backend";
import { PianoRollScene, sameClip } from "./scene";
import { RecordingRenderer, firstClip, projectView, trackView } from "./testing";
import {
  KEYBOARD_WIDTH,
  RULER_HEIGHT,
  pitchToY,
  tickToX,
  velocityLane,
} from "./viewport";

const note = (id: string, start: number, pitch = 72): NoteView => ({
  id,
  pitch,
  velocity: 100,
  start,
  length: 480,
});

let scene: PianoRollScene;
let renderer: RecordingRenderer;

/** Shows `project`'s first clip, as the piano roll does in a new project. */
const show = (project: ProjectView) => scene.setProject(project, firstClip(project));

beforeEach(() => {
  scene = new PianoRollScene();
  renderer = new RecordingRenderer();
  show(projectView({}, [note("a", 0), note("b", 7680)]));
  scene.setRenderer(renderer);
  scene.resize(KEYBOARD_WIDTH + 768, RULER_HEIGHT + 480, 2);
});

describe("PianoRollScene", () => {
  it("opens with the whole clip in view", () => {
    const view = scene.getView()!;
    expect(view.pixelsPerTick).toBe(768 / (4 * 3840));
    expect(view.scrollTicks).toBe(0);
  });

  it("draws every layer at first, then only the playhead as it moves", () => {
    scene.draw(0);
    expect([renderer.grids.length, renderer.notes.length, renderer.tops.length]).toEqual([1, 1, 1]);
    scene.draw(0);
    expect(renderer.tops).toHaveLength(1);
    scene.draw(100);
    scene.draw(200);
    expect([renderer.grids.length, renderer.notes.length]).toEqual([1, 1]);
    expect(renderer.tops).toEqual([0, 100, 200]);
  });

  it("redraws the notes, but not the grid, when only the notes change", () => {
    scene.draw(0);
    show(projectView({}, [note("a", 0)]));
    scene.draw(0);
    expect(renderer.grids).toHaveLength(1);
    expect(renderer.notes).toHaveLength(2);
    expect(renderer.lastNotes().map((n) => n.id)).toEqual(["a"]);
  });

  it("doesn't redraw the notes when a new view from Rust has the same notes", () => {
    scene.draw(0);
    // Rust sends a fresh copy with every change, here for a volume change.
    const fresh = JSON.parse(
      JSON.stringify(projectView({ volumeDb: -20 }, [note("a", 0), note("b", 7680)])),
    ) as ReturnType<typeof projectView>;
    show(fresh);
    scene.draw(0);
    expect([renderer.grids.length, renderer.notes.length]).toEqual([1, 1]);
  });

  it("draws another clip's notes when it's shown instead", () => {
    scene.draw(0);
    const project = projectView({}, [note("a", 0)]);
    const other = { id: "clip-2", start: 0, length: 4 * 3840, notes: [note("c", 960, 60)] };
    const withTwo = { ...project, tracks: [...project.tracks, trackView("track-2", "Synth 2", [other])] };
    scene.setProject(withTwo, other);
    scene.draw(0);
    expect(renderer.lastNotes().map((n) => [n.id, n.pitch])).toEqual([["c", 60]]);
    // Back to the first: its notes again.
    show(withTwo);
    scene.draw(0);
    expect(renderer.lastNotes().map((n) => n.id)).toEqual(["a"]);
  });

  it("redraws the notes when a note changes in a new view from Rust", () => {
    scene.draw(0);
    show(projectView({}, [note("a", 0, 73), note("b", 7680)]));
    scene.draw(0);
    expect(renderer.notes).toHaveLength(2);
    expect(renderer.lastNotes().map((n) => n.pitch)).toEqual([73, 72]);
  });

  it("redraws the grid when the loop changes", () => {
    scene.draw(0);
    show(projectView({ loopLength: 2 * 3840 }));
    scene.draw(0);
    expect(renderer.grids).toHaveLength(2);
    expect(renderer.grids[1].loopEnd).toBe(2 * 3840);
  });

  it("redraws the grid when the loop is switched off", () => {
    scene.draw(0);
    expect(renderer.grids[0].loopEnabled).toBe(true);
    show(projectView({ loopEnabled: false }));
    scene.draw(0);
    expect(renderer.grids).toHaveLength(2);
    expect(renderer.grids[1].loopEnabled).toBe(false);
  });

  it("shades outside the clip, and redraws the grid when the clip moves or resizes", () => {
    scene.draw(0);
    expect([renderer.grids[0].clipStart, renderer.grids[0].clipEnd]).toEqual([0, 4 * 3840]);
    const project = projectView({}, [note("a", 0), note("b", 7680)]);
    const clip = firstClip(project);
    scene.setProject(project, { ...clip, start: 3840, length: 2 * 3840 });
    scene.draw(0);
    expect(renderer.grids).toHaveLength(2);
    expect([renderer.grids[1].clipStart, renderer.grids[1].clipEnd]).toEqual([3840, 3 * 3840]);
    // Moving the clip it shows doesn't move the view.
    expect(scene.getView()!.scrollTicks).toBe(0);
  });

  it("shows the whole of another clip when it's opened, keeping the pitches in view", () => {
    scene.changeView((view) => ({ ...view, scrollY: view.scrollY + 48 }));
    const scrollY = scene.getView()!.scrollY;
    const other = { id: "clip-2", start: 8 * 3840, length: 2 * 3840, notes: [note("c", 0)] };
    const project = projectView({
      songEnd: 11 * 3840,
      tracks: [trackView("track-1", "Synth 1", [firstClip(projectView()), other])],
    });
    scene.setProject(project, other);
    const view = scene.getView()!;
    expect(view.scrollTicks).toBe(8 * 3840);
    expect(view.pixelsPerTick).toBe(768 / (2 * 3840));
    expect(view.scrollY).toBe(scrollY);
    scene.draw(0);
    // In song time.
    expect(renderer.lastNotes().map((n) => [n.id, n.start])).toEqual([["c", 8 * 3840]]);
  });

  it("redraws the grid and notes on scroll", () => {
    scene.draw(0);
    scene.changeView((view) => ({ ...view, scrollY: view.scrollY + 24 }));
    scene.draw(0);
    expect([renderer.grids.length, renderer.notes.length, renderer.tops.length]).toEqual([2, 2, 2]);
  });

  it("draws only the notes in view", () => {
    scene.changeView((view) => ({ ...view, pixelsPerTick: view.pixelsPerTick * 2 }));
    scene.draw(0);
    // Two bars fit: the note at bar 3 is out of view.
    expect(renderer.lastNotes().map((n) => n.id)).toEqual(["a"]);
    scene.changeView((view) => ({ ...view, scrollTicks: 7680 }));
    scene.draw(0);
    expect(renderer.lastNotes().map((n) => n.id)).toEqual(["b"]);
  });

  it("draws only the pitches in view", () => {
    show(projectView({}, [note("high", 0, 80), note("low", 0, 40)]));
    scene.draw(0);
    // Opens with about C3 to C6 in view: pitch 40 (E2) is below it.
    expect(renderer.lastNotes().map((n) => n.id)).toEqual(["high"]);
    // Scroll down until pitch 80 is above the view and 40 is in it.
    scene.changeView((view) => ({ ...view, scrollY: (127 - 50) * view.keyHeight }));
    scene.draw(0);
    expect(renderer.lastNotes().map((n) => n.id)).toEqual(["low"]);
  });

  it("sizes a new renderer to the piano roll", () => {
    const next = new RecordingRenderer();
    scene.setRenderer(next);
    expect(next.sizes).toEqual([[KEYBOARD_WIDTH + 768, RULER_HEIGHT + 480]]);
    scene.draw(0);
    expect(next.grids).toHaveLength(1);
  });

  it("draws nothing without a renderer", () => {
    scene.setRenderer(null);
    expect(() => scene.draw(0)).not.toThrow();
  });

  it("redraws only the notes when the selection changes", () => {
    scene.draw(0);
    scene.setSelection(new Set(["b"]));
    scene.draw(0);
    scene.setSelection(new Set(["b"]));
    scene.draw(0);
    scene.setSelection(new Set(["a", "b"]));
    scene.draw(0);
    expect(renderer.grids).toHaveLength(1);
    expect(renderer.notes.map((drawn) => [...drawn.selected])).toEqual([[], ["b"], ["a", "b"]]);
  });

  it("finds the note under the pointer, only where notes are drawn", () => {
    const view = scene.getView()!;
    const y = pitchToY(view, 72) + view.keyHeight / 2;
    expect(scene.hitTest(tickToX(view, 240), y)?.note.id).toBe("a");
    expect(scene.hitTest(tickToX(view, 7680 + 240), y)?.note.id).toBe("b");
    expect(scene.hitTest(tickToX(view, 1000), y)).toBeNull();
    expect(scene.inNoteArea(KEYBOARD_WIDTH - 1, y)).toBe(false);
    expect(scene.inNoteArea(KEYBOARD_WIDTH, RULER_HEIGHT - 1)).toBe(false);
    expect(scene.inNoteArea(KEYBOARD_WIDTH, RULER_HEIGHT)).toBe(true);
  });

  it("finds the notes inside a box, only where notes are drawn", () => {
    const view = scene.getView()!;
    const row = (pitch: number) => pitchToY(view, pitch);
    // Around "a" (0–480 at 72), from above its row to below it.
    const box = (from: number, to: number, high: number, low: number) => ({
      x: tickToX(view, from),
      y: row(high),
      width: tickToX(view, to) - tickToX(view, from),
      height: row(low) + view.keyHeight - row(high),
    });
    expect(scene.notesIn(box(100, 200, 73, 71))).toEqual(["a"]);
    expect(scene.notesIn(box(500, 7000, 80, 60))).toEqual([]);
    expect(scene.notesIn(box(0, 8000, 72, 72))).toEqual(["a", "b"]);
    // A row above or below misses.
    expect(scene.notesIn(box(0, 8000, 74, 73))).toEqual([]);
    // Dragged out past the keyboard, into the ruler: kept to the notes.
    expect(scene.notesIn({ x: 0, y: 0, width: tickToX(view, 100), height: row(71) })).toEqual([
      "a",
    ]);
  });

  it("finds a note's velocity bar in the lane", () => {
    const view = scene.getView()!;
    const lane = velocityLane(view);
    expect(scene.inVelocityLane(KEYBOARD_WIDTH + 10, lane.y + 10)).toBe(true);
    expect(scene.inNoteArea(KEYBOARD_WIDTH + 10, lane.y + 10)).toBe(false);
    expect(scene.inVelocityLane(KEYBOARD_WIDTH + 10, lane.y - 1)).toBe(false);
    expect(scene.hitVelocity(tickToX(view, 7680) + 2)?.id).toBe("b");
    expect(scene.hitVelocity(tickToX(view, 3840))).toBeNull();
  });

  it("draws every note's velocity bar in view, at any pitch", () => {
    show(projectView({}, [note("a", 0), note("low", 960, 5)]));
    scene.draw(0);
    const drawn = renderer.notes.at(-1)!;
    expect(drawn.notes.map((n) => n.id)).toEqual(["a"]);
    expect(drawn.velocities.map((n) => n.id)).toEqual(["a", "low"]);
  });

  it("redraws only the top layer as the selection box moves", () => {
    scene.draw(0);
    const box = { x: 100, y: 100, width: 50, height: 40 };
    scene.setBox(box);
    scene.draw(0);
    scene.setBox(null);
    scene.draw(0);
    expect([renderer.grids.length, renderer.notes.length]).toEqual([1, 1]);
    expect(renderer.boxes).toEqual([null, box, null]);
  });
});

describe("PianoRollScene's sounding and hovered notes", () => {
  it("passes the notes under the playhead as sounding, only while playing", () => {
    scene.draw(100);
    expect(renderer.marks.at(-1)?.sounding).toEqual([]);
    scene.draw(100, true);
    expect(renderer.marks.at(-1)?.sounding.map((n) => n.id)).toEqual(["a"]);
    scene.draw(600, true);
    expect(renderer.marks.at(-1)?.sounding).toEqual([]);
    scene.draw(7700, true);
    expect(renderer.marks.at(-1)?.sounding.map((n) => n.id)).toEqual(["b"]);
  });

  it("clears the sounding notes when playback stops, even if the playhead doesn't move", () => {
    scene.draw(100, true);
    scene.draw(100, false);
    expect(renderer.tops).toEqual([100, 100]);
    expect(renderer.marks.at(-1)?.sounding).toEqual([]);
  });

  it("redraws only the top layer when the note under the pointer changes", () => {
    scene.draw(0);
    const view = scene.getView()!;
    const hit = scene.hitTest(tickToX(view, 100), pitchToY(view, 72) + 3);
    expect(hit?.note.id).toBe("a");
    scene.setHovered(hit!.note);
    scene.draw(0);
    scene.setHovered(hit!.note);
    scene.draw(0);
    scene.setHovered(null);
    scene.draw(0);
    expect([renderer.grids.length, renderer.notes.length]).toEqual([1, 1]);
    expect(renderer.marks.map((marks) => marks.hovered?.id ?? null)).toEqual([null, "a", null]);
  });

  it("forgets the note under the pointer when the notes change", () => {
    scene.draw(0);
    const view = scene.getView()!;
    scene.setHovered(scene.hitTest(tickToX(view, 100), pitchToY(view, 72) + 3)!.note);
    show(projectView({}, [note("a", 960), note("b", 7680)]));
    scene.draw(0);
    expect(renderer.marks.at(-1)?.hovered).toBeNull();
  });

  it("says when the clip has no notes", () => {
    scene.draw(0);
    expect(renderer.notes.at(-1)?.empty).toBe(false);
    show(projectView({}, []));
    scene.draw(0);
    expect(renderer.notes.at(-1)?.empty).toBe(true);
  });
});

describe("sameClip", () => {
  it("doesn't compare the notes one by one when they're the same array", () => {
    let reads = 0;
    const notes = new Proxy([note("a", 0), note("b", 480)], {
      get(target, key, receiver) {
        if (typeof key === "string" && /^\d+$/.test(key)) reads += 1;
        return Reflect.get(target, key, receiver) as unknown;
      },
    });
    const clip = { id: "c", start: 0, length: 3840, notes };
    expect(sameClip(clip, { ...clip })).toBe(true);
    expect(reads).toBe(0);
    expect(sameClip(clip, { ...clip, notes: [note("a", 0), note("b", 480)] })).toBe(true);
    expect(sameClip(clip, { ...clip, start: 960 })).toBe(false);
  });
});
