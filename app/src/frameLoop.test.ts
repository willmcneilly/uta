import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FrameLoop } from "./frameLoop";
import { FrameStats } from "./pianoRoll/frameStats";

let pending: Map<number, FrameRequestCallback>;
let nextRequest: number;

/** Runs the frame that's waiting, as the browser would at `now`. */
function runFrame(now: number) {
  const callbacks = [...pending.values()];
  pending.clear();
  for (const callback of callbacks) callback(now);
}

beforeEach(() => {
  pending = new Map();
  nextRequest = 0;
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    nextRequest += 1;
    pending.set(nextRequest, callback);
    return nextRequest;
  });
  vi.stubGlobal("cancelAnimationFrame", (request: number) => pending.delete(request));
});

afterEach(() => vi.unstubAllGlobals());

describe("FrameLoop", () => {
  it("draws every view once a frame, and records one frame for them all", () => {
    const stats = new FrameStats();
    const record = vi.spyOn(stats, "record");
    const loop = new FrameLoop(stats);
    const pianoRoll = vi.fn();
    const timeline = vi.fn();
    loop.add(pianoRoll);
    loop.add(timeline);
    runFrame(16);
    runFrame(32);
    expect(pianoRoll.mock.calls).toEqual([[16], [32]]);
    expect(timeline.mock.calls).toEqual([[16], [32]]);
    expect(record.mock.calls.map(([now]) => now)).toEqual([16, 32]);
  });

  it("stops asking for frames when nothing is left to draw", () => {
    const loop = new FrameLoop(new FrameStats());
    const draw = vi.fn();
    const remove = loop.add(draw);
    runFrame(16);
    remove();
    expect(pending.size).toBe(0);
    runFrame(32);
    expect(draw).toHaveBeenCalledTimes(1);
  });

  it("doesn't count the time with nothing drawing as a slow frame", () => {
    const stats = new FrameStats();
    const loop = new FrameLoop(stats);
    const remove = loop.add(() => {});
    runFrame(0);
    runFrame(16);
    remove();
    loop.add(() => {});
    runFrame(5_000);
    runFrame(5_016);
    expect(stats.summary()!.intervalP99).toBe(16);
  });
});
