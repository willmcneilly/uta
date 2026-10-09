import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { EditMenuItem, Frame, NoteView, ProjectView } from "../backend";
import {
  HeldReplies,
  type RecordingRenderer,
  Updates,
  announceChange,
  firstClip,
  projectView,
  recordingFactory,
  withFirstClipNotes,
} from "./testing";
import type { RendererFactory } from "./renderer";
import { pitchToY, tickToX, velocityLane, velocityPerPixel } from "./viewport";

// Editing notes in the piano roll, against Tauri's mocked back end. The mock
// applies the note commands to its own project and sends it back, so the
// tests check both what the piano roll sends and that it draws the result.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

let calls: Call[];
let project: ProjectView;
// Rust's side of the updates: what it has sent the UI of each clip's notes.
let updates: Updates;
/** The project as each gesture found it, so cancelling can put it back. */
let beforeGesture: Map<number, ProjectView>;
let renderer: RecordingRenderer;
let factory: RendererFactory;
let frames: Channel<Frame> | null;

const note = (id: string, pitch: number, start: number, length = 480): NoteView => ({
  id,
  pitch,
  velocity: 100,
  start,
  length,
});

const withNotes = (notes: NoteView[]): ProjectView => ({
  ...withFirstClipNotes(project, notes),
  canUndo: true,
});

// jsdom doesn't lay anything out, so the piano roll is given a size: 800 px
// of notes (a 4-bar loop, so a sixteenth is 12.5 px) by 480.
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
  project = projectView({}, [note("low", 60, 0), note("high", 72, 3840)]);
  beforeGesture = new Map();
  frames = null;
  ({ renderer, factory } = recordingFactory());
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  updates = new Updates();
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      const notes = firstClip(project).notes;
      const gesture = args.gesture as number | null;
      if (gesture !== null && gesture !== undefined && !beforeGesture.has(gesture)) {
        beforeGesture.set(gesture, project);
      }
      switch (cmd) {
        case "get_project":
          return updates.send(project);
        case "get_notes":
          return updates.notes(project, args.clip as string);
        case "add_notes":
          project = withNotes([...notes, ...(args.notes as NoteView[])]);
          return updates.send(project);
        case "set_notes": {
          const changed = args.notes as NoteView[];
          project = withNotes(notes.map((n) => changed.find((c) => c.id === n.id) ?? n));
          return updates.send(project);
        }
        case "remove_notes": {
          const ids = args.notes as string[];
          project = withNotes(notes.filter((n) => !ids.includes(n.id)));
          return updates.send(project);
        }
        case "trim_notes":
          // Rust works out the trim; the tests check it's asked for.
          return updates.send(project);
        case "cancel_gesture":
          project = beforeGesture.get(gesture as number) ?? project;
          return updates.send(project);
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
  // The drawing loop runs on animation frames; wait for the first.
  await waitFor(() => expect(renderer.notes.length).toBeGreaterThan(0));
}

const roll = () => screen.getByRole("application", { name: "Notes" });

/** The pointer at `tick` (from the song's start) in the middle of `pitch`'s row. */
interface Modifiers {
  metaKey?: boolean;
  shiftKey?: boolean;
}

function at(tick: number, pitch: number, modifiers: Modifiers = {}) {
  const view = renderer.lastView();
  return {
    clientX: tickToX(view, tick),
    clientY: pitchToY(view, pitch) + view.keyHeight / 2,
    button: 0,
    ...modifiers,
  };
}

async function press(tick: number, pitch: number, modifiers: Modifiers = {}) {
  await act(async () => {
    fireEvent.pointerDown(roll(), at(tick, pitch, modifiers));
  });
}

async function moveTo(tick: number, pitch: number, modifiers: Modifiers = {}) {
  await act(async () => {
    fireEvent.pointerMove(window, at(tick, pitch, modifiers));
  });
}

async function release() {
  await act(async () => {
    fireEvent.pointerUp(window);
  });
}

async function key(target: Window | HTMLElement, name: string) {
  await act(async () => {
    fireEvent.keyDown(target, { key: name });
  });
}

/** Clicks a note: presses and releases without moving. */
async function click(tick: number, pitch: number, modifiers: Modifiers = {}) {
  await press(tick, pitch, modifiers);
  await release();
}

