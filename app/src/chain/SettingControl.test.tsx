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

  it("reads a value by its unit and range: a multiple with a ×, a slow rate to two places, Off at an off end", () => {
    const number = (name: string, unit: "times" | "hz", limits: [number, number], value: number) =>
      ({ ...knob, name, label: name, unit, scale: "log", limits, value, default: value }) as EffectSettingView;
    render(
      <>
        <SettingControl owner="Reverb" setting={number("Mult", "times", [0.25, 4], 0.5)} onChange={vi.fn()} />
        <SettingControl owner="Reverb" setting={number("Rate", "hz", [0.05, 5], 0.05)} onChange={vi.fn()} />
        <SettingControl owner="Reverb" setting={number("Cut", "hz", [1000, 20000], 1000)} onChange={vi.fn()} />
        <SettingControl
          owner="Reverb"
          setting={{ ...number("Low cut", "hz", [20, 1000], 20), offAtLowest: true }}
          onChange={vi.fn()}
        />
        <SettingControl
          owner="Reverb"
          setting={{ ...number("High", "hz", [20, 1000], 200), offAtLowest: true }}
          onChange={vi.fn()}
        />
      </>,
    );
    const valueOf = (name: string) => screen.getByRole("slider", { name: `Reverb ${name}` }).getAttribute("aria-valuetext");
    expect(valueOf("mult")).toBe("0.50×");
    expect(valueOf("rate")).toBe("0.05 Hz");
    expect(valueOf("cut")).toBe("1.00 kHz");
    expect(valueOf("low cut")).toBe("Off");
    expect(valueOf("high")).toBe("200 Hz");
  });

  it("is labelled with Rust's short name where the panel asks, keeping the full one for its accessible name", () => {
    render(
      <SettingControl owner="Reverb" setting={{ ...knob, label: "Early size", short: "Size" }} short onChange={vi.fn()} />,
    );
    const size = screen.getByRole("slider", { name: "Reverb early size" });
    expect(size.closest(".knob")).toHaveTextContent("Size");
    expect(size.closest(".knob")).not.toHaveTextContent("Early");
  });
});
