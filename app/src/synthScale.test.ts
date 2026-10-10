import { describe, expect, it } from "vitest";
import type { Limits } from "./backend";
import {
  SETTING_STEPS,
  formatAmount,
  formatHz,
  formatLevel,
  formatSeconds,
  fromPosition,
  toPosition,
} from "./synthScale";

const cutoff: Limits = [20, 20_000];
const envelope: Limits = [0.001, 10];
const level: Limits = [0, 1];

describe("fromPosition", () => {
  it("gives exactly the limits at the ends", () => {
    expect(fromPosition(0, cutoff, "log")).toBe(20);
    expect(fromPosition(SETTING_STEPS, cutoff, "log")).toBe(20_000);
    expect(fromPosition(0, envelope, "log")).toBe(0.001);
    expect(fromPosition(SETTING_STEPS, envelope, "log")).toBe(10);
    expect(fromPosition(SETTING_STEPS, level, "linear")).toBe(1);
  });

  it("moves each step by the same ratio on a log scale", () => {
    // 20 Hz to 20 kHz is three decades, so a third of the way is 200 Hz.
    expect(fromPosition(SETTING_STEPS / 3, cutoff, "log")).toBeCloseTo(200, 6);
    expect(fromPosition(SETTING_STEPS / 2, cutoff, "log")).toBeCloseTo(632.46, 1);
    // 1 ms to 10 s is four decades, so halfway is 100 ms.
    expect(fromPosition(SETTING_STEPS / 2, envelope, "log")).toBeCloseTo(0.1, 9);
  });

  it("moves each step by the same amount on a linear scale", () => {
    expect(fromPosition(250, level, "linear")).toBe(0.25);
  });
});

describe("toPosition", () => {
  it("undoes fromPosition at every step, even through an f32", () => {
    for (const [limits, scale] of [
      [cutoff, "log"],
      [envelope, "log"],
      [level, "linear"],
    ] as const) {
      for (let position = 0; position <= SETTING_STEPS; position += 1) {
        const f32 = Math.fround(fromPosition(position, limits, scale));
        expect(toPosition(f32, limits, scale)).toBe(position);
      }
    }
  });

  it("clamps values outside the limits to the slider", () => {
    expect(toPosition(5, cutoff, "log")).toBe(0);
    expect(toPosition(40_000, cutoff, "log")).toBe(SETTING_STEPS);
    expect(toPosition(0, envelope, "log")).toBe(0);
    expect(toPosition(Number.NaN, level, "linear")).toBe(0);
  });
});

describe("formatting", () => {
  it("shows frequencies in Hz or kHz", () => {
    expect(formatHz(20)).toBe("20 Hz");
    expect(formatHz(632.46)).toBe("632 Hz");
    expect(formatHz(1234)).toBe("1.23 kHz");
    expect(formatHz(20_000)).toBe("20.0 kHz");
  });

  it("shows times in ms or s", () => {
    expect(formatSeconds(0.001)).toBe("1.0 ms");
    expect(formatSeconds(0.005)).toBe("5.0 ms");
    expect(formatSeconds(0.2)).toBe("200 ms");
    expect(formatSeconds(1.5)).toBe("1.50 s");
    expect(formatSeconds(10)).toBe("10.00 s");
  });

  it("shows resonance as an amount and sustain as a percentage", () => {
    expect(formatAmount(0.35)).toBe("0.35");
    expect(formatLevel(0.7)).toBe("70%");
  });
});
