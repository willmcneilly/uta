import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { projectView } from "./pianoRoll/testing";
import { Transport } from "./Transport";

function transport(playing: boolean) {
  return (
    <Transport
      project={projectView()}
      playing={playing}
      position={{ bar: 1, beat: 1 }}
      onPlay={vi.fn()}
      onStop={vi.fn()}
      onTempo={vi.fn()}
      onLoopEnabled={vi.fn()}
    />
  );
}

describe("Transport", () => {
  it("marks the Play key while the song plays, so its mark is drawn in the live ink", () => {
    const { rerender } = render(transport(true));
    expect(screen.getByRole("button", { name: "Play" })).toHaveAttribute("data-playing");
    expect(screen.getByRole("button", { name: "Stop" })).not.toHaveAttribute("data-playing");

    rerender(transport(false));
    expect(screen.getByRole("button", { name: "Play" })).not.toHaveAttribute("data-playing");
  });
});
