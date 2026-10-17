import { describe, expect, it } from "vitest";
import { LIT_SECONDS, MAX_BARS, QUIETEST, barsFor, echoesOf, litAt, sweepAt } from "./echoes";

const plain = { spacing: 0.5, feedback: 0.5, drive: 0, pingPong: false };

describe("echoesOf", () => {
  it("puts an echo every `spacing` seconds, until they're too quiet to draw", () => {
    const echoes = echoesOf(plain, 10);
    expect(echoes.map((echo) => echo.seconds).slice(0, 3)).toEqual([0.5, 1, 1.5]);
    expect(echoes[0].level).toBe(1);
    expect(echoes.at(-1)?.level).toBeGreaterThanOrEqual(QUIETEST);
    expect(echoes.length).toBeLessThan(20);
  });

  it("draws a single echo with no feedback", () => {
    expect(echoesOf({ ...plain, feedback: 0 }, 10)).toHaveLength(1);
  });

  it("holds the echoes at full height past 100% feedback, as the tape's saturation does", () => {
    const echoes = echoesOf({ ...plain, feedback: 1.1, drive: 1 }, 10);
    expect(echoes).toHaveLength(20);
    for (const echo of echoes) expect(echo.level).toBeLessThanOrEqual(1);
  });

  it("bounces ping-pong's echoes left, right, left", () => {
    const sides = echoesOf({ ...plain, pingPong: true }, 2).map((echo) => echo.side);
    expect(sides).toEqual(["left", "right", "left", "right"]);
  });
});

describe("barsFor", () => {
  it("is as many whole bars as the echoes last, from one to the most", () => {
    expect(barsFor({ ...plain, feedback: 0 }, 2)).toBe(1);
    expect(barsFor({ ...plain, spacing: 1.5, feedback: 0.6 }, 2)).toBeGreaterThan(1);
    expect(barsFor({ ...plain, feedback: 1.1 }, 2)).toBe(MAX_BARS);
  });
});

describe("litAt and sweepAt", () => {
  it("lights an echo as it sounds and fades it over a moment", () => {
    expect(litAt(0.9, 1)).toBe(0);
    expect(litAt(1, 1)).toBe(1);
    expect(litAt(1 + LIT_SECONDS / 2, 1)).toBeCloseTo(0.5);
    expect(litAt(1 + LIT_SECONDS, 1)).toBe(0);
  });

  it("is as tall as the echo it last passed, easing to the next, and to nothing after the last", () => {
    const echoes = echoesOf(plain, 10);
    expect(sweepAt(0, echoes, 0.5).level).toBe(1);
    expect(sweepAt(0.5, echoes, 0.5).level).toBe(echoes[0].level);
    const between = sweepAt(0.75, echoes, 0.5).level;
    expect(between).toBeLessThan(echoes[0].level);
    expect(between).toBeGreaterThan(echoes[1].level);
    const last = echoes.at(-1)?.seconds ?? 0;
    expect(sweepAt(last + 0.5, echoes, 0.5).level).toBe(0);
  });
});
