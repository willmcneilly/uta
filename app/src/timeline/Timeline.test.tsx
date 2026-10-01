import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { ClipPosition, ClipView, Frame, NoteView, ProjectView, TrackView } from "../backend";
import {
  type RecordingRenderer,
  projectView,
  recordingFactory,
  trackView,
} from "../pianoRoll/testing";
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

async function click(tick: number, track: number) {
  await press(tick, track);
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

const sent = (cmd: string) => calls.filter((call) => call.cmd === cmd).map((call) => call.args);
const edits = () =>
  calls.filter((call) => /clip|notes|gesture/.test(call.cmd)).map((call) => call.cmd);
const drawn = () => timeline.lastClips().map((c) => [c.id, c.track, c.start, c.length]);
const selectedClip = () => timeline.lastClips().find((c) => c.selected)?.id;
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
    await act(() =>
      emit("project-changed", {
        ...project,
        tracks: [project.tracks[0], { ...project.tracks[1], clips: [] }],
      }),
    );
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
});
