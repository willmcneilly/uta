import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SynthSettingView } from "./backend";
import { SynthPanel } from "./SynthPanel";
import { knobAt, pressKnob } from "./design/knobTesting";
import { SYNTH_DEFAULTS, type SynthValues, synthSettings } from "./pianoRoll/testing";
import { DRAWING_HEIGHT, DRAWING_WIDTH } from "./synth/Drawings";
import { envelopeShape } from "./synth/envelopeShape";
import { type Point, filterCurve, xOfFrequency } from "./synth/filterCurve";

const SYNTH: SynthValues = {
  ...SYNTH_DEFAULTS,
  cutoffHz: 2_000,
  resonance: 0.5,
  attackSeconds: 0.01,
  decaySeconds: 0.2,
  sustain: 0.7,
  releaseSeconds: 0.3,
};

function renderPanel(synth = SYNTH, sampleRate: number | null = 48_000) {
  return renderSettings(synthSettings(synth, SYNTH), sampleRate);
}

function renderSettings(settings: SynthSettingView[], sampleRate: number | null = 48_000) {
  const onChange = vi.fn();
  const view = render(<SynthPanel settings={settings} sampleRate={sampleRate} onChange={onChange} />);
  const rerender = (next: SynthValues, rate = sampleRate) =>
    view.rerender(
      <SynthPanel settings={synthSettings(next, SYNTH)} sampleRate={rate} onChange={onChange} />,
    );
  return { onChange, rerender };
}

const drawing = (name: "filter" | "envelope") => screen.getByTestId(`${name}-drawing`);
const slider = (name: string) => screen.getByRole("slider", { name: new RegExp(`^${name}`) });

/** The points of the curve `name` draws. */
function drawn(name: "filter" | "envelope"): Point[] {
  const d = drawing(name).querySelector(".curve")!.getAttribute("d")!;
  return [...d.matchAll(/[ML](-?[\d.]+),(-?[\d.]+)/g)].map(([, x, y]) => ({ x: +x, y: +y }));
}

function expectCurve(name: "filter" | "envelope", expected: Point[]) {
  const points = drawn(name);
  expect(points).toHaveLength(expected.length);
  points.forEach((point, i) => {
    expect(point.x).toBeCloseTo(expected[i].x, 2);
    expect(point.y).toBeCloseTo(expected[i].y, 2);
  });
}

const filterAt = (cutoffHz: number, resonance: number, rate = 48_000) =>
  filterCurve(cutoffHz, resonance, rate, DRAWING_WIDTH, DRAWING_HEIGHT);
const envelopeOf = (synth: SynthValues) => envelopeShape(synth, DRAWING_WIDTH, DRAWING_HEIGHT).points;
const marks = (name: "filter" | "envelope") => drawing(name).querySelectorAll(".dimension");

