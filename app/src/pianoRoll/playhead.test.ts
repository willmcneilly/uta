import { describe, expect, it } from "vitest";
import { MAX_GUESS_MS, PlayheadClock } from "./playhead";

// 120 BPM at 960 ticks a quarter: 1920 ticks a second, 1.92 a millisecond.
const TICKS_PER_MS = 1.92;
const LOOP = 4 * 3840;

function clock(): PlayheadClock {
  const c = new PlayheadClock();
  c.setTiming({ bpm: 120, ticksPerQuarter: 960, loopStart: 0, loopLength: LOOP });
  return c;
}

describe("PlayheadClock", () => {
  it("stays where the engine says while stopped", () => {
    const c = clock();
    c.report(500, false, 1000);
    expect(c.at(1000)).toBe(500);
    expect(c.at(1050)).toBe(500);
  });

  it("moves on at the tempo between reports while playing", () => {
    const c = clock();
    c.report(1000, true, 0);
    expect(c.at(0)).toBe(1000);
    expect(c.at(10)).toBeCloseTo(1000 + 10 * TICKS_PER_MS, 9);
    expect(c.at(16.7)).toBeCloseTo(1000 + 16.7 * TICKS_PER_MS, 9);
  });

  it("stops guessing if reports stop", () => {
    const c = clock();
    c.report(0, true, 0);
    expect(c.at(5000)).toBeCloseTo(MAX_GUESS_MS * TICKS_PER_MS, 9);
  });

  it("goes back to the loop start at the loop end", () => {
    const c = clock();
    c.report(LOOP - 10, true, 0);
    expect(c.at(10)).toBeCloseTo(10 * TICKS_PER_MS - 10, 9);
  });

  it("eases towards reports close to its guess, rather than jumping", () => {
    const c = clock();
    c.report(1000, true, 0);
    // The engine is 8 ticks behind the guess (about 4 ms): jitter.
    const guess = c.at(16);
    c.report(guess - 8, true, 16);
    const eased = c.at(16);
    expect(eased).toBeLessThan(guess);
    expect(eased).toBeGreaterThan(guess - 8);
  });

  it("eases across the loop point without jumping a whole loop", () => {
    const c = clock();
    c.report(LOOP - 20, true, 0);
    // The guess wrapped to just past the start; the engine is a little behind it, before the end.
    c.report(LOOP - 2, true, 10);
    const at = c.at(10);
    expect(at < 50 || at > LOOP - 50).toBe(true);
  });

  it("jumps to reports far from its guess: a stop, a restart or a tempo change", () => {
    const c = clock();
    c.report(1000, true, 0);
    c.report(0, true, 16);
    expect(c.at(16)).toBe(0);
    c.report(3000, false, 32);
    expect(c.at(100)).toBe(3000);
  });

  it("follows a tempo change", () => {
    const c = clock();
    c.setTiming({ bpm: 60, ticksPerQuarter: 960, loopStart: 0, loopLength: LOOP });
    c.report(0, true, 0);
    expect(c.at(50)).toBeCloseTo(50 * 0.96, 9);
  });
});
