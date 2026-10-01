import { describe, expect, it } from "vitest";
import { DEFAULT_CLIP_SNAP, clipSnapStep, minClipLength, timelineGridStep } from "./snap";

const BAR = 3840;

describe("clip snapping", () => {
  it("snaps to the grid on screen by default", () => {
    expect(DEFAULT_CLIP_SNAP).toBe("grid");
  });

  it("draws grid lines at the finest of sixteenths, eighths, beats and bars at least 12 px apart", () => {
    // A bar is 100 px: a sixteenth is 6.25, an eighth 12.5.
    expect(timelineGridStep(100 / BAR, 960, 4)).toBe(480);
    // A bar is 50 px: a beat is 12.5.
    expect(timelineGridStep(50 / BAR, 960, 4)).toBe(960);
    // A bar is 40 px: a beat is 10.
    expect(timelineGridStep(40 / BAR, 960, 4)).toBe(BAR);
    // A bar is 200 px: a sixteenth is 12.5.
    expect(timelineGridStep(200 / BAR, 960, 4)).toBe(240);
    // A bar is 5 px: every third bar.
    expect(timelineGridStep(5 / BAR, 960, 4)).toBe(3 * BAR);
  });

  it("follows the zoom on Grid, and not on bars, beats or off", () => {
    expect(clipSnapStep("grid", 960, 4, 100 / BAR)).toBe(480);
    expect(clipSnapStep("grid", 960, 4, 40 / BAR)).toBe(BAR);
    expect(clipSnapStep("bar", 960, 4, 100 / BAR)).toBe(BAR);
    expect(clipSnapStep("beat", 960, 4, 5 / BAR)).toBe(960);
    expect(clipSnapStep("off", 960, 4, 100 / BAR)).toBe(1);
  });

  it("keeps clips at least a step long, or a sixteenth unsnapped", () => {
    expect(minClipLength(BAR, 960)).toBe(BAR);
    expect(minClipLength(960, 960)).toBe(960);
    expect(minClipLength(1, 960)).toBe(240);
  });
});
