import { describe, expect, it } from "vitest";
import { lfoAt, lfoPoints, osc2Ratio, oscillatorAt, oscillatorPoints } from "./waves";

const IN_TUNE = { osc1Octave: 0, osc2Octave: 0, osc2PitchSemitones: 0, sync: false };

describe("the oscillators' waves", () => {
  // The engine's `naive` shapes (oscillator.rs): each starts its cycle
  // where the engine's does.
  it("are the engine's shapes", () => {
    expect(oscillatorAt("sine", 0.25, 0.5)).toBeCloseTo(1, 6);
    expect(oscillatorAt("triangle", 0, 0.5)).toBe(-1);
    expect(oscillatorAt("triangle", 0.5, 0.5)).toBe(1);
    expect(oscillatorAt("saw", 0, 0.5)).toBe(-1);
    expect(oscillatorAt("saw", 0.75, 0.5)).toBe(0.5);
    expect(oscillatorAt("pulse", 0.2, 0.25)).toBe(1);
    expect(oscillatorAt("pulse", 0.3, 0.25)).toBe(-1);
  });

  it("tune oscillator 2 against oscillator 1 by octave and pitch", () => {
    expect(osc2Ratio(IN_TUNE)).toBe(1);
    expect(osc2Ratio({ ...IN_TUNE, osc1Octave: -1, osc2Octave: 1 })).toBe(4);
    expect(osc2Ratio({ ...IN_TUNE, osc2PitchSemitones: 12 })).toBeCloseTo(2, 12);
    expect(osc2Ratio({ ...IN_TUNE, osc2PitchSemitones: 7 })).toBeCloseTo(1.498, 3);
  });

  it("draw two of oscillator 1's cycles, across the drawing, full level at the top", () => {
    const points = oscillatorPoints(1, "saw", 0.5, IN_TUNE, 120, 30);
    expect(points[0]).toEqual({ x: 0, y: 30 });
    expect(points.at(-1)!.x).toBe(120);
    // A saw's two ramps: half way through the first, half way up.
    expect(points.find((point) => point.x === 30)!.y).toBeCloseTo(15, 6);
    expect(points.every((point) => point.y >= 0 && point.y <= 30)).toBe(true);
  });

  it("draw a pulse's width, and leave oscillator 2's at a square", () => {
    const high = (which: 1 | 2) =>
      oscillatorPoints(which, "pulse", 0.2, IN_TUNE, 100, 10).filter(
        (point) => point.x < 50 && point.y === 0,
      ).length;
    expect(high(1) / high(2)).toBeCloseTo(0.2 / 0.5, 1);
  });

  it("restart oscillator 2 with oscillator 1 when synced, so its pitch tears the wave", () => {
    const at = (sync: boolean) =>
      oscillatorPoints(2, "saw", 0.5, { ...IN_TUNE, osc2PitchSemitones: 7, sync }, 90, 30);
    // At oscillator 1's second cycle, a synced saw starts again at the bottom.
    const restart = (sync: boolean) => at(sync).find((point) => point.x === 45)!.y;
    expect(restart(true)).toBe(30);
    expect(restart(false)).not.toBe(30);
  });
});

describe("the LFO's wave", () => {
  // The engine's `shape_value` (lfo.rs): on the bar, the sine and triangle
  // start at zero, rising; the saw at the bottom; the square high.
  it("is the engine's shapes", () => {
    expect(lfoAt("sine", 0)).toBe(0);
    expect(lfoAt("sine", 0.25)).toBeCloseTo(1, 6);
    expect(lfoAt("triangle", 0)).toBe(0);
    expect(lfoAt("triangle", 0.25)).toBe(1);
    expect(lfoAt("triangle", 0.75)).toBe(-1);
    expect(lfoAt("saw", 0)).toBe(-1);
    expect(lfoAt("square", 0)).toBe(1);
    expect(lfoAt("square", 0.5)).toBe(-1);
  });

  it("holds sample-and-hold's level for a cycle, then jumps", () => {
    expect(lfoAt("sample_and_hold", 2.1)).toBe(lfoAt("sample_and_hold", 2.9));
    expect(lfoAt("sample_and_hold", 2.9)).not.toBe(lfoAt("sample_and_hold", 3.1));
    expect(Math.abs(lfoAt("sample_and_hold", 5.5))).toBeLessThanOrEqual(1);
  });

  it("draws as many cycles across the bar as fit in it", () => {
    // Four cycles of a square: it falls four times.
    const points = lfoPoints("square", 4, 120, 40);
    const falls = points.filter((point, i) => i > 0 && point.y > points[i - 1].y).length;
    expect(falls).toBe(4);
    expect(points.at(-1)!.x).toBe(120);
  });
});