/** The pointer in the velocity lane at `tick`, `fromTop` pixels below its top. */
function inLane(tick: number, fromTop: number, modifiers: Modifiers = {}) {
  const view = renderer.lastView();
  return {
    clientX: tickToX(view, tick),
    clientY: velocityLane(view).y + fromTop,
    button: 0,
    ...modifiers,
  };
}

async function pressLane(tick: number, fromTop: number, modifiers: Modifiers = {}) {
  await act(async () => {
    fireEvent.pointerDown(roll(), inLane(tick, fromTop, modifiers));
  });
}

async function moveInLane(tick: number, fromTop: number) {
  await act(async () => {
    fireEvent.pointerMove(window, inLane(tick, fromTop));
  });
}

/** Chooses Copy, Paste or Duplicate from the Edit menu. */
async function menu(item: EditMenuItem) {
  await act(() => emit("edit-menu", item));
}

const sent = (cmd: string) => calls.filter((call) => call.cmd === cmd).map((call) => call.args);
const edits = () =>
  calls.filter((call) => /notes|gesture/.test(call.cmd)).map((call) => call.cmd);
const drawnNote = (id: string) => renderer.lastNotes().find((n) => n.id === id);

describe("drawing a note", () => {
  it("adds a note where you click, snapped to the grid, with its ID chosen first", async () => {
    await renderApp();
    // Just past beat 2, above the other notes.
    await press(1000, 64);
    await release();

    const [add] = sent("add_notes");
    const added = (add.notes as NoteView[])[0];
    expect(add.clip).toBe("clip-1");
    // A random (version 4) UUID, as Rust expects.
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    expect(added.id).toMatch(uuid);
    expect(added).toEqual({ id: added.id, pitch: 64, velocity: 100, start: 960, length: 240 });
    expect(add.gesture).toEqual(expect.any(Number));
    // It's drawn from what Rust sent back, and selected.
    await waitFor(() => expect(drawnNote(added.id)).toBeDefined());
    await waitFor(() => expect(renderer.lastSelected()).toEqual([added.id]));
  });

  it("plays the note as you place it", async () => {
    await renderApp();
    await press(1000, 64);
    expect(sent("audition_note")).toEqual([{ track: "track-1", pitch: 64, velocity: 100 }]);
  });

  it("drags out its length as part of the same gesture", async () => {
    await renderApp();
    await press(1000, 64);
    await moveTo(1000 + 700, 64);
    await release();

    const [add] = sent("add_notes");
    const [set] = sent("set_notes");
    const id = (add.notes as NoteView[])[0].id;
    // The end moves 700 ticks, from 1200 to 1900, and snaps to 1920.
    expect(set).toEqual({ clip: "clip-1", notes: [note(id, 64, 960, 960)], gesture: add.gesture });
    await waitFor(() => expect(drawnNote(id)?.length).toBe(960));
    expect(sent("audition_note")).toHaveLength(1);
  });

  it("uses the grid picked in the Snap menu", async () => {
    await renderApp();
    fireEvent.change(screen.getByLabelText("Snap"), { target: { value: "1/4" } });
    await press(1000, 64);
    const quarter = (sent("add_notes")[0].notes as NoteView[])[0];
    expect(quarter).toMatchObject({ start: 960, length: 960 });

    await release();
    fireEvent.change(screen.getByLabelText("Snap"), { target: { value: "off" } });
    await press(2000.4, 65);
    // Placed on the tick under the pointer, a sixteenth long.
    const added = (sent("add_notes")[1].notes as NoteView[])[0];
    expect(Math.abs(added.start - 2000)).toBeLessThanOrEqual(1);
    expect(added.start % 240).not.toBe(0);
    expect(added.length).toBe(240);
  });

  it("starts at 1/16", async () => {
    await renderApp();
    expect(screen.getByLabelText("Snap")).toHaveValue("1/16");
  });
});

