import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RoundKey } from "./RoundKey";

describe("RoundKey", () => {
  it("is a toggle button with its letter drawn, named for assistive tech", () => {
    render(<RoundKey letter="M" aria-label="Mute Bass" pressed={false} />);
    const key = screen.getByRole("button", { name: "Mute Bass" });
    expect(key).toHaveAttribute("type", "button");
    expect(key).toHaveClass("round-key");
    expect(key).toHaveAttribute("aria-pressed", "false");
    expect(key).toHaveTextContent("M");
  });

  it("shows the state it's given, and leaves changing it to whoever owns it", () => {
    const onClick = vi.fn();
    const { rerender } = render(
      <RoundKey letter="S" aria-label="Solo Bass" pressed={false} onClick={onClick} />,
    );
    const key = screen.getByRole("button", { name: "Solo Bass" });
    fireEvent.click(key, { altKey: true });
    expect(onClick).toHaveBeenCalledWith(expect.objectContaining({ altKey: true }));
    expect(key).toHaveAttribute("aria-pressed", "false");
    rerender(<RoundKey letter="S" aria-label="Solo Bass" pressed onClick={onClick} />);
    expect(key).toHaveAttribute("aria-pressed", "true");
  });

  it("does nothing while disabled", () => {
    const onClick = vi.fn();
    render(<RoundKey letter="M" aria-label="Mute" pressed disabled onClick={onClick} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute" }));
    expect(onClick).not.toHaveBeenCalled();
  });
});
