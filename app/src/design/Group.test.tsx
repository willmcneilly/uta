import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Group } from "./Group";

describe("Group", () => {
  it("is a group named by its section title", () => {
    render(
      <Group title="Filter" className="here">
        <button type="button">Inside</button>
      </Group>,
    );
    const group = screen.getByRole("group", { name: "Filter" });
    expect(group).toHaveClass("group", "here");
    expect(screen.getByText("Filter")).toHaveClass("group-title");
    expect(group).toContainElement(screen.getByRole("button", { name: "Inside" }));
  });

  it("gives its title an ID, for a group with a role of its own", () => {
    render(
      <Group title="Waveform" titleId="wave" role="radiogroup" aria-labelledby="wave">
        <input type="radio" aria-label="Sine" />
      </Group>,
    );
    expect(screen.getByText("Waveform")).toHaveAttribute("id", "wave");
    expect(screen.getByRole("radiogroup", { name: "Waveform" })).toBeInTheDocument();
  });
});
