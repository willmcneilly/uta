import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SynthParam, SynthSettingView } from "./backend";
import { SynthPanel } from "./SynthPanel";
import { press } from "./design/faderTesting";
import { knobAt, pressKnob } from "./design/knobTesting";
import { SYNTH_DEFAULTS, type SynthValues, synthSettings, synthSource } from "./pianoRoll/testing";
import { envelopeShape } from "./synth/envelopeShape";
import { type Point, filterCurve, reachHz, xOfFrequency } from "./synth/filterCurve";
import { SIZES } from "./synth/sizes";
import { lfoPoints, oscillatorPoints } from "./synth/waves";

/** A drawing's size. */
type Size = { width: number; height: number };

/** One bar at 120 BPM, in seconds. */
const BAR = 2;

const SYNTH: SynthValues = {
  ...SYNTH_DEFAULTS,
  osc1Wave: "pulse",
  osc1Width: 0.3,
  osc2PitchSemitones: 7,
  cutoffHz: 2_000,
  resonance: 0.5,
  filterEnvOctaves: 2,
  filterAttackSeconds: 0.01,
  filterDecaySeconds: 0.4,
  filterSustain: 0.2,
  filterReleaseSeconds: 0.5,
  attackSeconds: 0.01,
  decaySeconds: 0.2,
  sustain: 0.7,
  releaseSeconds: 0.3,
  lfoShape: "triangle",
  lfoRateHz: 4,
};

/** The synth's settings, described, with `values`. */
function renderPanel(values: SynthValues = SYNTH) {
  return renderSettings(synthSource(values).settings, synthSource(values).lfoSeconds);
}

function renderSettings(settings: SynthSettingView[], lfoSeconds = 0.25) {
  const onChange = vi.fn<(param: SynthParam, gesture?: number) => void>();
  const panel = (next: SynthSettingView[], seconds: number) => (
    <SynthPanel
      settings={next}
      lfoSeconds={seconds}
      barSeconds={BAR}
      beatsPerBar={4}
      sampleRate={48_000}
      onChange={onChange}
    />
  );
  const view = render(panel(settings, lfoSeconds));
  const rerender = (values: SynthValues) =>
    view.rerender(panel(synthSource(values).settings, synthSource(values).lfoSeconds));
  return { onChange, rerender };
}

const stage = (name: string) => fireEvent.click(screen.getByRole("tab", { name }));
const page = () => within(screen.getByRole("tabpanel"));
const slider = (name: string) => screen.getByRole("slider", { name });
const valueOf = (control: HTMLElement) => Number(control.getAttribute("aria-valuenow"));

/** The points of the curve a drawing draws. */
function drawn(drawing: Element): Point[] {
  const d = drawing.querySelector(".curve")!.getAttribute("d")!;
  return [...d.matchAll(/[ML](-?[\d.]+),(-?[\d.]+)/g)].map(([, x, y]) => ({ x: +x, y: +y }));
}

function expectCurve(drawing: Element, expected: Point[]) {
  const points = drawn(drawing);
  expect(points).toHaveLength(expected.length);
  points.forEach((point, i) => {
    expect(point.x).toBeCloseTo(expected[i].x, 2);
    expect(point.y).toBeCloseTo(expected[i].y, 2);
  });
}

/** A drawing on the page, not its tab's small one. */
const onPage = (testId: string) => page().getByTestId(testId);

const tuning = (values: SynthValues) => ({
  osc1Octave: values.osc1Octave,
  osc2Octave: values.osc2Octave,
  osc2PitchSemitones: values.osc2PitchSemitones,
  sync: values.osc2Sync,
});
const oscillatorOf = (which: 1 | 2, values: SynthValues, size: Size = SIZES.oscillator) =>
  oscillatorPoints(
    which,
    which === 1 ? values.osc1Wave : values.osc2Wave,
    values.osc1Width,
    tuning(values),
    size.width,
    size.height,
  );
const filterOf = (cutoffHz: number, values: SynthValues, size: Size = SIZES.filter) =>
  filterCurve(cutoffHz, values.resonance, values.filterMode, 48_000, size.width, size.height);
