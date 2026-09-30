import { describe, expect, it } from "vitest";
import {
  METER_FLOOR_DB,
  MeterLevel,
  SILENT,
  TrackLevels,
  clipLit,
  meterFraction,
  nextShown,
  toDb,
} from "./meterLevel";

/** Steps the meter through `seconds` of silence, a frame at a time, from a peak at `db`. */
function afterSilence(db: number, seconds: number): number {
  const frame = 1 / 60;
  let shown = { db, heldSeconds: 0 };
  for (let t = frame; t <= seconds + 1e-9; t += frame) shown = nextShown(shown, 0, frame);
  return shown.db;
}

describe("meter level", () => {
  it("keeps the loudest peak between two drawn frames", () => {
    const level = new MeterLevel();
    level.push(0.2);
    level.push(0.5);
    level.push(0.1);
    expect(level.take()).toBe(0.5);
    expect(level.take()).toBe(0);
  });

  it("keeps each track's peaks apart", () => {
    const levels = new TrackLevels();
    levels.push({ a: 0.5, b: 0.1 });
    levels.push({ a: 0.2 });
    expect(levels.level("a").take()).toBe(0.5);
    expect(levels.level("b").take()).toBe(0.1);
    expect(levels.level("c").take()).toBe(0);
    expect(levels.level("a")).toBe(levels.level("a"));
  });

  it("converts levels to dB", () => {
    expect(toDb(1)).toBe(0);
    expect(toDb(0.5)).toBeCloseTo(-6.02, 2);
    expect(toDb(0)).toBe(-Infinity);
  });

  it("jumps up to a new peak", () => {
    const shown = nextShown({ db: -40, heldSeconds: 0.5 }, 0.5, 1 / 60);
    expect(shown.db).toBeCloseTo(-6.02, 2);
    expect(shown.heldSeconds).toBe(0);
  });

  it("holds a peak for about a second", () => {
    expect(afterSilence(-6, 0.5)).toBe(-6);
    expect(afterSilence(-6, 1)).toBeCloseTo(-6, 5);
  });

  it("then falls 20 dB in 1.7 seconds", () => {
    expect(afterSilence(-6, 1 + 0.85)).toBeCloseTo(-16, 3);
    expect(afterSilence(-6, 1 + 1.7)).toBeCloseTo(-26, 3);
  });

  it("counts the hold across frames of any length", () => {
    // One long frame covers the hold and some of the fall.
    expect(nextShown({ db: -6, heldSeconds: 0 }, 0, 1 + 0.85).db).toBeCloseTo(-16, 5);
    // A frame that straddles the end of the hold only falls for the part after it.
    expect(nextShown({ db: -6, heldSeconds: 0.9 }, 0, 0.2).db).toBeCloseTo(-6 - 20 / 17, 5);
  });

  it("takes a quieter peak once the fall reaches it", () => {
    // Held -20 dB falls to about -31.8 dB in a second, below the new -26 dB peak.
    const shown = nextShown({ db: -20, heldSeconds: 3 }, 0.05, 1);
    expect(shown.db).toBeCloseTo(toDb(0.05), 5);
    expect(shown.heldSeconds).toBe(0);
  });

  it("stops at the floor", () => {
    expect(afterSilence(-59, 10)).toBe(METER_FLOOR_DB);
    expect(nextShown(SILENT, 0, 1 / 60).db).toBe(METER_FLOOR_DB);
  });

  it("maps dB onto the meter", () => {
    expect(meterFraction(METER_FLOOR_DB)).toBe(0);
    expect(meterFraction(-30)).toBe(0.5);
    expect(meterFraction(0)).toBe(1);
    expect(meterFraction(6)).toBe(1);
    expect(meterFraction(-Infinity)).toBe(0);
  });

  it("lights the clip light until it's clicked", () => {
    expect(clipLit(0, 0)).toBe(false);
    expect(clipLit(3, 0)).toBe(true);
    expect(clipLit(3, 3)).toBe(false);
    expect(clipLit(5, 3)).toBe(true);
  });
});
