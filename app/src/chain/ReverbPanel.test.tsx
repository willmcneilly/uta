import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EffectView } from "../backend";
import { pressKnob, slideKnob } from "../design/knobTesting";
import { FrameLoop } from "../frameLoop";
import { FrameStats } from "../pianoRoll/frameStats";
import { ReverbPanel } from "./ReverbPanel";
import { TAIL_WIDTH } from "./TailDrawing";
import { type ReverbValues, reverbView } from "./reverbTesting";
import { xOf } from "./tail";

// The Reverb's panel, drawn from Rust's description of its settings: the
// core view, the Early and Late groups behind the pull tab, each control
// sending its own command, and the drawing of the tail following the
// settings.

/** One bar at 120 BPM. */
const BAR = 2;

let frames: FrameRequestCallback[];

beforeEach(() => {
  frames = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => frames.push(callback));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

/** Runs the frames waiting, at `now` milliseconds. */
function frame(now: number) {
  const waiting = frames;
  frames = [];
  act(() => waiting.forEach((callback) => callback(now)));
}

interface Options {
  open?: boolean;
  songSeconds?: ((now: number) => number) | null;
}

function renderPanel(values: Partial<ReverbValues> = {}, options: Options = {}) {
  const onSetting = vi.fn();
  const onMix = vi.fn();
  const onOpen = vi.fn();
  const loop = new FrameLoop(new FrameStats());
  const panel = (effect: EffectView, open = options.open ?? false) => (
    <ReverbPanel
      effect={effect}
      open={open}
      onOpen={onOpen}
      barSeconds={BAR}
      songSeconds={options.songSeconds ?? null}
      frames={loop}
      onSetting={onSetting}
      onMix={onMix}
    />
  );
  const view = render(panel(reverbView("r-1", values)));
  const rerender = (next: Partial<ReverbValues>, open?: boolean) =>
    view.rerender(panel(reverbView("r-1", { ...values, ...next }), open));
  return { onSetting, onMix, onOpen, rerender };
}

const knobNames = () => screen.getAllByRole("slider").map((knob) => knob.getAttribute("aria-label"));
const drawing = () => screen.getByRole("img", { name: "The reverb's shape" });
const tab = () => screen.getByRole("button", { name: /early and late settings/ });
/** Where each early reflection's tick is across the drawing. */
const ticks = () =>
  Array.from(drawing().querySelectorAll<SVGLineElement>(".early line")).map((tick) => Number(tick.getAttribute("x1")));
/** When the drawing says the reverb is silent, in seconds. */
const end = () => Number(drawing().dataset.end);

describe("ReverbPanel", () => {
  it("opens in core: Mode beside the drawing, the core knobs in a row by their short names, Mix last", () => {
    renderPanel();
    expect(screen.getByRole("radio", { name: "Room" })).toBeChecked();
    expect(knobNames()).toEqual([
      "Reverb predelay",
      "Reverb decay",
      "Reverb late size",
      "Reverb depth",
      "Reverb high cut",
      "Reverb low cut",
      "Reverb mix",
    ]);
    // Late size is "Size" in the core, its short name.
    const size = screen.getByRole("slider", { name: "Reverb late size" });
    expect(size.closest(".knob")).toHaveTextContent("Size");
    // Values are drawn as Rust gives them, and the Low cut is off at its lowest.
    expect(screen.getByRole("slider", { name: "Reverb decay" })).toHaveAttribute("aria-valuetext", "2.00 s");
    expect(screen.getByRole("slider", { name: "Reverb low cut" })).toHaveAttribute("aria-valuetext", "Off");
    expect(tab()).toHaveAttribute("aria-expanded", "false");
    expect(tab()).toHaveAccessibleName("Show the Reverb's early and late settings");
    expect(screen.queryByRole("group", { name: "Early" })).not.toBeInTheDocument();
  });

  it("the pull tab opens it wide, to the Early and Late groups, and closes it", () => {
    const { onOpen, rerender } = renderPanel();
    fireEvent.click(tab());
    expect(onOpen).toHaveBeenLastCalledWith(true);

    rerender({}, true);
    expect(tab()).toHaveAttribute("aria-expanded", "true");
    expect(tab()).toHaveAccessibleName("Hide the Reverb's early and late settings");
    const early = screen.getByRole("group", { name: "Early" });
    const late = screen.getByRole("group", { name: "Late" });
    expect(tab()).toHaveAttribute("aria-controls", early.parentElement?.id);
    const labels = (group: HTMLElement) =>
      Array.from(group.querySelectorAll(".knob-label")).map((label) => label.textContent);
    expect(labels(early)).toEqual(["Size", "Cross", "Diffusion", "Send", "Mod rate", "Mod depth"]);
    expect(labels(late)).toEqual(["Low ×", "Low until", "High ×", "High from", "Cross", "Mod rate", "Mod depth"]);
    // A multiple reads with a ×, and a slow rate to two places.
    expect(within(late).getByRole("slider", { name: "Reverb bass mult" })).toHaveAttribute("aria-valuetext", "1.00×");
    expect(within(late).getByRole("slider", { name: "Reverb late mod rate" })).toHaveAttribute(
      "aria-valuetext",
      "0.50 Hz",
    );
    fireEvent.click(tab());
    expect(onOpen).toHaveBeenLastCalledWith(false);
  });

  it("each control sends its own setting, a drag's changes sharing a gesture; Mix sends the Mix", () => {
    const { onSetting, onMix } = renderPanel({}, { open: true });
    // Each knob's setting, by its accessible name, as Rust describes it.
    const nameOf = new Map(
      reverbView("r-1")
        .settings.filter((s) => s.kind === "number")
        .map((s) => [`Reverb ${s.label.toLowerCase()}`, s.name]),
    );
    const knobs = screen.getAllByRole("slider").filter((knob) => knob.getAttribute("aria-label") !== "Reverb mix");
    expect(knobs).toHaveLength(nameOf.size);
    for (const knob of knobs) {
      onSetting.mockClear();
      slideKnob(knob, 0.7, 0.8);
      const params = onSetting.mock.calls.map(([param]) => param.name);
      expect(params.length).toBeGreaterThan(0);
      expect(new Set(params)).toEqual(new Set([nameOf.get(knob.getAttribute("aria-label") ?? "")]));
      const gestures = new Set(onSetting.mock.calls.map(([, gesture]) => gesture));
      expect(gestures.size).toBe(1);
      expect([...gestures][0]).toEqual(expect.any(Number));
    }
    slideKnob(screen.getByRole("slider", { name: "Reverb mix" }), 0.5);
    expect(onMix).toHaveBeenCalled();
  });

  it("sends the Mode chosen, from the choices Rust lists", () => {
    const onSetting = vi.fn();
    const effect = reverbView("r-1");
    // A second mode, as Rust will send once Chamber lands.
    const withChamber: EffectView = {
      ...effect,
      settings: effect.settings.map((s) =>
        s.kind === "choice" ? { ...s, options: [...s.options, { value: "chamber", label: "Chamber" }] } : s,
      ),
    };
    render(
      <ReverbPanel
        effect={withChamber}
        open={false}
        onOpen={vi.fn()}
        barSeconds={BAR}
        songSeconds={null}
        frames={new FrameLoop(new FrameStats())}
        onSetting={onSetting}
        onMix={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("radio", { name: "Chamber" }));
    expect(onSetting).toHaveBeenLastCalledWith({ name: "mode", value: "chamber" }, undefined);
  });

  it("draws the tail from the settings Rust sent: Decay sets how long it lasts, Depth the early ticks", () => {
    const { rerender } = renderPanel();
    const twoSeconds = end();
    rerender({ decay_seconds: 6 });
    expect(end()).toBeGreaterThan(twoSeconds * 2);
    // The end sits where it says it does, on the time scale every sound shares.
    const dot = drawing().querySelector(".end-handle .dot");
    expect(Number(dot?.getAttribute("cx"))).toBeCloseTo(xOf(end(), TAIL_WIDTH), 1);

    // All tail: no early reflections. All early: ticks after Predelay.
    rerender({ depth: 1 });
    expect(ticks()).toEqual([]);
    rerender({ depth: 0, predelay_seconds: 0.1, early_size_seconds: 0.3 });
    expect(ticks().length).toBeGreaterThan(3);
    expect(Math.min(...ticks())).toBeGreaterThan(xOf(0.1, TAIL_WIDTH));

    // The Mults set how long the lows and highs last; the longest sets the end.
    rerender({ depth: 0.5, predelay_seconds: 0.01, early_size_seconds: 0.03, decay_seconds: 2, bass_mult: 1, high_mult: 0.5 });
    const level = end();
    rerender({ bass_mult: 3 });
    expect(end()).toBeGreaterThan(level * 2);
  });

  it("leaves the Low cut out of the drawing where it reads Off, at its lowest", () => {
    // Lows held long, so the Low cut's shelf would shorten the tail.
    const effect = reverbView("r-1", { bass_mult: 3, low_cut_hz: 20 });
    const panel = (view: EffectView) => (
      <ReverbPanel
        effect={view}
        open={false}
        onOpen={vi.fn()}
        barSeconds={BAR}
        songSeconds={null}
        frames={new FrameLoop(new FrameStats())}
        onSetting={vi.fn()}
        onMix={vi.fn()}
      />
    );
    const { rerender } = render(panel(effect));
    const off = end();
    // The same 20 Hz, from a setting that isn't off at its lowest: the cut is drawn.
    const cutting: EffectView = {
      ...effect,
      settings: effect.settings.map((s) => (s.name === "low_cut_hz" ? { ...s, offAtLowest: undefined } : s)),
    };
    rerender(panel(cutting));
    expect(end()).toBeLessThan(off);
  });

  it("follows a knob as it's dragged, before Rust replies", () => {
    renderPanel();
    const before = end();
    const drag = pressKnob(screen.getByRole("slider", { name: "Reverb decay" })).to(0.9);
    // Rust hasn't replied: the panel was given no new settings.
    expect(end()).toBeGreaterThan(before * 3);
    drag.release();
    expect(end()).toBe(before);
  });

  it("shows a band's own edge, with its length, for a moment as one of its settings is touched", () => {
    vi.useFakeTimers();
    renderPanel({}, { open: true });
    expect(drawing().querySelector(".band-edge")).toBeNull();
    slideKnob(screen.getByRole("slider", { name: "Reverb high mult" }), 0.2);
    expect(drawing().querySelector(".band-edge")).toHaveAttribute("data-band", "high");
    expect(drawing()).toHaveTextContent(/Highs \d/);
    slideKnob(screen.getByRole("slider", { name: "Reverb bass xover" }), 0.2);
    expect(drawing().querySelector(".band-edge")).toHaveAttribute("data-band", "low");
    act(() => vi.advanceTimersByTime(2000));
    expect(drawing().querySelector(".band-edge")).toBeNull();
    // Others don't move a band of their own.
    slideKnob(screen.getByRole("slider", { name: "Reverb diffusion" }), 0.2);
    expect(drawing().querySelector(".band-edge")).toBeNull();
  });

  it("dragging the drawing's end sets Decay, within Rust's limits, as one gesture", () => {
    const { onSetting } = renderPanel();
    drawing().getBoundingClientRect = () => ({ left: 100, width: TAIL_WIDTH, top: 0, height: 52 }) as DOMRect;
    const handle = drawing().querySelector(".end-handle") as Element;
    fireEvent.pointerDown(handle, { button: 0, clientX: 100 + xOf(end(), TAIL_WIDTH) });
    fireEvent.pointerMove(window, { clientX: 100 + xOf(8, TAIL_WIDTH) });
    const longer = onSetting.mock.lastCall;
    expect(longer?.[0]).toMatchObject({ name: "decay_seconds" });
    expect(longer?.[0].value).toBeGreaterThan(4);
    // The drawing follows the drag before Rust replies.
    expect(end()).toBeCloseTo(8, 0);
    // Past the right edge, Decay stops at its longest.
    fireEvent.pointerMove(window, { clientX: 100 + TAIL_WIDTH * 3 });
    expect(onSetting.mock.lastCall?.[0]).toEqual({ name: "decay_seconds", value: 30 });
    fireEvent.pointerUp(window);
    const gestures = new Set(onSetting.mock.calls.map(([, gesture]) => gesture));
    expect(gestures.size).toBe(1);
    // Let go, it's back to what Rust sent until Rust replies.
    expect(end()).toBeLessThan(5);
  });

  it("lights the early reflections as they arrive and runs a line under the edge while the song plays", () => {
    // The song is 0.05 s into a bar.
    renderPanel({ depth: 0.3, predelay_seconds: 0.02, early_size_seconds: 0.1 }, { songSeconds: () => BAR * 3 + 0.05 });
    frame(1000);
    expect(drawing()).toHaveAttribute("data-lighting", "playing");
    const lit = Array.from(drawing().querySelectorAll<SVGLineElement>(".early line[data-lit]")).map((tick) =>
      Number(tick.dataset.at),
    );
    expect(lit.length).toBeGreaterThan(0);
    for (const at of lit) {
      expect(at).toBeLessThanOrEqual(0.05);
      expect(at).toBeGreaterThan(0.05 - 0.06);
    }
    const sweep = drawing().querySelector<SVGLineElement>(".sweep");
    expect(sweep?.style.visibility).toBe("");
    expect(Number(sweep?.getAttribute("x1"))).toBeCloseTo(xOf(0.05, TAIL_WIDTH), 3);
  });

  it("plays through while you point at it with the song stopped, and never lights when it's off", () => {
    const { rerender } = renderPanel();
    const panel = drawing().closest(".reverb-panel") as HTMLElement;
    fireEvent.pointerEnter(panel);
    frame(1000);
    expect(drawing()).toHaveAttribute("data-lighting", "preview");
    fireEvent.pointerLeave(panel);
    expect(drawing()).not.toHaveAttribute("data-lighting");

    rerender({ enabled: false });
    fireEvent.pointerEnter(panel);
    frame(2000);
    expect(drawing()).not.toHaveAttribute("data-lighting");
    expect(drawing()).toHaveAttribute("data-off");
  });
});
