import { describe, expect, it } from "vitest";
import type { Limits } from "./backend";
import {
  SETTING_STEPS,
  formatAmount,
  formatHz,
  formatLevel,
  formatOctaves,
  formatSeconds,
  formatSemitones,
  fromPosition,
  toPosition,
} from "./synthScale";

const cutoff: Limits = [20, 20_000];
const envelope: Limits = [0.001, 10];
const level: Limits = [0, 1];
const pitch: Limits = [-12, 12];

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

  it("is fine near the centre on a cubic scale, and exactly zero there", () => {
    expect(fromPosition(0, pitch, "cubic")).toBe(-12);
    expect(fromPosition(SETTING_STEPS / 2, pitch, "cubic")).toBe(0);
    expect(fromPosition(SETTING_STEPS, pitch, "cubic")).toBe(12);
    // Half way out from the centre is an eighth of the way to the end.
    expect(fromPosition(750, pitch, "cubic")).toBeCloseTo(1.5, 9);
    expect(fromPosition(250, pitch, "cubic")).toBeCloseTo(-1.5, 9);
    // One step from the centre is well under a cent.
    expect(fromPosition(501, pitch, "cubic")).toBeLessThan(0.0001);
  });
});

describe("toPosition", () => {
  it("undoes fromPosition at every step, even through an f32", () => {
    for (const [limits, scale] of [
      [cutoff, "log"],
      [envelope, "log"],
      [level, "linear"],
      [pitch, "cubic"],
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

  it("shows pitch offsets signed, with a true minus, and zero unsigned", () => {
    expect(formatSemitones(0.07)).toBe("+0.07 st");
    expect(formatSemitones(-12)).toBe("\u221212.00 st");
    expect(formatSemitones(0)).toBe("0.00 st");
    expect(formatSemitones(-0.001)).toBe("0.00 st");
    expect(formatOctaves(1)).toBe("+1 oct");
    expect(formatOctaves(-2)).toBe("\u22122 oct");
    expect(formatOctaves(0)).toBe("0 oct");
  });
});
