import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DragField, PIXELS_PER_STEP } from "./DragField";
import { openForTyping, press, typeInto } from "./dragFieldTesting";
import { describeNumberControl } from "./numberControlBehaviour";
import { linearScale } from "./numberScale";
import { parseTyped } from "./parseTyped";
import type { NumberControlOptions } from "./useNumberControl";

const DB = { db: 1 };

// The shared tests drag a control the same number of pixels from end to end
// whatever its scale, as a fader's rail is, so the field is given that.
const TRAVEL = 480;

describeNumberControl("DragField", {
  render: (options) => {
    const field = (next: NumberControlOptions) => (
      <DragField label="Volume" units={DB} pixelsPerStep={TRAVEL / next.scale.steps} {...next} />
    );
    const view = render(field(options));
    return {
      control: screen.getByRole("slider", { name: "Volume" }),
      rerender: (next: NumberControlOptions) => view.rerender(field(next)),
    };
  },
  press,
  travel: TRAVEL,
});

// A tempo from 20 to 300 BPM in whole steps.
const scale = linearScale(20, 300, 1);
const formatBpm = (bpm: number) => `${bpm} BPM`;

function renderTempo(value = 120, defaultValue = 120) {
  const onChange = vi.fn<(value: number, gesture?: number) => void>();
  const field = (next: number) => (
    <DragField
      label="Tempo"
      value={next}
      defaultValue={defaultValue}
      scale={scale}
      format={formatBpm}
      units={{ bpm: 1 }}
      onChange={onChange}
    />
  );
  const view = render(field(value));
  const tempo = screen.getByRole("slider", { name: "Tempo" });
  const sent = () => onChange.mock.calls.map(([value]) => value);
  const gestures = () => onChange.mock.calls.map(([, gesture]) => gesture);
  /** Rust sends a new tempo. */
  const project = (next: number) => view.rerender(field(next));
  return { tempo, onChange, sent, gestures, project };
}

const box = () => screen.queryByRole("textbox", { name: "Tempo" });

describe("parseTyped", () => {
  it.each([
    ["120", { bpm: 1 }, 120],
    ["120 bpm", { bpm: 1 }, 120],
    [" 120BPM ", { bpm: 1 }, 120],
    ["98.5", { bpm: 1 }, 98.5],
    [".5", { bpm: 1 }, 0.5],
    ["1k", { hz: 1 }, 1000],
    ["1.5 kHz", { hz: 1 }, 1500],
    ["2 khz", { hz: 1, khz: 1000 }, 2000],
    ["250 ms", { s: 1, ms: 0.001 }, 0.25],
    ["2 s", { s: 1, ms: 0.001 }, 2],
    ["-6", { db: 1 }, -6],
    ["−6", { db: 1 }, -6],
    ["−6 dB", { db: 1 }, -6],
    ["+3", { db: 1 }, 3],
  ])("reads %j as %s", (text, units, expected) => {
    expect(parseTyped(text, units)).toBeCloseTo(expected);
  });

  it.each([
    ["", { bpm: 1 }],
    ["fast", { bpm: 1 }],
    ["120 ms", { bpm: 1 }],
    ["12-0", { bpm: 1 }],
    ["1.2.3", { bpm: 1 }],
    ["--6", { db: 1 }],
  ])("can't read %j", (text, units) => {
    expect(parseTyped(text, units)).toBeNull();
  });
});

