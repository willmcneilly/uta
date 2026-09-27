import { beforeEach, describe, expect, it } from "vitest";
import type { NoteView } from "../backend";
import { PianoRollScene } from "./scene";
import { RecordingRenderer, projectView } from "./testing";
import { KEYBOARD_WIDTH, RULER_HEIGHT } from "./viewport";

const note = (id: string, start: number, pitch = 72): NoteView => ({
  id,
  pitch,
  velocity: 100,
  start,
  length: 480,
});

let scene: PianoRollScene;
let renderer: RecordingRenderer;

beforeEach(() => {
  scene = new PianoRollScene();
  renderer = new RecordingRenderer();
  scene.setProject(projectView({}, [note("a", 0), note("b", 7680)]));
  scene.setRenderer(renderer);
  scene.resize(KEYBOARD_WIDTH + 768, RULER_HEIGHT + 480, 2);
});

describe("PianoRollScene", () => {
  it("opens with the whole loop in view", () => {
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
    scene.setProject(projectView({}, [note("a", 0)]));
    scene.draw(0);
    expect(renderer.grids).toHaveLength(1);
    expect(renderer.notes).toHaveLength(2);
    expect(renderer.lastNotes().map((n) => n.id)).toEqual(["a"]);
  });

  it("redraws the grid when the loop changes", () => {
    scene.draw(0);
    scene.setProject(projectView({ loopBars: 2, loopLength: 2 * 3840 }));
    scene.draw(0);
    expect(renderer.grids).toHaveLength(2);
    expect(renderer.grids[1].loopEnd).toBe(2 * 3840);
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
});
