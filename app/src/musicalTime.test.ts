import { describe, expect, it } from "vitest";
import { barBeat, formatBarBeat } from "./musicalTime";

describe("barBeat", () => {
  it("counts bars and beats from 1", () => {
    expect(barBeat(0, 960, 4)).toEqual({ bar: 1, beat: 1 });
    expect(barBeat(959, 960, 4)).toEqual({ bar: 1, beat: 1 });
    expect(barBeat(960, 960, 4)).toEqual({ bar: 1, beat: 2 });
    expect(barBeat(3 * 960, 960, 4)).toEqual({ bar: 1, beat: 4 });
    expect(barBeat(4 * 960, 960, 4)).toEqual({ bar: 2, beat: 1 });
    expect(barBeat(15 * 4 * 960 + 2 * 960 + 1, 960, 4)).toEqual({ bar: 16, beat: 3 });
  });

  it("formats as bar.beat", () => {
    expect(formatBarBeat({ bar: 3, beat: 2 })).toBe("3.2");
  });
});
