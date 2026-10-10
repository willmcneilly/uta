import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { ClipView, EditMenuItem, Frame, KitRowView, NoteView, ProjectView } from "../backend";
import {
  KIT_ROWS,
  type RecordingRenderer,
  Updates,
  drumTrackView,
  projectView,
  recordingFactory,
  trackView,
} from "./testing";
import type { RendererFactory } from "./renderer";
import { noteArea, pitchToY, tickToX } from "./viewport";

// A drum track's Notes tab: the piano roll with the kit's lanes in place of
// the keyboard (RFC-006, "In the window"), against Tauri's mocked back end.
// The mock applies the note commands to its own project and sends it back.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

let calls: Call[];
let project: ProjectView;
let updates: Updates;
let renderer: RecordingRenderer;
let factory: RendererFactory;
let frames: Channel<Frame> | null;

const note = (id: string, pitch: number, start: number, length = 240): NoteView => ({
  id,
  pitch,
  velocity: 100,
  start,
  length,
});

const clip = (id: string, notes: NoteView[]): ClipView => ({ id, start: 0, length: 15_360, notes });

/** Drums 1 on top, so it's what the Notes tab opens on, and Synth 1 under it. */
function withKit(rows: KitRowView[] = KIT_ROWS): ProjectView {
  const drums = drumTrackView("drums", "Drums 1", [
    clip("beat", [note("kick", 36, 0), note("snare", 38, 960)]),
  ]);
  return projectView({
    tracks: [
      { ...drums, source: { kind: "drums", kit: { rows } } },
      trackView("synth", "Synth 1", [
        clip("tune", [note("on-kit", 50, 0), note("c4", 60, 480), note("e4", 64, 960)]),
      ]),
    ],
  });
}

/** `notes` in place of clip `id`'s. */
function withClipNotes(id: string, change: (notes: NoteView[]) => NoteView[]): ProjectView {
  return {
    ...project,
    canUndo: true,
    tracks: project.tracks.map((track) => ({
      ...track,
      clips: track.clips.map((c) => (c.id === id ? { ...c, notes: change(c.notes) } : c)),
    })),
  };
}

// jsdom doesn't lay anything out, so the piano roll is given a size.
class FixedSizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe() {
    const entry = { contentRect: { width: 888, height: 504 } } as ResizeObserverEntry;
    this.callback([entry], this as unknown as ResizeObserver);
  }
  unobserve() {}
  disconnect() {}
}

