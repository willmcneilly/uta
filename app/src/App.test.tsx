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
import type { Frame, NoteView, ProjectView, SynthParam, SynthView } from "./backend";
import { type RecordingRenderer, projectView, recordingFactory } from "./pianoRoll/testing";
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
        case "set_loop_length": {
          const bars = args.bars as number;
          const loopLength = bars * 3840;
          const clip = { ...project.track.clip, length: loopLength };
          project = { ...project, loopBars: bars, loopLength, track: { ...project.track, clip } };
          return project;
        }
        case "set_synth_param": {
          const param = args.param as SynthParam;
          // Rust stores f32s, so what comes back isn't quite what was sent.
          const value = param.name === "waveform" ? param.value : Math.fround(param.value);
          const synth = { ...project.track.synth, [SYNTH_FIELDS[param.name]]: value };
          project = { ...project, canUndo: true, track: { ...project.track, synth } };
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
const loopLength = () => screen.getByRole("slider", { name: /Loop/ });
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

  it("shows the project's tempo and loop length from Rust", async () => {
    await renderApp();
    expect(tempo()).toHaveValue("120");
    expect(screen.getByText("120 BPM")).toBeInTheDocument();
    expect(loopLength()).toHaveValue("4");
    expect(screen.getByText("4 bars")).toBeInTheDocument();
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

  it("sends loop length changes to Rust, one gesture per drag", async () => {
    await renderApp();
    fireEvent.pointerDown(loopLength());
    fireEvent.change(loopLength(), { target: { value: "8" } });
    fireEvent.change(loopLength(), { target: { value: "1" } });
    fireEvent.pointerUp(window);
    fireEvent.change(loopLength(), { target: { value: "2" } });

    await waitFor(() => expect(screen.getByText("2 bars")).toBeInTheDocument());
    const sent = calls.filter((c) => c.cmd === "set_loop_length").map((c) => c.args);
    expect(sent.map((args) => args.bars)).toEqual([8, 1, 2]);
    expect(sent[1].gesture).toBe(sent[0].gesture);
    expect(sent[2].gesture).toBeNull();
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
    fireEvent.change(loopLength(), { target: { value: "2" } });
    await waitFor(() => expect(renderer.grids.at(-1)?.loopEnd).toBe(2 * 3840));
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

    it("shows the synth settings Rust sends", async () => {
      await renderApp();
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
      await renderApp();
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
      await renderApp();
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
      await renderApp();
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
      await renderApp();
      fireEvent.change(slider("Cutoff"), { target: { value: "0" } });
      fireEvent.change(slider("Release"), { target: { value: "1000" } });
      await waitFor(() => expect(synthCalls()).toHaveLength(2));
      expect(synthCalls().map((args) => args.param)).toEqual([
        { name: "cutoff_hz", value: 20 },
        { name: "release_seconds", value: 10 },
      ]);
    });

    it("marks every change in one drag with the same gesture", async () => {
      await renderApp();
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
      await renderApp();
      const undone = projectView();
      undone.track.synth = {
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
});
