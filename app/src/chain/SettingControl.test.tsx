import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EffectSettingView } from "../backend";
import { SettingControl } from "./SettingControl";

// An effect's control is drawn from Rust's description of its setting
// alone, so a choice or a toggle a later effect brings needs nothing here.

const option = (value: string) => ({ value, label: value.toUpperCase() });

const short: EffectSettingView = {
  name: "mode",
  kind: "choice",
  label: "Mode",
  sounds: "",
  value: "a",
  options: ["a", "b"].map(option),
  default: "a",
};
const long: EffectSettingView = {
  name: "division",
  kind: "choice",
  label: "Division",
  sounds: "",
  value: "c",
  options: ["a", "b", "c", "d", "e", "f"].map(option),
  default: "a",
};
const toggle: EffectSettingView = { name: "sync", kind: "toggle", label: "Sync", sounds: "", value: false, default: true };
const knob: EffectSettingView = {
  name: "mix",
  kind: "number",
  label: "Mix",
  sounds: "",
  value: 0.3,
  unit: "fraction",
  scale: "linear",
  limits: [0, 1],
  default: 0.3,
};

describe("SettingControl", () => {
  it("draws a short choice as a segmented choice, a long one as a menu, a toggle as a key and a number as a knob", () => {
    const onChange = vi.fn();
    render(
      <>
        {[short, long, toggle, knob].map((setting) => (
          <SettingControl key={setting.name} owner="Delay" setting={setting} onChange={onChange} />
        ))}
      </>,
    );

    fireEvent.click(screen.getByRole("radio", { name: "B" }));
    expect(onChange).toHaveBeenLastCalledWith("b");

    const menu = screen.getByRole("combobox", { name: "Delay division" });
    expect(menu).toHaveValue("c");
    fireEvent.change(menu, { target: { value: "f" } });
    expect(onChange).toHaveBeenLastCalledWith("f");

    const sync = screen.getByRole("button", { name: "Delay sync" });
    expect(sync).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(sync);
    expect(onChange).toHaveBeenLastCalledWith(true);

    expect(screen.getByRole("slider", { name: "Delay mix" })).toHaveAttribute("aria-valuetext", "30%");
  });

  it("gives each panel's segmented choice a radio group of its own", () => {
    render(
      <>
        <SettingControl owner="Delay" setting={short} onChange={vi.fn()} />
        <SettingControl owner="Delay" setting={{ ...short, value: "b" }} onChange={vi.fn()} />
      </>,
    );
    const checked = screen.getAllByRole("radio").filter((radio) => (radio as HTMLInputElement).checked);
    expect(checked.map((radio) => radio.getAttribute("value"))).toEqual(["a", "b"]);
  });
});
