import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { type BenchmarkOptions, type Clock, DEFAULT_OPTIONS } from "./benchmark/run";
import type {
  Frame,
  MixerView,
  NoteView,
  ProjectView,
  SynthParam,
  SynthView,
  TrackView,
} from "./backend";
import {
  type RecordingRenderer,
  projectView,
  recordingFactory,
  trackView,
} from "./pianoRoll/testing";
import type { RendererFactory } from "./pianoRoll/renderer";

// Tauri's mocked back end stands in for Rust. It keeps its own project so the
// tests can check the UI shows whatever Rust sends back, never its own copy.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

const SYNTH_FIELDS: Record<SynthParam["name"], keyof SynthView> = {
  waveform: "waveform",
  cutoff_hz: "cutoffHz",
  resonance: "resonance",
  attack_seconds: "attackSeconds",
  decay_seconds: "decaySeconds",
  sustain: "sustain",
  release_seconds: "releaseSeconds",
};

let calls: Call[];
let project: ProjectView;
let frames: Channel<Frame> | null;
let failWith: string | null;
let renderer: RecordingRenderer;
let factory: RendererFactory;

const view = (volumeDb: number, notes: NoteView[] = []): ProjectView =>
  projectView({ volumeDb }, notes);

const note = (id: string, pitch: number, start: number, velocity = 100): NoteView => ({
  id,
  pitch,
  velocity,
  start,
  length: 480,
});

const frame = (overrides: Partial<Frame> = {}): Frame => ({
  playing: false,
  playhead: 0,
  peak: 0,
  trackPeaks: {},
  clips: 0,
  dropouts: 0,
  output: {
    state: "running",
    device: "MacBook Pro Speakers",
    sampleRate: 48000,
    bufferSize: 128,
    requestedBufferSize: 128,
    bufferSizes: [32, 64, 128],
  },
  ...overrides,
});

/** `project` with the track `id` changed by `change`, as Rust would send it back. */
function changeTrack(id: string, change: (track: TrackView) => TrackView): ProjectView {
  return {
    ...project,
    canUndo: true,
    tracks: project.tracks.map((track) => (track.id === id ? change(track) : track)),
  };
}

