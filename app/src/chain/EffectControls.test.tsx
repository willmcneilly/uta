import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EffectView } from "../backend";
import { EffectControls } from "./EffectControls";

// An effect's controls are drawn from Rust's description of its settings
// alone, so a choice or a toggle a later effect brings needs nothing here.

const mix: EffectView["mix"] = {
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

const option = (value: string) => ({ value, label: value.toUpperCase() });

const effect: EffectView = {
  id: "e-1",
  name: "Delay",
  enabled: true,
  kind: "delay",
  mix,
  settings: [
    { name: "mode", kind: "choice", label: "Mode", sounds: "", value: "a", options: ["a", "b"].map(option), default: "a" },
    {
      name: "division",
      kind: "choice",
      label: "Division",
      sounds: "",
      value: "c",
      options: ["a", "b", "c", "d", "e", "f"].map(option),
      default: "a",
    },
    { name: "sync", kind: "toggle", label: "Sync", sounds: "", value: false, default: true },
  ],
};

describe("EffectControls", () => {
  it("draws a short choice as a segmented choice, a long one as a menu, and a toggle as a key", () => {
    const onSetting = vi.fn();
    render(<EffectControls effect={effect} onSetting={onSetting} onMix={vi.fn()} />);

    fireEvent.click(screen.getByRole("radio", { name: "B" }));
    expect(onSetting).toHaveBeenLastCalledWith({ name: "mode", value: "b" });

    const menu = screen.getByRole("combobox", { name: "Delay division" });
    expect(menu).toHaveValue("c");
    fireEvent.change(menu, { target: { value: "f" } });
    expect(onSetting).toHaveBeenLastCalledWith({ name: "division", value: "f" });

    const sync = screen.getByRole("button", { name: "Delay sync" });
    expect(sync).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(sync);
    expect(onSetting).toHaveBeenLastCalledWith({ name: "sync", value: true });

    expect(screen.getByRole("slider", { name: "Delay mix" })).toBeInTheDocument();
  });
});
