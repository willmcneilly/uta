import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type {
  ClipPosition,
  ClipView,
  EditMenuItem,
  Frame,
  NoteView,
  PastedClip,
  ProjectView,
  TrackView,
} from "../backend";
import {
  type RecordingRenderer,
  announceChange,
  projectView,
  recordingFactory,
  trackView,
} from "../pianoRoll/testing";
import { pitchToY, tickToX as rollTickToX } from "../pianoRoll/viewport";
import type { ClipSnap } from "./snap";
import { type RecordingTimelineRenderer, recordingTimelineFactory } from "./testing";
import { TRACK_HEIGHT, tickToX, trackToY } from "./viewport";

// Editing clips on the timeline, against Tauri's mocked back end. The mock
// applies the clip commands to its own project and sends it back, so the
// tests check both what the timeline sends and that it draws the result.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

const BAR = 3840;
const BEAT = 960;

let calls: Call[];
let project: ProjectView;
/** The project as each gesture found it, so cancelling can put it back. */
let beforeGesture: Map<number, ProjectView>;
let timeline: RecordingTimelineRenderer;
let roll: RecordingRenderer;
let frames: Channel<Frame> | null;

const note = (id: string, pitch: number, start: number): NoteView => ({
  id,
  pitch,
  velocity: 100,
  start,
  length: 480,
});

const clip = (id: string, start: number, length: number, notes: NoteView[] = []): ClipView => ({
  id,
  start,
  length,
  notes,
});

/** Clips in a track are in order of start, then ID, as Rust keeps them. */
const byStart = (clips: ClipView[]) =>
  [...clips].sort((a, b) => a.start - b.start || a.id.localeCompare(b.id));

function withTracks(tracks: TrackView[]): ProjectView {
  const end = Math.max(0, ...tracks.flatMap((t) => t.clips.map((c) => c.start + c.length)));
  return { ...project, canUndo: true, tracks, songEnd: end + BAR };
}

function setClips(positions: ClipPosition[]): ProjectView {
  const moving = new Map(positions.map((p) => [p.id, p]));
  const moved = project.tracks
    .flatMap((t) => t.clips)
    .filter((c) => moving.has(c.id))
    .map((c) => ({ ...c, start: moving.get(c.id)!.start, length: moving.get(c.id)!.length }));
  return withTracks(
    project.tracks.map((track) => ({
      ...track,
      clips: byStart([
        ...track.clips.filter((c) => !moving.has(c.id)),
        ...moved.filter((c) => moving.get(c.id)!.track === track.id),
      ]),
    })),
  );
}

// jsdom doesn't lay anything out, so every view is given a size: the
// timeline is 856 px wide, showing 16 bars, and 504 px tall.
class FixedSizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe() {
    const entry = { contentRect: { width: 856, height: 504 } } as ResizeObserverEntry;
    this.callback([entry], this as unknown as ResizeObserver);
  }
  unobserve() {}
  disconnect() {}
}

