import { describe, expect, it } from "vitest";
import { DEFAULT_CLIP_SNAP, clipSnapStep, minClipLength } from "./snap";

describe("clip snapping", () => {
  it("snaps to bars by default, or beats, or single ticks when off", () => {
    expect(DEFAULT_CLIP_SNAP).toBe("bar");
    expect(clipSnapStep("bar", 960, 4)).toBe(3840);
    expect(clipSnapStep("beat", 960, 4)).toBe(960);
    expect(clipSnapStep("off", 960, 4)).toBe(1);
  });

  it("keeps clips at least a step long, or a sixteenth unsnapped", () => {
    expect(minClipLength(3840, 960)).toBe(3840);
    expect(minClipLength(960, 960)).toBe(960);
    expect(minClipLength(1, 960)).toBe(240);
  });
});
