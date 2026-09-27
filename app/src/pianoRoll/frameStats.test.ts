import { describe, expect, it } from "vitest";
import { FRAME_WINDOW, FrameStats, percentile } from "./frameStats";

describe("percentile", () => {
  it("takes the nearest rank", () => {
    const values = Array.from({ length: 100 }, (_, i) => i + 1);
    expect(percentile(values, 0.5)).toBe(50);
    expect(percentile(values, 0.99)).toBe(99);
    expect(percentile([7], 0.99)).toBe(7);
    expect(percentile([], 0.5)).toBeNaN();
  });
});

describe("FrameStats", () => {
  it("has nothing to say before two frames", () => {
    const stats = new FrameStats();
    expect(stats.summary()).toBeNull();
    stats.record(0, 1);
    expect(stats.summary()).toBeNull();
  });

  it("reports the time between frames and the drawing time", () => {
    const stats = new FrameStats();
    let now = 0;
    stats.record(now, 0);
    for (let i = 0; i < 99; i++) {
      now += 16;
      stats.record(now, 1);
    }
    // One slow frame.
    now += 50;
    stats.record(now, 20);
    const summary = stats.summary()!;
    expect(summary.intervalP50).toBe(16);
    expect(summary.intervalP99).toBe(16);
    expect(summary.drawP50).toBe(1);
    stats.record(now + 50, 20);
    expect(stats.summary()!.intervalP99).toBe(50);
  });

  it("only covers the recent frames", () => {
    const stats = new FrameStats();
    let now = 0;
    stats.record(now, 0);
    for (let i = 0; i < FRAME_WINDOW; i++) stats.record((now += 100), 0);
    for (let i = 0; i < FRAME_WINDOW; i++) stats.record((now += 10), 0);
    expect(stats.summary()!.intervalP99).toBe(10);
  });

  it("doesn't count a pause as a slow frame", () => {
    const stats = new FrameStats();
    stats.record(0, 0);
    stats.record(16, 0);
    stats.pause();
    stats.record(10_000, 0);
    stats.record(10_016, 0);
    expect(stats.summary()!.intervalP99).toBe(16);
  });
});
