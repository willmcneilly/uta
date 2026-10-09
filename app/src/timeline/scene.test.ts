import { describe, expect, it } from "vitest";
import type { ClipView, NoteView, ProjectView } from "../backend";
import { projectView, trackView } from "../pianoRoll/testing";
import { TimelineScene } from "./scene";
import { recordingTimelineFactory } from "./testing";
import { RULER_HEIGHT, TRACK_HEIGHT, visibleTicks } from "./viewport";

const BAR = 3840;

const note = (id: string, pitch: number, start: number, length = 480): NoteView => ({
  id,
  pitch,
  velocity: 100,
  start,
  length,
});

const clip = (id: string, start: number, length: number, notes: NoteView[] = []): ClipView => ({
  id,
  start,
  length,
  notes,
});

function project(tracks: ProjectView["tracks"]): ProjectView {
  return projectView({ tracks, songEnd: 17 * BAR });
}

/** A scene 1600 px wide, showing 16 bars (100 px a bar), with a recording renderer. */
function scene(view: ProjectView, height = 400) {
  const { factory, renderer } = recordingTimelineFactory();
  const timeline = new TimelineScene();
  timeline.setProject(view);
  timeline.resize(1600, height, 1);
  timeline.setRenderer(factory({} as never));
  timeline.draw(0);
  return { timeline, renderer };
}

