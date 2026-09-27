import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import App from "./App";
import type { Frame, ProjectView } from "./backend";

// Tauri's mocked back end stands in for Rust. It keeps its own project so the
// tests can check the UI shows whatever Rust sends back, never its own copy.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

let calls: Call[];
let project: ProjectView;
let frames: Channel<Frame> | null;
let failWith: string | null;

const view = (volumeDb: number): ProjectView => ({
  volumeDb,
  minVolumeDb: -60,
  maxVolumeDb: 0,
  canUndo: false,
  canRedo: false,
});

const frame = (overrides: Partial<Frame> = {}): Frame => ({
  playing: false,
  positionSeconds: 0,
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

beforeEach(() => {
  calls = [];
  project = view(-12);
  frames = null;
  failWith = null;
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
          project = { ...view(Math.round(args.volumeDb as number)), canUndo: true };
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
  // Unmount while the mocks are still there: the app stops listening on the
  // way out, and that goes through them.
  cleanup();
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});

const commands = () => calls.map((call) => call.cmd);

async function renderApp() {
  render(<App />);
  await screen.findByRole("slider");
  await waitFor(() => expect(frames).not.toBeNull());
}

function sendFrame(overrides: Partial<Frame> = {}) {
  act(() => frames!.onmessage(frame(overrides)));
}

describe("App", () => {
  it("shows the project's volume from Rust", async () => {
    await renderApp();
    expect(screen.getByRole("slider")).toHaveValue("-12");
    expect(screen.getByText("-12.0 dB")).toBeInTheDocument();
  });

  it("shows the output, buffer size and dropouts from the frame stream", async () => {
    await renderApp();
    sendFrame({ dropouts: 3 });
    expect(screen.getByTestId("device")).toHaveTextContent("MacBook Pro Speakers · 48 kHz");
    expect(screen.getByLabelText("Buffer")).toHaveValue("128");
    expect(screen.getByTestId("dropouts")).toHaveTextContent("3");
  });

  it("shows the transport state and position from the frame stream", async () => {
    await renderApp();
    expect(screen.getByTestId("transport")).toHaveTextContent("Stopped");
    sendFrame({ playing: true, positionSeconds: 2.46 });
    expect(screen.getByTestId("transport")).toHaveTextContent("Playing");
    expect(screen.getByTestId("position")).toHaveTextContent("2.4 s");
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
    fireEvent.change(screen.getByRole("slider"), { target: { value: "-20.5" } });
    await waitFor(() => expect(screen.getByText("-20.0 dB")).toBeInTheDocument());
    const call = calls.find((c) => c.cmd === "set_volume")!;
    expect(call.args).toEqual({ volumeDb: -20.5, gesture: null });
  });

  it("marks every change in one drag with the same gesture", async () => {
    await renderApp();
    const slider = screen.getByRole("slider");
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

  it("shows undo and redo from the menu, which Rust announces", async () => {
    await renderApp();
    await act(() => emit("project-changed", view(-3)));
    expect(screen.getByRole("slider")).toHaveValue("-3");
  });

  it("offers only the buffer sizes the device supports", async () => {
    await renderApp();
    sendFrame({
      output: { ...frame().output, bufferSizes: [64, 128] },
    });
    const options = screen.getAllByRole<HTMLOptionElement>("option");
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
