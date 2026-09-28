import { describe, expect, it } from "vitest";
import { DEFAULT_SNAP, SNAPS, snapDown, snapNearest, snapStep } from "./snap";

describe("the snap grid", () => {
  it("offers 1/4 to 1/32 or off, at 1/16 by default", () => {
    expect(SNAPS).toEqual(["1/4", "1/8", "1/16", "1/32", "off"]);
    expect(DEFAULT_SNAP).toBe("1/16");
  });

  it("steps by the note length, in ticks at 960 a quarter", () => {
    expect(snapStep("1/4", 960)).toBe(960);
    expect(snapStep("1/8", 960)).toBe(480);
    expect(snapStep("1/16", 960)).toBe(240);
    expect(snapStep("1/32", 960)).toBe(120);
  });

  it("steps by a whole tick with snapping off", () => {
    expect(snapStep("off", 960)).toBe(1);
    expect(snapNearest(1234.4, 1)).toBe(1234);
    expect(snapDown(1234.9, 1)).toBe(1234);
  });

  it("snaps to the nearest line, or the one at or before", () => {
    expect(snapNearest(119, 240)).toBe(0);
    expect(snapNearest(120, 240)).toBe(240);
    expect(snapNearest(361, 240)).toBe(480);
    expect(snapDown(479, 240)).toBe(240);
    expect(snapDown(480, 240)).toBe(480);
  });
});