describe("moving and resizing a note", () => {
  it("selects a note you click, without changing it", async () => {
    await renderApp();
    await press(240, 60);
    await moveTo(240 + 20, 60); // 1 px: not a drag
    await release();
    expect(edits()).toEqual([]);
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["low"]));
  });

  it("moves it in time and pitch, snapped, as one gesture", async () => {
    await renderApp();
    await press(240, 60);
    await moveTo(240 + 500, 62);
    await moveTo(240 + 740, 62);
    await moveTo(240 + 750, 62); // snaps to the same place: nothing to send
    await release();

    const sets = sent("set_notes");
    expect(sets.map((set) => set.notes)).toEqual([[note("low", 62, 480)], [note("low", 62, 720)]]);
    expect(sets[0].gesture).toEqual(expect.any(Number));
    expect(sets[1].gesture).toBe(sets[0].gesture);
    await waitFor(() => expect(drawnNote("low")).toMatchObject({ pitch: 62, start: 720 }));
  });

  it("gives each drag its own gesture", async () => {
    await renderApp();
    await press(240, 60);
    await moveTo(240 + 480, 60);
    await release();
    await press(480 + 240, 60);
    await moveTo(480 + 240 + 480, 60);
    await release();
    const [first, second] = sent("set_notes");
    expect(second.gesture).not.toBe(first.gesture);
  });

  it("plays the note when a drag moves it to a new pitch", async () => {
    await renderApp();
    await press(240, 60);
    await moveTo(240 + 480, 60); // time only
    await moveTo(240 + 480, 63);
    await moveTo(240 + 960, 63); // time only
    await moveTo(240 + 960, 58);
    expect(sent("audition_note")).toEqual([
      { track: "track-1", pitch: 63, velocity: 100 },
      { track: "track-1", pitch: 58, velocity: 100 },
    ]);
  });

  it("turns snapping off while ⌘ is held", async () => {
    await renderApp();
    await press(240, 60);
    await moveTo(240 + 100, 60, { metaKey: true });
    const start = (sent("set_notes")[0].notes as NoteView[])[0].start;
    expect(Math.abs(start - 100)).toBeLessThanOrEqual(1);
    await moveTo(240 + 100, 60);
    expect((sent("set_notes")[1].notes as NoteView[])[0].start).toBe(0);
  });

  it("resizes from its end", async () => {
    await renderApp();
    // "low" runs from 0 to 480; its last few pixels resize it.
    await press(470, 60);
    await moveTo(470 + 500, 61);
    expect(sent("set_notes")[0].notes).toEqual([note("low", 60, 0, 960)]);
  });

  it("resizes from its start", async () => {
    await renderApp();
    await press(3840 + 10, 72);
    await moveTo(3840 + 10 + 250, 72);
    expect(sent("set_notes")[0].notes).toEqual([note("high", 72, 4080, 240)]);
  });
});

describe("Esc during a drag", () => {
  it("puts the note back where the drag started", async () => {
    await renderApp();
    await press(240, 60);
    await moveTo(240 + 960, 67);
    await key(window, "Escape");
    const gesture = sent("set_notes")[0].gesture;
    expect(sent("cancel_gesture")).toEqual([{ gesture }]);
    await waitFor(() => expect(drawnNote("low")).toMatchObject({ pitch: 60, start: 0 }));

    // The drag is over: moving and releasing send nothing more.
    await moveTo(240 + 1920, 70);
    await release();
    expect(edits()).toEqual(["set_notes", "cancel_gesture"]);
  });

  it("removes a note that's being drawn", async () => {
    await renderApp();
    await press(1000, 64);
    await moveTo(1500, 64);
    await key(window, "Escape");
    const gesture = sent("add_notes")[0].gesture;
    expect(sent("cancel_gesture")).toEqual([{ gesture }]);
    await waitFor(() => expect(renderer.lastNotes()).toHaveLength(2));
    await waitFor(() => expect(renderer.lastSelected()).toEqual([]));
  });

  it("sends nothing if the drag hasn't changed anything", async () => {
    await renderApp();
    await press(240, 60);
    await key(window, "Escape");
    expect(edits()).toEqual([]);
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["low"]));
  });
});