/** The name Rust gives a new track: one more than the highest number in use. */
function nextName(): string {
  const numbers = project.tracks.map((track) => Number(track.name.replace("Synth ", "")));
  return `Synth ${Math.max(0, ...numbers) + 1}`;
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
  project = view(-12, [note("low", 60, 0), note("high", 72, 3840, 30)]);
  frames = null;
  failWith = null;
  ({ renderer, factory } = recordingFactory());
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      if (failWith) throw new Error(failWith);
      switch (cmd) {
        case "get_project":
          return project;
        case "set_volume":
          // Rust rounds to the nearest dB here, to show the UI renders what
          // comes back rather than what it sent.
          project = { ...project, volumeDb: Math.round(args.volumeDb as number), canUndo: true };
          return project;
        case "set_tempo":
          project = { ...project, bpm: args.bpm as number, canUndo: true };
          return project;
        case "set_loop":
          project = {
            ...project,
            loopStart: (args.startBar as number) * 3840,
            loopLength: (args.bars as number) * 3840,
            canUndo: true,
          };
          return project;
        case "set_loop_enabled":
          project = { ...project, loopEnabled: args.enabled as boolean, canUndo: true };
          return project;
        case "set_synth_param": {
          const param = args.param as SynthParam;
          // Rust stores f32s, so what comes back isn't quite what was sent.
          const value = param.name === "waveform" ? param.value : Math.fround(param.value);
          project = changeTrack(args.track as string, (track) => ({
            ...track,
            synth: { ...track.synth, [SYNTH_FIELDS[param.name]]: value },
          }));
          return project;
        }
        case "set_track_mixer": {
          const mixer = args.mixer as MixerView;
          // Rust stores f32s, so what comes back isn't quite what was sent.
          project = changeTrack(args.track as string, (track) => ({
            ...track,
            mixer: { ...mixer, volumeDb: Math.fround(mixer.volumeDb), pan: Math.fround(mixer.pan) },
          }));
          return project;
        }
        case "solo_track_alone":
          project = {
            ...project,
            canUndo: true,
            tracks: project.tracks.map((track) => ({
              ...track,
              mixer: { ...track.mixer, solo: track.id === args.track },
            })),
          };
          return project;
        case "add_track":
          project = {
            ...project,
            canUndo: true,
            tracks: [...project.tracks, trackView(args.id as string, nextName())],
          };
          return project;
        case "duplicate_track": {
          const index = project.tracks.findIndex((track) => track.id === args.track);
          const copy = {
            ...project.tracks[index],
            id: args.id as string,
            name: nextName(),
            clips: project.tracks[index].clips.map((clip) => ({ ...clip, id: `${clip.id}-copy` })),
          };
          const tracks = [...project.tracks];
          tracks.splice(index + 1, 0, copy);
          project = { ...project, canUndo: true, tracks };
          return project;
        }
        case "remove_track":
          project = {
            ...project,
            canUndo: true,
            tracks: project.tracks.filter((track) => track.id !== args.track),
          };
          return project;
        case "move_track": {
          const moving = project.tracks.find((track) => track.id === args.track)!;
          const tracks = project.tracks.filter((track) => track !== moving);
          tracks.splice(args.index as number, 0, moving);
          project = { ...project, canUndo: true, tracks };
          return project;
        }
        case "add_stress_notes":
          // What's added is Rust's business; the tests check where it goes.
          return project;
        case "build_test_song": {
          // Two tracks of two clips of three notes, named after the song.
          const song = args.song as string;
          const clip = (t: number, c: number) => ({
            id: `${song}-clip-${t}-${c}`,
            start: c * 3840,
            length: 3840,
            notes: [0, 1, 2].map((n) => note(`${song}-note-${t}-${c}-${n}`, 60 + n, n * 480)),
          });
          project = {
            ...project,
            canUndo: true,
            tracks: [1, 2].map((t) =>
              trackView(`${song}-track-${t}`, `Synth ${t}`, [clip(t, 0), clip(t, 1)]),
            ),
          };
          return project;
        }
        case "remove_notes": {
          const gone = new Set(args.notes as string[]);
          project = {
            ...project,
            canUndo: true,
            tracks: project.tracks.map((track) => ({
              ...track,
              clips: track.clips.map((clip) =>
                clip.id === args.clip
                  ? { ...clip, notes: clip.notes.filter((n) => !gone.has(n.id)) }
                  : clip,
              ),
            })),
          };
          return project;
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
  // Unmount while the mocks are still there: the app stops listening on the
  // way out, and that goes through them.
  cleanup();
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
  vi.unstubAllGlobals();
});

const commands = () => calls.map((call) => call.cmd);

async function renderApp() {
  render(<App createRenderer={factory} />);
  await screen.findByRole("slider", { name: /Volume/ });
  await waitFor(() => expect(frames).not.toBeNull());
}

const volume = () => screen.getByRole("slider", { name: /Volume/ });
const tempo = () => screen.getByRole("slider", { name: /Tempo/ });
const loopSwitch = () => screen.getByRole("button", { name: "Loop" });
const drawnNotes = () => renderer.lastNotes().map((n) => [n.id, n.pitch, n.start, n.velocity]);

function sendFrame(overrides: Partial<Frame> = {}) {
  act(() => frames!.onmessage(frame(overrides)));
}

describe("App", () => {
  it("shows the project's volume from Rust", async () => {
    await renderApp();
    expect(volume()).toHaveValue("-12");
    expect(screen.getByText("-12.0 dB")).toBeInTheDocument();
  });

  it("shows the output, buffer size and dropouts from the frame stream", async () => {
    await renderApp();
    sendFrame({ dropouts: 3 });
    expect(screen.getByTestId("device")).toHaveTextContent("MacBook Pro Speakers · 48 kHz");
    expect(screen.getByLabelText("Buffer")).toHaveValue("128");
    expect(screen.getByTestId("dropouts")).toHaveTextContent("3");
  });

  it("shows the transport state and position in bars and beats from the frame stream", async () => {
    await renderApp();
    expect(screen.getByTestId("transport")).toHaveTextContent("Stopped");
    expect(screen.getByTestId("position")).toHaveTextContent("1.1");
    // Bar 3, beat 2, and a bit.
    sendFrame({ playing: true, playhead: 2 * 3840 + 960 + 100 });
    expect(screen.getByTestId("transport")).toHaveTextContent("Playing");
    expect(screen.getByTestId("position")).toHaveTextContent("3.2");
  });

  it("shows the project's tempo and loop switch from Rust, and no loop length control", async () => {
    await renderApp();
    expect(tempo()).toHaveValue("120");
    expect(screen.getByText("120 BPM")).toBeInTheDocument();
    expect(loopSwitch()).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("slider", { name: /Loop/ })).not.toBeInTheDocument();
  });

  it("sends tempo changes to Rust, one gesture per drag", async () => {
    await renderApp();
    fireEvent.pointerDown(tempo());
    fireEvent.change(tempo(), { target: { value: "128" } });
    fireEvent.change(tempo(), { target: { value: "140" } });
    fireEvent.pointerUp(window);
    fireEvent.pointerDown(tempo());
    fireEvent.change(tempo(), { target: { value: "90" } });

    await waitFor(() => expect(screen.getByText("90 BPM")).toBeInTheDocument());
    const sent = calls.filter((c) => c.cmd === "set_tempo").map((c) => c.args);
    expect(sent.map((args) => args.bpm)).toEqual([128, 140, 90]);
    expect(sent[0].gesture).toEqual(expect.any(Number));
    expect(sent[1].gesture).toBe(sent[0].gesture);
    expect(sent[2].gesture).not.toBe(sent[0].gesture);
  });

  it("switches the loop off and on, and shows what Rust sends back", async () => {
    await renderApp();
    fireEvent.click(loopSwitch());
    await waitFor(() => expect(loopSwitch()).toHaveAttribute("aria-pressed", "false"));
    fireEvent.click(loopSwitch());
    await waitFor(() => expect(loopSwitch()).toHaveAttribute("aria-pressed", "true"));
    const sent = calls.filter((c) => c.cmd === "set_loop_enabled").map((c) => c.args);
    expect(sent).toEqual([{ enabled: false }, { enabled: true }]);
  });

  it("never gives drags of different controls the same gesture", async () => {
    await renderApp();
    fireEvent.pointerDown(volume());
    fireEvent.change(volume(), { target: { value: "-20" } });
    fireEvent.pointerUp(window);
    fireEvent.pointerDown(tempo());
    fireEvent.change(tempo(), { target: { value: "100" } });
    await waitFor(() => expect(commands()).toContain("set_tempo"));
    const volumeGesture = calls.find((c) => c.cmd === "set_volume")!.args.gesture;
    const tempoGesture = calls.find((c) => c.cmd === "set_tempo")!.args.gesture;
    expect(tempoGesture).not.toBe(volumeGesture);
  });

  it("draws the notes Rust sends in the piano roll", async () => {
    await renderApp();
    await waitFor(() =>
      expect(drawnNotes()).toEqual([
        ["low", 60, 0, 100],
        ["high", 72, 3840, 30],
      ]),
    );
    // Undo, say, sends a new project from Rust.
    await act(() => emit("project-changed", view(-12, [note("other", 64, 960, 90)])));
    await waitFor(() => expect(drawnNotes()).toEqual([["other", 64, 960, 90]]));
  });

  it("draws the loop Rust sends in the piano roll's grid", async () => {
    await renderApp();
    await waitFor(() => expect(renderer.grids.at(-1)?.loopEnd).toBe(4 * 3840));
    fireEvent.click(loopSwitch());
    await waitFor(() => expect(renderer.grids.at(-1)?.loopEnabled).toBe(false));
  });

  it("draws the playhead where the engine reports it", async () => {
    await renderApp();
    sendFrame({ playhead: 1234 });
    await waitFor(() => expect(renderer.tops.at(-1)).toBe(1234));
  });

  it("zooms the piano roll in and out", async () => {
    await renderApp();
    await waitFor(() => expect(renderer.notes.length).toBeGreaterThan(0));
    const zoom = () => renderer.notes.at(-1)!.view;
    const before = zoom();
    fireEvent.click(screen.getByRole("button", { name: "Zoom in time" }));
    await waitFor(() => expect(zoom().pixelsPerTick).toBeGreaterThan(before.pixelsPerTick));
    fireEvent.click(screen.getByRole("button", { name: "Zoom in pitch" }));
    await waitFor(() => expect(zoom().keyHeight).toBeGreaterThan(before.keyHeight));
  });

  it("shows how long frames take", async () => {
    await renderApp();
    expect(screen.getByTestId("frame-time")).toBeInTheDocument();
  });

  it("draws the level meter on a canvas", async () => {
    await renderApp();
    expect(screen.getByRole("img", { name: "Level meter" }).tagName).toBe("CANVAS");
  });

  it("sends Play and Stop to Rust", async () => {
    await renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Play" }));
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(commands()).toContain("stop"));
    expect(commands().filter((cmd) => cmd === "play" || cmd === "stop")).toEqual(["play", "stop"]);
  });

  describe("Space", () => {
    const transport = () =>
      commands().filter((cmd) => ["play", "stop", "pause", "resume"].includes(cmd));
    const press = (target: Element | Window, init: KeyboardEventInit = {}) => {
      const down = fireEvent.keyDown(target, { key: " ", code: "Space", ...init });
      const up = fireEvent.keyUp(target, { key: " ", code: "Space", ...init });
      // `false` when the key's own action was prevented.
      return { down, up };
    };

    it("plays while stopped, and stops while playing", async () => {
      await renderApp();
      press(document.body);
      sendFrame({ playing: true });
      press(document.body);
      sendFrame({ playing: false });
      press(document.body);
      await waitFor(() => expect(transport()).toEqual(["play", "stop", "play"]));
    });

    it("with Shift, pauses while playing, and carries on while stopped", async () => {
      await renderApp();
      sendFrame({ playing: true });
      press(document.body, { shiftKey: true });
      sendFrame({ playing: false, playhead: 5000 });
      press(document.body, { shiftKey: true });
      await waitFor(() => expect(transport()).toEqual(["pause", "resume"]));
    });

    it("works with a button focused, without pressing it", async () => {
      await renderApp();
      const stopButton = screen.getByRole("button", { name: "Stop" });
      stopButton.focus();
      const { down, up } = press(stopButton);
      expect([down, up]).toEqual([false, false]);
      await waitFor(() => expect(transport()).toEqual(["play"]));
    });

    it("works with a slider focused", async () => {
      await renderApp();
      press(volume());
      await waitFor(() => expect(transport()).toEqual(["play"]));
    });

    it("types a space in a text field instead", async () => {
      await renderApp();
      const field = document.createElement("input");
      document.body.append(field);
      const { down } = press(field);
      press(field, { shiftKey: true });
      field.remove();
      expect(down).toBe(true);
      expect(transport()).toEqual([]);
    });

    it("leaves ⌘, Ctrl and ⌥ with Space alone, and ignores a held key repeating", async () => {
      await renderApp();
      press(document.body, { metaKey: true });
      press(document.body, { ctrlKey: true });
      press(document.body, { altKey: true });
      fireEvent.keyDown(document.body, { key: " ", repeat: true });
      expect(transport()).toEqual([]);
    });
  });

  it("sends volume changes to Rust and shows what comes back", async () => {
    await renderApp();
    fireEvent.change(volume(), { target: { value: "-20.5" } });
    await waitFor(() => expect(screen.getByText("-20.0 dB")).toBeInTheDocument());
    const call = calls.find((c) => c.cmd === "set_volume")!;
    expect(call.args).toEqual({ volumeDb: -20.5, gesture: null });
  });

  it("marks every change in one drag with the same gesture", async () => {
    await renderApp();
    const slider = volume();
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "-20" } });
    fireEvent.change(slider, { target: { value: "-30" } });
    fireEvent.pointerUp(window);
    fireEvent.change(slider, { target: { value: "-40" } });
    fireEvent.pointerDown(slider);
    fireEvent.change(slider, { target: { value: "-50" } });

    await waitFor(() => expect(commands().filter((c) => c === "set_volume")).toHaveLength(4));
    const gestures = calls.filter((c) => c.cmd === "set_volume").map((c) => c.args.gesture);
    expect(gestures[0]).toEqual(expect.any(Number));
    expect(gestures[1]).toBe(gestures[0]);
    expect(gestures[2]).toBeNull();
    expect(gestures[3]).toEqual(expect.any(Number));
    expect(gestures[3]).not.toBe(gestures[0]);
  });

  describe("synth panel", () => {
    const synth = () => within(screen.getByRole("region", { name: "Synth" }));
    const slider = (name: string) =>
      synth().getByRole("slider", { name: new RegExp(`^${name}`) });
    const synthCalls = () => calls.filter((c) => c.cmd === "set_synth_param").map((c) => c.args);

    async function renderSound() {
      await renderApp();
      fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
    }

    it("shows the synth settings Rust sends", async () => {
      await renderSound();
      expect(synth().getByRole("radio", { name: "Saw" })).toBeChecked();
      expect(slider("Cutoff")).toHaveAttribute("aria-valuetext", "20.0 kHz");
      expect(slider("Cutoff")).toHaveValue("1000");
      expect(slider("Resonance")).toHaveAttribute("aria-valuetext", "0.00");
      expect(slider("Attack")).toHaveAttribute("aria-valuetext", "5.0 ms");
      expect(slider("Decay")).toHaveAttribute("aria-valuetext", "200 ms");
      expect(slider("Sustain")).toHaveAttribute("aria-valuetext", "70%");
      expect(slider("Sustain")).toHaveValue("700");
      expect(slider("Release")).toHaveAttribute("aria-valuetext", "200 ms");
    });

    it("sends each waveform to Rust and shows what comes back", async () => {
      await renderSound();
      for (const [label, value] of [
        ["Sine", "sine"],
        ["Triangle", "triangle"],
        ["Square", "square"],
        ["Saw", "saw"],
      ]) {
        fireEvent.click(synth().getByRole("radio", { name: label }));
        await waitFor(() => expect(synth().getByRole("radio", { name: label })).toBeChecked());
        expect(synthCalls().at(-1)).toEqual({
          track: "track-1",
          param: { name: "waveform", value },
          gesture: null,
        });
      }
    });

    it("focuses a waveform when it's clicked, and moves with the arrow keys", async () => {
      await renderSound();
      const radio = (name: string) => synth().getByRole("radio", { name });
      // Only the checked waveform is in the Tab order.
      expect(radio("Saw")).toHaveAttribute("tabindex", "0");
      expect(radio("Sine")).toHaveAttribute("tabindex", "-1");

      fireEvent.click(radio("Triangle"));
      await waitFor(() => expect(radio("Triangle")).toBeChecked());
      expect(radio("Triangle")).toHaveFocus();

      fireEvent.keyDown(radio("Triangle"), { key: "ArrowRight" });
      await waitFor(() => expect(radio("Saw")).toBeChecked());
      expect(radio("Saw")).toHaveFocus();
      fireEvent.keyDown(radio("Saw"), { key: "ArrowDown" });
      await waitFor(() => expect(radio("Square")).toBeChecked());
      // The ends wrap round.
      fireEvent.keyDown(radio("Square"), { key: "ArrowRight" });
      await waitFor(() => expect(radio("Sine")).toBeChecked());
      fireEvent.keyDown(radio("Sine"), { key: "ArrowUp" });
      await waitFor(() => expect(radio("Square")).toBeChecked());
      expect(radio("Square")).toHaveFocus();
      expect(radio("Square")).toHaveAttribute("tabindex", "0");

      const waveforms = synthCalls().map((args) => (args.param as SynthParam).value);
      expect(waveforms).toEqual(["triangle", "saw", "square", "sine", "square"]);
    });

    it("sends each slider's setting to Rust and shows what comes back", async () => {
      await renderSound();
      // Halfway along each slider: log scales land on the geometric middle.
      const cases: [string, SynthParam["name"], number, string][] = [
        ["Cutoff", "cutoff_hz", 632.456, "632 Hz"],
        ["Resonance", "resonance", 0.5, "0.50"],
        ["Attack", "attack_seconds", 0.1, "100 ms"],
        ["Decay", "decay_seconds", 0.1, "100 ms"],
        ["Sustain", "sustain", 0.5, "50%"],
        ["Release", "release_seconds", 0.1, "100 ms"],
      ];
      for (const [label, name, value, shown] of cases) {
        fireEvent.change(slider(label), { target: { value: "500" } });
        await waitFor(() => expect(slider(label)).toHaveAttribute("aria-valuetext", shown));
        expect(slider(label)).toHaveValue("500");
        const sent = synthCalls().at(-1)!;
        expect(sent.track).toBe("track-1");
        expect(sent.gesture).toBeNull();
        const param = sent.param as SynthParam;
        expect(param.name).toBe(name);
        expect(param.value).toBeCloseTo(value, 3);
      }
    });

    it("sends the limits exactly at each end of a slider", async () => {
      await renderSound();
      fireEvent.change(slider("Cutoff"), { target: { value: "0" } });
      fireEvent.change(slider("Release"), { target: { value: "1000" } });
      await waitFor(() => expect(synthCalls()).toHaveLength(2));
      expect(synthCalls().map((args) => args.param)).toEqual([
        { name: "cutoff_hz", value: 20 },
        { name: "release_seconds", value: 10 },
      ]);
    });

    it("marks every change in one drag with the same gesture", async () => {
      await renderSound();
      fireEvent.pointerDown(slider("Cutoff"));
      fireEvent.change(slider("Cutoff"), { target: { value: "800" } });
      fireEvent.change(slider("Cutoff"), { target: { value: "600" } });
      fireEvent.pointerUp(window);
      fireEvent.pointerDown(slider("Sustain"));
      fireEvent.change(slider("Sustain"), { target: { value: "300" } });
      fireEvent.pointerUp(window);
      fireEvent.change(slider("Sustain"), { target: { value: "200" } });

      await waitFor(() => expect(synthCalls()).toHaveLength(4));
      const gestures = synthCalls().map((args) => args.gesture);
      expect(gestures[0]).toEqual(expect.any(Number));
      expect(gestures[1]).toBe(gestures[0]);
      expect(gestures[2]).toEqual(expect.any(Number));
      expect(gestures[2]).not.toBe(gestures[0]);
      expect(gestures[3]).toBeNull();
    });

    it("follows undo and redo, which Rust announces", async () => {
      await renderSound();
      const undone = projectView();
      undone.tracks[0].synth = {
        waveform: "square",
        cutoffHz: 200,
        resonance: 0.8,
        attackSeconds: 1,
        decaySeconds: 0.01,
        sustain: 0.25,
        releaseSeconds: 2.5,
      };
      await act(() => emit("project-changed", undone));
      expect(synth().getByRole("radio", { name: "Square" })).toBeChecked();
      expect(slider("Cutoff")).toHaveValue("333");
      expect(slider("Cutoff")).toHaveAttribute("aria-valuetext", "200 Hz");
      expect(slider("Resonance")).toHaveValue("800");
      expect(slider("Attack")).toHaveValue("750");
      expect(slider("Attack")).toHaveAttribute("aria-valuetext", "1.00 s");
      expect(slider("Decay")).toHaveAttribute("aria-valuetext", "10 ms");
      expect(slider("Sustain")).toHaveValue("250");
      expect(slider("Release")).toHaveAttribute("aria-valuetext", "2.50 s");
    });
  });

  it("shows undo and redo from the menu, which Rust announces", async () => {
    await renderApp();
    await act(() => emit("project-changed", view(-3)));
    expect(volume()).toHaveValue("-3");
  });

  it("offers only the buffer sizes the device supports", async () => {
    await renderApp();
    sendFrame({
      output: { ...frame().output, bufferSizes: [64, 128] },
    });
    const options = within(screen.getByLabelText("Buffer")).getAllByRole<HTMLOptionElement>(
      "option",
    );
    expect(options.map((o) => [o.value, o.disabled])).toEqual([
      ["64", false],
      ["128", false],
    ]);
  });

  it("sends a new buffer size to Rust", async () => {
    await renderApp();
    sendFrame();
    fireEvent.change(screen.getByLabelText("Buffer"), { target: { value: "64" } });
    await waitFor(() => expect(commands()).toContain("set_buffer_size"));
    expect(calls.find((c) => c.cmd === "set_buffer_size")!.args).toEqual({ size: 64 });
  });

  it("says when the device uses a different buffer size", async () => {
    await renderApp();
    sendFrame({
      output: {
        ...frame().output,
        bufferSize: 64,
        requestedBufferSize: 32,
        bufferSizes: [64, 128],
      },
    });
    expect(screen.getByText(/The device is using 64/)).toBeInTheDocument();
    const unsupported = screen.getByRole<HTMLOptionElement>("option", { name: "32 samples" });
    expect(unsupported.disabled).toBe(true);
  });

  it("says when there's no output device", async () => {
    await renderApp();
    sendFrame({ output: { ...frame().output, state: "waiting" } });
    expect(screen.getByTestId("device")).toHaveTextContent("No output device");
  });

  it("shows errors from Rust", async () => {
    await renderApp();
    failWith = "the engine's command queue is full";
    fireEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("command queue is full");
  });

  describe("tracks", () => {
    /** Three tracks: the first as usual, a muted one panned left, and a soloed one with no clips. */
    function threeTracks() {
      const [first] = project.tracks;
      const second: TrackView = {
        ...trackView("track-2", "Synth 2", [
          {
            id: "clip-2",
            start: 0,
            length: 3840,
            notes: [note("bass", 60, 0)],
          },
        ]),
        mixer: { volumeDb: -6, pan: -0.5, mute: true, solo: false },
        synth: { ...first.synth, waveform: "square" },
      };
      const third: TrackView = {
        ...trackView("track-3", "Synth 3"),
        mixer: { volumeDb: 3, pan: 1, mute: false, solo: true },
      };
      project = { ...project, tracks: [first, second, third] };
    }

    const headers = () => within(screen.getByRole("region", { name: "Tracks" }));
    const header = (name: string) => headers().getByRole("listitem", { name });
    // A header is selected as soon as the pointer goes down on it, so a
    // drag of its volume selects it too.
    const select = (name: string) => fireEvent.pointerDown(header(name));
    const headerNames = () =>
      headers()
        .getAllByRole("listitem")
        .map((item) => item.getAttribute("aria-label"));
    const sent = (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args);
    const openTab = (name: "Notes" | "Sound") => fireEvent.click(screen.getByRole("tab", { name }));
    const synthWaveform = () =>
      within(screen.getByRole("region", { name: "Synth" }))
        .getAllByRole<HTMLInputElement>("radio")
        .find((radio) => radio.checked)
        ?.closest("label")?.textContent;

    it("draws a header for every track Rust sends, in order", async () => {
      threeTracks();
      await renderApp();
      expect(headerNames()).toEqual(["Synth 1", "Synth 2", "Synth 3"]);

      const second = within(header("Synth 2"));
      expect(second.getByRole("slider", { name: "Synth 2 volume" })).toHaveValue("-6");
      expect(second.getByText("-6.0 dB")).toBeInTheDocument();
      expect(second.getByRole("slider", { name: "Synth 2 pan" })).toHaveValue("-0.5");
      expect(second.getByText("L 50")).toBeInTheDocument();
      expect(second.getByRole("button", { name: "Mute Synth 2" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
      expect(second.getByRole("button", { name: "Solo Synth 2" })).toHaveAttribute(
        "aria-pressed",
        "false",
      );
      expect(second.getByRole("img", { name: "Synth 2 meter" }).tagName).toBe("CANVAS");

      const third = within(header("Synth 3"));
      expect(third.getByRole("slider", { name: "Synth 3 volume" })).toHaveValue("3");
      expect(third.getByText("R 100")).toBeInTheDocument();
      expect(third.getByRole("button", { name: "Solo Synth 3" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );

      // Undo, say, sends a new project from Rust.
      await act(() => emit("project-changed", projectView()));
      expect(headerNames()).toEqual(["Synth 1"]);
    });

    it("offers volumes from -60 to +6 dB", async () => {
      await renderApp();
      const slider = header("Synth 1").querySelector("input[aria-label='Synth 1 volume']");
      expect(slider).toHaveAttribute("min", "-60");
      expect(slider).toHaveAttribute("max", "6");
    });

    it("sends volume and pan drags to Rust, one gesture per drag, and shows what comes back", async () => {
      threeTracks();
      await renderApp();
      const volume = () => screen.getByRole("slider", { name: "Synth 2 volume" });
      const pan = () => screen.getByRole("slider", { name: "Synth 2 pan" });
      fireEvent.pointerDown(volume());
      fireEvent.change(volume(), { target: { value: "-3" } });
      fireEvent.change(volume(), { target: { value: "4.5" } });
      fireEvent.pointerUp(window);
      await waitFor(() =>
        expect(within(header("Synth 2")).getByText("4.5 dB")).toBeInTheDocument(),
      );
      fireEvent.pointerDown(pan());
      fireEvent.change(pan(), { target: { value: "0.25" } });
      fireEvent.pointerUp(window);

      await waitFor(() => expect(within(header("Synth 2")).getByText("R 25")).toBeInTheDocument());
      expect(within(header("Synth 2")).getByText("4.5 dB")).toBeInTheDocument();
      const mixers = sent("set_track_mixer");
      expect(mixers.map((args) => args.track)).toEqual(["track-2", "track-2", "track-2"]);
      expect(mixers.map((args) => args.mixer)).toEqual([
        { volumeDb: -3, pan: -0.5, mute: true, solo: false },
        { volumeDb: 4.5, pan: -0.5, mute: true, solo: false },
        { volumeDb: 4.5, pan: 0.25, mute: true, solo: false },
      ]);
      expect(mixers[0].gesture).toEqual(expect.any(Number));
      expect(mixers[1].gesture).toBe(mixers[0].gesture);
      expect(mixers[2].gesture).not.toBe(mixers[0].gesture);

      // Once the pointer is up, a change (from the keyboard, say) is its own step.
      fireEvent.change(volume(), { target: { value: "1" } });
      fireEvent.change(pan(), { target: { value: "0.5" } });
      await waitFor(() => expect(sent("set_track_mixer")).toHaveLength(5));
      expect(sent("set_track_mixer").slice(3).map((args) => args.gesture)).toEqual([null, null]);
    });

    it("sends mute and solo to Rust", async () => {
      threeTracks();
      await renderApp();
      fireEvent.click(screen.getByRole("button", { name: "Mute Synth 2" }));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Mute Synth 2" })).toHaveAttribute(
          "aria-pressed",
          "false",
        ),
      );
      fireEvent.click(screen.getByRole("button", { name: "Solo Synth 1" }));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Solo Synth 1" })).toHaveAttribute(
          "aria-pressed",
          "true",
        ),
      );
      expect(sent("set_track_mixer")).toEqual([
        {
          track: "track-2",
          mixer: { volumeDb: -6, pan: -0.5, mute: false, solo: false },
          gesture: null,
        },
        {
          track: "track-1",
          mixer: { volumeDb: 0, pan: 0, mute: false, solo: true },
          gesture: null,
        },
      ]);
      // Additive: the other soloed track stays soloed.
      expect(screen.getByRole("button", { name: "Solo Synth 3" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    });

    it("solos a track on its own with ⌥-click", async () => {
      threeTracks();
      await renderApp();
      fireEvent.click(screen.getByRole("button", { name: "Solo Synth 1" }), {
        altKey: true,
      });
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Solo Synth 3" })).toHaveAttribute(
          "aria-pressed",
          "false",
        ),
      );
      expect(sent("solo_track_alone")).toEqual([{ track: "track-1" }]);
      expect(sent("set_track_mixer")).toEqual([]);
      expect(screen.getByRole("button", { name: "Solo Synth 1" })).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    });

    it("adds a track below the others, and selects it", async () => {
      await renderApp();
      fireEvent.click(screen.getByRole("button", { name: "+ Add track" }));
      await waitFor(() => expect(headerNames()).toEqual(["Synth 1", "Synth 2"]));
      const [added] = sent("add_track");
      expect(added.id).toMatch(/^[0-9a-f-]{36}$/);
      expect(header("Synth 2")).toHaveAttribute("aria-current", "true");
      expect(header("Synth 1")).not.toHaveAttribute("aria-current");
      expect(screen.getByText("Synth 2 has no clips yet.")).toBeInTheDocument();
    });

    it("can't add more than the most tracks", async () => {
      project = { ...project, maxTracks: 1 };
      await renderApp();
      expect(screen.getByRole("button", { name: "+ Add track" })).toBeDisabled();
    });

    it("adds, duplicates and deletes tracks from the Track menu", async () => {
      threeTracks();
      await renderApp();
      select("Synth 2");

      await act(() => emit("track-menu", "duplicate-track"));
      await waitFor(() =>
        expect(headerNames()).toEqual(["Synth 1", "Synth 2", "Synth 4", "Synth 3"]),
      );
      const [duplicated] = sent("duplicate_track");
      expect(duplicated.track).toBe("track-2");
      expect(duplicated.id).toMatch(/^[0-9a-f-]{36}$/);
      expect(header("Synth 4")).toHaveAttribute("aria-current", "true");

      await act(() => emit("track-menu", "delete-track"));
      await waitFor(() => expect(headerNames()).toEqual(["Synth 1", "Synth 2", "Synth 3"]));
      expect(sent("remove_track")).toEqual([{ track: duplicated.id }]);
      // The track below takes the deleted one's place.
      expect(header("Synth 3")).toHaveAttribute("aria-current", "true");

      await act(() => emit("track-menu", "delete-track"));
      await waitFor(() => expect(headerNames()).toEqual(["Synth 1", "Synth 2"]));
      // The last track's place goes to the one above.
      expect(header("Synth 2")).toHaveAttribute("aria-current", "true");

      await act(() => emit("track-menu", "add-track"));
      await waitFor(() => expect(headerNames()).toEqual(["Synth 1", "Synth 2", "Synth 3"]));
      expect(sent("add_track")).toHaveLength(1);
    });

    describe("reordering", () => {
      /** Lays the headers out 60 px tall, one under the other, as jsdom doesn't. */
      function layOut() {
        headers()
          .getAllByRole("listitem")
          .forEach((item, index) => {
            item.getBoundingClientRect = () =>
              ({ top: index * 60, bottom: index * 60 + 60 }) as DOMRect;
          });
      }
      const grab = (name: string) => within(header(name)).getByText(name);

      it("moves a track to where its header is dropped, as one command", async () => {
        threeTracks();
        await renderApp();
        layOut();
        fireEvent.pointerDown(grab("Synth 3"), { button: 0, clientY: 150 });
        fireEvent.pointerMove(window, { clientY: 80 });
        expect(header("Synth 2")).toHaveClass("drop-above");
        fireEvent.pointerMove(window, { clientY: 20 });
        expect(header("Synth 1")).toHaveClass("drop-above");
        fireEvent.pointerUp(window, { clientY: 20 });

        await waitFor(() => expect(headerNames()).toEqual(["Synth 3", "Synth 1", "Synth 2"]));
        expect(sent("move_track")).toEqual([{ track: "track-3", index: 0 }]);
        expect(
          headers()
            .getAllByRole("listitem")
            .some((item) => item.className.includes("drop")),
        ).toBe(false);
      });

      it("moves a track down", async () => {
        threeTracks();
        await renderApp();
        layOut();
        fireEvent.pointerDown(grab("Synth 1"), { button: 0, clientY: 30 });
        fireEvent.pointerMove(window, { clientY: 100 });
        expect(header("Synth 2")).toHaveClass("drop-below");
        fireEvent.pointerUp(window);
        await waitFor(() => expect(sent("move_track")).toEqual([{ track: "track-1", index: 1 }]));
      });

      it("sends nothing when a header is dropped where it was, or on Esc", async () => {
        threeTracks();
        await renderApp();
        layOut();
        fireEvent.pointerDown(grab("Synth 2"), { button: 0, clientY: 90 });
        fireEvent.pointerMove(window, { clientY: 100 });
        fireEvent.pointerUp(window);
        fireEvent.pointerDown(grab("Synth 2"), { button: 0, clientY: 90 });
        fireEvent.pointerMove(window, { clientY: 10 });
        fireEvent.keyDown(window, { key: "Escape" });
        fireEvent.pointerUp(window);
        await new Promise((resolve) => setTimeout(resolve, 0));
        expect(sent("move_track")).toEqual([]);
        expect(headerNames()).toEqual(["Synth 1", "Synth 2", "Synth 3"]);
      });

      it("drops nothing when the drag is lost to a pointer cancel or the window losing focus", async () => {
        threeTracks();
        await renderApp();
        layOut();
        for (const lose of [() => fireEvent.pointerCancel(window), () => fireEvent.blur(window)]) {
          fireEvent.pointerDown(grab("Synth 3"), { button: 0, clientY: 150 });
          fireEvent.pointerMove(window, { clientY: 10 });
          lose();
          expect(header("Synth 1")).not.toHaveClass("drop-above");
          // The next click anywhere doesn't finish the lost drag.
          fireEvent.pointerUp(window);
        }
        await new Promise((resolve) => setTimeout(resolve, 0));
        expect(sent("move_track")).toEqual([]);
      });

      it("keeps the index in range when the tracks change mid-drag", async () => {
        threeTracks();
        await renderApp();
        layOut();
        fireEvent.pointerDown(grab("Synth 1"), { button: 0, clientY: 30 });
        fireEvent.pointerMove(window, { clientY: 900 });
        // Undo, say, takes a track away during the drag.
        const [first, , third] = project.tracks;
        await act(() => emit("project-changed", { ...project, tracks: [first, third] }));
        fireEvent.pointerUp(window);
        await waitFor(() => expect(sent("move_track")).toEqual([{ track: "track-1", index: 1 }]));
      });
    });

    it("selects a track when one of its controls gets focus, from the keyboard say", async () => {
      threeTracks();
      await renderApp();
      act(() => screen.getByRole("slider", { name: "Synth 2 volume" }).focus());
      expect(header("Synth 2")).toHaveAttribute("aria-current", "true");
      openTab("Sound");
      expect(synthWaveform()).toBe("Square");
    });

    it("doesn't select a track again when it comes back after an undo", async () => {
      await renderApp();
      fireEvent.click(screen.getByRole("button", { name: "+ Add track" }));
      await waitFor(() => expect(header("Synth 2")).toHaveAttribute("aria-current", "true"));
      const withBoth = project;
      await act(() => emit("project-changed", projectView()));
      expect(header("Synth 1")).toHaveAttribute("aria-current", "true");
      // Redo brings it back, but the selection stays where it fell.
      await act(() => emit("project-changed", withBoth));
      expect(header("Synth 1")).toHaveAttribute("aria-current", "true");
      expect(header("Synth 2")).not.toHaveAttribute("aria-current");
    });

    it("doesn't add a track from the Track menu when the project is full", async () => {
      project = { ...project, maxTracks: 1 };
      await renderApp();
      await act(() => emit("track-menu", "add-track"));
      expect(commands()).not.toContain("add_track");
    });

    it("shows the selected track's clip in Notes and its synth in Sound", async () => {
      threeTracks();
      await renderApp();
      expect(header("Synth 1")).toHaveAttribute("aria-current", "true");
      await waitFor(() => expect(drawnNotes().map(([id]) => id)).toEqual(["low", "high"]));

      select("Synth 2");
      expect(header("Synth 2")).toHaveAttribute("aria-current", "true");
      await waitFor(() => expect(drawnNotes()).toEqual([["bass", 60, 0, 100]]));
      openTab("Sound");
      expect(synthWaveform()).toBe("Square");
      fireEvent.click(
        within(screen.getByRole("region", { name: "Synth" })).getByRole("radio", { name: "Sine" }),
      );
      await waitFor(() => expect(sent("set_synth_param")).toHaveLength(1));
      expect(sent("set_synth_param")[0].track).toBe("track-2");

      select("Synth 1");
      expect(synthWaveform()).toBe("Saw");

      select("Synth 3");
      openTab("Notes");
      expect(screen.getByText("Synth 3 has no clips yet.")).toBeInTheDocument();
      expect(screen.queryByRole("application", { name: "Notes" })).not.toBeInTheDocument();
    });

    it("sends the stress notes to the clip in the Notes tab", async () => {
      threeTracks();
      await renderApp();
      select("Synth 2");
      await act(() => emit("develop-menu", "add-stress-notes"));
      await waitFor(() => expect(sent("add_stress_notes")).toEqual([{ clip: "clip-2" }]));

      // A track with no clips has nowhere to put them.
      select("Synth 3");
      await act(() => emit("develop-menu", "add-stress-notes"));
      expect(sent("add_stress_notes")).toHaveLength(1);
    });

    it("says so when there are no tracks", async () => {
      project = { ...project, tracks: [] };
      await renderApp();
      expect(screen.getByText(/No track selected/)).toBeInTheDocument();
      await act(() => emit("track-menu", "delete-track"));
      await act(() => emit("track-menu", "duplicate-track"));
      expect(commands()).not.toContain("remove_track");
      expect(commands()).not.toContain("duplicate_track");
    });
  });

  describe("benchmark", () => {
    // Frames and pauses come at once, so the run takes no time at all.
    let time = 0;
    const clock: Clock = {
      now: () => (time += 1),
      frame: () => new Promise((resolve) => setTimeout(() => resolve((time += 16)), 0)),
      sleep: () => new Promise((resolve) => setTimeout(resolve, 0)),
    };
    const options: BenchmarkOptions = {
      ...DEFAULT_OPTIONS,
      sliderSteps: 2,
      noteDeletes: 2,
      dragSteps: 3,
      eventWaitMs: 0,
      settleMs: 0,
    };
    const writeText = vi.fn<(text: string) => Promise<void>>();

    beforeEach(() => {
      writeText.mockReset().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", {
        value: { writeText },
        configurable: true,
      });
    });

    async function openBenchmark() {
      render(<App createRenderer={factory} benchmark={{ options, clock }} />);
      await screen.findByRole("slider", { name: /Volume/ });
      await act(() => emit("develop-menu", "run-benchmark"));
      return screen.getByRole("dialog", { name: "Benchmark" });
    }

    const benchmarkCalls = () =>
      calls.filter((c) => ["build_test_song", "set_track_mixer", "remove_notes"].includes(c.cmd));

    it("asks first, and sends nothing if cancelled", async () => {
      const dialog = await openBenchmark();
      expect(dialog).toHaveTextContent(/replaces the current song/);
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(benchmarkCalls()).toEqual([]);
    });

    it("builds each song in turn and works its controls, then shows the report", async () => {
      const dialog = await openBenchmark();
      fireEvent.click(within(dialog).getByRole("button", { name: "Replace and Run" }));
      const report = await screen.findByLabelText("Benchmark report");

      const songs = ["heavy", "wide", "check-7"];
      const sentCalls = benchmarkCalls();
      expect(sentCalls).toHaveLength(songs.length * 8);
      songs.forEach((song, index) => {
        const [build, slider1, slider2, delete1, delete2, ...drag] = sentCalls.slice(
          index * 8,
          index * 8 + 8,
        );
        // Rust builds the song; the UI only says which.
        expect(build).toEqual({ cmd: "build_test_song", args: { song } });
        const track = `${song}-track-1`;
        // Slider steps on the top track's volume, each its own change.
        for (const step of [slider1, slider2]) {
          expect(step.cmd).toBe("set_track_mixer");
          expect(step.args).toMatchObject({ track, gesture: null });
        }
        // Note deletes in the clip the piano roll shows, one note at a time.
        const clip = `${song}-clip-1-0`;
        expect([delete1, delete2]).toEqual([
          { cmd: "remove_notes", args: { clip, notes: [`${song}-note-1-0-0`] } },
          { cmd: "remove_notes", args: { clip, notes: [`${song}-note-1-0-1`] } },
        ]);
        // A drag: one gesture, a step a frame, the volume moving each time.
        expect(drag.map((step) => step.cmd)).toEqual(Array(3).fill("set_track_mixer"));
        const gestures = new Set(drag.map((step) => step.args.gesture));
        expect(gestures.size).toBe(1);
        expect([...gestures][0]).toEqual(expect.any(Number));
        const volumes = drag.map((step) => (step.args.mixer as MixerView).volumeDb);
        expect(new Set(volumes).size).toBe(3);
      });

      expect(report).toHaveTextContent(/heavy: 2 tracks, 4 clips, 12 notes/);
      expect(report).toHaveTextContent(/check 7: 2 tracks, 4 clips, 12 notes/);
      expect(report).toHaveTextContent(/max ≤ 50/);
      // The piano roll shows the clip the notes were deleted from.
      await waitFor(() =>
        expect(renderer.lastNotes().map((n) => n.id)).toEqual(["check-7-note-1-0-2"]),
      );

      fireEvent.click(within(dialog).getByRole("button", { name: "Copy as Text" }));
      await within(dialog).findByRole("button", { name: "Copied" });
      expect(writeText).toHaveBeenCalledWith(report.textContent);
      fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });

    it("moves on to the next clip when one runs out of notes", async () => {
      render(
        <App
          createRenderer={factory}
          benchmark={{ options: { ...options, songs: ["wide"], noteDeletes: 5 }, clock }}
        />,
      );
      await screen.findByRole("slider", { name: /Volume/ });
      await act(() => emit("develop-menu", "run-benchmark"));
      fireEvent.click(screen.getByRole("button", { name: "Replace and Run" }));
      await screen.findByLabelText("Benchmark report");
      // Each clip holds three notes, so the fourth delete is in the next clip.
      const deletes = calls.filter((c) => c.cmd === "remove_notes").map((c) => c.args);
      expect(deletes).toEqual([
        { clip: "wide-clip-1-0", notes: ["wide-note-1-0-0"] },
        { clip: "wide-clip-1-0", notes: ["wide-note-1-0-1"] },
        { clip: "wide-clip-1-0", notes: ["wide-note-1-0-2"] },
        { clip: "wide-clip-1-1", notes: ["wide-note-1-1-0"] },
        { clip: "wide-clip-1-1", notes: ["wide-note-1-1-1"] },
      ]);
      await waitFor(() =>
        expect(renderer.lastNotes().map((n) => n.id)).toEqual(["wide-note-1-1-2"]),
      );
    });

    it("says why it stopped if a command fails", async () => {
      const dialog = await openBenchmark();
      failWith = "no room";
      fireEvent.click(within(dialog).getByRole("button", { name: "Replace and Run" }));
      expect(await within(dialog).findByRole("alert")).toHaveTextContent(
        /The benchmark stopped: .*no room/,
      );
    });
  });

  describe("clip light", () => {
    const light = () => screen.getByRole("button", { name: /Master (clipped|hasn't clipped)/ });

    it("lights when the master clips, and stays lit until it's clicked", async () => {
      await renderApp();
      sendFrame();
      expect(light()).toHaveAttribute("data-lit", "false");
      sendFrame({ clips: 4 });
      expect(light()).toHaveAttribute("data-lit", "true");
      expect(light()).toHaveAccessibleName("Master clipped. Click to reset");
      // It stays lit with no new clips.
      sendFrame({ clips: 4 });
      expect(light()).toHaveAttribute("data-lit", "true");

      fireEvent.click(light());
      expect(light()).toHaveAttribute("data-lit", "false");
      sendFrame({ clips: 4 });
      expect(light()).toHaveAttribute("data-lit", "false");
      sendFrame({ clips: 9 });
      expect(light()).toHaveAttribute("data-lit", "true");
    });
  });

  describe("editor", () => {
    const divider = () => screen.getByRole("separator", { name: "Editor height" });
    const editorHeight = () => screen.getByRole("region", { name: "Editor" }).style.height;

    it("opens on Notes, and switches between Notes and Sound", async () => {
      await renderApp();
      expect(screen.getByRole("tab", { name: "Notes" })).toHaveAttribute("aria-selected", "true");
      expect(screen.getByRole("application", { name: "Notes" })).toBeInTheDocument();
      expect(screen.queryByRole("region", { name: "Synth" })).not.toBeInTheDocument();

      fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
      expect(screen.getByRole("tab", { name: "Sound" })).toHaveAttribute("aria-selected", "true");
      expect(screen.getByRole("region", { name: "Synth" })).toBeInTheDocument();
      expect(screen.queryByRole("application", { name: "Notes" })).not.toBeInTheDocument();
    });

    it("has a divider above it to drag or step with the arrow keys", async () => {
      vi.stubGlobal("innerHeight", 1000);
      await renderApp();
      expect(editorHeight()).toBe("280px");

      fireEvent.pointerDown(divider(), { button: 0, clientY: 500 });
      fireEvent.pointerMove(window, { clientY: 400 });
      expect(editorHeight()).toBe("380px");
      fireEvent.pointerMove(window, { clientY: 580 });
      expect(editorHeight()).toBe("200px");
      // No smaller than the smallest.
      fireEvent.pointerMove(window, { clientY: 900 });
      expect(editorHeight()).toBe("160px");
      fireEvent.pointerUp(window);
      fireEvent.pointerMove(window, { clientY: 100 });
      expect(editorHeight()).toBe("160px");

      fireEvent.keyDown(divider(), { key: "ArrowUp" });
      fireEvent.keyDown(divider(), { key: "ArrowUp" });
      expect(editorHeight()).toBe("192px");
      fireEvent.keyDown(divider(), { key: "ArrowDown" });
      fireEvent.keyDown(divider(), { key: "ArrowDown" });
      fireEvent.keyDown(divider(), { key: "ArrowDown" });
      expect(editorHeight()).toBe("160px");
      expect(divider()).toHaveAttribute("aria-valuenow", "160");

      // A drag the window loses ends where it is.
      fireEvent.pointerDown(divider(), { button: 0, clientY: 500 });
      fireEvent.pointerMove(window, { clientY: 484 });
      fireEvent.pointerCancel(window);
      fireEvent.pointerMove(window, { clientY: 100 });
      expect(editorHeight()).toBe("176px");
      fireEvent.pointerDown(divider(), { button: 0, clientY: 500 });
      fireEvent.blur(window);
      fireEvent.pointerMove(window, { clientY: 100 });
      expect(editorHeight()).toBe("176px");

      // It leaves room above for the transport and the tracks.
      fireEvent.pointerDown(divider(), { button: 0, clientY: 500 });
      fireEvent.pointerMove(window, { clientY: -2000 });
      expect(editorHeight()).toBe("740px");
      fireEvent.pointerUp(window);

      // And still does when the window gets shorter.
      vi.stubGlobal("innerHeight", 600);
      act(() => void window.dispatchEvent(new Event("resize")));
      expect(editorHeight()).toBe("340px");
      expect(divider()).toHaveAttribute("aria-valuemax", "340");
    });
  });
});
