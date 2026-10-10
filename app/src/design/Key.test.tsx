import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Key } from "./Key";

describe("Key", () => {
  it("is a button named by its words, that never submits a form", () => {
    const onClick = vi.fn();
    render(<Key onClick={onClick}>Stop</Key>);
    const key = screen.getByRole("button", { name: "Stop" });
    expect(key).toHaveAttribute("type", "button");
    expect(key).toHaveClass("key");
    fireEvent.click(key);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("draws its mark, in the live ink while what it starts is playing", () => {
    const { rerender } = render(<Key mark="play">Play</Key>);
    const key = screen.getByRole("button", { name: "Play" });
    expect(key).toHaveAttribute("data-mark", "play");
    expect(key).not.toHaveAttribute("data-playing");
    rerender(<Key mark="play" playing>Play</Key>);
    expect(key).toHaveAttribute("data-playing");
  });

  it("is a toggle button only when it's given a pressed state", () => {
    const { rerender } = render(<Key>Loop</Key>);
    const key = screen.getByRole("button", { name: "Loop" });
    expect(key).not.toHaveAttribute("aria-pressed");
    rerender(<Key pressed={false}>Loop</Key>);
    expect(key).toHaveAttribute("aria-pressed", "false");
    rerender(<Key pressed>Loop</Key>);
    expect(key).toHaveAttribute("aria-pressed", "true");
  });

  it("marks the one that goes ahead, and keeps the class it's given for its place", () => {
    render(
      <>
        <Key className="here">Cancel</Key>
        <Key primary>Run</Key>
      </>,
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveClass("key", "here");
    expect(screen.getByRole("button", { name: "Cancel" })).not.toHaveClass("key-primary");
    expect(screen.getByRole("button", { name: "Run" })).toHaveClass("key-primary");
  });

  it("does nothing while disabled", () => {
    const onClick = vi.fn();
    render(
      <Key disabled onClick={onClick}>
        + Add track
      </Key>,
    );
    const key = screen.getByRole("button", { name: "+ Add track" });
    expect(key).toBeDisabled();
    fireEvent.click(key);
    expect(onClick).not.toHaveBeenCalled();
  });
});
