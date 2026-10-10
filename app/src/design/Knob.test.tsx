import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { KNOB_TRAVEL, Knob } from "./Knob";
import { knobAt, pressKnob } from "./knobTesting";
import { describeNumberControl } from "./numberControlBehaviour";
import { linearScale } from "./numberScale";
import type { NumberControlOptions } from "./useNumberControl";

describeNumberControl("Knob", {
  render: (options) => {
    const view = render(<Knob label="Volume" {...options} />);
    return {
      control: screen.getByRole("slider", { name: "Volume" }),
      rerender: (next: NumberControlOptions) => view.rerender(<Knob label="Volume" {...next} />),
    };
  },
  press: (knob, options) => pressKnob(knob, options),
  travel: KNOB_TRAVEL,
});

// A pan from left to right in hundredths: 200 positions.
const pan = linearScale(-1, 1, 0.01);
const formatPan = (value: number) => (value === 0 ? "C" : `${value < 0 ? "L" : "R"} ${Math.abs(Math.round(value * 100))}`);

function renderKnob(value = 0, { defaultValue = 0, centred = false, hideLabel = false } = {}) {
  const onChange = vi.fn<(value: number, gesture?: number) => void>();
  render(
    <Knob
      label="Pan"
      ariaLabel={hideLabel ? "Synth 1 pan" : undefined}
      hideLabel={hideLabel}
      centred={centred}
      value={value}
      defaultValue={defaultValue}
      scale={pan}
      format={formatPan}
      onChange={onChange}
    />,
  );
  const knob = screen.getByRole("slider", { name: hideLabel ? "Synth 1 pan" : "Pan" });
  return { knob, onChange };
}

const valueArc = (knob: HTMLElement) => knob.querySelector(".knob-value")?.getAttribute("d");
const defaultTick = (knob: HTMLElement) =>
  knob.querySelector(".knob-default")?.getAttribute("transform");

describe("Knob", () => {
  it("is a vertical slider, dragged up and down, named by its label", () => {
    const { knob } = renderKnob();
    expect(knob).toHaveAttribute("aria-orientation", "vertical");
    expect(screen.getByText("Pan")).toBeVisible();
  });

  it("can leave its label out and take its name from ariaLabel", () => {
    renderKnob(0, { hideLabel: true });
    expect(screen.queryByText("Pan")).not.toBeInTheDocument();
  });

  it("writes its value underneath the dial, inside the hit area", () => {
    const { knob } = renderKnob(-0.35);
    expect(knob).toHaveTextContent("L 35");
    expect(knob.querySelector(".knob-dial")?.nextElementSibling).toHaveTextContent("L 35");
  });

  it("draws its indicator at its value, round 270 degrees from the bottom left", () => {
    const { knob } = renderKnob(-1);
    expect(knobAt(knob)).toBe(0);
    const indicator = knob.querySelector(".knob-indicator");
    expect(indicator).toHaveAttribute("transform", "rotate(-135 16 16)");
  });

  it("moves up for more and down for less, never round in a circle", () => {
    const { knob, onChange } = renderKnob(0);
    // A drag sideways does nothing.
    const drag = pressKnob(knob);
    fireEvent.pointerMove(window, { clientX: KNOB_TRAVEL, clientY: KNOB_TRAVEL / 2 });
    expect(onChange).not.toHaveBeenCalled();
    drag.by(KNOB_TRAVEL / 4);
    expect(knobAt(knob)).toBeCloseTo(0.75, 6);
    drag.by(-KNOB_TRAVEL / 2);
    expect(knobAt(knob)).toBeCloseTo(0.25, 6);
    drag.release();
    expect(onChange.mock.calls.map(([value]) => value)).toEqual([0.5, -0.5]);
  });

  it("fills the value from the start of the range", () => {
    const { knob } = renderKnob(-1, { defaultValue: -1 });
    expect(valueArc(knob)).toBeUndefined();
    const drag = pressKnob(knob).to(0.5);
    // From the bottom left round to the top.
    expect(valueArc(knob)).toBe("M8.222,23.778 A11,11 0 0 1 16.000,5.000");
    drag.release();
  });

  it("fills a centred setting from the middle, either way", () => {
    const { knob } = renderKnob(0, { centred: true });
    expect(valueArc(knob)).toBeUndefined();
    let drag = pressKnob(knob).to(1);
    // From the top round to the bottom right.
    expect(valueArc(knob)).toBe("M16.000,5.000 A11,11 0 0 1 23.778,23.778");
    drag.release();
    drag = pressKnob(knob).to(0);
    // From the bottom left round to the top.
    expect(valueArc(knob)).toBe("M8.222,23.778 A11,11 0 0 1 16.000,5.000");
    drag.release();
  });

  it("marks its default with a tick outside the arc", () => {
    const { knob } = renderKnob(0, { defaultValue: 0.5 });
    // Three quarters of the way round: 67.5 degrees right of the top.
    expect(defaultTick(knob)).toBe("rotate(67.5 16 16)");
  });
});
