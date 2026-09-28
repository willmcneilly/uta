import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { NoteView, ProjectView } from "../backend";
import { type RecordingRenderer, projectView, recordingFactory } from "./testing";
import type { RendererFactory } from "./renderer";
import { pitchToY, tickToX } from "./viewport";

// Editing notes in the piano roll, against Tauri's mocked back end. The mock
// applies the note commands to its own project and sends it back, so the
// tests check both what the piano roll sends and that it draws the result.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

let calls: Call[];
let project: ProjectView;
/** The project as each gesture found it, so cancelling can put it back. */
let beforeGesture: Map<number, ProjectView>;
let renderer: RecordingRenderer;
let factory: RendererFactory;

const note = (id: string, pitch: number, start: number, length = 480): NoteView => ({
  id,
  pitch,
  velocity: 100,
  start,
  length,
});

const withNotes = (notes: NoteView[]): ProjectView => ({
  ...project,
  canUndo: true,
  track: { ...project.track, clip: { ...project.track.clip, notes } },
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
  ({ renderer, factory } = recordingFactory());
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      const notes = project.track.clip.notes;
      const gesture = args.gesture as number | null;
      if (gesture !== null && gesture !== undefined && !beforeGesture.has(gesture)) {
        beforeGesture.set(gesture, project);
      }
      switch (cmd) {
        case "get_project":
          return project;
        case "add_notes":
          project = withNotes([...notes, ...(args.notes as NoteView[])]);
          return project;
        case "set_notes": {
          const changed = args.notes as NoteView[];
          project = withNotes(notes.map((n) => changed.find((c) => c.id === n.id) ?? n));
          return project;
        }
        case "remove_notes": {
          const ids = args.notes as string[];
          project = withNotes(notes.filter((n) => !ids.includes(n.id)));
          return project;
        }
        case "cancel_gesture":
          project = beforeGesture.get(gesture as number) ?? project;
          return project;
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
function at(tick: number, pitch: number, modifiers: { metaKey?: boolean } = {}) {
  const view = renderer.lastView();
  return {
    clientX: tickToX(view, tick),
    clientY: pitchToY(view, pitch) + view.keyHeight / 2,
    button: 0,
    ...modifiers,
  };
}

async function press(tick: number, pitch: number, modifiers: { metaKey?: boolean } = {}) {
  await act(async () => {
    fireEvent.pointerDown(roll(), at(tick, pitch, modifiers));
  });
}

async function moveTo(tick: number, pitch: number, modifiers: { metaKey?: boolean } = {}) {
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
    await waitFor(() => expect(renderer.lastSelected()).toBe(added.id));
  });

  it("plays the note as you place it", async () => {
    await renderApp();
    await press(1000, 64);
    expect(sent("audition_note")).toEqual([{ pitch: 64, velocity: 100 }]);
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
    await waitFor(() => expect(renderer.lastSelected()).toBe("low"));
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
      { pitch: 63, velocity: 100 },
      { pitch: 58, velocity: 100 },
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
    await waitFor(() => expect(renderer.lastSelected()).toBeNull());
  });

  it("sends nothing if the drag hasn't changed anything", async () => {
    await renderApp();
    await press(240, 60);
    await key(window, "Escape");
    expect(edits()).toEqual([]);
    await waitFor(() => expect(renderer.lastSelected()).toBe("low"));
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
    await act(() => emit("project-changed", withNotes([note("high", 72, 3840)])));
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
