import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ProjectView } from "../backend";
import { Updates, projectView, trackView } from "../pianoRoll/testing";
import { type BenchmarkHost, type BenchmarkOptions, type Clock, runBenchmark } from "./run";

const FRAME = 16;

/**
 * Screen frames 16 ms apart, shared by everyone waiting on one: a frame
 * comes once every waiter so far has asked for it, as with
 * `requestAnimationFrame`. Callbacks can be put on a later frame, before
 * its waiters run, as an event arriving between frames is.
 */
class FakeClock implements Clock {
  time = 1000;
  frames = 0;
  private waiters: ((time: number) => void)[] = [];
  private scheduled = new Map<number, (() => void)[]>();
  private ticking = false;

  now = () => this.time;

  frame = () =>
    new Promise<number>((resolve) => {
      this.waiters.push(resolve);
      if (!this.ticking) {
        this.ticking = true;
        setTimeout(() => this.tick(), 0);
      }
    });

  sleep = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

  /** Runs `callback` just before the frame `count` frames from now. */
  in(count: number, callback: () => void) {
    const at = this.frames + count;
    this.scheduled.set(at, [...(this.scheduled.get(at) ?? []), callback]);
  }

  private tick() {
    this.ticking = false;
    this.frames += 1;
    this.time += FRAME;
    for (const callback of this.scheduled.get(this.frames) ?? []) callback();
    this.scheduled.delete(this.frames);
    const waiters = this.waiters;
    this.waiters = [];
    for (const resolve of waiters) resolve(this.time);
  }
}

const options = (overrides: Partial<BenchmarkOptions> = {}): BenchmarkOptions => ({
  songs: ["heavy"],
  sliderSteps: 2,
  noteDeletes: 2,
  dragSteps: 3,
  eventWaitMs: 250,
  settleMs: 0,
  ...overrides,
});

/** A song of one track whose clip holds four notes. */
const song: ProjectView = projectView({
  tracks: [
    trackView("track-1", "Synth 1", [
      {
        id: "clip-1",
        start: 0,
        length: 3840,
        notes: ["a", "b", "c", "d"].map((id, i) => ({
          id,
          pitch: 60 + i,
          velocity: 100,
          start: i * 480,
          length: 480,
        })),
      },
    ]),
  ],
});

let clock: FakeClock;
let events: number;
/** When Rust's `project-changed` for each change arrives, if it does. */
let event: "none" | "before reply" | { framesLater: number };

const host: BenchmarkHost = {
  apply: () => {},
  project: () => song,
  events: () => events,
  showClip: () => {},
};

beforeEach(() => {
  clock = new FakeClock();
  events = 0;
  event = "none";
  mockIPC((cmd) => {
    if (event === "before reply") events += 1;
    else if (event !== "none") clock.in(event.framesLater, () => (events += 1));
    // Every command answers with the same song: only the timing is tested.
    return cmd === "build_test_song" || cmd === "set_track_mixer" || cmd === "remove_notes"
      ? new Updates().send(song)
      : null;
  });
});

afterEach(() => clearMocks());

const run = async (overrides: Partial<BenchmarkOptions> = {}) => {
  const report = await runBenchmark(host, () => {}, options(overrides), clock);
  return report.loads[0];
};

describe("runBenchmark's timing", () => {
  it("ends a step at the frame after its reply, when no event comes", async () => {
    const load = await run();
    expect(load.sliderSteps).toEqual([FRAME, FRAME]);
    expect(load.noteDeletes).toEqual([FRAME, FRAME]);
    expect(load.buildMs).toBe(FRAME);
    expect(load.events).toBe(0);
  });

  it("doesn't count the wait for an event that never comes", async () => {
    const quick = await run({ eventWaitMs: 0 });
    const patient = await run({ eventWaitMs: 250 });
    expect(patient.sliderSteps).toEqual(quick.sliderSteps);
    expect(patient.dragMs).toBe(quick.dragMs);
    expect(patient.dragFrames).toEqual(quick.dragFrames);
  });

  it("waits for a late event and counts until the frame that draws it", async () => {
    event = { framesLater: 3 };
    const load = await run();
    // The reply is drawn a frame after sending; the event arrives before
    // the third frame, and is drawn a frame after that.
    expect(load.sliderSteps).toEqual([4 * FRAME, 4 * FRAME]);
    expect(load.noteDeletes).toEqual([4 * FRAME, 4 * FRAME]);
    expect(load.events).toBe(load.changes);
  });

  it("counts an event that beats its reply as drawn with the reply", async () => {
    event = "before reply";
    const load = await run();
    expect(load.sliderSteps).toEqual([FRAME, FRAME]);
    expect(load.events).toBe(load.changes);
  });

  it("ends the drag when its last reply is drawn, and records only its frames", async () => {
    const load = await run();
    // A step a frame for three frames, each reply drawn a frame later.
    expect(load.dragSteps).toEqual([FRAME, FRAME, FRAME]);
    expect(load.dragMs).toBe(4 * FRAME);
    expect(load.dragFrames).toEqual([FRAME, FRAME, FRAME]);
  });

  it("sends the drag's steps one at a time, latest wins, when replies are slow", async () => {
    let inFlight = 0;
    let most = 0;
    const volumes: number[] = [];
    mockIPC((cmd, payload) => {
      if (cmd !== "set_track_mixer")
        return cmd === "build_test_song" ? new Updates().send(song) : null;
      const { mixer } = payload as { mixer: { volumeDb: number } };
      volumes.push(mixer.volumeDb);
      inFlight += 1;
      most = Math.max(most, inFlight);
      // Each reply takes three frames.
      return new Promise((resolve) =>
        clock.in(3, () => {
          inFlight -= 1;
          resolve(new Updates().send(song));
        }),
      );
    });
    const load = await run({ sliderSteps: 0, noteDeletes: 0, dragSteps: 8 });

    expect(most).toBe(1);
    expect(load.dragSent).toBeLessThan(8);
    expect(load.dragSent).toBe(volumes.length);
    // The first step goes at once and the final one always goes; between
    // them, only the newest of those that waited.
    expect(volumes[0]).toBe(-10);
    expect(volumes.at(-1)).toBe(-10 - 7 * 0.5);
    expect([...volumes].sort((a, b) => b - a)).toEqual(volumes);
    // Every step is timed, including those replaced by a newer one.
    expect(load.dragSteps).toHaveLength(8);
  });

  it("ends the drag when its last event is drawn, frames included", async () => {
    event = { framesLater: 3 };
    const load = await run();
    // The last step is sent on frame 3; its event arrives before frame 6
    // and is drawn on frame 7.
    expect(load.dragMs).toBe(7 * FRAME);
    expect(load.dragFrames).toEqual(Array(6).fill(FRAME));
  });
});