describe("deleting a note", () => {
  it("removes the selected note with Backspace or Delete", async () => {
    await renderApp();
    await press(240, 60);
    await release();
    await key(roll(), "Backspace");
    expect(sent("remove_notes")).toEqual([{ clip: "clip-1", notes: ["low"] }]);
    await waitFor(() => expect(drawnNote("low")).toBeUndefined());

    await press(3840 + 240, 72);
    await release();
    await key(roll(), "Delete");
    expect(sent("remove_notes")[1]).toEqual({ clip: "clip-1", notes: ["high"] });
  });

  it("does nothing with no note selected", async () => {
    await renderApp();
    await key(roll(), "Backspace");
    // Or when the selected note has gone, undone from the menu, say.
    await press(240, 60);
    await release();
    project = withNotes([note("high", 72, 3840)]);
    await announceChange();
    await key(roll(), "Delete");
    expect(sent("remove_notes")).toEqual([]);
  });

  it("removes a note you double-click", async () => {
    await renderApp();
    await act(async () => {
      fireEvent.doubleClick(roll(), at(3840 + 240, 72));
    });
    expect(sent("remove_notes")).toEqual([{ clip: "clip-1", notes: ["high"] }]);
    await waitFor(() => expect(drawnNote("high")).toBeUndefined());
  });
});

describe("selecting several notes", () => {
  it("adds and removes notes with Shift-click", async () => {
    await renderApp();
    await click(240, 60);
    await click(3840 + 240, 72, { shiftKey: true });
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["high", "low"]));
    await click(240, 60, { shiftKey: true });
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["high"]));
    expect(edits()).toEqual([]);
  });

  it("box-selects with Shift-drag on empty space, adding to the selection", async () => {
    await renderApp();
    await click(3840 + 240, 72);
    // From empty space, back over "low" (0 to 480, at 60).
    await press(1920, 62, { shiftKey: true });
    await moveTo(100, 59);
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["high", "low"]));
    expect(renderer.boxes.at(-1)).not.toBeNull();
    // Shrinking the box leaves "low" out again.
    await moveTo(1000, 59);
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["high"]));
    await moveTo(100, 59);
    await release();
    await waitFor(() => expect(renderer.boxes.at(-1)).toBeNull());
    expect(renderer.lastSelected()).toEqual(["high", "low"]);
    // Box-selecting draws and changes nothing.
    expect(edits()).toEqual([]);
  });

  it("puts the selection back on Esc during a box", async () => {
    await renderApp();
    await click(3840 + 240, 72);
    await press(1920, 62, { shiftKey: true });
    await moveTo(100, 59);
    await key(window, "Escape");
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["high"]));
    await waitFor(() => expect(renderer.boxes.at(-1)).toBeNull());
  });

  it("clears the selection on Esc", async () => {
    await renderApp();
    await click(240, 60);
    await key(roll(), "Escape");
    await waitFor(() => expect(renderer.lastSelected()).toEqual([]));
  });

  it("selects just the note clicked from a larger selection", async () => {
    await renderApp();
    await click(240, 60);
    await click(3840 + 240, 72, { shiftKey: true });
    await click(240, 60);
    await waitFor(() => expect(renderer.lastSelected()).toEqual(["low"]));
  });
});

describe("moving and deleting a selection", () => {
  const selectBoth = async () => {
    await click(240, 60);
    await click(3840 + 240, 72, { shiftKey: true });
  };

  it("moves every selected note together, as one gesture", async () => {
    await renderApp();
    await selectBoth();
    await press(240, 60);
    await moveTo(240 + 960, 62);
    await moveTo(240 + 1920, 62);
    await release();

    const sets = sent("set_notes");
    expect(sets.map((set) => set.notes)).toEqual([
      [note("low", 62, 960), note("high", 74, 4800)],
      [note("low", 62, 1920), note("high", 74, 5760)],
    ]);
    expect(sets[1].gesture).toBe(sets[0].gesture);
    await waitFor(() => expect(drawnNote("high")).toMatchObject({ pitch: 74, start: 5760 }));
    // Still both selected, to move again.
    expect(renderer.lastSelected()).toEqual(["high", "low"]);
  });

  it("puts them all back on Esc", async () => {
    await renderApp();
    await selectBoth();
    await press(3840 + 240, 72);
    await moveTo(3840 + 240 + 960, 70);
    await key(window, "Escape");
    expect(sent("cancel_gesture")).toEqual([{ gesture: sent("set_notes")[0].gesture }]);
    await waitFor(() => expect(drawnNote("low")).toMatchObject({ pitch: 60, start: 0 }));
    expect(drawnNote("high")).toMatchObject({ pitch: 72, start: 3840 });
  });

  it("deletes the selection with Backspace, in one command", async () => {
    await renderApp();
    await selectBoth();
    await key(roll(), "Backspace");
    expect(sent("remove_notes")).toEqual([{ clip: "clip-1", notes: ["low", "high"] }]);
    await waitFor(() => expect(renderer.lastNotes()).toEqual([]));
    expect(renderer.lastSelected()).toEqual([]);
  });
});

