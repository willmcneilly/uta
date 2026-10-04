import { describe, expect, it } from "vitest";
import {
  type LoadResult,
  type Report,
  allMet,
  formatReport,
  meets,
  summarise,
  targetsFor,
} from "./report";

const load = (song: LoadResult["song"], overrides: Partial<LoadResult> = {}): LoadResult => ({
  song,
  tracks: 12,
  clips: 768,
  notes: 49_152,
  buildMs: 812.4,
  sliderSteps: [10, 12, 11],
  noteDeletes: [9, 13],
  dragSteps: [14, 15],
  dragFrames: [16, 17, 18],
  dragMs: 530,
  changes: 36,
  events: 36,
  ...overrides,
});

const report = (...loads: LoadResult[]): Report => ({
  startedAt: new Date("2026-10-03T14:05:00Z"),
  loads,
});

describe("summarise", () => {
  it("gives the p50, p99 and max by nearest rank, whatever the order", () => {
    const values = Array.from({ length: 100 }, (_, i) => 100 - i);
    expect(summarise(values)).toEqual({ n: 100, p50: 50, p99: 99, max: 100 });
    expect(summarise([80, 3, 5])).toEqual({ n: 3, p50: 5, p99: 80, max: 80 });
  });

  it("has nothing to say about nothing", () => {
    const summary = summarise([]);
    expect(summary.n).toBe(0);
    expect(summary.p99).toBeNaN();
    expect(summary.max).toBeNaN();
  });
});

describe("targets", () => {
  it("asks for each step on the next frame and 20 ms frames at heavy and wide", () => {
    for (const song of ["heavy", "wide"] as const) {
      expect(targetsFor(song)).toEqual([
        { measure: "sliderSteps", statistic: "p99", limit: 20, inclusive: true },
        { measure: "noteDeletes", statistic: "p99", limit: 20, inclusive: true },
        { measure: "dragFrames", statistic: "p99", limit: 20, inclusive: false },
      ]);
    }
  });

  it("asks only for no frame over 50 ms at check 7", () => {
    expect(targetsFor("check-7")).toEqual([
      { measure: "dragFrames", statistic: "max", limit: 50, inclusive: true },
    ]);
  });

  it("judges under as under, and no more than as allowing the limit", () => {
    const [, , frames20] = targetsFor("heavy");
    expect(meets(summarise([19.9]), frames20)).toBe(true);
    expect(meets(summarise([20]), frames20)).toBe(false);
    const [slider] = targetsFor("heavy");
    // A step drawn on the next frame measures 17 ms, even if it took no time.
    expect(meets(summarise([17, 20]), slider)).toBe(true);
    expect(meets(summarise([20.1]), slider)).toBe(false);
    const [frames] = targetsFor("check-7");
    expect(meets(summarise([17, 50]), frames)).toBe(true);
    expect(meets(summarise([17, 50.1]), frames)).toBe(false);
    // Nothing measured is never a pass.
    expect(meets(summarise([]), frames)).toBe(false);
  });

  it("says whether every load met every target", () => {
    expect(allMet(report(load("heavy"), load("check-7", { dragFrames: [17, 48] })))).toBe(true);
    expect(allMet(report(load("heavy"), load("check-7", { dragFrames: [17, 1200] })))).toBe(false);
    expect(allMet(report(load("wide", { noteDeletes: [80] })))).toBe(false);
  });
});

describe("formatReport", () => {
  it("lays out each load's figures with its targets beside them", () => {
    const text = formatReport(
      report(
        load("heavy", { sliderSteps: [80, 82, 95] }),
        load("check-7", {
          tracks: 7,
          clips: 196,
          notes: 588_000,
          dragFrames: [17, 8800],
          dragMs: 36_000,
          events: 52,
          changes: 52,
        }),
      ),
    );
    expect(text).toContain("Uta benchmark, 2026-10-03 14:05 UTC");
    expect(text).toContain("heavy: 12 tracks, 768 clips, 49,152 notes (built and drawn in 812 ms)");
    expect(text).toContain(
      "  slider step     3     82.0     95.0     95.0   p99 ≤ 20  ✗ missed",
    );
    expect(text).toContain("  note delete     2      9.0     13.0     13.0   p99 ≤ 20  ✓ met");
    expect(text).toContain("  drag step       2     14.0     15.0     15.0\n");
    expect(text).toContain("  drag frames     3     17.0     18.0     18.0   p99 < 20  ✓ met");
    expect(text).toContain("check 7: 7 tracks, 196 clips, 588,000 notes");
    expect(text).toContain("  drag frames     2     17.0   8800.0   8800.0   max ≤ 50  ✗ missed");
    expect(text).toContain("  the drag took 36,000 ms from its first step until all drawn");
    expect(text).toContain("  project-changed events: 52 for 52 changes");
    expect(text.endsWith("Some targets missed.")).toBe(true);
  });

  it("says so when everything met its target", () => {
    expect(formatReport(report(load("wide"))).endsWith("Every target met.")).toBe(true);
  });
});
