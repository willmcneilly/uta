import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Divider } from "./Divider";

const divider = () => screen.getByRole("separator", { name: "Editor height" });

describe("Divider", () => {
  // The grip is drawn in ink while it's dragged, from this attribute.
  it("is marked as dragging from press to release, however the drag ends", () => {
    render(<Divider height={280} min={160} max={600} onChange={() => {}} />);
    expect(divider()).not.toHaveAttribute("data-dragging");

    fireEvent.pointerDown(divider(), { button: 0, clientY: 500 });
    expect(divider()).toHaveAttribute("data-dragging");
    fireEvent.pointerMove(window, { clientY: 400 });
    expect(divider()).toHaveAttribute("data-dragging");
    fireEvent.pointerUp(window);
    expect(divider()).not.toHaveAttribute("data-dragging");

    // A drag the window loses ends too.
    fireEvent.pointerDown(divider(), { button: 0, clientY: 500 });
    fireEvent.blur(window);
    expect(divider()).not.toHaveAttribute("data-dragging");
  });

  it("isn't marked as dragging by a right click or the arrow keys", () => {
    render(<Divider height={280} min={160} max={600} onChange={() => {}} />);
    fireEvent.pointerDown(divider(), { button: 2, clientY: 500 });
    expect(divider()).not.toHaveAttribute("data-dragging");
    fireEvent.keyDown(divider(), { key: "ArrowUp" });
    expect(divider()).not.toHaveAttribute("data-dragging");
  });
});