describe("copy, paste and duplicate from the Edit menu", () => {
  const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

  it("pastes the copied notes at the playhead, snapped to the grid, as one AddNotes", async () => {
    await renderApp();
    await click(240, 60);
    await click(3840 + 240, 72, { shiftKey: true });
    await menu("copy");
    expect(edits()).toEqual([]);

    // The playhead just past bar 3: the paste snaps back to it.
    await waitFor(() => expect(frames).not.toBeNull());
    act(() =>
      frames!.onmessage({
        playing: false,
        playhead: 7680 + 100,
        peak: 0,
        trackPeaks: {},
        clips: 0,
        dropouts: 0,
        slowestBlock: 0,
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
    await menu("paste");

    const adds = sent("add_notes");
    expect(adds).toHaveLength(1);
    const pasted = adds[0].notes as NoteView[];
    expect(pasted.map(({ pitch, start }) => [pitch, start])).toEqual([
      [60, 7680],
      [72, 7680 + 3840],
    ]);
    for (const added of pasted) expect(added.id).toMatch(uuid);
    expect(adds[0].gesture).toEqual(expect.any(Number));
    // The pasted notes are drawn from what Rust sent back, and selected.
    await waitFor(() => expect(renderer.lastNotes()).toHaveLength(4));
    expect(renderer.lastSelected()).toEqual(pasted.map((n) => n.id).sort());

    // The clipboard stays for another paste, with new IDs.
    await menu("paste");
    const again = sent("add_notes")[1].notes as NoteView[];
    expect(again.map((n) => n.start)).toEqual([7680, 7680 + 3840]);
    expect(again.map((n) => n.id)).not.toContain(pasted[0].id);
  });

  it("duplicates the selection right after itself, rounded up to the beat", async () => {
    await renderApp();
    // "low" runs from 0 to 480: its copy goes a beat on.
    await click(240, 60);
    await menu("duplicate");
    const [first] = sent("add_notes");
    const copy = (first.notes as NoteView[])[0];
    expect(copy).toEqual({ ...note("low", 60, 960), id: copy.id });
    expect(copy.id).toMatch(uuid);
    await waitFor(() => expect(renderer.lastSelected()).toEqual([copy.id]));

    // Duplicating again repeats the copy, since it's now selected.
    await menu("duplicate");
    expect((sent("add_notes")[1].notes as NoteView[])[0].start).toBe(1920);
  });

  it("duplicates a spread of notes past the last one's end", async () => {
    await renderApp();
    await click(240, 60);
    await click(3840 + 240, 72, { shiftKey: true });
    await menu("duplicate");
    // From 0 to 4320, rounded up to 4800.
    const copies = sent("add_notes")[0].notes as NoteView[];
    expect(copies.map(({ pitch, start }) => [pitch, start])).toEqual([
      [60, 4800],
      [72, 8640],
    ]);
  });

  it("does nothing with nothing selected or copied", async () => {
    await renderApp();
    await menu("copy");
    await menu("paste");
    await menu("duplicate");
    expect(edits()).toEqual([]);
  });
});

describe("trimming the notes an edit covers", () => {
  // "low" runs from 0 to 480 and "next", the same pitch, from 960 to 1440.
  beforeEach(() => {
    project = projectView({}, [note("low", 60, 0), note("next", 60, 960), note("high", 72, 3840)]);
  });

  it("trims once a lengthen ends, never mid-drag, as part of its gesture", async () => {
    await renderApp();
    await press(470, 60);
    await moveTo(470 + 1440, 60); // over "next"
    await moveTo(470, 60); // and back
    await moveTo(470 + 720, 60); // half over it
    expect(sent("trim_notes")).toEqual([]);
    await release();

    const sets = sent("set_notes");
    expect(sets.map((set) => set.notes)).toEqual([
      [note("low", 60, 0, 1920)],
      [note("low", 60, 0, 480)],
      [note("low", 60, 0, 1200)],
    ]);
    expect(sent("trim_notes")).toEqual([
      { clip: "clip-1", notes: ["low"], gesture: sets[0].gesture },
    ]);
    expect(edits()).toEqual(["set_notes", "set_notes", "set_notes", "trim_notes"]);
  });

  it("trims once a move of a selection ends, with every moved note", async () => {
    await renderApp();
    await click(240, 60);
    await click(3840 + 240, 72, { shiftKey: true });
    await press(240, 60);
    await moveTo(240 + 960, 60);
    expect(sent("trim_notes")).toEqual([]);
    await moveTo(240 + 480, 60);
    await release();

    const sets = sent("set_notes");
    expect(sets.at(-1)?.notes).toEqual([note("low", 60, 480), note("high", 72, 4320)]);
    expect(sent("trim_notes")).toEqual([
      { clip: "clip-1", notes: ["low", "high"], gesture: sets[0].gesture },
    ]);
  });

  it("trims under a drawn note when it's placed", async () => {
    await renderApp();
    await press(500, 60); // in the gap: draws from 480
    await moveTo(500 + 480, 60); // out over "next"
    expect(sent("trim_notes")).toEqual([]);
    await release();
    const [add] = sent("add_notes");
    const id = (add.notes as NoteView[])[0].id;
    expect(sent("set_notes")).toEqual([
      { clip: "clip-1", notes: [note(id, 60, 480, 720)], gesture: add.gesture },
    ]);
    expect(sent("trim_notes")).toEqual([{ clip: "clip-1", notes: [id], gesture: add.gesture }]);
  });

  it("trims nothing for a click, Esc, or a velocity drag", async () => {
    await renderApp();
    await click(240, 60);
    await press(240, 60);
    await moveTo(240 + 960, 60);
    await key(window, "Escape");
    await release();
    await pressLane(0, 30);
    await moveInLane(0, 50);
    await release();
    expect(sent("set_notes")).toHaveLength(2);
    expect(sent("trim_notes")).toEqual([]);
  });

  it("trims under a paste, as part of its gesture", async () => {
    await renderApp();
    await click(240, 60);
    await menu("copy");
    // The playhead is at 0, so the copy lands on "low" itself.
    await menu("paste");
    const [add] = sent("add_notes");
    const ids = (add.notes as NoteView[]).map((n) => n.id);
    expect(sent("trim_notes")).toEqual([{ clip: "clip-1", notes: ids, gesture: add.gesture }]);
    expect(edits()).toEqual(["add_notes", "trim_notes"]);
  });

  it("trims under a duplicate, as part of its gesture", async () => {
    await renderApp();
    await click(240, 60);
    // "low"'s copy goes a beat on, onto "next".
    await menu("duplicate");
    const [add] = sent("add_notes");
    const copy = (add.notes as NoteView[])[0];
    expect(copy).toEqual({ ...note("low", 60, 960), id: copy.id });
    expect(sent("trim_notes")).toEqual([
      { clip: "clip-1", notes: [copy.id], gesture: add.gesture },
    ]);
    expect(edits()).toEqual(["add_notes", "trim_notes"]);
  });
});

describe("the velocity lane", () => {
  /** The velocity change for dragging `pixels` up the lane. */
  const change = (pixels: number) => Math.round(pixels * velocityPerPixel(renderer.lastView()));

  it("drags a note's bar to change its velocity, as one gesture", async () => {
    await renderApp();
    await pressLane(0, 30);
    await moveInLane(0, 40);
    await moveInLane(0, 50);
    await release();

    const sets = sent("set_notes");
    expect(sets.map((set) => set.notes)).toEqual([
      [{ ...note("low", 60, 0), velocity: 100 - change(10) }],
      [{ ...note("low", 60, 0), velocity: 100 - change(20) }],
    ]);
    expect(sets[1].gesture).toBe(sets[0].gesture);
    await waitFor(() => expect(drawnNote("low")?.velocity).toBe(100 - change(20)));
    expect(renderer.lastSelected()).toEqual(["low"]);
  });

  it("changes every selected note together", async () => {
    await renderApp();
    project = withNotes([note("low", 60, 0), { ...note("high", 72, 3840), velocity: 40 }]);
    await announceChange();
    await click(240, 60);
    await click(3840 + 240, 72, { shiftKey: true });
    await pressLane(3840, 40);
    await moveInLane(3840, 30);
    await release();

    expect(sent("set_notes").map((set) => set.notes)).toEqual([
      [
        { ...note("low", 60, 0), velocity: 100 + change(10) },
        { ...note("high", 72, 3840), velocity: 40 + change(10) },
      ],
    ]);
  });

  it("puts the velocities back on Esc", async () => {
    await renderApp();
    await pressLane(0, 30);
    await moveInLane(0, 60);
    await key(window, "Escape");
    expect(sent("cancel_gesture")).toEqual([{ gesture: sent("set_notes")[0].gesture }]);
    await waitFor(() => expect(drawnNote("low")?.velocity).toBe(100));
  });

  it("does nothing where there's no bar", async () => {
    await renderApp();
    await pressLane(1920, 30);
    await moveInLane(1920, 60);
    await release();
    expect(edits()).toEqual([]);
    expect(renderer.lastSelected()).toEqual([]);
  });
});

describe("a drag while Rust is slow to reply", () => {
  const NOTE_COMMANDS = ["add_notes", "set_notes", "trim_notes", "cancel_gesture"];

  beforeEach(() => {
    project = projectView({}, [note("low", 60, 0), note("next", 60, 1920), note("high", 72, 3840)]);
  });

  it("sends one step at a time, the newest next, then the final one, then the trim", async () => {
    await renderApp();
    const held = new HeldReplies(NOTE_COMMANDS);
    await press(240, 60);
    await moveTo(240 + 480, 60);
    await moveTo(240 + 960, 61);
    await moveTo(240 + 1440, 62);
    await moveTo(240 + 1920, 60); // onto "next"
    await release();

    expect(held.inFlight).toBe(1);
    expect(edits()).toEqual(["set_notes"]);
    await held.reply();
    expect(held.inFlight).toBe(1);
    await held.replyToAll();

    const sets = sent("set_notes");
    expect(sets.map((set) => set.notes)).toEqual([[note("low", 60, 480)], [note("low", 60, 1920)]]);
    expect(sets[1].gesture).toBe(sets[0].gesture);
    // The trim goes after the final step, as part of the same gesture.
    expect(edits()).toEqual(["set_notes", "set_notes", "trim_notes"]);
    expect(sent("trim_notes")).toEqual([
      { clip: "clip-1", notes: ["low"], gesture: sets[0].gesture },
    ]);
    await waitFor(() => expect(drawnNote("low")).toMatchObject({ pitch: 60, start: 1920 }));
  });

  it("on Esc, drops the waiting step and cancels after the one in flight", async () => {
    await renderApp();
    const held = new HeldReplies(NOTE_COMMANDS);
    await press(240, 60);
    await moveTo(240 + 480, 62);
    await moveTo(240 + 960, 64);
    await key(window, "Escape");
    await release();
    await held.replyToAll();

    const [set] = sent("set_notes");
    expect(set.notes).toEqual([note("low", 62, 480)]);
    expect(edits()).toEqual(["set_notes", "cancel_gesture"]);
    expect(sent("cancel_gesture")).toEqual([{ gesture: set.gesture }]);
    await waitFor(() => expect(drawnNote("low")).toMatchObject({ pitch: 60, start: 0 }));
  });

  it("adds a drawn note even when the add has to wait, before any of its steps", async () => {
    await renderApp();
    const held = new HeldReplies(NOTE_COMMANDS);
    // A note drag whose step is still in flight, so the drawing's add waits.
    await press(240, 60);
    await moveTo(240 + 480, 60);
    await release();
    await press(2400, 64);
    await moveTo(2900, 64);
    await moveTo(3400, 64);
    await release();
    expect(held.inFlight).toBe(1);
    await held.replyToAll();

    expect(edits()).toEqual(["set_notes", "trim_notes", "add_notes", "set_notes", "trim_notes"]);
    const [added] = sent("add_notes")[0].notes as NoteView[];
    const drawingSet = sent("set_notes")[1];
    expect(drawingSet.gesture).toBe(sent("add_notes")[0].gesture);
    expect((drawingSet.notes as NoteView[])[0].id).toBe(added.id);
    await waitFor(() => expect(drawnNote(added.id)).toBeDefined());
  });

  it("on Esc while drawing a note, still adds it first, then cancels it", async () => {
    await renderApp();
    const held = new HeldReplies(NOTE_COMMANDS);
    await press(1000, 64);
    await moveTo(1500, 64);
    await moveTo(2000, 64);
    await key(window, "Escape");
    await held.replyToAll();

    expect(edits()).toEqual(["add_notes", "cancel_gesture"]);
    const gesture = sent("add_notes")[0].gesture;
    expect(sent("cancel_gesture")).toEqual([{ gesture }]);
    await waitFor(() => expect(renderer.lastNotes()).toHaveLength(3));
  });
});

describe("hovering over a note", () => {
  const hover = async (target: { clientX: number; clientY: number }) => {
    await act(async () => {
      fireEvent.pointerMove(roll(), target);
    });
  };
  const hovered = () => renderer.marks.at(-1)?.hovered?.id ?? null;

  it("marks the note under the pointer, and shows whether pressing would move or resize it", async () => {
    await renderApp();
    await hover(at(240, 60));
    await waitFor(() => expect(hovered()).toBe("low"));
    expect(roll().style.cursor).toBe("grab");
    await hover(at(478, 60));
    expect(roll().style.cursor).toBe("ew-resize");
    await hover(at(2, 60));
    expect(roll().style.cursor).toBe("ew-resize");
    await hover(at(1920, 66));
    await waitFor(() => expect(hovered()).toBeNull());
    expect(roll().style.cursor).toBe("");
  });

  it("marks the note whose velocity stem the pointer is over", async () => {
    await renderApp();
    await hover(inLane(0, 30));
    await waitFor(() => expect(hovered()).toBe("low"));
    expect(roll().style.cursor).toBe("ns-resize");
  });

  it("forgets the note when the pointer leaves", async () => {
    await renderApp();
    await hover(at(240, 60));
    await waitFor(() => expect(hovered()).toBe("low"));
    await act(async () => {
      fireEvent.pointerLeave(roll());
    });
    await waitFor(() => expect(hovered()).toBeNull());
  });

  it("shows a closed hand while moving a note, and clears it when the drag ends", async () => {
    await renderApp();
    await hover(at(240, 60));
    await press(240, 60);
    expect(roll().style.cursor).toBe("grabbing");
    await moveTo(240 + 500, 62);
    await release();
    expect(roll().style.cursor).toBe("");
    await waitFor(() => expect(hovered()).toBeNull());
  });
});

describe("the notes sounding now", () => {
  const report = (playing: boolean, playhead: number) =>
    act(() =>
      frames!.onmessage({
        playing,
        playhead,
        peak: 0,
        trackPeaks: {},
        clips: 0,
        dropouts: 0,
        slowestBlock: 0,
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
  const sounding = () => renderer.marks.at(-1)?.sounding.map((n) => n.id) ?? [];

  it("are the notes under the playhead while playing, worked out from where it is", async () => {
    await renderApp();
    await waitFor(() => expect(frames).not.toBeNull());
    report(true, 100);
    await waitFor(() => expect(sounding()).toEqual(["low"]));
    report(true, 3840 + 100);
    await waitFor(() => expect(sounding()).toEqual(["high"]));
    report(false, 3840 + 100);
    await waitFor(() => expect(sounding()).toEqual([]));
  });

  it("are none while stopped, even with the playhead on a note", async () => {
    await renderApp();
    await waitFor(() => expect(frames).not.toBeNull());
    report(false, 100);
    await waitFor(() => expect(renderer.tops.at(-1)).toBe(100));
    expect(sounding()).toEqual([]);
  });
});