describe("SynthPanel's drawings", () => {
  afterEach(() => document.documentElement.removeAttribute("style"));

  it("draws the filter's response and the envelope from the settings Rust sends", () => {
    renderPanel();
    expectCurve("filter", filterAt(2_000, 0.5));
    expectCurve("envelope", envelopeOf(SYNTH));
    expect(
      screen.getByRole("img", { name: "Filter response: low-pass at 2.00 kHz, resonance 0.50" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("img", {
        name: "Envelope: attack 10 ms, decay 200 ms, sustain 70%, release 300 ms",
      }),
    ).toBeInTheDocument();
  });

  it("draws the filter at the engine's sample rate, or its default before a device is open", () => {
    const { rerender } = renderPanel({ ...SYNTH, cutoffHz: 15_000 }, 44_100);
    expectCurve("filter", filterAt(15_000, 0.5, 44_100));
    rerender({ ...SYNTH, cutoffHz: 15_000 }, null);
    expectCurve("filter", filterAt(15_000, 0.5, 48_000));
  });

  it("follows a slider during a drag, from the value it shows, before Rust replies", () => {
    renderPanel();
    // Halfway along the cutoff's log scale is 632 Hz.
    const drag = pressKnob(slider("Cutoff")).to(0.5);
    expect(slider("Cutoff")).toHaveAttribute("aria-valuetext", "632 Hz");
    const shown = Number(slider("Cutoff").getAttribute("aria-valuenow"));
    expectCurve("filter", filterAt(shown, 0.5));
    expectCurve("envelope", envelopeOf(SYNTH));

    // Released with no reply yet, it shows Rust's value again, and so does the drawing.
    drag.release();
    expectCurve("filter", filterAt(2_000, 0.5));
  });

  it("follows every envelope slider during a drag", () => {
    renderPanel();
    for (const [name, field] of [
      ["Attack", "attackSeconds"],
      ["Decay", "decaySeconds"],
      ["Sustain", "sustain"],
      ["Release", "releaseSeconds"],
    ] as const) {
      const drag = pressKnob(slider(name)).to(0.35);
      const shown = Number(slider(name).getAttribute("aria-valuenow"));
      expect(shown).not.toBe(SYNTH[field]);
      expectCurve("envelope", envelopeOf({ ...SYNTH, [field]: shown }));
      drag.release();
      expectCurve("envelope", envelopeOf(SYNTH));
    }
  });

  it("shows the latest settings once Rust replies", () => {
    const { rerender } = renderPanel();
    rerender({ ...SYNTH, cutoffHz: 300, resonance: 1, sustain: 0.2 });
    expectCurve("filter", filterAt(300, 1));
    expectCurve("envelope", envelopeOf({ ...SYNTH, sustain: 0.2 }));
  });

  it("marks what a slider moves only while it's dragged", () => {
    renderPanel();
    expect(marks("filter")).toHaveLength(0);
    expect(marks("envelope")).toHaveLength(0);

    // The cutoff, for both filter sliders.
    for (const name of ["Cutoff", "Resonance"]) {
      const drag = pressKnob(slider(name)).to(0.6);
      const cutoff = Number(slider("Cutoff").getAttribute("aria-valuenow"));
      const [mark] = marks("filter");
      expect(Number(mark.getAttribute("x1"))).toBeCloseTo(xOfFrequency(cutoff, DRAWING_WIDTH), 6);
      expect(mark.getAttribute("x2")).toBe(mark.getAttribute("x1"));
      expect(marks("envelope")).toHaveLength(0);
      drag.release();
      expect(marks("filter")).toHaveLength(0);
    }

    // A timed stage's start and end.
    let drag = pressKnob(slider("Decay")).to(0.3);
    const decay = envelopeShape(
      { ...SYNTH, decaySeconds: Number(slider("Decay").getAttribute("aria-valuenow")) },
      DRAWING_WIDTH,
      DRAWING_HEIGHT,
    ).stages[1];
    expect([...marks("envelope")].map((mark) => Number(mark.getAttribute("x1")))).toEqual([
      decay.start,
      decay.end,
    ]);
    drag.release();

    // The sustain level, across.
    drag = pressKnob(slider("Sustain")).to(0.4);
    const [level] = marks("envelope");
    expect(Number(level.getAttribute("y1"))).toBeCloseTo((1 - 0.4) * DRAWING_HEIGHT, 6);
    expect(Number(level.getAttribute("x2"))).toBe(DRAWING_WIDTH);
    drag.release();
    expect(marks("envelope")).toHaveLength(0);
  });

  it("hatches under each curve, every hatch-gap", () => {
    document.documentElement.style.setProperty("--hatch-gap", "8px");
    renderPanel();
    for (const name of ["filter", "envelope"] as const) {
      const pattern = drawing(name).querySelector("pattern")!;
      expect(pattern).toHaveAttribute("width", "8");
      expect(drawing(name).querySelector(".under")).toHaveAttribute(
        "fill",
        `url(#${pattern.id})`,
      );
      expect(within(drawing(name)).queryAllByRole("slider")).toHaveLength(0);
    }
  });

  it("draws a cycle of each waveform on its key", () => {
    renderPanel();
    for (const name of ["Sine", "Triangle", "Saw", "Pulse"]) {
      const key = screen.getByRole("radio", { name }).closest("label")!;
      expect(key.querySelector("svg.cycle path")).toHaveAttribute("d", expect.stringMatching(/^M/));
    }
  });
});

describe("SynthPanel's controls", () => {
  it("draws each control from the setting Rust describes: its label, choices, limits, scale and default", async () => {
    // A description unlike today's, so a panel that kept its own would show it.
    const settings = synthSettings(SYNTH).map((setting): SynthSettingView => {
      if (setting.name === "osc1_wave" || setting.name === "osc2_wave") {
        return { ...setting, label: "Shape", options: setting.options.slice(0, 2), value: "sine" };
      }
      if (setting.name !== "resonance") return setting;
      return { ...setting, label: "Emphasis", limits: [0, 2], value: 0.5, default: 1.5 };
    });
    const { onChange } = renderSettings(settings);

    expect(screen.getByRole("radiogroup", { name: "Shape" })).toBeInTheDocument();
    expect(screen.getAllByRole("radio").map((radio) => radio.getAttribute("value"))).toEqual([
      "sine",
      "triangle",
    ]);
    const emphasis = slider("Emphasis");
    expect(screen.queryByRole("slider", { name: /^Resonance/ })).toBeNull();
    // A quarter of the way along a linear 0 to 2.
    expect(knobAt(emphasis)).toBeCloseTo(0.25, 6);
    expect(emphasis).toHaveAttribute("aria-valuetext", "0.50");
    fireEvent.doubleClick(emphasis);
    await waitFor(() => expect(onChange).toHaveBeenCalledWith({ name: "resonance", value: 1.5 }, undefined));
  });
});
