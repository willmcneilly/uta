import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type {
  ChainAddress,
  DelayParam,
  EffectKind,
  EffectView,
  Frame,
  ProjectView,
  TrackView,
} from "../backend";
import { knobAt, slideKnob } from "../design/knobTesting";
import {
  Updates,
  announceChange,
  drumTrackView,
  projectView,
  recordingFactory,
  trackView,
} from "../pianoRoll/testing";
import { delayView } from "./delayTesting";
import { ECHO_WIDTH } from "./EchoDrawing";

// The Sound tab's chain against Tauri's mocked back end, which keeps its own
// project and its own undo, so the tests check the UI draws what Rust sends
// back, never a copy of its own.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

let calls: Call[];
let project: ProjectView;
// The projects before each change, for the mock's undo.
let history: ProjectView[];
let updates: Updates;
// The frame stream, once the app subscribes.
let frames: Channel<Frame> | null;

/** Every chain in `project`, with `change` made to each effect. */
function eachEffect(change: (effect: EffectView) => EffectView): ProjectView {
  return {
    ...project,
    canUndo: true,
    masterEffects: project.masterEffects.map(change),
    tracks: project.tracks.map((track) => ({ ...track, effects: track.effects.map(change) })),
  };
}

/** `project` with the chain at `address` replaced by `change`'s. */
function changeChain(address: ChainAddress, change: (effects: EffectView[]) => EffectView[]): ProjectView {
  if (address === "master") return { ...project, canUndo: true, masterEffects: change(project.masterEffects) };
  return {
    ...project,
    canUndo: true,
    tracks: project.tracks.map((track) =>
      track.id === address.track ? { ...track, effects: change(track.effects) } : track,
    ),
  };
}

/** Which chain holds `effect`. */
function chainOf(effect: string): ChainAddress {
  if (project.masterEffects.some((other) => other.id === effect)) return "master";
  const track = project.tracks.find((t) => t.effects.some((other) => other.id === effect));
  if (!track) throw new Error(`unknown effect ${effect}`);
  return { track: track.id };
}

/** Applies a change to the mock's project, keeping the one before for undo. */
function apply(next: ProjectView) {
  history.push(project);
  project = next;
  return updates.send(project);
}

/** Undo, as the Edit menu's (⌘Z) does in Rust: the project goes back, and the UI is told. */
async function undo() {
  const previous = history.pop();
  if (!previous) throw new Error("nothing to undo");
  project = previous;
  await announceChange();
}

// jsdom doesn't lay anything out, so the piano roll is given a size.
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
  frames = null;
  history = [];
  updates = new Updates();
  project = projectView();
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      switch (cmd) {
        case "get_project":
          return updates.send(project);
        case "subscribe":
          frames = args.onFrame as Channel<Frame>;
          return null;
        case "get_notes":
          return updates.notes(project, args.clip as string);
        case "add_effect": {
          const address = args.chain as ChainAddress;
          return apply(changeChain(address, (effects) => [...effects, delayView(args.id as string)]));
        }
        case "remove_effects": {
          const ids = args.effects as string[];
          return apply(
            changeChain(chainOf(ids[0]), (effects) => effects.filter((e) => !ids.includes(e.id))),
          );
        }
        case "move_effect": {
          const id = args.effect as string;
          return apply(
            changeChain(chainOf(id), (effects) => {
              const moving = effects.find((e) => e.id === id) as EffectView;
              const rest = effects.filter((e) => e.id !== id);
              rest.splice(args.index as number, 0, moving);
              return rest;
            }),
          );
        }
        case "set_effect_enabled":
          return apply(
            eachEffect((e) => (e.id === args.effect ? { ...e, enabled: args.enabled as boolean } : e)),
          );
        case "set_effect_mix":
          return apply(
            eachEffect((e) =>
              e.id === args.effect ? { ...e, mix: { ...e.mix, value: Math.fround(args.mix as number) } } : e,
            ),
          );
        case "set_delay_param": {
          const param = args.param as DelayParam;
          return apply(
            eachEffect((e) =>
              e.id === args.effect
                ? {
                    ...e,
                    settings: e.settings.map((s) =>
                      s.name === param.name ? ({ ...s, value: param.value } as typeof s) : s,
                    ),
                  }
                : e,
            ),
          );
        }
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
});

