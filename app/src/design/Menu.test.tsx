import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Menu } from "./Menu";

function renderMenu(props: { disabled?: boolean } = {}) {
  const onChange = vi.fn<(value: string) => void>();
  render(
    <Menu
      aria-label="Snap"
      className="here"
      value="1/16"
      onChange={(event) => onChange(event.target.value)}
      {...props}
    >
      <option value="1/8">1/8</option>
      <option value="1/16">1/16</option>
      <option value="off" disabled>
        Off
      </option>
    </Menu>,
  );
  return { onChange, menu: screen.getByRole("combobox", { name: "Snap" }) };
}

describe("Menu", () => {
  it("is the system's own pop-up list, showing the value it's given", () => {
    const { menu } = renderMenu();
    expect(menu.tagName).toBe("SELECT");
    expect(menu).toHaveValue("1/16");
    expect(screen.getAllByRole("option")).toHaveLength(3);
    expect(screen.getByRole("option", { name: "Off" })).toBeDisabled();
  });

  it("sends what's picked", () => {
    const { menu, onChange } = renderMenu();
    fireEvent.change(menu, { target: { value: "1/8" } });
    expect(onChange).toHaveBeenCalledWith("1/8");
  });

  it("is wrapped in the box its chevron is drawn on, which takes the class for its place", () => {
    const { menu } = renderMenu();
    expect(menu.parentElement).toHaveClass("menu", "here");
  });

  it("can be disabled", () => {
    const { menu } = renderMenu({ disabled: true });
    expect(menu).toBeDisabled();
  });
});
