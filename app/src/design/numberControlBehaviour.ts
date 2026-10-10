// The behaviour every control that sets a number shares (RFC-005, part 7),
// as one set of tests. Each control's own test file runs them against it
// with `describeNumberControl`, so the fader, the knob and the drag field
// are held to the same behaviour.

import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { linearScale, type NumberScale } from "./numberScale";
import { DRAG_THRESHOLD, FINE_DRAG, type NumberControlOptions } from "./useNumberControl";

/** Keys held during a press or a move. */
export interface Keys {
  shiftKey?: boolean;
  altKey?: boolean;
}

/** A drag along the control, in pixels the way it's dragged to raise the value. */
export interface ControlDrag {
  by: (pixels: number, keys?: Keys) => ControlDrag;
  release: () => void;
}

/** How the tests render and drag one kind of control. */
export interface NumberControlHarness {
  /** Renders the control, and returns its slider and a way to render it again with new options. */
  render: (options: NumberControlOptions) => {
    control: HTMLElement;
    rerender: (options: NumberControlOptions) => void;
  };
  /**
   * Presses on `control`, `offset` pixels along it from where its value is
   * drawn (on it, unless given), with `keys` held.
   */
  press: (control: HTMLElement, options?: { offset?: number } & Keys) => ControlDrag;
  /** How many pixels of drag take it from one end to the other. */
  travel: number;
}

// A volume from -60 to 0 dB in 0.5 dB steps: 120 positions.
const scale = linearScale(-60, 0, 0.5);
const formatDb = (db: number) => `${db.toFixed(1)} dB`;
// The same volume in 0.01 dB steps, so a pixel of drag is several steps.
const fineScale = linearScale(-60, 0, 0.01);

