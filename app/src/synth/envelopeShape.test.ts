import { describe, expect, it } from "vitest";
import { SUSTAIN_SHARE, curveAt, envelopeShape } from "./envelopeShape";

const levelOf = (y: number, height: number) => 1 - y / height;

describe("the envelope's shape", () => {
  // 0.1 + 0.2 + 0.5 seconds over 800 of 1000 pixels: 1,000 pixels a second.
  const times = { attackSeconds: 0.1, decaySeconds: 0.2, sustain: 0.5, releaseSeconds: 0.5 };
  const { stages, points } = envelopeShape(times, 1000, 100);

  it("gives each timed stage a length in proportion to its time, the sustain a fixed share", () => {
    expect(SUSTAIN_SHARE).toBe(0.2);
    expect(stages.map(({ name, start, end }) => [name, start, end])).toEqual([
      ["attack", 0, 100],
      ["decay", 100, 300],
      ["sustain", 300, 500],
      ["release", 500, 1000],
    ]);
  });

  it("gives each stage the right levels", () => {
    expect(stages.map(({ from, to }) => [from, to])).toEqual([
      [0, 1],
      [1, 0.5],
      [0.5, 0.5],
      [0.5, 0],
    ]);
  });

  it("draws the attack straight up from silence to full level", () => {
    expect(points[0]).toEqual({ x: 0, y: 100 });
    expect(points[1]).toEqual({ x: 100, y: 0 });
  });

  it("lands each stage on its end level at its end", () => {
    const at = (x: number) => points.find((point) => Math.abs(point.x - x) < 1e-9)!;
    expect(levelOf(at(300).y, 100)).toBeCloseTo(0.5, 9);
    expect(levelOf(at(500).y, 100)).toBeCloseTo(0.5, 9);
    expect(levelOf(at(1000).y, 100)).toBeCloseTo(0, 9);
    expect(points.at(-1)!.x).toBe(1000);
  });

  it("falls on the engine's exponential curve, fast then slow", () => {
    expect(curveAt(0)).toBeCloseTo(1, 12);
    expect(curveAt(1)).toBeCloseTo(0, 12);
    // Halfway through the decay, it's fallen most of the way.
    const half = points.find((point) => Math.abs(point.x - 200) < 1e-9)!;
    expect(levelOf(half.y, 100)).toBeCloseTo(0.5 + 0.5 * curveAt(0.5), 9);
    // The engine's level halfway through a decay to 0.5, from its overshoot
    // of 0.001: 0.5 + 0.5 × (1.001 × (0.001 / 1.001)^0.5 − 0.001).
    expect(levelOf(half.y, 100)).toBeCloseTo(0.5153, 4);
    const decay = points.filter((point) => point.x > 100 && point.x <= 300);
    expect(decay.every((point, i) => i === 0 || point.y > decay[i - 1].y)).toBe(true);
  });

  it("draws no decay at full sustain, and a release to silence from no sustain", () => {
    const full = envelopeShape({ ...times, sustain: 1 }, 1000, 100);
    expect(full.points.filter((p) => p.x > 100 && p.x <= 500).every((p) => p.y === 0)).toBe(true);
    const none = envelopeShape({ ...times, sustain: 0 }, 1000, 100);
    expect(none.points.filter((p) => p.x >= 300).every((p) => Math.abs(p.y - 100) < 1e-9)).toBe(
      true,
    );
  });

  it("keeps a short attack visible next to a long release", () => {
    const shape = envelopeShape(
      { attackSeconds: 0.001, decaySeconds: 0.2, sustain: 0.7, releaseSeconds: 10 },
      288,
      80,
    );
    expect(shape.stages[0].end).toBeGreaterThan(0);
    expect(shape.stages[3].end - shape.stages[3].start).toBeGreaterThan(200);
  });
});