afterEach(async () => {
  // Unmount while the mocks are still there: the app stops listening on the
  // way out, and that goes through them.
  cleanup();
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const sent = (cmd: string) => calls.filter((call) => call.cmd === cmd).map((call) => call.args);

/** Renders the app with `tracks`, and opens the Sound tab. */
async function openSound(tracks?: TrackView[]) {
  if (tracks) project = { ...project, tracks };
  const { factory } = recordingFactory();
  render(<App createRenderer={factory} />);
  await screen.findByRole("slider", { name: "Master volume" });
  fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
}

/** A synth track with `effects` in its chain. */
const synthWith = (...effects: EffectView[]): TrackView => ({ ...trackView("track-1", "Synth 1"), effects });

const panels = () => screen.queryAllByRole("region", { name: "Delay" });
const onKey = (panel: HTMLElement) => within(panel).getByRole("button", { name: "Delay on" });
const addKey = () => screen.getByRole("button", { name: /Add effect/ });

describe("the Sound tab's chain", () => {
  it("draws the chain from the outline: the synth, each effect in order, then +", async () => {
    await openSound([synthWith(delayView("d-1", { feedback: 0.5 }), delayView("d-2", { enabled: false }))]);

    expect(screen.getByRole("region", { name: "Synth" })).toBeInTheDocument();
    const [first, second] = panels();
    expect(first).toHaveAttribute("data-panel", "d-1");
    expect(second).toHaveAttribute("data-panel", "d-2");
    expect(onKey(first)).toHaveAttribute("aria-pressed", "true");
    expect(onKey(second)).toHaveAttribute("aria-pressed", "false");
    expect(second).toHaveAttribute("data-off");
    // Each control is drawn from Rust's description of the setting.
    expect(within(first).getByRole("slider", { name: "Delay feedback" })).toHaveAttribute(
      "aria-valuetext",
      "50%",
    );
    expect(within(first).getByRole("combobox", { name: "Delay time" })).toHaveValue("1/8d");
    expect(within(first).getByRole("slider", { name: "Delay mix" })).toBeInTheDocument();
    // The + comes after the last effect, and the line ends at the mixer.
    expect(second.compareDocumentPosition(addKey()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText("Mixer")).toBeInTheDocument();
    // The chain map names each panel in order.
    const map = screen.getByRole("navigation", { name: "Chain" });
    expect(within(map).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Synth",
      "Delay",
      "Delay",
    ]);
  });

  it("draws a Delay's echoes over bars at the project's tempo", async () => {
    // One echo a second after the note: half a bar at 120 BPM, a quarter at 60.
    const lone = delayView("d-1", { feedback: 0, wobble: 0, echoSeconds: 1 });
    const echoX = () =>
      Number(within(panels()[0]).getByRole("img", { name: "The echoes" }).querySelector<SVGGElement>(".echo")?.dataset.x);
    project = { ...project, bpm: 120 };
    await openSound([synthWith(lone)]);
    expect(echoX()).toBeCloseTo(0.5 + (ECHO_WIDTH - 1) / 2, 1);
    cleanup();
    project = { ...project, bpm: 60 };
    await openSound([synthWith(lone)]);
    expect(echoX()).toBeCloseTo(0.5 + (ECHO_WIDTH - 1) / 4, 1);
  });

  it("lights a Delay's echoes where the song is while it plays, at the project's tempo", async () => {
    const waiting: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => waiting.push(callback));
    vi.stubGlobal("cancelAnimationFrame", vi.fn());
    // A quarter note apart at 60 BPM: one echo a second.
    project = { ...project, bpm: 60 };
    await openSound([synthWith(delayView("d-1", { echoSeconds: 1 }))]);
    // The song is one beat in: a second, so the first echo has just sounded.
    act(() =>
      frames?.onmessage({
        playing: true,
        playhead: project.ticksPerQuarter,
        peak: 0,
        trackPeaks: {},
        clips: 0,
        dropouts: 0,
        slowestBlock: 0,
        output: {
          state: "running",
          device: "Test",
          sampleRate: 48000,
          bufferSize: 128,
          requestedBufferSize: 128,
          bufferSizes: [128],
        },
      }),
    );
    const now = performance.now();
    act(() => waiting.splice(0).forEach((callback) => callback(now)));
    const drawing = within(panels()[0]).getByRole("img", { name: "The echoes" });
    expect(drawing).toHaveAttribute("data-lighting", "playing");
    const lit = Array.from(drawing.querySelectorAll<SVGGElement>(".sounding"))
      .filter((mark) => Number(mark.style.opacity) > 0)
      .map((mark) => Number(mark.dataset.at));
    expect(lit).toEqual([1]);
  });

  it("shows a drum track's kit before its chain", async () => {
    await openSound([{ ...drumTrackView("track-1", "Drums 1"), effects: [delayView("d-1")] }]);
    expect(screen.getByRole("region", { name: "Kick" })).toBeInTheDocument();
    expect(panels()).toHaveLength(1);
  });

  it("+ lists the kinds of effect from Rust and adds one at the end, selected", async () => {
    await openSound([synthWith(delayView("d-1"))]);
    fireEvent.click(addKey());
    const menu = screen.getByRole("menu", { name: "Add effect" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Delay"]);
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Delay" }));

    await waitFor(() => expect(panels()).toHaveLength(2));
    const [added] = sent("add_effect");
    expect(added).toEqual({ chain: { track: "track-1" }, id: expect.any(String), kind: "delay" });
    expect(panels()[1]).toHaveAttribute("data-panel", added.id);
    expect(panels()[1]).toHaveAttribute("aria-current", "true");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    // The new panel has focus, so Delete deletes it straight away.
    expect(panels()[1]).toHaveFocus();
    fireEvent.keyDown(panels()[1], { key: "Delete" });
    await waitFor(() => expect(panels()).toHaveLength(1));
    expect(sent("remove_effects")).toEqual([{ effects: [added.id] }]);
  });

  it("+ opens its menu with the first kind focused, moves with the arrow keys, and closes on Esc", async () => {
    // A second kind, as Rust will send once there is one, so the keys have somewhere to go.
    const echo = { kind: "echo" as unknown as EffectKind, name: "Echo" };
    project = { ...project, effectKinds: [...project.effectKinds, echo] };
    await openSound([synthWith()]);
    fireEvent.click(addKey());
    const delay = screen.getByRole("menuitem", { name: "Delay" });
    const second = screen.getByRole("menuitem", { name: "Echo" });
    expect(delay).toHaveFocus();
    fireEvent.keyDown(delay, { key: "ArrowDown" });
    expect(second).toHaveFocus();
    fireEvent.keyDown(second, { key: "ArrowDown" });
    expect(delay).toHaveFocus();
    fireEvent.keyDown(delay, { key: "ArrowUp" });
    expect(second).toHaveFocus();
    fireEvent.keyDown(second, { key: "Home" });
    expect(delay).toHaveFocus();
    fireEvent.keyDown(delay, { key: "End" });
    expect(second).toHaveFocus();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(addKey()).toHaveFocus();
    expect(sent("add_effect")).toEqual([]);
  });

  it("opens the + menu against the window, to the right of +, or to its left at the window's edge", async () => {
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(120);
    const placeAt = (left: number) => {
      addKey().getBoundingClientRect = () => ({ left, right: left + 32, top: 500 }) as DOMRect;
    };
    await openSound([synthWith()]);
    // Near the start of the row, as on an empty master chain: it opens rightwards.
    placeAt(40);
    fireEvent.click(addKey());
    let menu = screen.getByRole("menu", { name: "Add effect" });
    expect(menu.style.left).toBe("72px");
    expect(menu.style.top).toBe("524px");
    fireEvent.keyDown(window, { key: "Escape" });

    // Near the window's right edge (jsdom's is 1024 wide): it opens leftwards.
    placeAt(950);
    fireEvent.click(addKey());
    menu = screen.getByRole("menu", { name: "Add effect" });
    expect(menu.style.left).toBe("830px");

    // Scrolling the row moves the +, so the menu closes rather than float away.
    fireEvent.scroll(window);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("greys out + when the chain holds eight", async () => {
    const eight = Array.from({ length: 8 }, (_, n) => delayView(`d-${n}`));
    await openSound([synthWith(...eight.slice(0, 7))]);
    expect(addKey()).toBeEnabled();
    fireEvent.click(addKey());
    fireEvent.click(screen.getByRole("menuitem", { name: "Delay" }));
    await waitFor(() => expect(panels()).toHaveLength(8));
    expect(addKey()).toBeDisabled();
    expect(addKey()).toHaveAttribute("title", "A chain holds at most 8 effects");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("the on/off key sends its command and shows what comes back", async () => {
    await openSound([synthWith(delayView("d-1"))]);
    fireEvent.click(onKey(panels()[0]));
    await waitFor(() => expect(panels()[0]).toHaveAttribute("data-off"));
    expect(sent("set_effect_enabled")).toEqual([{ effect: "d-1", enabled: false }]);
    expect(onKey(panels()[0])).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(onKey(panels()[0]));
    await waitFor(() => expect(panels()[0]).not.toHaveAttribute("data-off"));
    expect(sent("set_effect_enabled")).toEqual([
      { effect: "d-1", enabled: false },
      { effect: "d-1", enabled: true },
    ]);
  });

  describe("dragging a panel by its spine", () => {
    /** Lays the effect panels out 200 px wide, side by side after a 400 px synth, as jsdom doesn't. */
    function layOut() {
      panels().forEach((panel, index) => {
        const left = 400 + index * 200;
        panel.getBoundingClientRect = () => ({ left, right: left + 200 }) as DOMRect;
      });
    }
    const spine = (panel: HTMLElement) => within(panel).getByTitle("Drag to move");

    it("sends a move to where it's dropped, as one command", async () => {
      await openSound([synthWith(delayView("d-1"), delayView("d-2"), delayView("d-3"))]);
      layOut();
      fireEvent.pointerDown(spine(panels()[2]), { button: 0, clientX: 850 });
      fireEvent.pointerMove(window, { clientX: 560 });
      expect(panels()[1]).toHaveAttribute("data-drop", "before");
      fireEvent.pointerMove(window, { clientX: 450 });
      expect(panels()[0]).toHaveAttribute("data-drop", "before");
      fireEvent.pointerUp(window);

      await waitFor(() =>
        expect(panels().map((panel) => panel.dataset.panel)).toEqual(["d-3", "d-1", "d-2"]),
      );
      expect(sent("move_effect")).toEqual([{ effect: "d-3", index: 0 }]);
      expect(panels().some((panel) => panel.hasAttribute("data-drop"))).toBe(false);
    });

    it("sends nothing when dropped where it was, or on Esc", async () => {
      await openSound([synthWith(delayView("d-1"), delayView("d-2"))]);
      layOut();
      fireEvent.pointerDown(spine(panels()[0]), { button: 0, clientX: 500 });
      fireEvent.pointerMove(window, { clientX: 520 });
      fireEvent.pointerUp(window);
      fireEvent.pointerDown(spine(panels()[0]), { button: 0, clientX: 500 });
      fireEvent.pointerMove(window, { clientX: 750 });
      expect(panels()[1]).toHaveAttribute("data-drop", "after");
      fireEvent.keyDown(window, { key: "Escape" });
      fireEvent.pointerUp(window);
      expect(sent("move_effect")).toEqual([]);
    });
  });

  it("Delete removes the selected panel, and ⌘Z brings it back", async () => {
    await openSound([synthWith(delayView("d-1"), delayView("d-2"))]);
    const [, second] = panels();
    fireEvent.pointerDown(second);
    expect(second).toHaveAttribute("aria-current", "true");
    expect(second).toHaveFocus();
    fireEvent.keyDown(second, { key: "Delete" });

    await waitFor(() => expect(panels()).toHaveLength(1));
    expect(sent("remove_effects")).toEqual([{ effects: ["d-2"] }]);

    await undo();
    expect(panels().map((panel) => panel.dataset.panel)).toEqual(["d-1", "d-2"]);
  });

  it("Delete with nothing focused deletes the panel shown as selected", async () => {
    await openSound([synthWith(delayView("d-1"), delayView("d-2"))]);
    const [first] = panels();
    fireEvent.pointerDown(within(first).getByRole("slider", { name: "Delay mix" }));
    expect(first).toHaveAttribute("aria-current", "true");
    // A click on something that doesn't take focus leaves it on the page.
    (document.activeElement as HTMLElement).blur();
    // ⌘⌫ is Delete Track's: it's left to the menu bar.
    fireEvent.keyDown(document.body, { key: "Backspace", metaKey: true });
    fireEvent.keyDown(first, { key: "Backspace", metaKey: true });
    expect(sent("remove_effects")).toEqual([]);
    fireEvent.keyDown(document.body, { key: "Backspace" });
    await waitFor(() => expect(panels()).toHaveLength(1));
    expect(sent("remove_effects")).toEqual([{ effects: ["d-1"] }]);
  });

  it("Delete on one of a panel's knobs resets the knob, not the panel", async () => {
    await openSound([synthWith(delayView("d-1", { feedback: 0.7 }))]);
    const feedback = within(panels()[0]).getByRole("slider", { name: "Delay feedback" });
    fireEvent.focus(feedback);
    fireEvent.keyDown(feedback, { key: "Delete" });
    await waitFor(() =>
      expect(sent("set_delay_param")).toEqual([
        { effect: "d-1", param: { name: "feedback", value: 0.4 }, gesture: null },
      ]),
    );
    expect(sent("remove_effects")).toEqual([]);
  });

  it("Mix and the settings send their commands, one gesture per drag", async () => {
    await openSound([synthWith(delayView("d-1"))]);
    const mix = within(panels()[0]).getByRole("slider", { name: "Delay mix" });
    slideKnob(mix, 0.6, 0.8);
    await waitFor(() => expect(knobAt(mix)).toBeCloseTo(0.8, 2));
    const mixes = sent("set_effect_mix");
    expect(mixes.map((args) => args.effect)).toEqual(mixes.map(() => "d-1"));
    expect(mixes.at(-1)?.mix).toBeCloseTo(0.8, 2);
    expect(new Set(mixes.map((args) => args.gesture)).size).toBe(1);
    expect(mixes[0].gesture).toEqual(expect.any(Number));

    slideKnob(within(panels()[0]).getByRole("slider", { name: "Delay feedback" }), 0.9);
    await waitFor(() => expect(sent("set_delay_param").at(-1)?.param).toMatchObject({ name: "feedback" }));
    const feedback = sent("set_delay_param").at(-1)?.param as DelayParam;
    // 90% of the way round Feedback's range, which goes to 110%.
    expect(feedback.value).toBeCloseTo(0.99, 2);
  });

  it("folds a panel to its spine when its name is clicked", async () => {
    await openSound([synthWith(delayView("d-1"))]);
    fireEvent.click(screen.getByTitle("Fold Synth"));
    expect(screen.queryByRole("region", { name: "Synth" })).not.toBeInTheDocument();
    expect(screen.getByTitle("Unfold Synth")).toHaveAttribute("aria-expanded", "false");
    expect(panels()).toHaveLength(1);
    fireEvent.click(screen.getByTitle("Fold Delay"));
    expect(within(panels()[0]).queryByRole("slider")).not.toBeInTheDocument();
  });
});

describe("the master", () => {
  it("has a header row below the tracks with its volume and meter", async () => {
    await openSound();
    const master = screen.getByRole("region", { name: "Master" });
    expect(within(master).getByRole("slider", { name: "Master volume" })).toHaveAttribute(
      "aria-valuetext",
      "-12.0 dB",
    );
    expect(within(master).getByRole("img", { name: "Level meter" })).toBeInTheDocument();
    expect(within(master).getByRole("button", { name: "Master hasn't clipped" })).toBeInTheDocument();
  });

  it("selecting it shows the master's chain with no source, and Notes has nothing to show", async () => {
    project = { ...project, masterEffects: [delayView("m-1")] };
    await openSound([synthWith(delayView("d-1"), delayView("d-2"))]);
    // Start on Notes, to see selecting the master opens Sound.
    fireEvent.click(screen.getByRole("tab", { name: "Notes" }));
    fireEvent.pointerDown(screen.getByRole("region", { name: "Master" }));

    expect(screen.getByRole("tab", { name: "Sound" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("region", { name: "Master" })).toHaveAttribute("aria-current", "true");
    expect(panels().map((panel) => panel.dataset.panel)).toEqual(["m-1"]);
    expect(screen.queryByRole("region", { name: "Synth" })).not.toBeInTheDocument();
    expect(screen.getByText("Tracks")).toBeInTheDocument();
    expect(screen.getByText("Out")).toBeInTheDocument();
    // No track is selected while the master is.
    const synth = within(screen.getByRole("region", { name: "Tracks" })).getByRole("listitem", {
      name: "Synth 1",
    });
    expect(synth).not.toHaveAttribute("aria-current");

    // + adds to the master's chain.
    fireEvent.click(addKey());
    fireEvent.click(screen.getByRole("menuitem", { name: "Delay" }));
    await waitFor(() => expect(panels()).toHaveLength(2));
    expect(sent("add_effect")[0].chain).toBe("master");

    fireEvent.click(screen.getByRole("tab", { name: "Notes" }));
    expect(screen.getByText("The master has no notes. Its effects are on the Sound tab.")).toBeInTheDocument();

    // The Track menu's Delete and Duplicate leave the tracks alone while the master is selected.
    await act(async () => {
      await emit("track-menu", "delete-track");
      await emit("track-menu", "duplicate-track");
    });
    expect(sent("remove_track")).toEqual([]);
    expect(sent("duplicate_track")).toEqual([]);

    // Selecting a track again shows its chain.
    fireEvent.pointerDown(synth);
    fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
    expect(panels().map((panel) => panel.dataset.panel)).toEqual(["d-1", "d-2"]);
    expect(screen.getByRole("region", { name: "Synth" })).toBeInTheDocument();
  });

  it("can be selected from the keyboard by its name", async () => {
    await openSound();
    fireEvent.click(screen.getByRole("tab", { name: "Notes" }));
    const name = screen.getByRole("button", { name: "Master" });
    expect(name).toHaveAttribute("aria-pressed", "false");
    // A key's click is what Return and Space on it send.
    fireEvent.click(name);
    expect(screen.getByRole("region", { name: "Master" })).toHaveAttribute("aria-current", "true");
    expect(name).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("tab", { name: "Sound" })).toHaveAttribute("aria-selected", "true");
  });

  it("using its volume or clip light leaves the selected track and its notes open", async () => {
    await openSound();
    fireEvent.click(screen.getByRole("tab", { name: "Notes" }));
    const master = screen.getByRole("region", { name: "Master" });
    fireEvent.pointerDown(within(master).getByRole("slider", { name: "Master volume" }));
    fireEvent.focus(within(master).getByRole("slider", { name: "Master volume" }));
    fireEvent.pointerDown(within(master).getByRole("button", { name: "Master hasn't clipped" }));
    expect(master).not.toHaveAttribute("aria-current");
    expect(screen.getByRole("tab", { name: "Notes" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("region", { name: "Piano roll" })).toBeInTheDocument();
  });
});
