import { describe, expect, it } from "vitest";
import { METER_FLOOR_DB, MeterLevel, meterFraction, nextShownDb, toDb } from "./meterLevel";

describe("meter level", () => {
  it("keeps the loudest peak between two drawn frames", () => {
    const level = new MeterLevel();
    level.push(0.2);
    level.push(0.5);
    level.push(0.1);
    expect(level.take()).toBe(0.5);
    expect(level.take()).toBe(0);
  });

  it("converts levels to dB", () => {
    expect(toDb(1)).toBe(0);
    expect(toDb(0.5)).toBeCloseTo(-6.02, 2);
    expect(toDb(0)).toBe(-Infinity);
  });

  it("jumps up to a new peak", () => {
    expect(nextShownDb(-40, 0.5, 1 / 60)).toBeCloseTo(-6.02, 2);
  });

  it("falls away slowly after a peak", () => {
    expect(nextShownDb(-6, 0, 0.5)).toBe(-18);
  });

  it("stops at the floor", () => {
    expect(nextShownDb(-59, 0, 10)).toBe(METER_FLOOR_DB);
  });

  it("maps dB onto the meter", () => {
    expect(meterFraction(METER_FLOOR_DB)).toBe(0);
    expect(meterFraction(-30)).toBe(0.5);
    expect(meterFraction(0)).toBe(1);
    expect(meterFraction(6)).toBe(1);
    expect(meterFraction(-Infinity)).toBe(0);
  });
});
