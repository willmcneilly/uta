import { describe, expect, it } from "vitest";
// Measured from the engine's real filter by a Rust test (see filterCurve.ts).
import measuredJson from "./filter-response.json?raw";
import {
  GAINS,
  type Point,
  filterCurve,
  filterGainDb,
  gainOfY,
  xOfFrequency,
  yOfGain,
} from "./filterCurve";

interface Measured {
  sampleRate: number;
  cutoffHz: number;
  resonance: number;
  frequencyHz: number;
  gainDb: number;
}

const measured = (JSON.parse(measuredJson) as { points: Measured[] }).points;

/** The drawn curve's y at `x`, between the points either side. */
function yAt(curve: Point[], x: number): number {
  const after = curve.findIndex((point) => point.x >= x);
  if (curve[after].x === x || after === 0) return curve[after].y;
  const [a, b] = [curve[after - 1], curve[after]];
  return a.y + ((b.y - a.y) * (x - a.x)) / (b.x - a.x);
}

describe("the filter curve", () => {
  it("has the engine's measurements to check against", () => {
    expect(measured.length).toBeGreaterThan(200);
    expect(new Set(measured.map((m) => m.sampleRate))).toEqual(new Set([44_100, 48_000]));
  });

  it("matches the engine's filter within 0.5 dB at every measured point", () => {
    for (const m of measured) {
      const db = filterGainDb(m.frequencyHz, m.cutoffHz, m.resonance, m.sampleRate);
      expect(Math.abs(db - m.gainDb), JSON.stringify(m)).toBeLessThan(0.5);
    }
  });

  it("draws the engine's filter within 0.5 dB, and anything quieter on the bottom edge", () => {
    const [width, height] = [288, 96];
    const [bottom] = GAINS;
    for (const m of measured) {
      const curve = filterCurve(m.cutoffHz, m.resonance, m.sampleRate, width, height);
      const y = yAt(curve, xOfFrequency(m.frequencyHz, width));
      if (m.gainDb > bottom + 0.5) {
        expect(Math.abs(gainOfY(y, height) - m.gainDb), JSON.stringify(m)).toBeLessThan(0.5);
      } else {
        expect(y, JSON.stringify(m)).toBeCloseTo(yOfGain(bottom, height), 0);
      }
    }
  });

  it("is a Butterworth curve at no resonance: flat, then 3 dB down at the cutoff", () => {
    expect(filterGainDb(20, 1_000, 0, 48_000)).toBeCloseTo(0, 2);
    expect(filterGainDb(1_000, 1_000, 0, 48_000)).toBeCloseTo(-3.01, 2);
  });

  it("peaks 20 dB at the cutoff at full resonance, and the drawing reaches the peak", () => {
    expect(filterGainDb(1_000, 1_000, 1, 48_000)).toBeCloseTo(20, 1);
    const curve = filterCurve(1_000, 1, 48_000, 288, 96);
    const top = Math.min(...curve.map((point) => point.y));
    expect(gainOfY(top, 96)).toBeCloseTo(20, 1);
  });

  it("passes nothing at or above half the sample rate", () => {
    expect(filterGainDb(22_050, 20_000, 0, 44_100)).toBe(-Infinity);
  });

  it("spans the drawing from edge to edge, inside it", () => {
    const curve = filterCurve(500, 0.5, 48_000, 288, 96);
    expect(curve[0].x).toBe(0);
    expect(curve.at(-1)!.x).toBe(288);
    expect(curve.every((point, i) => i === 0 || point.x > curve[i - 1].x)).toBe(true);
    expect(curve.every((point) => point.y >= 0 && point.y <= 96)).toBe(true);
  });
});
