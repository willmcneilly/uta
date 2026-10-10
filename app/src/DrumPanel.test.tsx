import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Channel } from "@tauri-apps/api/core";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { DrumParam, DrumSettingView, Frame, KitRowView, ProjectView } from "./backend";
import { pressKnob, knobAt } from "./design/knobTesting";
import { press as pressFader, thumbX, RAIL_WIDTH } from "./design/faderTesting";
import {
  KIT_ROWS,
  Updates,
  drumTrackView,
  projectView,
  recordingFactory,
  trackView,
} from "./pianoRoll/testing";

// A drum track's Sound tab: a strip per sound, from the outline (RFC-006,
// "In the window"), against Tauri's mocked back end. The mock applies
// set_drum_param to its own project and sends it back, as Rust does.

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

let calls: Call[];
let project: ProjectView;
let updates: Updates;
let frames: Channel<Frame> | null;

/** Drums 1 on top, so it's the track selected when the app opens. */
function withKit(rows: KitRowView[] = KIT_ROWS): ProjectView {
  return projectView({
    tracks: [
      { ...drumTrackView("drums", "Drums 1"), source: { kind: "drums", kit: { rows } } },
      trackView("synth", "Synth 1"),
    ],
  });
}

/** The kit's rows in the project, with `sound`'s `param` set as Rust would. */
function setDrum(sound: string, param: DrumParam): ProjectView {
  return {
    ...project,
    canUndo: true,
    tracks: project.tracks.map((track) => {
      if (track.source.kind !== "drums") return track;
      const rows = track.source.kit.rows.map((row) =>
        row.sound !== sound
          ? row
          : {
              ...row,
              settings: row.settings.map((setting) =>
                // Rust stores f32s, so what comes back isn't quite what was sent.
                setting.name === param.name ? { ...setting, value: Math.fround(param.value) } : setting,
              ),
            },
      );
      return { ...track, source: { kind: "drums", kit: { rows } } };
    }),
  };
}

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
  vi.stubGlobal("ResizeObserver", FixedSizeObserver);
  updates = new Updates();
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      switch (cmd) {
        case "get_project":
          return updates.send(project);
        case "get_notes":
          return updates.notes(project, args.clip as string);
        case "set_drum_param":
          project = setDrum(args.sound as string, args.param as DrumParam);
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

async function renderSound() {
  render(<App createRenderer={recordingFactory().factory} />);
  await waitFor(() => expect(frames).not.toBeNull());
  fireEvent.click(screen.getByRole("tab", { name: "Sound" }));
  await screen.findByRole("group", { name: "Drum kit" });
}

const kit = () => within(screen.getByRole("group", { name: "Drum kit" }));
const strip = (name: string) => within(kit().getByRole("region", { name }));
const slider = (name: string) => kit().getByRole("slider", { name });
const sent = (cmd: string) => calls.filter((call) => call.cmd === cmd).map((call) => call.args);

describe("the drum panel", () => {
  it("has a strip per sound from the outline, kick on the left, each with its own controls", async () => {
    await renderSound();
    const names = kit()
      .getAllByRole("region")
      .map((region) => region.getAttribute("aria-label"));
    expect(names).toEqual(KIT_ROWS.map((row) => row.name));

    // The kick: its knobs, labelled and in order, and its Level, a fader on end.
    const kick = strip("Kick");
    expect(kick.getAllByRole("slider").map((s) => s.getAttribute("aria-label"))).toEqual([
      "Kick tune",
      "Kick tone",
      "Kick decay",
      "Kick level",
    ]);
    expect(slider("Kick tune")).toHaveAttribute("aria-orientation", "vertical");
    expect(slider("Kick level").closest(".fader")).not.toBeNull();
    expect(slider("Kick level")).toHaveAttribute("aria-orientation", "vertical");

    // Each reads in its own unit: Tone differs from sound to sound.
    expect(slider("Kick tune")).toHaveAttribute("aria-valuetext", "49 Hz");
    expect(slider("Kick tone")).toHaveAttribute("aria-valuetext", "20%");
    expect(slider("Kick decay")).toHaveAttribute("aria-valuetext", "300 ms");
    expect(slider("Kick level")).toHaveAttribute("aria-valuetext", "0.0 dB");
    expect(slider("Snare tone")).toHaveAttribute("aria-valuetext", "160 ms");
    expect(slider("Snare snappy")).toHaveAttribute("aria-valuetext", "50%");
    expect(slider("Clap tone")).toHaveAttribute("aria-valuetext", "1000 Hz");
    expect(strip("Snare").getByText("Snappy")).toBeVisible();

    // The hats: Tone in Hz, whole up to 10 kHz; the open hat's Tune and
    // Tone are the closed hat's, and it says so.
    expect(slider("Closed hat tone")).toHaveAttribute("aria-valuetext", "7100 Hz");
    expect(strip("Open hat").getAllByRole("slider").map((s) => s.getAttribute("aria-label"))).toEqual([
      "Open hat decay",
      "Open hat level",
    ]);
    expect(strip("Open hat").getByText("Tune and Tone: the closed hat’s")).toBeInTheDocument();
    expect(kit().getAllByText(/: the .*’s$/)).toHaveLength(1);

    // A sound without its circuit yet has a name and nothing to turn.
    expect(strip("Cymbal").queryAllByRole("slider")).toEqual([]);
    expect(strip("Cymbal").getByText("No controls yet")).toBeInTheDocument();
  });

  it("draws a sound it has never seen, from the outline alone", async () => {
    const hz = (value: number, limits: [number, number]): DrumSettingView => ({
      name: "tune_hz",
      label: "Tune",
      value,
      limits,
      default: value,
      unit: "hz",
    });
    const tom: KitRowView = {
      sound: "low_tom",
      name: "Low tom",
      pitch: 45,
      settings: [
        hz(90, [80, 100]),
        { name: "decay_seconds", label: "Decay", value: 0.2, limits: [0.1, 0.6], default: 0.2, unit: "seconds" },
        { name: "level_db", label: "Level", value: -6, limits: [-60, 6], default: -6, unit: "db" },
      ],
      shares: null,
    };
    project = withKit(KIT_ROWS.map((row) => (row.sound === "low_tom" ? tom : row)));
    await renderSound();
    expect(strip("Low tom").getAllByRole("slider").map((s) => s.getAttribute("aria-valuetext"))).toEqual([
      "90 Hz",
      "200 ms",
      "-6.0 dB",
    ]);
    // Halfway up its log scale from 80 to 100 Hz.
    expect(knobAt(slider("Low tom tune"))).toBeCloseTo(Math.log(90 / 80) / Math.log(100 / 80), 2);
  });

  it("sends a knob drag to Rust as one gesture, and shows what comes back", async () => {
    await renderSound();
    const drag = pressKnob(slider("Kick tune"));
    drag.to(0.6);
    drag.to(1);
    drag.release();
    await waitFor(() => expect(slider("Kick tune")).toHaveAttribute("aria-valuetext", "80 Hz"));
    const steps = sent("set_drum_param");
    expect(steps.length).toBeGreaterThanOrEqual(1);
    for (const step of steps) {
      expect(step).toMatchObject({ track: "drums", sound: "kick", param: { name: "tune_hz" } });
    }
    // Every step of the drag shares a gesture, so Rust undoes it as one.
    expect(new Set(steps.map((step) => step.gesture)).size).toBe(1);
    expect(steps[0].gesture).toEqual(expect.any(Number));
    expect(steps.at(-1)?.param).toEqual({ name: "tune_hz", value: 80 });
  });

  it("sends a Level drag to Rust as one gesture, dragged up and down", async () => {
    await renderSound();
    const level = slider("Snare level");
    // From 0 dB, a tenth of the way down the rail: 66 dB of range, 6.6 dB down.
    const from = thumbX(level);
    pressFader(level).to((from - RAIL_WIDTH / 10) / RAIL_WIDTH).release();
    await waitFor(() => expect(sent("set_drum_param")).not.toHaveLength(0));
    const steps = sent("set_drum_param");
    expect(new Set(steps.map((step) => step.gesture)).size).toBe(1);
    expect(steps.at(-1)).toMatchObject({
      track: "drums",
      sound: "snare",
      param: { name: "level_db", value: -6.5 },
    });
    await waitFor(() => expect(level).toHaveAttribute("aria-valuetext", "-6.5 dB"));
  });

  it("resets a control to the outline's default on double-click", async () => {
    // The clap's decay is away from its default, and the default isn't the
    // value the test kit starts at.
    const rows = KIT_ROWS.map((row) =>
      row.sound !== "clap"
        ? row
        : {
            ...row,
            settings: row.settings.map((s) =>
              s.name === "decay_seconds" ? { ...s, value: 0.35, default: 0.12 } : s,
            ),
          },
    );
    project = withKit(rows);
    await renderSound();
    fireEvent.doubleClick(slider("Clap decay"));
    await waitFor(() => expect(sent("set_drum_param")).toHaveLength(1));
    expect(sent("set_drum_param")[0]).toEqual({
      track: "drums",
      sound: "clap",
      param: { name: "decay_seconds", value: 0.12 },
      gesture: null,
    });
    await waitFor(() => expect(slider("Clap decay")).toHaveAttribute("aria-valuetext", "120 ms"));
  });

  it("plays a sound when its name is clicked, without changing the project", async () => {
    await renderSound();
    fireEvent.click(strip("Snare").getByRole("button", { name: "Snare" }));
    fireEvent.click(strip("Cymbal").getByRole("button", { name: "Cymbal" }));
    expect(sent("audition_note")).toEqual([
      { track: "drums", pitch: 38, velocity: 100 },
      { track: "drums", pitch: 49, velocity: 100 },
    ]);
    expect(sent("set_drum_param")).toEqual([]);
  });

  it("follows the selected track: a synth track's Sound tab is the synth", async () => {
    await renderSound();
    await act(async () => {
      fireEvent.pointerDown(
        within(screen.getByRole("region", { name: "Tracks" })).getByRole("listitem", {
          name: "Synth 1",
        }),
      );
    });
    expect(screen.queryByRole("group", { name: "Drum kit" })).toBeNull();
    expect(screen.getByRole("region", { name: "Synth" })).toBeInTheDocument();
  });
});
