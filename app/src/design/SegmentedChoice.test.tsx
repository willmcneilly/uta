import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { type Option, SegmentedChoice } from "./SegmentedChoice";

type Shape = "circle" | "square" | "star";

const OPTIONS: Option<Shape>[] = [
  { value: "circle", label: "Circle", drawing: <svg data-testid="circle-drawing" /> },
  { value: "square", label: "Square" },
  { value: "star", label: "Star" },
];

/** A choice that, like the app, shows whatever value it's given back. */
function renderChoice(value: Shape = "square") {
  const onChange = vi.fn<(value: Shape) => void>();
  const choice = (chosen: Shape) => (
    <SegmentedChoice legend="Shape" name="shape" options={OPTIONS} value={chosen} onChange={onChange} />
  );
  const view = render(choice(value));
  const radio = (name: string) => screen.getByRole("radio", { name });
  /** Rust replies with the value sent last. */
  const reply = () => view.rerender(choice(onChange.mock.lastCall![0]));
  return { onChange, radio, reply };
}

describe("SegmentedChoice", () => {
  it("is a radio group named by its legend, with a radio for each option", () => {
    const { radio } = renderChoice();
    const group = screen.getByRole("radiogroup", { name: "Shape" });
    expect(within(group).getAllByRole("radio")).toHaveLength(3);
    expect(radio("Square")).toBeChecked();
    expect(radio("Circle")).not.toBeChecked();
  });

  it("draws an option's drawing hidden from assistive tech, above its label", () => {
    renderChoice();
    const drawing = screen.getByTestId("circle-drawing");
    expect(drawing.parentElement).toHaveAttribute("aria-hidden", "true");
    expect(drawing.closest("label")).toHaveTextContent("Circle");
  });

  it("puts only the chosen option in the Tab order", () => {
    const { radio } = renderChoice("star");
    expect(radio("Star")).toHaveAttribute("tabindex", "0");
    expect(radio("Circle")).toHaveAttribute("tabindex", "-1");
    expect(radio("Square")).toHaveAttribute("tabindex", "-1");
  });

  it("chooses an option when it's clicked, and focuses it", () => {
    const { onChange, radio, reply } = renderChoice();
    fireEvent.click(radio("Circle"));
    expect(onChange).toHaveBeenCalledExactlyOnceWith("circle");
    expect(radio("Circle")).toHaveFocus();
    reply();
    expect(radio("Circle")).toBeChecked();
    expect(radio("Circle")).toHaveAttribute("tabindex", "0");
  });

  it("moves the choice with the arrow keys, round from one end to the other, taking focus with it", () => {
    const { onChange, radio, reply } = renderChoice("circle");
    const press = (from: string, key: string) => {
      fireEvent.keyDown(radio(from), { key });
      reply();
    };
    press("Circle", "ArrowRight");
    expect(radio("Square")).toHaveFocus();
    press("Square", "ArrowDown");
    expect(radio("Star")).toHaveFocus();
    press("Star", "ArrowRight");
    expect(radio("Circle")).toHaveFocus();
    press("Circle", "ArrowLeft");
    expect(radio("Star")).toHaveFocus();
    press("Star", "ArrowUp");
    expect(radio("Square")).toHaveFocus();
    expect(radio("Square")).toBeChecked();
    expect(onChange.mock.calls.map(([value]) => value)).toEqual([
      "square",
      "star",
      "circle",
      "star",
      "square",
    ]);
  });

  it("leaves other keys alone", () => {
    const { onChange, radio } = renderChoice();
    const notPrevented = fireEvent.keyDown(radio("Square"), { key: "Tab" });
    expect(notPrevented).toBe(true);
    expect(onChange).not.toHaveBeenCalled();
  });
});
