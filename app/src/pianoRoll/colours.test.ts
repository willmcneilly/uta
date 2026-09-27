import { describe, expect, it } from "vitest";
import { isBlackKey, octaveName, velocityColours } from "./colours";

describe("velocityColours", () => {
  it("runs from the soft colour at velocity 1 to the hard one at 127", () => {
    const colours = velocityColours("#000000", "#7e7e7e");
    expect(colours).toHaveLength(128);
    expect(colours[1]).toBe("rgb(0, 0, 0)");
    expect(colours[64]).toBe("rgb(63, 63, 63)");
    expect(colours[127]).toBe("rgb(126, 126, 126)");
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