beforeEach(() => {
  calls = [];
  const first = trackView("track-1", "Synth 1", [
    clip("clip-1", 0, 4 * BAR, [note("n1", 60, 0), note("n2", 67, BAR)]),
  ]);
  const second = {
    ...trackView("track-2", "Synth 2", [clip("clip-2", 8 * BAR, 2 * BAR)]),
    synth: { ...trackView("", "").synth, waveform: "square" as const },
  };
  project = projectView({ tracks: [first, second], songEnd: 11 * BAR });
  beforeGesture = new Map();
  frames = null;
  ({ renderer: timeline } = recordingTimelineFactory());
  ({ renderer: roll } = recordingFactory());
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      const gesture = args.gesture as number | null | undefined;
      if (gesture !== null && gesture !== undefined && !beforeGesture.has(gesture)) {
        beforeGesture.set(gesture, project);
      }
      switch (cmd) {
        case "get_project":
          return project;
        case "add_clip":
          project = withTracks(
            project.tracks.map((track) =>
              track.id === args.track
                ? {
                    ...track,
                    clips: byStart([
                      ...track.clips,
                      clip(args.id as string, args.start as number, args.length as number),
                    ]),
                  }
                : track,
            ),
          );
          return project;
        case "set_clips":
          project = setClips(args.clips as ClipPosition[]);
          return project;
        case "remove_clips": {
          const ids = args.clips as string[];
          project = withTracks(
            project.tracks.map((t) => ({
              ...t,
              clips: t.clips.filter((c) => !ids.includes(c.id)),
            })),
          );
          return project;
        }
        case "paste_clips": {
          // Rust gives every note a new ID; here, the clip's ID and the note's.
          const pasted = (args.clips as PastedClip[]).map(({ track, ...rest }) => ({
            track,
            clip: { ...rest, notes: rest.notes.map((n) => ({ ...n, id: `${rest.id}:${n.id}` })) },
          }));
          project = withTracks(
            project.tracks.map((t) => ({
              ...t,
              clips: byStart([
                ...t.clips,
                ...pasted.filter((p) => p.track === t.id).map((p) => p.clip),
              ]),
            })),
          );
          return project;
        }
        case "set_loop":
          project = {
            ...project,
            canUndo: true,
            loopStart: (args.startBar as number) * BAR,
            loopLength: (args.bars as number) * BAR,
          };
          return project;
        case "set_loop_enabled":
          project = { ...project, canUndo: true, loopEnabled: args.enabled as boolean };
          return project;
        case "trim_notes":
          // Nothing to trim in these tests.
          return project;
        case "add_notes":
        case "remove_notes": {
          const changeNotes = (notes: NoteView[]) =>
            cmd === "add_notes"
              ? [...notes, ...(args.notes as NoteView[])]
              : notes.filter((n) => !(args.notes as string[]).includes(n.id));
          project = withTracks(
            project.tracks.map((t) => ({
              ...t,
              clips: t.clips.map((c) =>
                c.id === args.clip ? { ...c, notes: changeNotes(c.notes) } : c,
              ),
            })),
          );
          return project;
        }
        case "cancel_gesture":
          project = beforeGesture.get(gesture as number) ?? project;
          return project;
        case "subscribe":
          frames = args.onFrame as Channel<Frame>;
          return null;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
});

afterEach(async () => {
  cleanup();
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
  vi.unstubAllGlobals();
});

/**
 * Renders the app, with clips snapping to `snap`. Most tests snap to bars,
 * which don't change with the zoom; `null` leaves the default.
 */
async function renderApp(snap: ClipSnap | null = "bar") {
  render(<App createRenderer={() => roll} createTimelineRenderer={() => timeline} />);
  await screen.findByRole("application", { name: "Clips" });
  // The drawing loop runs on animation frames; wait for the first.
  await waitFor(() => expect(timeline.clips.length).toBeGreaterThan(0));
  await waitFor(() => expect(frames).not.toBeNull());
  if (snap) fireEvent.change(screen.getByLabelText("Clip snap"), { target: { value: snap } });
}

async function zoom(direction: "in" | "out", times: number) {
  const button = screen.getByRole("button", { name: `Zoom ${direction} timeline` });
  for (let i = 0; i < times; i++) fireEvent.click(button);
  const before = timeline.clips.length;
  // The new view arrives with the next frame.
  await waitFor(() => expect(timeline.clips.length).toBeGreaterThan(before));
}

const clipsArea = () => screen.getByRole("application", { name: "Clips" });

interface Modifiers {
  metaKey?: boolean;
  shiftKey?: boolean;
}

/** The pointer at `tick` (from the song's start) in the middle of track `track`'s row. */
function at(tick: number, track: number, modifiers: Modifiers = {}) {
  const view = timeline.lastView();
  return {
    clientX: tickToX(view, tick),
    clientY: trackToY(view, track) + TRACK_HEIGHT / 2,
    button: 0,
    ...modifiers,
  };
}

async function press(tick: number, track: number, modifiers: Modifiers = {}) {
  await act(async () => {
    fireEvent.pointerDown(clipsArea(), at(tick, track, modifiers));
  });
}

async function moveTo(tick: number, track: number, modifiers: Modifiers = {}) {
  await act(async () => {
    fireEvent.pointerMove(window, at(tick, track, modifiers));
  });
}

async function release() {
  await act(async () => {
    fireEvent.pointerUp(window);
  });
}

async function click(tick: number, track: number, modifiers: Modifiers = {}) {
  await press(tick, track, modifiers);
  await release();
}

async function doubleClick(tick: number, track: number, modifiers: Modifiers = {}) {
  await click(tick, track);
  await act(async () => {
    fireEvent.doubleClick(clipsArea(), at(tick, track, modifiers));
  });
}

async function key(target: Window | HTMLElement, name: string) {
  await act(async () => {
    fireEvent.keyDown(target, { key: name });
  });
}

/** The pointer at `tick` on the ruler. */
function onRuler(tick: number, modifiers: Modifiers = {}) {
  return { clientX: tickToX(timeline.lastView(), tick), clientY: 10, button: 0, ...modifiers };
}

async function pressRuler(tick: number, modifiers: Modifiers = {}) {
  await act(async () => {
    fireEvent.pointerDown(clipsArea(), onRuler(tick, modifiers));
  });
}

async function moveOnRuler(tick: number) {
  await act(async () => {
    fireEvent.pointerMove(window, onRuler(tick));
  });
}

function sendFrame(playing: boolean, playhead: number) {
  act(() =>
    frames!.onmessage({
      playing,
      playhead,
      peak: 0,
      trackPeaks: {},
      clips: 0,
      dropouts: 0,
      output: {
        state: "running",
        device: null,
        sampleRate: 48000,
        bufferSize: 128,
        requestedBufferSize: 128,
        bufferSizes: [128],
      },
    }),
  );
}

const sent = (cmd: string) => calls.filter((call) => call.cmd === cmd).map((call) => call.args);
const edits = () =>
  calls.filter((call) => /clip|notes|gesture/.test(call.cmd)).map((call) => call.cmd);
const drawn = () => timeline.lastClips().map((c) => [c.id, c.track, c.start, c.length]);
const selectedClip = () => timeline.lastClips().find((c) => c.selected)?.id;
const selectedClips = () =>
  timeline
    .lastClips()
    .filter((c) => c.selected)
    .map((c) => c.id);

/** Chooses Copy, Paste or Duplicate from the Edit menu. */
async function menu(item: EditMenuItem) {
  await act(() => emit("edit-menu", item));
}

/** Adds a third track, with no clips, below the other two. */
function withThirdTrack() {
  project = withTracks([...project.tracks, trackView("track-3", "Synth 3")]);
}
/** Where the piano roll last shaded outside its clip: the clip it shows. */
const pianoRollClip = () => {
  const grid = roll.grids.at(-1)!;
  return [grid.clipStart, grid.clipEnd];
};

describe("the timeline", () => {
  it("draws every clip Rust sends, with its notes, on its track", async () => {
    await renderApp();
    expect(drawn()).toEqual([
      ["clip-1", 0, 0, 4 * BAR],
      ["clip-2", 1, 8 * BAR, 2 * BAR],
    ]);
    expect(timeline.lastClips()[0].notes.map((n) => n.id)).toEqual(["n1", "n2"]);

    // Undo, say: whatever Rust announces is drawn.
    project = {
      ...project,
      tracks: [project.tracks[0], { ...project.tracks[1], clips: [] }],
    };
    await announceChange();
    await waitFor(() => expect(drawn()).toEqual([["clip-1", 0, 0, 4 * BAR]]));
  });

  it("gives each header the height of its track's row, under a gap as tall as the ruler", async () => {
    await renderApp();
    const header = screen.getByRole("listitem", { name: "Synth 2" });
    expect(header).toHaveStyle({ height: `${TRACK_HEIGHT}px` });
    const tracks = screen.getByRole("region", { name: "Tracks" });
    const view = timeline.lastView();
    expect(tracks.querySelector(".track-headers-ruler")).toHaveStyle({
      height: `${trackToY(view, 0)}px`,
    });
  });

  it("scrolls the headers with the timeline, and the timeline with the headers", async () => {
    project = {
      ...project,
      tracks: Array.from({ length: 8 }, (_, i) => trackView(`t${i}`, `Synth ${i + 1}`)),
    };
    await renderApp();
    const headers = screen.getByRole("region", { name: "Tracks" });
    await act(async () => {
      fireEvent.wheel(clipsArea(), { deltaY: 100 });
    });
    await waitFor(() => expect(timeline.lastView().scrollY).toBe(100));
    await waitFor(() => expect(headers.scrollTop).toBe(100));

    // The wheel over the headers scrolls the timeline.
    await act(async () => {
      fireEvent.wheel(headers, { deltaY: 50 });
    });
    await waitFor(() => expect(timeline.lastView().scrollY).toBe(150));

    // So does the headers scrolling themselves, to show a focused control.
    headers.scrollTop = 30;
    await act(async () => {
      fireEvent.scroll(headers);
    });
    await waitFor(() => expect(timeline.lastView().scrollY).toBe(30));
  });

  describe("drawing a clip", () => {
    it("drags across empty space for a clip that long, snapped to bars, added when the drag ends", async () => {
      await renderApp();
      await press(5.3 * BAR, 0);
      await moveTo(7.5 * BAR, 0);
      // Shown while dragging, but not added yet.
      await waitFor(() =>
        expect(timeline.lastDrawing()).toEqual({ track: 0, start: 5 * BAR, length: 3 * BAR }),
      );
      expect(sent("add_clip")).toEqual([]);
      await release();

      const [add] = sent("add_clip");
      const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
      expect(add).toEqual({
        track: "track-1",
        id: expect.stringMatching(uuid),
        start: 5 * BAR,
        length: 3 * BAR,
      });
      await waitFor(() => expect(timeline.lastDrawing()).toBeNull());
      // It's drawn from what Rust sent back, selected, and open in Notes.
      await waitFor(() => expect(selectedClip()).toBe(add.id));
      expect(drawn()).toContainEqual([add.id, 0, 5 * BAR, 3 * BAR]);
      await waitFor(() => expect(pianoRollClip()).toEqual([5 * BAR, 8 * BAR]));
    });

    it("drags leftwards too, and snaps to beats or not at all", async () => {
      await renderApp();
      fireEvent.change(screen.getByLabelText("Clip snap"), { target: { value: "beat" } });
      await press(14.3 * BAR, 1);
      await moveTo(12.6 * BAR, 1);
      await release();
      expect(sent("add_clip")[0]).toMatchObject({
        track: "track-2",
        start: 12 * BAR + 2 * BEAT,
        length: 2 * BAR,
      });

      // ⌘ turns snapping off for one drag.
      await press(12.1 * BAR, 0, { metaKey: true });
      await moveTo(14.4 * BAR, 0, { metaKey: true });
      await release();
      const unsnapped = sent("add_clip")[1];
      expect((unsnapped.start as number) % BEAT).not.toBe(0);
    });

    it("adds a one-bar clip on a double-click", async () => {
      await renderApp();
      await doubleClick(6.5 * BAR, 1);
      expect(sent("add_clip")).toEqual([
        { track: "track-2", id: expect.any(String), start: 6 * BAR, length: BAR },
      ]);
    });

    it("adds nothing on Esc, or on a click", async () => {
      await renderApp();
      await press(5.3 * BAR, 0);
      await moveTo(7.5 * BAR, 0);
      await key(window, "Escape");
      await release();
      await click(5.3 * BAR, 0);
      expect(sent("add_clip")).toEqual([]);
      await waitFor(() => expect(timeline.lastDrawing()).toBeNull());
    });

    it("adds nothing below the last track", async () => {
      await renderApp();
      await press(5 * BAR, 2);
      await moveTo(7 * BAR, 2);
      await release();
      await doubleClick(5 * BAR, 2);
      expect(sent("add_clip")).toEqual([]);
    });
  });

  describe("snapping to the grid on screen", () => {
    it("starts on Grid, which at 16 bars across snaps to beats", async () => {
      await renderApp(null);
      expect(screen.getByLabelText("Clip snap")).toHaveValue("grid");
      // Pressed at bar 2; moved 1.3 bars lands on beat 2 of bar 2.
      await press(BAR, 0);
      await moveTo(2.3 * BAR, 0);
      await release();
      const [move] = sent("set_clips");
      expect((move.clips as ClipPosition[])[0].start).toBe(BAR + BEAT);
    });

    it("snaps to bars once beats are too close to draw", async () => {
      await renderApp(null);
      await zoom("out", 1);
      await press(BAR, 0);
      await moveTo(2.3 * BAR, 0);
      await release();
      const [move] = sent("set_clips");
      expect((move.clips as ClipPosition[])[0].start).toBe(BAR);
    });

    it("snaps to sixteenths zoomed far enough in", async () => {
      await renderApp(null);
      await zoom("in", 6);
      // Around bar 8, after an empty stretch of the first track.
      await press(7.1 * BAR, 0);
      await moveTo(7.3 * BAR, 0);
      await release();
      const [add] = sent("add_clip");
      // From the sixteenth before 7.1 bars to the one after 7.3.
      expect(add).toMatchObject({ start: 7 * BAR + 240, length: 4 * 240 });
    });
  });

  describe("moving a clip", () => {
    it("drags it along its track, snapped to bars, as one gesture that trims nothing", async () => {
      await renderApp();
      await press(BAR, 0);
      await moveTo(2.2 * BAR, 0);
      await moveTo(3.4 * BAR, 0);
      await release();
      const moves = sent("set_clips");
      expect(moves).toEqual([
        {
          clips: [{ id: "clip-1", track: "track-1", start: BAR, length: 4 * BAR }],
          gesture: expect.any(Number),
        },
        {
          clips: [{ id: "clip-1", track: "track-1", start: 2 * BAR, length: 4 * BAR }],
          gesture: moves[0].gesture,
        },
      ]);
      // Moving a clip never trims the clips around it.
      expect(edits()).toEqual(["set_clips", "set_clips"]);
      await waitFor(() => expect(drawn()[0]).toEqual(["clip-1", 0, 2 * BAR, 4 * BAR]));
    });

    it("drags it onto another track in the same command", async () => {
      await renderApp();
      await press(BAR, 0);
      await moveTo(BAR, 1);
      await release();
      expect(sent("set_clips")).toEqual([
        {
          clips: [{ id: "clip-1", track: "track-2", start: 0, length: 4 * BAR }],
          gesture: expect.any(Number),
        },
      ]);
      await waitFor(() => expect(drawn()).toContainEqual(["clip-1", 1, 0, 4 * BAR]));
    });

    it("doesn't move on a click", async () => {
      await renderApp();
      await click(BAR, 0);
      expect(sent("set_clips")).toEqual([]);
    });

    it("moves freely with ⌘", async () => {
      await renderApp();
      await press(BAR, 0);
      await moveTo(BAR + 500, 0, { metaKey: true });
      await release();
      const [move] = sent("set_clips");
      const [position] = move.clips as ClipPosition[];
      expect(Math.abs(position.start - 500)).toBeLessThan(80);
      expect(position.start % BEAT).not.toBe(0);
    });

    it("puts it back on Esc", async () => {
      await renderApp();
      await press(BAR, 0);
      await moveTo(3 * BAR, 1);
      await key(window, "Escape");
      await release();
      const [move] = sent("set_clips");
      expect(sent("cancel_gesture")).toEqual([{ gesture: move.gesture }]);
      await waitFor(() => expect(drawn()[0]).toEqual(["clip-1", 0, 0, 4 * BAR]));
    });
  });

  describe("resizing a clip", () => {
    it("drags its right edge, snapped to bars, as one gesture", async () => {
      await renderApp();
      const edge = 4 * BAR - 2 / timeline.lastView().pixelsPerTick;
      await press(edge, 0);
      await moveTo(edge + 1.7 * BAR, 0);
      await moveTo(edge + 2.6 * BAR, 0);
      await release();
      const sizes = sent("set_clips");
      expect(sizes.map((s) => (s.clips as ClipPosition[])[0])).toEqual([
        { id: "clip-1", track: "track-1", start: 0, length: 6 * BAR },
        { id: "clip-1", track: "track-1", start: 0, length: 7 * BAR },
      ]);
      expect(sizes[1].gesture).toBe(sizes[0].gesture);
      await waitFor(() => expect(drawn()[0]).toEqual(["clip-1", 0, 0, 7 * BAR]));
    });

    it("keeps at least a bar, and puts it back on Esc", async () => {
      await renderApp();
      const edge = 4 * BAR - 2 / timeline.lastView().pixelsPerTick;
      await press(edge, 0);
      await moveTo(-6 * BAR, 0);
      expect((sent("set_clips")[0].clips as ClipPosition[])[0].length).toBe(BAR);
      await key(window, "Escape");
      await release();
      expect(sent("cancel_gesture")).toHaveLength(1);
      await waitFor(() => expect(drawn()[0]).toEqual(["clip-1", 0, 0, 4 * BAR]));
    });
  });

  describe("selecting a clip", () => {
    it("selects it and its track, and opens it in Notes; Sound shows its track's synth", async () => {
      await renderApp();
      await click(8.5 * BAR, 1);
      await waitFor(() => expect(selectedClip()).toBe("clip-2"));
      expect(screen.getByRole("listitem", { name: "Synth 2" })).toHaveAttribute(
        "aria-current",
        "true",
      );
      expect(timeline.grids.at(-1)!.selectedTrack).toBe(1);
      await waitFor(() => expect(pianoRollClip()).toEqual([8 * BAR, 10 * BAR]));
      // The piano roll shows the whole clip, in song time.
      expect(roll.lastView().scrollTicks).toBe(8 * BAR);

      fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
      const synth = screen.getByRole("region", { name: "Synth" });
      expect(within(synth).getByRole("radio", { name: "Square" })).toBeChecked();
    });

    it("opens a clip in the Notes tab on a double-click", async () => {
      await renderApp();
      fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
      await doubleClick(8.5 * BAR, 1);
      expect(screen.getByRole("tab", { name: "Notes" })).toHaveAttribute("aria-selected", "true");
      await waitFor(() => expect(pianoRollClip()).toEqual([8 * BAR, 10 * BAR]));
      expect(sent("add_clip")).toEqual([]);
    });

    it("selects the track and no clip on a click in empty space", async () => {
      await renderApp();
      await click(8.5 * BAR, 1);
      await click(3 * BAR, 1);
      expect(screen.getByRole("listitem", { name: "Synth 2" })).toHaveAttribute(
        "aria-current",
        "true",
      );
      await waitFor(() => expect(selectedClip()).toBeUndefined());
      // Notes still shows the track's first clip.
      await waitFor(() => expect(pianoRollClip()).toEqual([8 * BAR, 10 * BAR]));
    });

    it("deletes nothing with Backspace when no clip was clicked", async () => {
      await renderApp();
      // Synth 2's clip is at bar 9, not under the pointer.
      await click(3 * BAR, 1);
      await key(clipsArea(), "Backspace");
      // Nor after opening the app, before anything is clicked.
      await key(clipsArea(), "Delete");
      expect(sent("remove_clips")).toEqual([]);
      expect(drawn()).toHaveLength(2);
    });

    it("keeps a clip open, and its track selected, when it's moved to another track", async () => {
      await renderApp();
      await press(8.5 * BAR, 1);
      await moveTo(8.5 * BAR, 0);
      await release();
      await waitFor(() => expect(drawn()).toContainEqual(["clip-2", 0, 8 * BAR, 2 * BAR]));
      expect(selectedClip()).toBe("clip-2");
      expect(screen.getByRole("listitem", { name: "Synth 1" })).toHaveAttribute(
        "aria-current",
        "true",
      );
      expect(pianoRollClip()).toEqual([8 * BAR, 10 * BAR]);
    });

    it("keeps the selected clip when its track's header is clicked, and not another's", async () => {
      await renderApp();
      await click(8.5 * BAR, 1);
      fireEvent.pointerDown(screen.getByRole("listitem", { name: "Synth 2" }));
      await waitFor(() => expect(selectedClip()).toBe("clip-2"));
      fireEvent.pointerDown(screen.getByRole("listitem", { name: "Synth 1" }));
      // No clip is selected; Notes shows Synth 1's first clip.
      await waitFor(() => expect(selectedClip()).toBeUndefined());
      await waitFor(() => expect(pianoRollClip()).toEqual([0, 4 * BAR]));
    });

    it("deletes the selected clip with Backspace", async () => {
      await renderApp();
      await click(8.5 * BAR, 1);
      await key(clipsArea(), "Backspace");
      expect(sent("remove_clips")).toEqual([{ clips: ["clip-2"] }]);
      await waitFor(() => expect(drawn()).toEqual([["clip-1", 0, 0, 4 * BAR]]));
      const editor = screen.getByRole("region", { name: "Editor" });
      await waitFor(() =>
        expect(within(editor).getByText("Synth 2 has no clips yet.")).toBeInTheDocument(),
      );
    });

    it("shows the song's playhead in the piano roll", async () => {
      await renderApp();
      await click(8.5 * BAR, 1);
      act(() =>
        frames!.onmessage({
          playing: false,
          playhead: 9 * BAR,
          peak: 0,
          trackPeaks: {},
          clips: 0,
          dropouts: 0,
          output: {
            state: "running",
            device: null,
            sampleRate: 48000,
            bufferSize: 128,
            requestedBufferSize: 128,
            bufferSizes: [128],
          },
        }),
      );
      await waitFor(() => expect(roll.tops.at(-1)).toBe(9 * BAR));
      await waitFor(() => expect(timeline.tops.at(-1)).toBe(9 * BAR));
    });
  });

  describe("selecting several clips", () => {
    it("adds a clip to the selection with Shift-click, or takes it out", async () => {
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      await waitFor(() => expect(selectedClips()).toEqual(["clip-1", "clip-2"]));
      // The last clip picked is open in Notes, with its track selected.
      await waitFor(() => expect(pianoRollClip()).toEqual([8 * BAR, 10 * BAR]));
      expect(screen.getByRole("listitem", { name: "Synth 2" })).toHaveAttribute(
        "aria-current",
        "true",
      );
      await click(BAR, 0, { shiftKey: true });
      await waitFor(() => expect(selectedClips()).toEqual(["clip-2"]));
      // Shift-click only selects: nothing moves.
      expect(edits()).toEqual([]);
    });

    it("box-selects the clips a Shift-drag on empty space touches, adding to the selection", async () => {
      withThirdTrack();
      project = withTracks(
        project.tracks.map((t) =>
          t.id === "track-3" ? { ...t, clips: [clip("clip-3", 12 * BAR, BAR)] } : t,
        ),
      );
      await renderApp();
      await click(12.5 * BAR, 2);
      // From bar 12 on Synth 1 to bar 4 on Synth 2: clip-1 ends in bar 4.
      await press(11 * BAR, 0, { shiftKey: true });
      await moveTo(3 * BAR, 1);
      await waitFor(() => expect(timeline.lastBox()).not.toBeNull());
      await waitFor(() => expect(selectedClips().sort()).toEqual(["clip-1", "clip-2", "clip-3"]));
      await release();
      await waitFor(() => expect(timeline.lastBox()).toBeNull());
      expect(selectedClips().sort()).toEqual(["clip-1", "clip-2", "clip-3"]);
      // It only selects: nothing is drawn or sent.
      expect(edits()).toEqual([]);
    });

    it("puts the selection back when a box is cancelled with Esc", async () => {
      await renderApp();
      await click(BAR, 0);
      await press(11 * BAR, 0, { shiftKey: true });
      await moveTo(3 * BAR, 1);
      await waitFor(() => expect(selectedClips()).toEqual(["clip-1", "clip-2"]));
      await key(window, "Escape");
      await waitFor(() => expect(selectedClips()).toEqual(["clip-1"]));
      expect(timeline.lastBox()).toBeNull();
    });

    it("deletes the whole selection with Backspace, as one command", async () => {
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      await key(clipsArea(), "Backspace");
      expect(sent("remove_clips")).toEqual([{ clips: ["clip-1", "clip-2"] }]);
      await waitFor(() => expect(drawn()).toEqual([]));
    });

    it("clears the selection with Esc", async () => {
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      await key(clipsArea(), "Escape");
      await waitFor(() => expect(selectedClips()).toEqual([]));
      await key(clipsArea(), "Backspace");
      expect(sent("remove_clips")).toEqual([]);
    });

    it("moves the selection together, across tracks too, as one gesture", async () => {
      withThirdTrack();
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      // Dragging clip-1 a bar right and a track down takes clip-2 with it.
      await press(BAR, 0);
      await moveTo(2.2 * BAR, 1);
      await release();
      const moves = sent("set_clips");
      expect(moves).toEqual([
        {
          clips: [
            { id: "clip-1", track: "track-2", start: BAR, length: 4 * BAR },
            { id: "clip-2", track: "track-3", start: 9 * BAR, length: 2 * BAR },
          ],
          gesture: expect.any(Number),
        },
      ]);
      await waitFor(() =>
        expect(drawn()).toEqual([
          ["clip-1", 1, BAR, 4 * BAR],
          ["clip-2", 2, 9 * BAR, 2 * BAR],
        ]),
      );
      expect(selectedClips()).toEqual(["clip-1", "clip-2"]);
    });

    it("keeps the whole selection on the tracks there are", async () => {
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      // clip-2 is on the last track already, so the selection can't go down.
      await press(BAR, 0);
      await moveTo(2 * BAR, 1);
      await release();
      expect(sent("set_clips")).toEqual([
        {
          clips: [
            { id: "clip-1", track: "track-1", start: BAR, length: 4 * BAR },
            { id: "clip-2", track: "track-2", start: 9 * BAR, length: 2 * BAR },
          ],
          gesture: expect.any(Number),
        },
      ]);
    });

    it("puts the whole selection back on Esc", async () => {
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      await press(BAR, 0);
      await moveTo(3 * BAR, 0);
      await key(window, "Escape");
      await release();
      expect(sent("cancel_gesture")).toEqual([{ gesture: sent("set_clips")[0].gesture }]);
      await waitFor(() =>
        expect(drawn()).toEqual([
          ["clip-1", 0, 0, 4 * BAR],
          ["clip-2", 1, 8 * BAR, 2 * BAR],
        ]),
      );
    });

    it("selects just the clip clicked of a larger selection", async () => {
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      await click(BAR, 0);
      await waitFor(() => expect(selectedClips()).toEqual(["clip-1"]));
      expect(sent("set_clips")).toEqual([]);
    });
  });

  describe("copying, pasting and duplicating clips", () => {
    it("pastes on the selected track at the playhead, snapped, with new IDs, and selects it", async () => {
      await renderApp();
      await click(BAR, 0);
      await menu("copy");
      // Select Synth 2, and stop with the playhead in bar 6.
      await click(3 * BAR, 1);
      sendFrame(false, 5.3 * BAR);
      await menu("paste");
      const [paste] = sent("paste_clips");
      expect(paste).toEqual({
        clips: [
          {
            id: expect.any(String),
            track: "track-2",
            start: 5 * BAR,
            length: 4 * BAR,
            notes: [note("n1", 60, 0), note("n2", 67, BAR)],
          },
        ],
      });
      const id = (paste.clips as PastedClip[])[0].id;
      expect(id).not.toBe("clip-1");
      // One command, so one undo step.
      expect(edits()).toEqual(["paste_clips"]);
      await waitFor(() => expect(drawn()).toContainEqual([id, 1, 5 * BAR, 4 * BAR]));
      await waitFor(() => expect(selectedClips()).toEqual([id]));
      // Synth 2 stays selected.
      expect(screen.getByRole("listitem", { name: "Synth 2" })).toHaveAttribute(
        "aria-current",
        "true",
      );
    });

    it("gives every paste of the same copy its own IDs", async () => {
      await renderApp();
      await click(BAR, 0);
      await menu("copy");
      await menu("paste");
      await menu("paste");
      const ids = sent("paste_clips").map((p) => (p.clips as PastedClip[])[0].id);
      expect(new Set([...ids, "clip-1"]).size).toBe(3);
      // The copies' notes are their own too: Rust gives each a new ID.
      await waitFor(() => expect(drawn()).toHaveLength(4));
      const noteIds = project.tracks.flatMap((t) =>
        t.clips.flatMap((c) => c.notes.map((n) => n.id)),
      );
      expect(new Set(noteIds).size).toBe(6);
    });

    it("pastes what was copied, even after the original is deleted", async () => {
      await renderApp();
      await click(BAR, 0);
      await menu("copy");
      await key(clipsArea(), "Backspace");
      await waitFor(() => expect(drawn()).toEqual([["clip-2", 1, 8 * BAR, 2 * BAR]]));
      await click(3 * BAR, 0);
      await menu("paste");
      expect(sent("paste_clips")[0].clips).toEqual([
        expect.objectContaining({ track: "track-1", start: 0, length: 4 * BAR }),
      ]);
    });

    it("keeps the layout of clips copied from several tracks", async () => {
      withThirdTrack();
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      await menu("copy");
      // On Synth 2, Synth 1's clip lands on Synth 2 and Synth 2's below it.
      await click(3 * BAR, 1);
      sendFrame(false, 16 * BAR);
      await menu("paste");
      const where = (n: number) =>
        (sent("paste_clips")[n].clips as PastedClip[]).map((c) => [c.track, c.start]);
      expect(where(0).sort()).toEqual([
        ["track-2", 16 * BAR],
        ["track-3", 24 * BAR],
      ]);
      // On the last track, the one that would fall past it goes on it too.
      await click(3 * BAR, 2);
      await menu("paste");
      expect(where(1).sort()).toEqual([
        ["track-3", 16 * BAR],
        ["track-3", 24 * BAR],
      ]);
      // Pasting never adds tracks, and the selected track stays selected.
      await waitFor(() => expect(project.tracks.flatMap((t) => t.clips)).toHaveLength(6));
      expect(sent("add_track")).toEqual([]);
      expect(screen.getByRole("listitem", { name: "Synth 3" })).toHaveAttribute(
        "aria-current",
        "true",
      );
    });

    it("duplicates the selection straight after it, and selects the copy", async () => {
      await renderApp();
      await click(BAR, 0);
      await click(8.5 * BAR, 1, { shiftKey: true });
      await menu("duplicate");
      const [duplicate] = sent("paste_clips");
      // From the start of bar 1 to the end of bar 10: ten bars on.
      expect(
        (duplicate.clips as PastedClip[]).map((c) => [c.track, c.start, c.length, c.notes.length]),
      ).toEqual([
        ["track-1", 10 * BAR, 4 * BAR, 2],
        ["track-2", 18 * BAR, 2 * BAR, 0],
      ]);
      const ids = (duplicate.clips as PastedClip[]).map((c) => c.id);
      expect(ids).not.toContain("clip-1");
      expect(ids).not.toContain("clip-2");
      // The copies are selected: the one in view is highlighted, and the
      // last is open in Notes.
      await waitFor(() => expect(selectedClips()).toEqual([ids[0]]));
      await waitFor(() => expect(pianoRollClip()).toEqual([18 * BAR, 20 * BAR]));
    });

    it("duplicates one clip right after itself", async () => {
      await renderApp();
      await click(8.5 * BAR, 1);
      await menu("duplicate");
      expect(sent("paste_clips")[0].clips).toEqual([
        expect.objectContaining({ track: "track-2", start: 10 * BAR, length: 2 * BAR }),
      ]);
    });

    it("does nothing with nothing selected or copied", async () => {
      await renderApp();
      await click(3 * BAR, 1);
      await menu("copy");
      await menu("paste");
      await menu("duplicate");
      expect(sent("paste_clips")).toEqual([]);
    });
  });

  describe("the Edit menu", () => {
    /** Clicks note n1 (pitch 60, the clip's first beat) in the piano roll. */
    async function clickNoteInPianoRoll() {
      const view = roll.lastView();
      const point = {
        clientX: rollTickToX(view, 240),
        clientY: pitchToY(view, 60) + view.keyHeight / 2,
        button: 0,
      };
      const notes = screen.getByRole("application", { name: "Notes" });
      await act(async () => {
        fireEvent.pointerDown(notes, point);
      });
      await release();
    }

    it("goes to whichever of the timeline and the piano roll was last clicked in", async () => {
      await renderApp();
      await click(BAR, 0);
      await waitFor(() => expect(pianoRollClip()).toEqual([0, 4 * BAR]));
      await clickNoteInPianoRoll();
      await menu("duplicate");
      expect(sent("add_notes")).toHaveLength(1);
      expect(sent("paste_clips")).toEqual([]);

      // The clip is still selected on the timeline; a click there makes the
      // menu act on it again.
      await click(BAR, 0);
      await menu("duplicate");
      expect(sent("paste_clips")).toHaveLength(1);
      expect(sent("add_notes")).toHaveLength(1);
    });

    it("counts a click on a track's header as the timeline", async () => {
      await renderApp();
      await click(BAR, 0);
      await waitFor(() => expect(pianoRollClip()).toEqual([0, 4 * BAR]));
      await clickNoteInPianoRoll();
      // Synth 1 is the selected clip's track, so the clip stays selected.
      await act(async () => {
        fireEvent.pointerDown(screen.getByRole("listitem", { name: "Synth 1" }));
      });
      await menu("duplicate");
      expect(sent("paste_clips")).toHaveLength(1);
      expect(sent("add_notes")).toEqual([]);
    });
  });

  describe("the ruler", () => {
    const loop = () => timeline.grids.at(-1)!.loop;

    it("draws the loop region Rust sends, greyed out while the loop is off", async () => {
      await renderApp();
      expect(loop()).toEqual({ start: 0, end: 4 * BAR, enabled: true });
      fireEvent.click(screen.getByRole("button", { name: "Loop" }));
      await waitFor(() => expect(loop().enabled).toBe(false));
    });

    it("sets the loop region with a drag, from bar line to bar line, as one gesture", async () => {
      await renderApp();
      await pressRuler(2.1 * BAR);
      await moveOnRuler(4.4 * BAR);
      await moveOnRuler(4.3 * BAR);
      await moveOnRuler(5.6 * BAR);
      await release();
      const loops = sent("set_loop");
      expect(loops.map((args) => [args.startBar, args.bars])).toEqual([
        [2, 2],
        [2, 4],
      ]);
      expect(loops[1].gesture).toBe(loops[0].gesture);
      expect(sent("locate")).toEqual([]);
      await waitFor(() => expect(loop()).toEqual({ start: 2 * BAR, end: 6 * BAR, enabled: true }));
    });

    it("sets the loop region dragging leftwards too", async () => {
      await renderApp();
      await pressRuler(6 * BAR);
      await moveOnRuler(3.2 * BAR);
      await release();
      expect(sent("set_loop").map((args) => [args.startBar, args.bars])).toEqual([[3, 3]]);
    });

    it("puts the loop region back on Esc", async () => {
      await renderApp();
      await pressRuler(2 * BAR);
      await moveOnRuler(5 * BAR);
      await key(window, "Escape");
      await release();
      const gesture = sent("set_loop")[0].gesture;
      expect(sent("cancel_gesture")).toEqual([{ gesture }]);
      await waitFor(() => expect(loop()).toEqual({ start: 0, end: 4 * BAR, enabled: true }));
    });

    it("moves the play start to the nearest grid line with a click, or anywhere with ⌘", async () => {
      await renderApp();
      await pressRuler(2.4 * BAR);
      await release();
      await pressRuler(6.7 * BAR);
      await release();
      await pressRuler(3 * BAR + 123, { metaKey: true });
      await release();
      expect(sent("locate")).toEqual([
        { ticks: 2 * BAR },
        { ticks: 7 * BAR },
        { ticks: expect.closeTo(3 * BAR + 123, 0) },
      ]);
      // It edits nothing, and leaves the selection alone.
      expect(edits()).toEqual([]);
      expect(sent("set_loop")).toEqual([]);
    });

    it("sends a click while playing too, which jumps there", async () => {
      await renderApp();
      sendFrame(true, 0);
      await pressRuler(5 * BAR);
      await release();
      expect(sent("locate")).toEqual([{ ticks: 5 * BAR }]);
    });
  });

  describe("following the playhead", () => {
    const scrollTicks = () => timeline.lastView().scrollTicks;

    /**
     * About 8 bars across, so a page is less than the song, scrolled to the
     * start. Scrolling pauses following until the next Play.
     */
    async function zoomedIn() {
      await renderApp();
      await zoom("in", 3);
      await act(async () => {
        fireEvent.wheel(clipsArea(), { deltaX: -100_000 });
      });
      await waitFor(() => expect(scrollTicks()).toBe(0));
    }

    it("turns a page when the playhead reaches the right edge", async () => {
      await zoomedIn();
      sendFrame(true, 4 * BAR);
      // Still in view: it stays put.
      await waitFor(() => expect(timeline.tops.at(-1)).toBeGreaterThanOrEqual(4 * BAR));
      expect(scrollTicks()).toBe(0);
      sendFrame(true, 9 * BAR);
      // A page on: the playhead at the left edge. The clock guesses ahead a little between reports.
      await waitFor(() => expect(scrollTicks()).toBeGreaterThanOrEqual(9 * BAR));
      expect(scrollTicks()).toBeLessThan(9 * BAR + 200);
    });

    it("goes back with the playhead when it jumps or goes round the loop", async () => {
      await zoomedIn();
      sendFrame(true, 9 * BAR);
      await waitFor(() => expect(scrollTicks()).toBeGreaterThanOrEqual(9 * BAR));
      sendFrame(true, BAR);
      await waitFor(() => expect(scrollTicks()).toBeLessThan(BAR + 200));
    });

    it("doesn't follow while stopped", async () => {
      await zoomedIn();
      sendFrame(false, 9 * BAR);
      await waitFor(() => expect(timeline.tops.at(-1)).toBe(9 * BAR));
      expect(scrollTicks()).toBe(0);
    });

    it("pauses when you scroll, and resumes on the next Play", async () => {
      await zoomedIn();
      sendFrame(true, BAR);
      await act(async () => {
        fireEvent.wheel(clipsArea(), { deltaX: 10 });
      });
      sendFrame(true, 9 * BAR);
      await waitFor(() => expect(timeline.tops.at(-1)).toBeGreaterThanOrEqual(9 * BAR));
      expect(scrollTicks()).toBeLessThan(BAR);

      sendFrame(false, 0);
      sendFrame(true, 9 * BAR);
      await waitFor(() => expect(scrollTicks()).toBeGreaterThanOrEqual(9 * BAR));
    });

    /** Plays from bar 1, does `edit`, then plays on past the page: does it follow? */
    async function followsAfter(edit: () => Promise<void>): Promise<boolean> {
      await zoomedIn();
      sendFrame(true, BAR);
      await edit();
      sendFrame(true, 9 * BAR);
      await waitFor(() => expect(timeline.tops.at(-1)).toBeGreaterThanOrEqual(9 * BAR));
      return scrollTicks() > 0;
    }

    it("pauses when you draw a clip", async () => {
      const follows = await followsAfter(async () => {
        await press(6 * BAR, 0);
        await moveTo(7 * BAR, 0);
        await release();
      });
      expect(sent("add_clip")).toHaveLength(1);
      expect(follows).toBe(false);
    });

    it("pauses when you add a clip with a double-click", async () => {
      expect(await followsAfter(() => doubleClick(6 * BAR, 0))).toBe(false);
      expect(sent("add_clip")).toHaveLength(1);
    });

    it("pauses when you move a clip", async () => {
      const follows = await followsAfter(async () => {
        await press(BAR, 0);
        await moveTo(2 * BAR, 0);
        await release();
      });
      expect(sent("set_clips")).toHaveLength(1);
      expect(follows).toBe(false);
    });

    it("pauses when you delete a clip", async () => {
      const follows = await followsAfter(async () => {
        await click(BAR, 0);
        await key(clipsArea(), "Backspace");
      });
      expect(sent("remove_clips")).toHaveLength(1);
      expect(follows).toBe(false);
    });

    it("keeps following after a click that only selects a track or a clip", async () => {
      const follows = await followsAfter(async () => {
        await click(6 * BAR, 0);
        await click(BAR, 0);
      });
      expect(edits()).toEqual([]);
      expect(follows).toBe(true);
    });

    it("keeps following after a click on the ruler", async () => {
      await zoomedIn();
      sendFrame(true, BAR);
      await pressRuler(2 * BAR);
      await release();
      sendFrame(true, 9 * BAR);
      await waitFor(() => expect(scrollTicks()).toBeGreaterThanOrEqual(9 * BAR));
    });

    it("turns the piano roll's pages too", async () => {
      // A clip 12 bars long, so the piano roll can scroll a long way.
      project = {
        ...project,
        tracks: [
          { ...project.tracks[0], clips: [clip("clip-1", 0, 12 * BAR)] },
          project.tracks[1],
        ],
        songEnd: 13 * BAR,
      };
      await renderApp();
      await waitFor(() => expect(roll.notes.length).toBeGreaterThan(0));
      const rollView = () => roll.notes.at(-1)!.view;
      // Zooming pauses following; the next Play starts it again.
      for (let i = 0; i < 6; i++) {
        fireEvent.click(screen.getByRole("button", { name: "Zoom in time" }));
      }
      await waitFor(() => expect(rollView().scrollTicks).toBeLessThan(5 * BAR));
      const before = rollView().scrollTicks;
      sendFrame(true, 8 * BAR);
      await waitFor(() => expect(rollView().scrollTicks).toBeGreaterThanOrEqual(8 * BAR));
      expect(before).toBeLessThan(5 * BAR);
    });

    describe("in the piano roll", () => {
      const rollView = () => roll.notes.at(-1)!.view;
      const notesArea = () => screen.getByRole("application", { name: "Notes" });

      /**
       * A 12-bar clip zoomed in to about 3 bars a page, playing at bar 1.
       * Does `edit`, then plays on to bar 9: does the piano roll follow?
       */
      async function rollFollowsAfter(edit: () => Promise<void>): Promise<boolean> {
        project = {
          ...project,
          tracks: [
            { ...project.tracks[0], clips: [clip("clip-1", 0, 12 * BAR, [note("n1", 60, 0)])] },
            project.tracks[1],
          ],
          songEnd: 13 * BAR,
        };
        await renderApp();
        await waitFor(() => expect(roll.notes.length).toBeGreaterThan(0));
        for (let i = 0; i < 6; i++) {
          fireEvent.click(screen.getByRole("button", { name: "Zoom in time" }));
        }
        sendFrame(true, 0);
        // Scroll to the start, then Play again, so it's following from there.
        await act(async () => {
          fireEvent.wheel(notesArea(), { deltaX: -100_000 });
        });
        await waitFor(() => expect(rollView().scrollTicks).toBe(0));
        sendFrame(false, 0);
        sendFrame(true, BAR / 2);
        await edit();
        sendFrame(true, 8 * BAR);
        await waitFor(() => expect(roll.tops.at(-1)).toBeGreaterThanOrEqual(8 * BAR));
        return rollView().scrollTicks > 0;
      }

      /** The pointer at `tick` and `pitch` in the piano roll. */
      const atNote = (tick: number, pitch: number) => {
        const view = rollView();
        const keyHeight = view.keyHeight;
        return {
          clientX: 56 + (tick - view.scrollTicks) * view.pixelsPerTick + 2,
          clientY: 24 + (127 - pitch) * keyHeight - view.scrollY + keyHeight / 2,
          button: 0,
        };
      };

      it("keeps following with no edits", async () => {
        expect(await rollFollowsAfter(async () => {})).toBe(true);
      });

      it("pauses when you draw a note", async () => {
        const follows = await rollFollowsAfter(async () => {
          await act(async () => {
            fireEvent.pointerDown(notesArea(), atNote(BAR, 64));
            fireEvent.pointerUp(window);
          });
        });
        expect(sent("add_notes")).toHaveLength(1);
        expect(follows).toBe(false);
      });

      it("pauses when you delete a note", async () => {
        const follows = await rollFollowsAfter(async () => {
          await act(async () => {
            fireEvent.pointerDown(notesArea(), atNote(0, 60));
            fireEvent.pointerUp(window);
          });
          await key(notesArea(), "Backspace");
        });
        expect(sent("remove_notes")).toHaveLength(1);
        expect(follows).toBe(false);
      });
    });

    it("pauses the piano roll's following when you scroll it, until the next Play", async () => {
      project = {
        ...project,
        tracks: [
          { ...project.tracks[0], clips: [clip("clip-1", 0, 12 * BAR)] },
          project.tracks[1],
        ],
        songEnd: 13 * BAR,
      };
      await renderApp();
      await waitFor(() => expect(roll.notes.length).toBeGreaterThan(0));
      const rollView = () => roll.notes.at(-1)!.view;
      for (let i = 0; i < 6; i++) {
        fireEvent.click(screen.getByRole("button", { name: "Zoom in time" }));
      }
      sendFrame(true, 0);
      await act(async () => {
        fireEvent.wheel(screen.getByRole("application", { name: "Notes" }), { deltaX: -100_000 });
      });
      await waitFor(() => expect(rollView().scrollTicks).toBe(0));
      sendFrame(true, 8 * BAR);
      await waitFor(() => expect(roll.tops.at(-1)).toBeGreaterThanOrEqual(8 * BAR));
      expect(rollView().scrollTicks).toBe(0);

      sendFrame(false, 0);
      sendFrame(true, 8 * BAR);
      await waitFor(() => expect(rollView().scrollTicks).toBeGreaterThanOrEqual(8 * BAR));
    });
  });
});