/** Runs the shared behaviour tests against the control `harness` renders. */
export function describeNumberControl(name: string, harness: NumberControlHarness) {
  /** How many pixels of drag move it `steps` positions. */
  const pixels = (steps: number) => (steps * harness.travel) / scale.steps;

  function renderControl(value = -12, defaultValue = -12, controlScale: NumberScale = scale) {
    const onChange = vi.fn<(value: number, gesture?: number) => void>();
    const onDrag = vi.fn<(value: number | null) => void>();
    const options = { defaultValue, scale: controlScale, format: formatDb, onChange, onDrag };
    const { control, rerender } = harness.render({ ...options, value });
    /** Rust sends a new project value. */
    const project = (next: number) => rerender({ ...options, value: next });
    const sent = () => onChange.mock.calls.map(([value]) => value);
    const gestures = () => onChange.mock.calls.map(([, gesture]) => gesture);
    return { control, onChange, onDrag, project, sent, gestures };
  }

  describe(`${name}: the shared behaviour of a control that sets a number`, () => {
    it("is a slider to assistive tech, with its value, range and unit", () => {
      const { control } = renderControl(-20);
      expect(control).toHaveAttribute("role", "slider");
      expect(control).toHaveAttribute("aria-valuemin", "-60");
      expect(control).toHaveAttribute("aria-valuemax", "0");
      expect(control).toHaveAttribute("aria-valuenow", "-20");
      expect(control).toHaveAttribute("aria-valuetext", "-20.0 dB");
      expect(control).toHaveAttribute("tabindex", "0");
    });

    it("shows its value and unit, and its own value during a drag", () => {
      const { control } = renderControl(-20);
      expect(screen.getByText("-20.0 dB")).toBeVisible();
      const drag = harness.press(control).by(pixels(10));
      expect(screen.getByText("-15.0 dB")).toBeVisible();
      drag.release();
    });

    it("takes focus when pressed, with no focus ring until a key is pressed", () => {
      const { control } = renderControl();
      harness.press(control).release();
      expect(control).toHaveFocus();
      expect(control).toHaveAttribute("data-pointer-focused");
      fireEvent.keyDown(control, { key: "ArrowRight" });
      expect(control).not.toHaveAttribute("data-pointer-focused");
      harness.press(control).release();
      fireEvent.blur(control);
      expect(control).not.toHaveAttribute("data-pointer-focused");
    });

    it("steps with the arrow keys, and bigger steps with Shift, each change on its own", () => {
      const { control, sent, gestures } = renderControl(-30);
      fireEvent.keyDown(control, { key: "ArrowRight" });
      fireEvent.keyDown(control, { key: "ArrowUp" });
      fireEvent.keyDown(control, { key: "ArrowLeft", shiftKey: true });
      fireEvent.keyDown(control, { key: "ArrowDown", shiftKey: true });
      fireEvent.keyDown(control, { key: "PageUp" });
      fireEvent.keyDown(control, { key: "PageDown" });
      fireEvent.keyDown(control, { key: "End" });
      fireEvent.keyDown(control, { key: "Home" });
      // Each steps from the project's value, which Rust hasn't changed.
      expect(sent()).toEqual([-29.5, -29.5, -35, -35, -25, -35, 0, -60]);
      expect(gestures()).toEqual(Array(8).fill(undefined));
    });

    it("sends nothing for a key at an end", () => {
      const { control, onChange } = renderControl(0);
      fireEvent.keyDown(control, { key: "ArrowRight" });
      fireEvent.keyDown(control, { key: "End" });
      expect(onChange).not.toHaveBeenCalled();
    });

    it("picks it up where it is wherever it's pressed, and drags it from there", () => {
      const { control, onChange, sent } = renderControl(-30);
      harness.press(control, { offset: pixels(30) }).release();
      harness.press(control, { offset: -pixels(30) }).release();
      expect(onChange).not.toHaveBeenCalled();
      expect(control).toHaveAttribute("aria-valuenow", "-30");
      harness.press(control, { offset: pixels(30) }).by(pixels(10)).release();
      expect(sent()).toEqual([-25]);
    });

    it("stays where it is until the pointer passes DRAG_THRESHOLD, then moves from the press", () => {
      const { control, onChange, sent } = renderControl(-30, -12, fineScale);
      const drag = harness.press(control).by(DRAG_THRESHOLD - 1).by(-(DRAG_THRESHOLD - 1) * 2);
      expect(onChange).not.toHaveBeenCalled();
      drag.by(DRAG_THRESHOLD * 2 - 1);
      const moved = Math.round((DRAG_THRESHOLD * fineScale.steps) / harness.travel);
      expect(sent()).toEqual([fineScale.fromPosition(fineScale.toPosition(-30) + moved)]);
      drag.release();
    });

    it("ignores a wobble during a double-click, so the reset is the only step", () => {
      const { control, onChange } = renderControl(-30, -12, fineScale);
      harness.press(control).by(1).by(-2).release();
      harness.press(control).by(DRAG_THRESHOLD - 1).release();
      fireEvent.doubleClick(control);
      expect(onChange).toHaveBeenCalledExactlyOnceWith(-12);
    });

    it("shows its own value during a drag, and the project's after", () => {
      const { control, sent, project } = renderControl(-30);
      const drag = harness.press(control).by(pixels(30));
      // Rust hasn't replied: it still shows where the drag took it.
      expect(sent()).toEqual([-15]);
      expect(control).toHaveAttribute("aria-valuenow", "-15");
      expect(control).toHaveAttribute("aria-valuetext", "-15.0 dB");
      // A reply to an older step doesn't move it back.
      project(-25);
      expect(control).toHaveAttribute("aria-valuenow", "-15");

      drag.release();
      expect(control).toHaveAttribute("aria-valuenow", "-25");
      expect(control).toHaveAttribute("aria-valuetext", "-25.0 dB");
      project(-15);
      expect(control).toHaveAttribute("aria-valuenow", "-15");
    });

    it("reports the value it shows during a drag, then null when the drag ends", () => {
      const { control, onDrag } = renderControl(-30);
      // A pixel is less than half a step, so the value shown doesn't change.
      const drag = harness.press(control).by(pixels(30)).by(1).by(-pixels(30) - 1);
      expect(onDrag.mock.calls.map(([value]) => value)).toEqual([-30, -15, -30]);
      drag.release();
      expect(onDrag).toHaveBeenLastCalledWith(null);
      // Keys change the project's value, which it shows as it is.
      fireEvent.keyDown(control, { key: "ArrowRight" });
      expect(onDrag).toHaveBeenCalledTimes(4);
    });

    it("sends each new step of a drag once, all with the drag's gesture", () => {
      const { control, sent, gestures } = renderControl(-30);
      // A pixel is less than half a step, so it sends nothing.
      harness.press(control).by(-pixels(30)).by(1).by(pixels(30) - 1).release();
      harness.press(control).by(pixels(30)).release();
      expect(sent()).toEqual([-45, -30, -15]);
      const [first, second, third] = gestures();
      expect(first).toEqual(expect.any(Number));
      expect(second).toBe(first);
      expect(third).toEqual(expect.any(Number));
      expect(third).not.toBe(first);
    });

    it("moves in fine steps with Shift held from the press", () => {
      const { control, sent, gestures } = renderControl(-30);
      harness.press(control, { shiftKey: true }).by(pixels(50), { shiftKey: true }).release();
      expect(sent()).toEqual([-30 + 0.5 * 50 * FINE_DRAG]);
      expect(gestures()[0]).toEqual(expect.any(Number));
    });

    it("goes fine when Shift is pressed mid-drag, from where it is, and back when it's let go", () => {
      const { control, sent, gestures } = renderControl(-30);
      const drag = harness.press(control).by(pixels(30));
      expect(sent()).toEqual([-15]);
      // Ten steps' worth with Shift is one step, from where it is.
      drag.by(-pixels(10), { shiftKey: true });
      expect(sent()).toEqual([-15, -15.5]);
      // Let go, and it moves at the plain rate again, from there.
      drag.by(pixels(10));
      expect(sent()).toEqual([-15, -15.5, -10.5]);
      drag.release();
      expect(new Set(gestures()).size).toBe(1);
    });

    it("doesn't drag with ⌥ held, which resets it instead", () => {
      const { control, onChange } = renderControl(-30, -12);
      harness.press(control, { altKey: true }).by(pixels(30), { altKey: true }).release();
      expect(onChange).toHaveBeenCalledExactlyOnceWith(-12);
    });

    describe.each([
      ["double-click", (control: HTMLElement) => fireEvent.doubleClick(control)],
      ["⌥-click", (control: HTMLElement) => harness.press(control, { altKey: true }).release()],
      ["Delete", (control: HTMLElement) => fireEvent.keyDown(control, { key: "Delete" })],
      ["Backspace", (control: HTMLElement) => fireEvent.keyDown(control, { key: "Backspace" })],
    ])("%s", (_gesture, reset) => {
      it("resets it to its default as a step of its own, and sends nothing when it's there", () => {
        const { control, onChange, project } = renderControl(-30, -12);
        reset(control);
        expect(onChange).toHaveBeenCalledExactlyOnceWith(-12);
        project(-12);
        reset(control);
        expect(onChange).toHaveBeenCalledTimes(1);
      });

      it("resets it again once an undo brings the old value back", () => {
        const { control, onChange, project } = renderControl(-30, -12);
        reset(control);
        project(-12);
        project(-30);
        reset(control);
        expect(onChange.mock.calls).toEqual([[-12], [-12]]);
      });
    });

    it("leaves Delete and Backspace with ⌘, Ctrl or ⌥ to the app's own shortcuts", () => {
      const { control, onChange } = renderControl(-30, -12);
      const notPrevented = [
        fireEvent.keyDown(control, { key: "Backspace", metaKey: true }),
        fireEvent.keyDown(control, { key: "Delete", ctrlKey: true }),
        fireEvent.keyDown(control, { key: "Backspace", altKey: true }),
      ];
      expect(notPrevented).toEqual([true, true, true]);
      expect(onChange).not.toHaveBeenCalled();
    });

    it("resets once for a double-click, with ⌥ or without, before Rust replies", () => {
      const { control, onChange } = renderControl(-30, -12);
      harness.press(control, { altKey: true }).release();
      harness.press(control, { altKey: true }).release();
      fireEvent.doubleClick(control, { altKey: true });
      harness.press(control).release();
      harness.press(control).release();
      fireEvent.doubleClick(control);
      expect(onChange).toHaveBeenCalledExactlyOnceWith(-12);
    });

    it("leaves the value alone when the scroll wheel turns over it", () => {
      const { control, onChange } = renderControl(-30);
      fireEvent.focus(control);
      const notPrevented = fireEvent.wheel(control, { deltaY: -100 });
      fireEvent.wheel(control, { deltaY: 100 });
      expect(notPrevented).toBe(true);
      expect(onChange).not.toHaveBeenCalled();
      expect(control).toHaveAttribute("aria-valuenow", "-30");
    });

    it("ignores keys during a drag", () => {
      const { control, sent } = renderControl(-30);
      const drag = harness.press(control).by(pixels(10));
      fireEvent.keyDown(control, { key: "ArrowRight" });
      fireEvent.keyDown(control, { key: "Delete" });
      drag.release();
      expect(sent()).toEqual([-25]);
    });

    it("ends a drag the window loses where it is", () => {
      const { control, sent, project } = renderControl(-30);
      const drag = harness.press(control).by(-pixels(30));
      fireEvent.blur(window);
      project(-45);
      drag.by(pixels(60));
      expect(sent()).toEqual([-45]);
      expect(control).toHaveAttribute("aria-valuenow", "-45");
    });
  });
}
