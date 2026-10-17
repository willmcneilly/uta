import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EffectView } from "../backend";
import { slideKnob } from "../design/knobTesting";
import { FrameLoop } from "../frameLoop";
import { FrameStats } from "../pianoRoll/frameStats";
import { DelayPanel } from "./DelayPanel";
import { ECHO_WIDTH } from "./EchoDrawing";
import { DIVISIONS, type DelayValues, delayView } from "./delayTesting";

// The Delay's panel, drawn from Rust's description of its settings: each
// control sends its own command, Time swaps between the menu and the knob
// with Sync, and the drawing of the echoes follows the settings.

/** One bar at 120 BPM. */
const BAR = 2;

let frames: FrameRequestCallback[];

beforeEach(() => {
  frames = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => frames.push(callback));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

/** Runs the frames waiting, at `now` milliseconds. */
function frame(now: number) {
  const waiting = frames;
  frames = [];
  act(() => waiting.forEach((callback) => callback(now)));
}

interface Options {
  songSeconds?: ((now: number) => number) | null;
}

function renderPanel(values: Partial<DelayValues> = {}, options: Options = {}) {
  const onSetting = vi.fn();
  const onMix = vi.fn();
  const loop = new FrameLoop(new FrameStats());
  const panel = (effect: EffectView) => (
    <DelayPanel
      effect={effect}
      barSeconds={BAR}
      beatsPerBar={4}
      songSeconds={options.songSeconds ?? null}
      frames={loop}
      onSetting={onSetting}
      onMix={onMix}
    />
  );
  const view = render(panel(delayView("d-1", values)));
  const rerender = (next: Partial<DelayValues>) => view.rerender(panel(delayView("d-1", { ...values, ...next })));
  return { onSetting, onMix, rerender, container: view.container };
}

const drawing = () => screen.getByRole("img", { name: "The echoes" });
/** When each lit mark sounds, in seconds after the note. */
const lit = () =>
  Array.from(drawing().querySelectorAll<SVGGElement>(".sounding"))
    .filter((mark) => Number(mark.style.opacity) > 0)
    .map((mark) => Number(mark.dataset.at));
/** Each echo the drawing shows: its side, level and where it is across. */
const echoes = () =>
  Array.from(drawing().querySelectorAll<SVGGElement>(".echo:not(.sounding)")).map((echo) => ({
    side: echo.dataset.side,
    level: Number(echo.dataset.level),
    x: Number(echo.dataset.x),
  }));

describe("DelayPanel", () => {
  it("lays out Time and Sync, then Mode, beside the drawing, and the rest as knobs, Mix last", () => {
    renderPanel();
    expect(screen.getByRole("combobox", { name: "Delay time" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delay sync" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("radio", { name: "Stereo" })).toBeChecked();
    expect(screen.getAllByRole("slider").map((knob) => knob.getAttribute("aria-label"))).toEqual([
      "Delay feedback",
      "Delay low cut",
      "Delay high cut",
      "Delay drive",
      "Delay wobble",
      "Delay mix",
    ]);
    // Values are drawn as Rust gives them.
    expect(screen.getByRole("slider", { name: "Delay feedback" })).toHaveAttribute("aria-valuetext", "40%");
  });

  it("lists Rust's eighteen divisions in the Time menu, and sends the one chosen", () => {
    const { onSetting } = renderPanel();
    const menu = screen.getByRole("combobox", { name: "Delay time" });
    expect(menu).toHaveValue("1/8d");
    const options = within(menu).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual(DIVISIONS.map((d) => d.label));
    expect(options).toHaveLength(18);
    fireEvent.change(menu, { target: { value: "1/4t" } });
    expect(onSetting.mock.lastCall?.[0]).toEqual({ name: "division", value: "1/4t" });
  });

  it("swaps the menu for the knob in seconds when Sync goes off, and back", () => {
    const { onSetting, rerender } = renderPanel();
    fireEvent.click(screen.getByRole("button", { name: "Delay sync" }));
    expect(onSetting.mock.lastCall?.[0]).toEqual({ name: "sync", value: false });

    rerender({ sync: false });
    expect(screen.queryByRole("combobox", { name: "Delay time" })).not.toBeInTheDocument();
    const time = screen.getByRole("slider", { name: "Delay time" });
    expect(time).toHaveAttribute("aria-valuetext", "375 ms");
    slideKnob(time, 0.9);
    expect(onSetting.mock.lastCall?.[0]).toMatchObject({ name: "time_seconds" });
    expect(onSetting.mock.lastCall?.[1]).toEqual(expect.any(Number));

    rerender({ sync: true });
    expect(screen.queryByRole("slider", { name: "Delay time" })).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Delay time" })).toBeInTheDocument();
  });

  it("sends each control's own command", () => {
    const { onSetting, onMix } = renderPanel();
    fireEvent.click(screen.getByRole("radio", { name: "Ping-pong" }));
    expect(onSetting.mock.lastCall?.[0]).toEqual({ name: "mode", value: "ping_pong" });

    for (const [label, name] of [
      ["feedback", "feedback"],
      ["low cut", "low_cut_hz"],
      ["high cut", "high_cut_hz"],
      ["drive", "drive"],
      ["wobble", "wobble"],
    ]) {
      slideKnob(screen.getByRole("slider", { name: `Delay ${label}` }), 0.8);
      expect(onSetting.mock.lastCall?.[0]).toMatchObject({ name });
    }
    slideKnob(screen.getByRole("slider", { name: "Delay mix" }), 0.8);
    expect(onMix.mock.lastCall?.[0]).toBeCloseTo(0.8, 2);
    expect(onMix.mock.lastCall?.[1]).toEqual(expect.any(Number));
  });
});

describe("the echo drawing", () => {
  it("draws a tick per echo, Feedback times the one before, spaced by Rust's time", () => {
    const { rerender } = renderPanel({ feedback: 0.5, drive: 0, wobble: 0 });
    const few = echoes();
    // Each a little quieter than the last, until too quiet to draw.
    expect(few.length).toBeGreaterThan(2);
    few.slice(1).forEach((echo, n) => expect(echo.level).toBeLessThan(few[n].level));
    // A dotted eighth apart, across as many 2 s bars as the echoes last.
    const span = () => BAR * Number(drawing().getAttribute("data-bars"));
    expect(few[1].x - few[0].x).toBeCloseTo((ECHO_WIDTH - 1) * (0.375 / span()), 1);

    rerender({ feedback: 0.8 });
    expect(echoes().length).toBeGreaterThan(few.length);

    rerender({ feedback: 0.5, echoSeconds: 0.5 });
    expect(echoes()[1].x - echoes()[0].x).toBeCloseTo((ECHO_WIDTH - 1) * (0.5 / span()), 1);
  });

  it("spans as many whole bars as the echoes last, up to four", () => {
    const { rerender } = renderPanel({ feedback: 0.2 });
    expect(drawing()).toHaveAttribute("data-bars", "1");
    rerender({ feedback: 0.8 });
    expect(Number(drawing().getAttribute("data-bars"))).toBeGreaterThan(1);
    // Past 100% the echoes never die away.
    rerender({ feedback: 1.1 });
    expect(drawing()).toHaveAttribute("data-bars", "4");
  });

  it("draws stereo on both sides and ping-pong bouncing from side to side", () => {
    const { rerender } = renderPanel();
    expect(new Set(echoes().map((echo) => echo.side))).toEqual(new Set(["both"]));
    rerender({ mode: "ping_pong" });
    expect(echoes().map((echo) => echo.side).slice(0, 4)).toEqual(["left", "right", "left", "right"]);
  });

  it("sways the echoes off the grid with Wobble", () => {
    const { rerender } = renderPanel({ wobble: 0 });
    const onGrid = echoes().map((echo) => echo.x);
    rerender({ wobble: 1 });
    const swayed = echoes().map((echo) => echo.x);
    expect(swayed).not.toEqual(onGrid);
    swayed.forEach((x, n) => expect(Math.abs(x - onGrid[n])).toBeLessThanOrEqual(3));
  });

  it("lights the echoes while the song plays, where the song is", () => {
    // The song is 0.375 s into the bar: the first echo has just sounded.
    renderPanel({}, { songSeconds: () => 0.375 });
    frame(0);
    expect(drawing()).toHaveAttribute("data-lighting", "playing");
    expect(lit()).toEqual([0.375]);
    expect(drawing().querySelector<SVGGElement>(".sweep")?.style.visibility).toBe("");
  });

  it("when stopped, plays its echoes while you point at the panel or focus a control", () => {
    const { container } = renderPanel();
    const panel = container.querySelector(".delay-panel") as HTMLElement;
    frame(0);
    expect(drawing()).not.toHaveAttribute("data-lighting");

    fireEvent.pointerEnter(panel);
    frame(1000);
    expect(drawing()).toHaveAttribute("data-lighting", "preview");
    fireEvent.pointerLeave(panel);
    expect(drawing()).not.toHaveAttribute("data-lighting");

    act(() => screen.getByRole("slider", { name: "Delay feedback" }).focus());
    frame(2000);
    expect(drawing()).toHaveAttribute("data-lighting", "preview");
  });

  it("never lights when the Delay is off, and draws it in the third ink", () => {
    const { container } = renderPanel({ enabled: false }, { songSeconds: () => 0.375 });
    fireEvent.pointerEnter(container.querySelector(".delay-panel") as HTMLElement);
    frame(0);
    expect(drawing()).not.toHaveAttribute("data-lighting");
    expect(drawing()).toHaveAttribute("data-off");
  });
});