beforeEach(() => {
  calls = [];
  project = withKit();
  frames = null;
  ({ renderer, factory } = recordingFactory());
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  updates = new Updates();
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      const id = args.clip as string;
      switch (cmd) {
        case "get_project":
        case "trim_notes":
          return updates.send(project);
        case "get_notes":
          return updates.notes(project, id);
        case "add_notes":
          project = withClipNotes(id, (notes) => [...notes, ...(args.notes as NoteView[])]);
          return updates.send(project);
        case "set_notes": {
          const changed = args.notes as NoteView[];
          project = withClipNotes(id, (notes) =>
            notes.map((n) => changed.find((c) => c.id === n.id) ?? n),
          );
          return updates.send(project);
        }
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

async function renderApp() {
  render(<App createRenderer={factory} />);
  await screen.findByRole("application", { name: "Notes" });
  await waitFor(() => expect(renderer.notes.length).toBeGreaterThan(0));
  await waitFor(() => expect(frames).not.toBeNull());
}

const roll = () => screen.getByRole("application", { name: "Notes" });
const lanes = () => within(screen.getByRole("group", { name: "Drum sounds" }));
const sent = (cmd: string) => calls.filter((call) => call.cmd === cmd).map((call) => call.args);

/** The pointer at `tick` in the middle of `pitch`'s lane. */
function at(tick: number, pitch: number, modifiers: { shiftKey?: boolean } = {}) {
  const view = renderer.lastView();
  return {
    clientX: tickToX(view, tick),
    clientY: pitchToY(view, pitch) + view.keyHeight / 2,
    button: 0,
    ...modifiers,
  };
}

async function press(tick: number, pitch: number, modifiers: { shiftKey?: boolean } = {}) {
  await act(async () => {
    fireEvent.pointerDown(roll(), at(tick, pitch, modifiers));
  });
}

async function moveTo(tick: number, pitch: number) {
  await act(async () => {
    fireEvent.pointerMove(window, at(tick, pitch));
  });
}

async function release() {
  await act(async () => {
    fireEvent.pointerUp(window);
  });
}

async function click(tick: number, pitch: number, modifiers: { shiftKey?: boolean } = {}) {
  await press(tick, pitch, modifiers);
  await release();
}

async function menu(item: EditMenuItem) {
  await act(() => emit("edit-menu", item));
}

/**
 * Selects a track by its header, which opens its first clip in the Notes
 * tab, then points at the editor, so the Edit menu acts on the piano roll.
 */
async function selectTrack(name: string) {
  const headers = within(screen.getByRole("region", { name: "Tracks" }));
  await act(async () => {
    fireEvent.pointerDown(headers.getByRole("listitem", { name }));
  });
  await act(async () => {
    fireEvent.pointerDown(screen.getByRole("tab", { name: "Notes" }));
  });
}

/** What the piano roll says about the last edit, if anything. */
const notice = () =>
  within(screen.getByRole("region", { name: "Piano roll" })).queryByRole("status");

/** The pitches the last set_notes sent, in order. */
const lastSetPitches = () => (sent("set_notes").at(-1)?.notes as NoteView[]).map((n) => n.pitch);

describe("a drum track's lanes", () => {
  it("are the kit's rows from the outline, kick at the bottom, all in view", async () => {
    // Rust names the rows; the UI shows whatever it's sent.
    const renamed = KIT_ROWS.map((row, i) => (i === 0 ? { ...row, name: "Bass drum" } : row));
    project = withKit(renamed);
    await renderApp();
    const labels = lanes()
      .getAllByRole("button")
      .map((button) => button.textContent);
    expect(labels).toEqual([...renamed].reverse().map((row) => row.name));
    expect(labels.at(-1)).toBe("Bass drum");
    // All eight share the height, so none needs scrolling to.
    const view = renderer.lastView();
    expect(view.rows.count).toBe(8);
    expect(view.keyHeight * 8).toBeCloseTo(noteArea(view).height);
    expect(view.scrollY).toBe(0);
    // They don't zoom in pitch, so there are no keys to.
    expect(screen.queryByRole("button", { name: "Zoom in pitch" })).toBeNull();
    expect(screen.getByRole("button", { name: "Zoom in time" })).toBeInTheDocument();
  });

  it("play their sound when a label is clicked, without changing the project", async () => {
    await renderApp();
    fireEvent.click(lanes().getByRole("button", { name: "Snare" }));
    fireEvent.click(lanes().getByRole("button", { name: "Closed hat" }));
    expect(sent("audition_note")).toEqual([
      { track: "drums", pitch: 38, velocity: 100 },
      { track: "drums", pitch: 42, velocity: 100 },
    ]);
    expect(calls.filter((call) => /notes/.test(call.cmd))).toEqual([]);
  });

  it("draw a note on the lane clicked, at its sound's note", async () => {
    await renderApp();
    // The closed hat's lane is above the snare's, whatever their notes.
    await click(1920, 42);
    const added = (sent("add_notes")[0].notes as NoteView[])[0];
    expect(added).toMatchObject({ pitch: 42, start: 1920 });
    expect(sent("audition_note")).toEqual([{ track: "drums", pitch: 42, velocity: 100 }]);
  });

  it("move a dragged note a lane at a time, staying on the kit", async () => {
    await renderApp();
    await press(960 + 100, 38);
    // Up one lane: from the snare to the clap.
    await moveTo(960 + 100, 39);
    expect(lastSetPitches()).toEqual([39]);
    // Up another: the low tom, the next lane up, not the next note.
    await moveTo(960 + 100, 45);
    expect(lastSetPitches()).toEqual([45]);
    // Far above the top lane, it stops at the cymbal.
    await act(async () => {
      fireEvent.pointerMove(window, { ...at(960 + 100, 49), clientY: -500 });
    });
    expect(lastSetPitches()).toEqual([49]);
    await release();
    // Each lane it reached was heard.
    expect(sent("audition_note").map((a) => a.pitch)).toEqual([39, 45, 49]);
  });

  it("move a selection a lane with the arrow keys, keeping its shape", async () => {
    await renderApp();
    await click(100, 36);
    await click(960 + 100, 38, { shiftKey: true });
    await act(async () => {
      fireEvent.keyDown(roll(), { key: "ArrowUp" });
    });
    // The kick to the snare, the snare to the clap.
    expect(lastSetPitches()).toEqual([38, 39]);
    await waitFor(() =>
      expect(renderer.lastNotes().map((n) => n.pitch).sort()).toEqual([38, 39]),
    );
    await act(async () => {
      fireEvent.keyDown(roll(), { key: "ArrowDown" });
    });
    expect(lastSetPitches()).toEqual([36, 38]);
    // The kick is on the bottom lane, so they go no lower.
    const before = sent("set_notes").length;
    await act(async () => {
      fireEvent.keyDown(roll(), { key: "ArrowDown" });
    });
    expect(sent("set_notes")).toHaveLength(before);
  });
});

describe("pasting into a drum clip", () => {
  /** Copies every note of Synth 1's clip, then opens Drums 1 again. */
  async function copySynthNotes() {
    await renderApp();
    await selectTrack("Synth 1");
    await waitFor(() => expect(renderer.lastView().rows.lanes).toBeNull());
    // Down far enough that D3 (50, the high tom's note) is in view.
    const top = renderer.lastView().scrollY;
    fireEvent.wheel(roll(), { deltaY: 200 });
    await waitFor(() => expect(renderer.lastView().scrollY).toBe(top + 200));
    await click(100, 50);
    await click(480 + 100, 60, { shiftKey: true });
    await click(960 + 100, 64, { shiftKey: true });
    await menu("copy");
    await selectTrack("Drums 1");
    await waitFor(() => expect(renderer.lastView().rows.lanes).not.toBeNull());
  }

  it("keeps only the notes on the kit's lanes, and says how many it left out", async () => {
    await copySynthNotes();
    await menu("paste");
    const [add] = sent("add_notes");
    expect(add.clip).toBe("beat");
    expect((add.notes as NoteView[]).map((n) => n.pitch)).toEqual([50]);
    expect(notice()).toHaveTextContent(
      "Left out 2 notes that aren't on the drum kit's rows.",
    );
  });

  it("pastes nothing when none of the notes are on the kit, and says so", async () => {
    project = withKit();
    project.tracks[1].clips[0].notes = [note("c4", 60, 480), note("c5", 72, 960)];
    await renderApp();
    await selectTrack("Synth 1");
    await waitFor(() => expect(renderer.lastView().rows.lanes).toBeNull());
    await click(480 + 100, 60);
    await click(960 + 100, 72, { shiftKey: true });
    await menu("copy");
    await selectTrack("Drums 1");
    await waitFor(() => expect(renderer.lastView().rows.lanes).not.toBeNull());
    await menu("paste");
    expect(sent("add_notes")).toEqual([]);
    expect(notice()).toHaveTextContent(
      "Nothing pasted: the 2 notes aren't on the drum kit's rows.",
    );
  });

  it("says nothing when every note is on the kit", async () => {
    await renderApp();
    await click(100, 36);
    await menu("copy");
    await menu("paste");
    expect(sent("add_notes")).toHaveLength(1);
    expect(notice()).toBeNull();
  });
});

describe("a drum track's Sound tab", () => {
  it("says the drum panel is coming, in the empty-panel style", async () => {
    await renderApp();
    fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
    const panel = screen.getByRole("tabpanel");
    expect(within(panel).getByText(/The drum panel is coming/)).toHaveClass("empty");
    expect(within(panel).queryByRole("radiogroup")).toBeNull();
  });
});

describe("+ Drums", () => {
  it("adds a drum track, picking its ID", async () => {
    await renderApp();
    const add = within(screen.getByRole("group", { name: "Add track" }));
    fireEvent.click(add.getByRole("button", { name: "+ Drums" }));
    const [added] = sent("add_track");
    expect(added).toEqual({ id: expect.stringMatching(/^[0-9a-f-]{36}$/), kind: "drums" });
  });
});
