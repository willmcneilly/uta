import { describe, expect, it } from "vitest";
import { HARDEST_FILL, SOFTEST_FILL, isBlackKey, octaveName, velocityAlpha } from "./colours";

describe("velocityAlpha", () => {
  it("fills harder notes more heavily, from the softest fill at 1 to the hardest at 127", () => {
    expect(velocityAlpha(1)).toBe(SOFTEST_FILL);
    expect(velocityAlpha(127)).toBe(HARDEST_FILL);
    expect(velocityAlpha(64)).toBeCloseTo((SOFTEST_FILL + HARDEST_FILL) / 2, 2);
    expect(velocityAlpha(100)).toBeGreaterThan(velocityAlpha(40));
  });
});

describe("keys", () => {
  it("knows the black keys", () => {
    expect([60, 61, 62, 63, 64, 65, 66].map(isBlackKey)).toEqual([
      false,
      true,
      false,
      true,
      false,
      false,
      true,
    ]);
  });

  it("names middle C as C4", () => {
    expect(octaveName(60)).toBe("C4");
    expect(octaveName(0)).toBe("C-1");
  });
});