const envelopeOf = (times: Parameters<typeof envelopeShape>[0], size: Size = SIZES.envelope) =>
  envelopeShape(times, size.width, size.height).points;
const ampOf = (values: SynthValues) => envelopeOf(values);
const filterEnvelopeOf = (values: SynthValues) =>
  envelopeOf({
    attackSeconds: values.filterAttackSeconds,
    decaySeconds: values.filterDecaySeconds,
    sustain: values.filterSustain,
    releaseSeconds: values.filterReleaseSeconds,
  });

/**
 * Every control on every page, by the setting it's named after: each
 * slider, choice, switch and menu Rust's settings are drawn as.
 */
function everyControl(): string[] {
  const names: string[] = [];
  for (const tab of screen.getAllByRole("tab")) {
    fireEvent.click(tab);
    for (const role of ["slider", "combobox"] as const) {
      names.push(...page().queryAllByRole(role).map((control) => control.getAttribute("aria-label") ?? ""));
    }
    for (const group of page().queryAllByRole("radiogroup")) {
      names.push(document.getElementById(group.getAttribute("aria-labelledby")!)!.textContent!);
    }
    for (const key of page().queryAllByRole("button")) {
      if (key.hasAttribute("aria-pressed")) names.push(key.getAttribute("aria-label")!);
    }
  }
  return names.filter(Boolean);
}

