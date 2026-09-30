import { describe, expect, it } from "vitest";
import { dropIndex, formatPan } from "./trackOrder";

describe("track order", () => {
  // Three 60 px headers, one under the other.
  const middles = [30, 90, 150];

  it("leaves a track where it is when it's dropped over itself", () => {
    expect(dropIndex(middles, 1, 90)).toBe(1);
    expect(dropIndex(middles, 1, 70)).toBe(1);
    expect(dropIndex(middles, 1, 110)).toBe(1);
  });

  it("moves a track up past the headers it's dropped above the middle of", () => {
    expect(dropIndex(middles, 2, 100)).toBe(2);
    expect(dropIndex(middles, 2, 80)).toBe(1);
    expect(dropIndex(middles, 2, 10)).toBe(0);
    expect(dropIndex(middles, 2, -50)).toBe(0);
  });

  it("moves a track down past the headers it's dropped below the middle of", () => {
    expect(dropIndex(middles, 0, 100)).toBe(1);
    expect(dropIndex(middles, 0, 170)).toBe(2);
    expect(dropIndex(middles, 0, 900)).toBe(2);
  });

  it("formats pan positions", () => {
    expect(formatPan(0)).toBe("C");
    expect(formatPan(0.001)).toBe("C");
    expect(formatPan(-0.5)).toBe("L 50");
    expect(formatPan(1)).toBe("R 100");
  });
});
