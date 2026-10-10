import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Menu } from "./Menu";
import { ToolChip, ZoomKeys } from "./ToolChip";

describe("ToolChip", () => {
  it("shows its caption to the eye only, leaving the control inside to name itself", () => {
    render(
      <ToolChip label="Snap">
        <Menu aria-label="Clip snap" value="bar" onChange={() => {}}>
          <option value="bar">Bar</option>
        </Menu>
      </ToolChip>,
    );
    expect(screen.getByRole("combobox", { name: "Clip snap" })).toBeInTheDocument();
    expect(screen.getByText("Snap")).toHaveAttribute("aria-hidden", "true");
    expect(screen.queryByLabelText("Snap")).toBeNull();
  });

  it("can be a named group of keys", () => {
    render(
      <ToolChip role="group" aria-label="Zoom" className="here">
        <ZoomKeys label="Time" what="time" onOut={() => {}} onIn={() => {}} />
      </ToolChip>,
    );
    const group = screen.getByRole("group", { name: "Zoom" });
    expect(group).toHaveClass("tool-chip", "here");
    expect(within(group).getAllByRole("button")).toHaveLength(2);
  });
});

describe("ZoomKeys", () => {
  it("are − and + keys either side of a caption, named for what they zoom", () => {
    const onOut = vi.fn();
    const onIn = vi.fn();
    render(<ZoomKeys label="Pitch" what="pitch" onOut={onOut} onIn={onIn} />);
    const out = screen.getByRole("button", { name: "Zoom out pitch" });
    const zoomIn = screen.getByRole("button", { name: "Zoom in pitch" });
    expect(out).toHaveTextContent("−");
    expect(zoomIn).toHaveTextContent("+");
    expect(out.nextElementSibling).toHaveTextContent("Pitch");
    expect(out.nextElementSibling?.nextElementSibling).toBe(zoomIn);
    fireEvent.click(out);
    expect(onOut).toHaveBeenCalledOnce();
    expect(onIn).not.toHaveBeenCalled();
    fireEvent.click(zoomIn);
    expect(onIn).toHaveBeenCalledOnce();
  });
});