describe("SynthPanel's stages", () => {
  it("shows the stages as tabs in the order the sound flows, oscillators first", () => {
    renderPanel();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "Osc",
      "Mixer",
      "Filter",
      "Env",
      "LFO",
      // Voice and Mod show a word over their names: Poly or Mono, and the slots used.
      "PolyVoice",
      "0/3Mod",
    ]);
    expect(screen.getByRole("tab", { name: "Osc" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveAccessibleName("Osc");
  });

  it("opens a stage's page when its tab is clicked, and moves along with the arrow keys", () => {
    renderPanel();
    stage("Filter");
    expect(screen.getByRole("tab", { name: "Filter" })).toHaveAttribute("aria-selected", "true");
    expect(page().getByRole("slider", { name: "Cutoff" })).toBeInTheDocument();
    expect(screen.queryByRole("slider", { name: "Osc 1 octave" })).toBeNull();

    const filter = screen.getByRole("tab", { name: "Filter" });
    fireEvent.keyDown(filter, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Env" })).toHaveFocus();
    expect(page().getByRole("slider", { name: "Attack" })).toBeInTheDocument();
    // The ends wrap round.
    fireEvent.keyDown(screen.getByRole("tab", { name: "Env" }), { key: "ArrowLeft" });
    fireEvent.keyDown(screen.getByRole("tab", { name: "Filter" }), { key: "ArrowLeft" });
    fireEvent.keyDown(screen.getByRole("tab", { name: "Mixer" }), { key: "ArrowLeft" });
    fireEvent.keyDown(screen.getByRole("tab", { name: "Osc" }), { key: "ArrowLeft" });
    expect(screen.getByRole("tab", { name: "Mod" })).toHaveAttribute("aria-selected", "true");
    // Only the chosen tab is in the Tab order.
    expect(screen.getAllByRole("tab").filter((tab) => tab.tabIndex === 0)).toHaveLength(1);
  });
});

describe("SynthPanel's controls, from Rust's descriptions", () => {
  it("draws a control for every setting Rust describes, named as Rust names it", () => {
    // Every mod slot in use, so each shows its controls; then synced, for
    // the LFO's rate as a note length.
    const shown = new Set<string>();
    for (const lfoSync of [false, true]) {
      renderPanel({
        ...SYNTH,
        lfoSync,
        mod1Destination: "cutoff",
        mod2Destination: "osc2_pitch",
        mod3Destination: "volume",
      });
      for (const name of everyControl()) shown.add(name);
      cleanup();
    }
    const described = synthSettings(SYNTH).map((setting) => setting.label);
    expect([...shown].sort()).toEqual([...described].sort());
  });

  it("labels a control with Rust's short label where its page says whose it is", () => {
    renderPanel();
    const octave = slider("Osc 1 octave").closest(".knob")!;
    expect(within(octave as HTMLElement).getByText("Octave")).toBeInTheDocument();
    stage("Mixer");
    expect(screen.getByText("Sub")).toBeInTheDocument();
    stage("Filter");
    expect(screen.getByText("Res")).toBeInTheDocument();
    // The envelopes' stages are named once, over their columns.
    stage("Env");
    expect(page().getAllByText("Attack")).toHaveLength(1);
    expect(slider("Filter attack")).toBeInTheDocument();
  });

  it("takes each label, choice, limit, step, scale and default from the description", async () => {
    // A description unlike today's, so a panel that kept its own would show it.
    const settings = synthSettings(SYNTH).map((setting): SynthSettingView => {
      switch (setting.name) {
        case "osc1_wave":
          return { ...setting, label: "Shape", options: setting.options.slice(0, 2), value: "sine" };
        case "resonance":
          return { ...setting, label: "Emphasis", short: "Emph", limits: [0, 2], value: 0.5, default: 1.5 };
        case "osc1_octave":
          return { ...setting, limits: [-3, 3], value: 1 };
        case "sub_octave":
          return { ...setting, limits: [-3, -1], value: -3 };
        case "osc2_sync":
          return { ...setting, label: "Hard sync", short: "Hard" };
        case "lfo_division":
          return { ...setting, value: "1/2", options: [{ value: "1/2", label: "Half" }] };
        default:
          return setting;
      }
    });
    const { onChange } = renderSettings(settings);

    const shape = screen.getByRole("radiogroup", { name: "Shape" });
    expect(within(shape).getAllByRole("radio").map((radio) => radio.getAttribute("value"))).toEqual([
      "sine",
      "triangle",
    ]);
    expect(screen.getByRole("button", { name: "Hard sync" })).toHaveTextContent("Hard");
    // Whole octaves, one a step: 1 is two thirds of the way from -3 to 3.
    expect(knobAt(slider("Osc 1 octave"))).toBeCloseTo(4 / 6, 6);
    expect(slider("Osc 1 octave")).toHaveAttribute("aria-valuetext", "+1 oct");

    stage("Mixer");
    const sub = screen.getByRole("radiogroup", { name: "Sub octave" });
    expect(within(sub).getAllByRole("radio").map((radio) => radio.getAttribute("value"))).toEqual([
      "-1",
      "-2",
      "-3",
    ]);

    stage("Filter");
    const emphasis = slider("Emphasis");
    expect(screen.queryByRole("slider", { name: "Resonance" })).toBeNull();
    // A quarter of the way along a linear 0 to 2.
    expect(knobAt(emphasis)).toBeCloseTo(0.25, 6);
    expect(emphasis).toHaveAttribute("aria-valuetext", "0.50");
    fireEvent.doubleClick(emphasis);
    await waitFor(() =>
      expect(onChange).toHaveBeenCalledWith({ name: "resonance", value: 1.5 }, undefined),
    );
  });
});

describe("SynthPanel's commands", () => {
  it("sends each choice, switch and menu's setting", () => {
    const { onChange } = renderPanel();
    const sent = () => onChange.mock.calls.at(-1)![0];
    const choose = (group: string, option: string) =>
      fireEvent.click(within(screen.getByRole("radiogroup", { name: group })).getByRole("radio", { name: option }));

    choose("Osc 1 wave", "Saw");
    expect(sent()).toEqual({ name: "osc1_wave", value: "saw" });
    choose("Osc 2 wave", "Triangle");
    expect(sent()).toEqual({ name: "osc2_wave", value: "triangle" });
    fireEvent.click(screen.getByRole("button", { name: "Osc 2 sync" }));
    expect(sent()).toEqual({ name: "osc2_sync", value: true });

    stage("Mixer");
    choose("Sub octave", "−2 oct");
    expect(sent()).toEqual({ name: "sub_octave", value: -2 });

    stage("Filter");
    choose("Filter mode", "High-pass");
    expect(sent()).toEqual({ name: "filter_mode", value: "high_pass" });

    stage("LFO");
    choose("LFO shape", "Sample & hold");
    expect(sent()).toEqual({ name: "lfo_shape", value: "sample_and_hold" });
    fireEvent.click(screen.getByRole("button", { name: "LFO sync" }));
    expect(sent()).toEqual({ name: "lfo_sync", value: true });
    fireEvent.click(screen.getByRole("button", { name: "LFO retrigger" }));
    expect(sent()).toEqual({ name: "lfo_retrigger", value: true });

    stage("Voice");
    choose("Voice mode", "Mono");
    expect(sent()).toEqual({ name: "voice_mode", value: "mono" });
    fireEvent.click(screen.getByRole("button", { name: "Legato" }));
    expect(sent()).toEqual({ name: "legato", value: true });
    fireEvent.click(screen.getByRole("button", { name: "Osc retrigger" }));
    expect(sent()).toEqual({ name: "osc_retrigger", value: false });
  });

  it("sends each knob's and fader's setting, every step of a drag one gesture", () => {
    const { onChange } = renderPanel();
    const knobs: [string, string, SynthParam["name"]][] = [
      ["Osc", "Osc 1 octave", "osc1_octave"],
      ["Osc", "Osc 1 width", "osc1_width"],
      ["Osc", "Osc 2 octave", "osc2_octave"],
      ["Osc", "Osc 2 pitch", "osc2_pitch_semitones"],
      ["Filter", "Cutoff", "cutoff_hz"],
      ["Filter", "Resonance", "resonance"],
      ["Filter", "Env amount", "filter_env_octaves"],
      ["Filter", "Key", "filter_key"],
      ["Filter", "Filter LFO", "filter_lfo_octaves"],
      ["Env", "Filter attack", "filter_attack_seconds"],
      ["Env", "Filter decay", "filter_decay_seconds"],
      ["Env", "Filter sustain", "filter_sustain"],
      ["Env", "Filter release", "filter_release_seconds"],
      ["Env", "Attack", "attack_seconds"],
      ["Env", "Decay", "decay_seconds"],
      ["Env", "Sustain", "sustain"],
      ["Env", "Release", "release_seconds"],
      ["LFO", "LFO rate", "lfo_rate_hz"],
      ["LFO", "Vibrato", "lfo_vibrato_semitones"],
      ["LFO", "LFO width", "lfo_width"],
      ["Voice", "Glide", "glide_seconds"],
    ];
    for (const [tab, label, name] of knobs) {
      stage(tab);
      onChange.mockClear();
      pressKnob(slider(label)).to(0.9).to(0.1).release();
      const params = onChange.mock.calls.map(([param]) => param);
      expect(params.map((param) => param.name), label).toEqual([name, name]);
      // At the far end of the knob, the bottom of Rust's range, or near it.
      const [min] = (synthSettings(SYNTH).find((s) => s.name === name) as { limits: [number, number] }).limits;
      expect((params[1] as { value: number }).value, label).toBeGreaterThanOrEqual(min);
      const [first, second] = onChange.mock.calls.map(([, gesture]) => gesture);
      expect(first).toEqual(expect.any(Number));
      expect(second).toBe(first);
    }

    stage("Mixer");
    for (const [label, name] of [
      ["Osc 1 level", "osc1_level"],
      ["Osc 2 level", "osc2_level"],
      ["Sub level", "sub_level"],
      ["Noise level", "noise_level"],
    ] as const) {
      onChange.mockClear();
      press(slider(label)).to(0.25).release();
      expect(onChange.mock.calls.map(([param]) => param), label).toEqual([{ name, value: 0.25 }]);
    }
  });

  it("sends each mod slot's source, destination and amount", () => {
    const { onChange } = renderPanel({ ...SYNTH, mod1Destination: "cutoff" });
    stage("Mod");
    fireEvent.change(screen.getByRole("combobox", { name: "Mod 1 source" }), { target: { value: "lfo" } });
    expect(onChange).toHaveBeenLastCalledWith({ name: "mod1_source", value: "lfo" }, undefined);
    fireEvent.change(screen.getByRole("combobox", { name: "Mod 1 destination" }), {
      target: { value: "resonance" },
    });
    expect(onChange).toHaveBeenLastCalledWith({ name: "mod1_destination", value: "resonance" }, undefined);
    pressKnob(slider("Mod 1 amount")).to(1).release();
    expect(onChange).toHaveBeenLastCalledWith({ name: "mod1_amount", value: 1 }, expect.any(Number));
  });

  it("shows the LFO's rate as a menu of note lengths when synced, and a knob in Hz when not", () => {
    const { onChange, rerender } = renderPanel();
    stage("LFO");
    expect(slider("LFO rate")).toHaveAttribute("aria-valuetext", "4 Hz");
    expect(screen.queryByRole("combobox", { name: "LFO rate (synced)" })).toBeNull();
    rerender({ ...SYNTH, lfoSync: true, lfoDivision: "1/8d" });
    expect(screen.queryByRole("slider", { name: "LFO rate" })).toBeNull();
    const menu = screen.getByRole("combobox", { name: "LFO rate (synced)" });
    expect(menu).toHaveValue("1/8d");
    fireEvent.change(menu, { target: { value: "1/16" } });
    expect(onChange).toHaveBeenLastCalledWith({ name: "lfo_division", value: "1/16" }, undefined);
  });

  it("draws a control that does nothing in the current settings in the third ink, still live", () => {
    const { onChange, rerender } = renderPanel({ ...SYNTH, osc1Wave: "saw" });
    const inactive = (control: HTMLElement) =>
      control.closest("[data-inactive]") !== null || control.hasAttribute("data-inactive");
    // Width is only the pulse's.
    expect(inactive(slider("Osc 1 width"))).toBe(true);
    pressKnob(slider("Osc 1 width")).to(0.2).release();
    expect(onChange).toHaveBeenCalled();
    rerender({ ...SYNTH, osc1Wave: "pulse" });
    expect(inactive(slider("Osc 1 width"))).toBe(false);
    stage("LFO");
    expect(inactive(slider("LFO width"))).toBe(false);

    // Glide and Legato are only Mono's.
    stage("Voice");
    expect(inactive(slider("Glide"))).toBe(true);
    expect(inactive(screen.getByRole("button", { name: "Legato" }))).toBe(true);
    expect(inactive(screen.getByRole("button", { name: "Osc retrigger" }))).toBe(false);
    rerender({ ...SYNTH, voiceMode: "mono" });
    expect(inactive(slider("Glide"))).toBe(false);
    expect(inactive(screen.getByRole("button", { name: "Legato" }))).toBe(false);
  });
});

describe("SynthPanel's Mod section", () => {
  it("opens with its tab and closes with another's", () => {
    renderPanel({ ...SYNTH, mod2Destination: "cutoff" });
    expect(screen.queryByRole("group", { name: "Mod 2" })).toBeNull();
    stage("Mod");
    expect(screen.getByRole("group", { name: "Mod 2" })).toBeInTheDocument();
    stage("Osc");
    expect(screen.queryByRole("group", { name: "Mod 2" })).toBeNull();
  });

  it("shows an empty slot as only its +, and a used one's source, destination and amount", () => {
    renderPanel({ ...SYNTH, mod2Destination: "cutoff", mod2Amount: 0.6 });
    stage("Mod");
    for (const slot of [1, 3]) {
      expect(screen.getByRole("button", { name: `Add mod ${slot}` })).toHaveTextContent("+");
      expect(screen.queryByRole("group", { name: `Mod ${slot}` })).toBeNull();
      expect(screen.queryByRole("combobox", { name: `Mod ${slot} source` })).toBeNull();
    }
    const used = within(screen.getByRole("group", { name: "Mod 2" }));
    expect(used.getByRole("combobox", { name: "Mod 2 source" })).toHaveValue("filter_env");
    expect(used.getByRole("combobox", { name: "Mod 2 destination" })).toHaveValue("cutoff");
    expect(used.getByRole("slider", { name: "Mod 2 amount" })).toHaveAttribute("aria-valuetext", "+60%");
    expect(screen.queryByRole("button", { name: "Add mod 2" })).toBeNull();
  });

  it("opens an empty slot with its + to choose a destination, and shuts it again on leaving", () => {
    const { onChange, rerender } = renderPanel();
    stage("Mod");
    fireEvent.click(screen.getByRole("button", { name: "Add mod 1" }));
    // It sends nothing until a destination is chosen.
    expect(onChange).not.toHaveBeenCalled();
    const destination = screen.getByRole("combobox", { name: "Mod 1 destination" });
    expect(destination).toHaveValue("off");
    fireEvent.change(destination, { target: { value: "osc1_width" } });
    expect(onChange).toHaveBeenLastCalledWith({ name: "mod1_destination", value: "osc1_width" }, undefined);
    rerender({ ...SYNTH, mod1Destination: "osc1_width" });
    expect(screen.getByRole("tab", { name: "Mod" })).toHaveTextContent("1/3");

    // An opened slot with no destination shuts when the page is left.
    fireEvent.click(screen.getByRole("button", { name: "Add mod 3" }));
    expect(screen.getByRole("group", { name: "Mod 3" })).toBeInTheDocument();
    stage("Osc");
    stage("Mod");
    expect(screen.queryByRole("group", { name: "Mod 3" })).toBeNull();
    expect(screen.getByRole("group", { name: "Mod 1" })).toBeInTheDocument();
 
    // And when it's left with the arrow keys.
    fireEvent.click(screen.getByRole("button", { name: "Add mod 3" }));
    fireEvent.keyDown(screen.getByRole("tab", { name: "Mod" }), { key: "ArrowLeft" });
    fireEvent.keyDown(screen.getByRole("tab", { name: "Voice" }), { key: "ArrowRight" });
    expect(screen.queryByRole("group", { name: "Mod 3" })).toBeNull();
  });
});

describe("SynthPanel's drawings", () => {
  afterEach(() => document.documentElement.removeAttribute("style"));

  it("draws each oscillator over two of oscillator 1's cycles, following a drag", () => {
    renderPanel();
    expectCurve(onPage("osc1-drawing"), oscillatorOf(1, SYNTH));
    expectCurve(onPage("osc2-drawing"), oscillatorOf(2, SYNTH));

    // Oscillator 1's width, and oscillator 2's pitch against it.
    const width = pressKnob(slider("Osc 1 width")).to(0.8);
    const shownWidth = valueOf(slider("Osc 1 width"));
    expectCurve(onPage("osc1-drawing"), oscillatorOf(1, { ...SYNTH, osc1Width: shownWidth }));
    width.release();
    expectCurve(onPage("osc1-drawing"), oscillatorOf(1, SYNTH));

    for (const name of ["Osc 2 pitch", "Osc 2 octave", "Osc 1 octave"] as const) {
      const drag = pressKnob(slider(name)).to(0.95);
      const field = { "Osc 2 pitch": "osc2PitchSemitones", "Osc 2 octave": "osc2Octave", "Osc 1 octave": "osc1Octave" } as const;
      expectCurve(onPage("osc2-drawing"), oscillatorOf(2, { ...SYNTH, [field[name]]: valueOf(slider(name)) }));
      drag.release();
    }
  });

  it("tears oscillator 2's wave when it's synced", () => {
    const { rerender } = renderPanel({ ...SYNTH, osc2Wave: "saw" });
    const free = drawn(onPage("osc2-drawing"));
    rerender({ ...SYNTH, osc2Wave: "saw", osc2Sync: true });
    const synced = drawn(onPage("osc2-drawing"));
    expect(synced).not.toEqual(free);
    expectCurve(onPage("osc2-drawing"), oscillatorOf(2, { ...SYNTH, osc2Wave: "saw", osc2Sync: true }));
  });

  it("draws the filter in its mode, with the envelope's reach, following a drag", () => {
    const { rerender } = renderPanel();
    stage("Filter");
    const filter = () => onPage("filter-drawing");
    expectCurve(filter(), filterOf(2_000, SYNTH));
    // The envelope opens it two octaves: the reach is dashed at 8 kHz.
    const reach = filter().querySelector("[data-testid=filter-reach]")!;
    expect(reach.querySelector(".reach")).toHaveAttribute("d", expect.stringMatching(/^M/));
    /** Where the dimension line from the cutoff ends: at the reach. */
    const spanTo = () => {
      const along = [...filter().querySelectorAll("line.span")].find(
        (line) => line.getAttribute("y1") === line.getAttribute("y2"),
      )!;
      return Number(along.getAttribute("x2"));
    };
    expect(spanTo()).toBeCloseTo(xOfFrequency(8_000, SIZES.filter.width), 2);

    // Cutoff and Env amount move the curve and the reach as they're dragged.
    const cutoff = pressKnob(slider("Cutoff")).to(0.5);
    const shown = valueOf(slider("Cutoff"));
    expectCurve(filter(), filterOf(shown, SYNTH));
    expect(spanTo()).toBeCloseTo(xOfFrequency(reachHz(shown, 2), SIZES.filter.width), 2);
    // The cutoff is marked while it's dragged.
    expect(Number(filter().querySelector(".dimension")!.getAttribute("x1"))).toBeCloseTo(
      xOfFrequency(shown, SIZES.filter.width),
      6,
    );
    cutoff.release();
    expect(filter().querySelector(".dimension")).toBeNull();

    const env = pressKnob(slider("Env amount")).to(0.3);
    const octaves = valueOf(slider("Env amount"));
    expect(octaves).toBeLessThan(0);
    expect(spanTo()).toBeCloseTo(xOfFrequency(reachHz(2_000, octaves), SIZES.filter.width), 2);
    env.release();

    const resonance = pressKnob(slider("Resonance")).to(1);
    expectCurve(filter(), filterCurve(2_000, 1, "low_pass", 48_000, SIZES.filter.width, SIZES.filter.height));
    resonance.release();

    // With no envelope there's no reach; in another mode, its own curve.
    rerender({ ...SYNTH, filterEnvOctaves: 0, filterMode: "band_pass" });
    expect(filter().querySelector("[data-testid=filter-reach]")).toBeNull();
    expectCurve(filter(), filterOf(2_000, { ...SYNTH, filterMode: "band_pass" }));
  });

  it("draws both envelopes, each following a drag of its own stages", () => {
    renderPanel();
    stage("Env");
    const filterEnvelope = () => onPage("filter-envelope-drawing");
    const ampEnvelope = () => onPage("amp-envelope-drawing");
    expectCurve(filterEnvelope(), filterEnvelopeOf(SYNTH));
    expectCurve(ampEnvelope(), ampOf(SYNTH));

    const fields = [
      ["Filter attack", "filterAttackSeconds"],
      ["Filter decay", "filterDecaySeconds"],
      ["Filter sustain", "filterSustain"],
      ["Filter release", "filterReleaseSeconds"],
      ["Attack", "attackSeconds"],
      ["Decay", "decaySeconds"],
      ["Sustain", "sustain"],
      ["Release", "releaseSeconds"],
    ] as const;
    for (const [label, field] of fields) {
      const drag = pressKnob(slider(label)).to(0.35);
      const values = { ...SYNTH, [field]: valueOf(slider(label)) };
      expect(values[field], label).not.toBe(SYNTH[field]);
      expectCurve(filterEnvelope(), filterEnvelopeOf(values));
      expectCurve(ampEnvelope(), ampOf(values));
      // The stage it moves is marked, on its own envelope only.
      const own = label.startsWith("Filter") ? filterEnvelope() : ampEnvelope();
      const other = label.startsWith("Filter") ? ampEnvelope() : filterEnvelope();
      expect(own.querySelectorAll(".dimension").length, label).toBeGreaterThan(0);
      expect(other.querySelectorAll(".dimension"), label).toHaveLength(0);
      drag.release();
    }
    expectCurve(ampEnvelope(), ampOf(SYNTH));
  });

  it("draws the LFO across one bar at Rust's cycle, and at the Rate knob's while it's dragged", () => {
    const { rerender } = renderPanel();
    stage("LFO");
    const lfo = () => onPage("lfo-drawing");
    const { width, height } = SIZES.lfo;
    // 4 Hz is eight cycles in a two-second bar; a line at each beat.
    expectCurve(lfo(), lfoPoints("triangle", 8, width, height));
    expect(lfo().querySelectorAll(".grid")).toHaveLength(3 + 1);

    const drag = pressKnob(slider("LFO rate")).to(0.5);
    const hz = valueOf(slider("LFO rate"));
    expectCurve(lfo(), lfoPoints("triangle", BAR * hz, width, height));
    drag.release();

    // Synced, the cycle is Rust's: synthSource sends a quarter (0.5 s), four a bar.
    rerender({ ...SYNTH, lfoSync: true });
    expectCurve(lfo(), lfoPoints("triangle", 4, width, height));
  });

  it("draws each stage at a glance on its tab, following a drag", () => {
    renderPanel({ ...SYNTH, voiceMode: "mono", mod1Destination: "cutoff" });
    const glance = (tab: string) => screen.getByRole("tab", { name: tab });
    expectCurve(within(glance("Osc")).getByTestId("osc1-drawing"), oscillatorOf(1, SYNTH, SIZES.tab));
    expect(glance("Voice")).toHaveTextContent("MonoVoice");
    expect(glance("Mod")).toHaveTextContent("1/3Mod");

    stage("Filter");
    const drag = pressKnob(slider("Cutoff")).to(0.5);
    expectCurve(
      within(glance("Filter")).getByTestId("filter-drawing"),
      filterOf(valueOf(slider("Cutoff")), SYNTH, SIZES.tab),
    );
    drag.release();

    stage("Mixer");
    const levels = () =>
      [...glance("Mixer").querySelectorAll(".level")].map((line) => Number(line.getAttribute("y2")));
    const { height } = SIZES.tab;
    expect(levels()).toEqual([height - (height - 1), height, height, height]);
    const sub = press(slider("Sub level")).to(0.5);
    expect(levels()[2]).toBeCloseTo(height - 0.5 * (height - 1), 6);
    sub.release();
  });

  it("hatches under the filter and the envelopes, every hatch-gap", () => {
    document.documentElement.style.setProperty("--hatch-gap", "8px");
    renderPanel();
    stage("Filter");
    for (const drawing of [onPage("filter-drawing")]) {
      const pattern = drawing.querySelector("pattern")!;
      expect(pattern).toHaveAttribute("width", "8");
      expect(drawing.querySelector(".under")).toHaveAttribute("fill", `url(#${pattern.id})`);
    }
    stage("Env");
    for (const drawing of [onPage("filter-envelope-drawing"), onPage("amp-envelope-drawing")]) {
      const pattern = drawing.querySelector("pattern")!;
      expect(drawing.querySelector(".under")).toHaveAttribute("fill", `url(#${pattern.id})`);
    }
  });

  it("draws each wave, filter mode and LFO shape on its option", () => {
    renderPanel();
    const drawings = (group: string) =>
      within(screen.getByRole("radiogroup", { name: group }))
        .getAllByRole("radio")
        .map((radio) => radio.closest("label")!.querySelector("svg path")?.getAttribute("d"));
    expect(drawings("Osc 1 wave").every((d) => d?.startsWith("M"))).toBe(true);
    stage("Filter");
    expect(drawings("Filter mode").every((d) => d?.startsWith("M"))).toBe(true);
    stage("LFO");
    expect(drawings("LFO shape").every((d) => d?.startsWith("M"))).toBe(true);
  });
});