describe("DragField", () => {
  it("is a vertical slider, named by its label, showing its value and unit", () => {
    const { tempo } = renderTempo(128);
    expect(tempo).toHaveAttribute("aria-orientation", "vertical");
    expect(tempo).toHaveTextContent("128 BPM");
  });

  it("goes up when dragged up and down when dragged down, PIXELS_PER_STEP a step", () => {
    const { tempo, sent } = renderTempo(120);
    press(tempo).by(PIXELS_PER_STEP * 10).release();
    press(tempo).by(-PIXELS_PER_STEP * 30).release();
    expect(sent()).toEqual([130, 90]);
  });

  it("isn't dragged left and right", () => {
    const { tempo, onChange } = renderTempo(120);
    fireEvent.pointerDown(tempo, { button: 0, clientX: 100, clientY: 500 });
    fireEvent.pointerMove(window, { clientX: 400, clientY: 500 });
    fireEvent.pointerUp(window);
    expect(onChange).not.toHaveBeenCalled();
  });

  describe("typing", () => {
    it("opens for typing on a click without dragging, with its value selected to type over", () => {
      const { tempo } = renderTempo(120);
      expect(box()).not.toBeInTheDocument();
      const typing = openForTyping(tempo);
      expect(typing).toBe(box());
      expect(typing).toHaveFocus();
      expect(typing).toHaveValue("120 BPM");
      expect(typing.selectionStart).toBe(0);
      expect(typing.selectionEnd).toBe("120 BPM".length);
    });

    it("doesn't open after a drag", () => {
      const { tempo } = renderTempo(120);
      press(tempo).up(5).release();
      expect(box()).not.toBeInTheDocument();
    });

    it.each([
      ["128", 128],
      ["128 bpm", 128],
      ["128 BPM", 128],
      ["98.4", 98],
      ["98.5", 99],
    ])("sets %j as %s, as one change with no gesture, so it's one undo step", (text, bpm) => {
      const { tempo, sent, gestures } = renderTempo(120);
      typeInto(tempo, text);
      expect(sent()).toEqual([bpm]);
      expect(gestures()).toEqual([undefined]);
      expect(box()).not.toBeInTheDocument();
      expect(tempo).toHaveFocus();
    });

    it.each([
      ["1k", 300],
      ["999", 300],
      ["5", 20],
      ["-6", 20],
      ["−6", 20],
    ])("sets %j, out of range, to the nearest limit: %s", (text, bpm) => {
      const { tempo, sent } = renderTempo(120);
      typeInto(tempo, text);
      expect(sent()).toEqual([bpm]);
    });

    it.each(["", "fast", "120 ms", "1.2.3"])("leaves the value as it was for %j, which it can't read", (text) => {
      const { tempo, onChange } = renderTempo(120);
      typeInto(tempo, text);
      expect(onChange).not.toHaveBeenCalled();
      expect(box()).not.toBeInTheDocument();
      expect(tempo).toHaveTextContent("120 BPM");
    });

    it("sends nothing when the value typed is the one it has", () => {
      const { tempo, onChange } = renderTempo(120);
      typeInto(tempo, "120");
      typeInto(tempo, "120.2");
      expect(onChange).not.toHaveBeenCalled();
    });

    it("cancels with Escape, leaving the value as it was and focus on the field", () => {
      const { tempo, onChange } = renderTempo(120);
      const typing = openForTyping(tempo);
      fireEvent.change(typing, { target: { value: "90" } });
      fireEvent.keyDown(typing, { key: "Escape" });
      expect(box()).not.toBeInTheDocument();
      expect(tempo).toHaveFocus();
      fireEvent.blur(tempo);
      expect(onChange).not.toHaveBeenCalled();
    });

    it("sets what was typed when focus leaves the box, once", () => {
      const { tempo, sent } = renderTempo(120);
      const typing = openForTyping(tempo);
      fireEvent.change(typing, { target: { value: "90" } });
      fireEvent.blur(typing);
      expect(box()).not.toBeInTheDocument();
      expect(sent()).toEqual([90]);
    });

    it("sets it once for Return, though focus leaves the box after", () => {
      const { tempo, sent } = renderTempo(120);
      const typing = openForTyping(tempo);
      fireEvent.change(typing, { target: { value: "90" } });
      fireEvent.keyDown(typing, { key: "Enter" });
      fireEvent.blur(typing);
      expect(sent()).toEqual([90]);
    });

    it("leaves keys typed in the box to the box: Backspace and the arrows edit the text", () => {
      const { tempo, onChange } = renderTempo(140, 120);
      const typing = openForTyping(tempo);
      fireEvent.keyDown(typing, { key: "Backspace" });
      fireEvent.keyDown(typing, { key: "ArrowUp" });
      expect(onChange).not.toHaveBeenCalled();
      expect(box()).toBeInTheDocument();
    });

    it("shows a new tempo from Rust once it's set", () => {
      const { tempo, project } = renderTempo(120);
      typeInto(tempo, "90");
      project(90);
      expect(tempo).toHaveTextContent("90 BPM");
      expect(tempo).toHaveAttribute("aria-valuenow", "90");
    });

    it("resets on a double-click, whose first click opened the box, and shuts the box", () => {
      const { tempo, onChange } = renderTempo(140, 120);
      const typing = openForTyping(tempo);
      // The second press of the double-click lands in the box.
      fireEvent.mouseDown(typing, { detail: 2 });
      expect(box()).not.toBeInTheDocument();
      expect(onChange).toHaveBeenCalledExactlyOnceWith(120);
    });

    it("leaves a later double-click in the box to select text", () => {
      const { tempo, onChange } = renderTempo(140, 120);
      const typing = openForTyping(tempo);
      fireEvent.mouseDown(typing, { detail: 1 });
      fireEvent.mouseDown(typing, { detail: 2 });
      expect(box()).toBeInTheDocument();
      expect(onChange).not.toHaveBeenCalled();
    });

    it("doesn't open on ⌥-click, which resets it", () => {
      const { tempo, onChange } = renderTempo(140, 120);
      press(tempo, { altKey: true }).release();
      expect(box()).not.toBeInTheDocument();
      expect(onChange).toHaveBeenCalledExactlyOnceWith(120);
    });

    it("hides its own value under the box while open", () => {
      const { tempo } = renderTempo(120);
      openForTyping(tempo);
      expect(tempo).toHaveAttribute("data-typing");
    });
  });
});