describe("TimelineScene", () => {
  it("fits 16 bars, or the whole song, in the width to start with", () => {
    const { renderer } = scene(project([trackView("t1", "Synth 1")]));
    expect(renderer.lastView().pixelsPerTick).toBeCloseTo(1600 / (16 * BAR));

    const long = scene(projectView({ songEnd: 32 * BAR }));
    expect(long.renderer.lastView().pixelsPerTick).toBeCloseTo(1600 / (32 * BAR));
  });

  it("draws every track's clips in order, each with its notes inside it", () => {
    const view = project([
      trackView("t1", "Synth 1", [
        clip("a", 0, 2 * BAR, [
          note("low", 48, 0),
          note("high", 72, BAR),
          note("past", 30, 2 * BAR),
        ]),
        clip("b", BAR, BAR),
      ]),
      trackView("t2", "Synth 2", [clip("c", 4 * BAR, BAR, [note("only", 60, 0)])]),
    ]);
    const { renderer } = scene(view);
    const drawn = renderer.lastClips();
    // In drawing order: where a and b overlap, b, starting later, is on top.
    expect(drawn.map((c) => [c.id, c.track, c.start, c.length])).toEqual([
      ["a", 0, 0, 2 * BAR],
      ["b", 0, BAR, BAR],
      ["c", 1, 4 * BAR, BAR],
    ]);
    // A note past its clip's end doesn't play, so isn't previewed, nor
    // counts towards its range of pitches.
    expect(drawn[0].notes.map((n) => [n.id, n.start])).toEqual([
      ["low", 0],
      ["high", BAR],
    ]);
    expect([drawn[0].low, drawn[0].high]).toEqual([48, 72]);
    // In song time.
    expect(drawn[2].notes.map((n) => n.start)).toEqual([4 * BAR]);
    expect([drawn[2].low, drawn[2].high]).toEqual([60, 60]);
  });

  it("draws only the clips and notes in view", () => {
    const notes = Array.from({ length: 40 }, (_, i) => note(`n${i}`, 60, i * BAR));
    const tracks = Array.from({ length: 8 }, (_, i) =>
      trackView(`t${i}`, `Synth ${i + 1}`, [
        clip(`long${i}`, 0, 40 * BAR, notes),
        clip(`late${i}`, 30 * BAR, BAR),
      ]),
    );
    const { timeline, renderer } = scene(project(tracks), 24 + 2 * TRACK_HEIGHT);
    // Two tracks tall, and 16 bars wide: the late clips are off to the right.
    expect(renderer.lastClips().map((c) => c.id)).toEqual(["long0", "long1"]);
    const inView = renderer.lastClips()[0].notes.map((n) => n.id);
    expect(inView).toContain("n15");
    expect(inView).not.toContain("n17");

    // A longer song, to scroll further along.
    timeline.setProject({ ...project(tracks), songEnd: 41 * BAR });
    timeline.changeView((view) => ({ ...view, scrollY: TRACK_HEIGHT, scrollTicks: 20 * BAR }));
    timeline.draw(0);
    expect(renderer.lastClips().map((c) => c.id)).toEqual(["long1", "late1", "long2", "late2"]);
    expect(renderer.lastClips()[0].notes[0].id).toBe("n20");
  });

  it("redraws the grid when the loop region or switch changes", () => {
    const view = project([trackView("t1", "Synth 1")]);
    const { timeline, renderer } = scene(view);
    expect(renderer.grids[0].loop).toEqual({ start: 0, end: 4 * BAR, enabled: true });
    timeline.setProject({ ...view, loopStart: BAR, loopLength: 2 * BAR });
    timeline.draw(0);
    expect(renderer.grids.at(-1)!.loop).toEqual({ start: BAR, end: 3 * BAR, enabled: true });
    timeline.setProject({ ...view, loopStart: BAR, loopLength: 2 * BAR, loopEnabled: false });
    timeline.draw(0);
    expect(renderer.grids).toHaveLength(3);
    expect(renderer.grids.at(-1)!.loop.enabled).toBe(false);
  });

  it("turns a page to follow the playhead, and redraws nothing while it's in view", () => {
    const view = project([trackView("t1", "Synth 1")]);
    const { timeline, renderer } = scene(view);
    // A longer song, to scroll a page along.
    timeline.setProject({ ...view, songEnd: 41 * BAR });
    timeline.draw(0);
    const before = [renderer.grids.length, renderer.clips.length];
    // The whole 17-bar song is in view to start with.
    const edge = visibleTicks(timeline.getView()!).end;
    timeline.follow(edge - 1);
    timeline.draw(edge - 1);
    expect([renderer.grids.length, renderer.clips.length]).toEqual(before);
    timeline.follow(edge);
    timeline.draw(edge);
    expect(timeline.getView()!.scrollTicks).toBe(edge);
    expect(renderer.grids.length).toBe(before[0] + 1);
  });

  it("redraws the clips only when they or the view change", () => {
    const view = project([trackView("t1", "Synth 1", [clip("a", 0, BAR, [note("n", 60, 0)])])]);
    const { timeline, renderer } = scene(view);
    expect([renderer.grids.length, renderer.clips.length, renderer.tops.length]).toEqual([1, 1, 1]);

    // A fresh view with the same clips: nothing to redraw.
    timeline.setProject({ ...structuredClone(view), volumeDb: -3 });
    timeline.draw(0);
    expect([renderer.grids.length, renderer.clips.length, renderer.tops.length]).toEqual([1, 1, 1]);

    // A moved clip redraws the clips, not the grid.
    const moved = project([trackView("t1", "Synth 1", [clip("a", BAR, BAR, [note("n", 60, 0)])])]);
    timeline.setProject(moved);
    timeline.draw(0);
    expect([renderer.grids.length, renderer.clips.length]).toEqual([1, 2]);
    expect(renderer.lastClips()[0].notes[0].start).toBe(BAR);

    // A new track redraws the grid too.
    timeline.setProject(project([...moved.tracks, trackView("t2", "Synth 2")]));
    timeline.draw(0);
    expect([renderer.grids.length, renderer.clips.length]).toEqual([2, 3]);

    // The playhead redraws only the top.
    timeline.draw(100);
    expect([renderer.grids.length, renderer.clips.length, renderer.tops.length]).toEqual([2, 3, 2]);
  });

  it("doesn't go through the clips when every track kept its list", () => {
    // projectCache.ts keeps an unchanged track's list of clips, so a wide
    // song's volume step has nothing to compare (UTA-33).
    const reads = { count: 0 };
    const watched = (clips: ClipView[]) =>
      new Proxy(clips, {
        get(target, key, receiver) {
          if (typeof key === "string" && /^\d+$/.test(key)) reads.count += 1;
          return Reflect.get(target, key, receiver) as unknown;
        },
      });
    const t1 = trackView("t1", "Synth 1", watched([clip("a", 0, BAR, [note("n", 60, 0)])]));
    const t2 = trackView("t2", "Synth 2", watched([clip("b", 0, BAR)]));
    const { timeline, renderer } = scene(project([t1, t2]));
    reads.count = 0;

    // New track objects, as each update makes, with the same lists.
    timeline.setProject({ ...project([{ ...t1 }, { ...t2 }]), volumeDb: -3 });
    timeline.draw(0);
    expect(reads.count).toBe(0);
    expect(renderer.clips.length).toBe(1);

    // Another track's list: everything is compared, and only that change drawn.
    const changed = { ...t2, clips: [clip("b", 0, BAR, [note("new", 60, 0)])] };
    timeline.setProject(project([{ ...t1 }, changed]));
    timeline.draw(0);
    expect(renderer.clips.length).toBe(2);
    expect(renderer.lastClips().map((drawn) => [drawn.id, drawn.notes.length])).toEqual([
      ["a", 1],
      ["b", 1],
    ]);
  });

  it("highlights the selected track and clips", () => {
    const view = project([
      trackView("t1", "Synth 1", [clip("a", 0, BAR), clip("c", BAR, BAR)]),
      trackView("t2", "Synth 2", [clip("b", 0, BAR)]),
    ]);
    const { timeline, renderer } = scene(view);
    expect(renderer.grids.at(-1)!.selectedTrack).toBeNull();
    timeline.setSelection("t2", new Set(["b", "c"]));
    timeline.draw(0);
    expect(renderer.grids.at(-1)!.selectedTrack).toBe(1);
    expect(renderer.lastClips().map((c) => [c.id, c.selected])).toEqual([
      ["a", false],
      ["c", true],
      ["b", true],
    ]);
    // The same selection again redraws nothing.
    const drawn = renderer.clips.length;
    timeline.setSelection("t2", new Set(["c", "b"]));
    timeline.draw(0);
    expect(renderer.clips.length).toBe(drawn);
  });

  it("marks the clip under the pointer, redrawing the clips only when it changes", () => {
    const view = project([trackView("t1", "Synth 1", [clip("a", 0, BAR), clip("b", BAR, BAR)])]);
    const { timeline, renderer } = scene(view);
    expect(renderer.lastClips().map((c) => c.hovered)).toEqual([false, false]);
    timeline.setHovered("b");
    timeline.draw(0);
    expect(renderer.lastClips().map((c) => [c.id, c.hovered])).toEqual([
      ["a", false],
      ["b", true],
    ]);
    const drawn = renderer.clips.length;
    timeline.setHovered("b");
    timeline.draw(0);
    expect(renderer.clips.length).toBe(drawn);
    timeline.setHovered(null);
    timeline.draw(0);
    expect(renderer.lastClips().every((c) => !c.hovered)).toBe(true);
  });

  it("finds the clips a selection box touches, and shows the box on the top layer", () => {
    const view = project([
      trackView("t1", "Synth 1", [clip("a", 0, BAR), clip("b", 4 * BAR, BAR)]),
      trackView("t2", "Synth 2", [clip("c", 2 * BAR, BAR)]),
      trackView("t3", "Synth 3", [clip("d", BAR, BAR)]),
    ]);
    const { timeline, renderer } = scene(view);
    // From the middle of bar 1 on the first track to bar 3 on the second:
    // 100 px a bar.
    const box = {
      x: 50,
      y: RULER_HEIGHT + 10,
      width: 200,
      height: TRACK_HEIGHT + 10,
    };
    expect(timeline.clipsIn(box)).toEqual(["a", "c"]);
    // Touching a clip's edge is enough.
    expect(timeline.clipsIn({ ...box, x: 399, width: 2 })).toEqual(["b"]);
    expect(timeline.clipsIn({ ...box, x: 600, width: 50 })).toEqual([]);
    // Above the tracks, the box only counts from the ruler down.
    expect(timeline.clipsIn({ x: 0, y: 0, width: 50, height: RULER_HEIGHT })).toEqual([]);

    timeline.setBox(box);
    timeline.draw(0);
    expect(renderer.lastBox()).toEqual(box);
    timeline.setBox(null);
    timeline.draw(0);
    expect(renderer.lastBox()).toBeNull();
  });

  it("shows the clip being drawn on the top layer", () => {
    const { timeline, renderer } = scene(project([trackView("t1", "Synth 1")]));
    timeline.setDrawing({ track: 0, start: BAR, length: 2 * BAR });
    timeline.draw(0);
    expect(renderer.lastDrawing()).toEqual({ track: 0, start: BAR, length: 2 * BAR });
    timeline.setDrawing(null);
    timeline.draw(0);
    expect(renderer.lastDrawing()).toBeNull();
  });

  it("keeps the view inside the song and the tracks", () => {
    const { timeline, renderer } = scene(project([trackView("t1", "Synth 1")]));
    timeline.changeView((view) => ({ ...view, scrollTicks: -BAR, scrollY: 500 }));
    timeline.draw(0);
    expect(renderer.lastView().scrollTicks).toBe(0);
    // One track fits: no scrolling down.
    expect(renderer.lastView().scrollY).toBe(0);
  });
});
