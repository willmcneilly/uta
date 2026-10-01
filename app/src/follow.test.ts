import { describe, expect, it } from "vitest";
import { Follow, followPage } from "./follow";
import { PlayheadClock } from "./pianoRoll/playhead";

describe("followPage", () => {
  it("stays put while the playhead is in view", () => {
    expect(followPage(0, 1000, 0)).toBeNull();
    expect(followPage(0, 1000, 999)).toBeNull();
  });

  it("turns a page when the playhead reaches the right edge", () => {
    expect(followPage(0, 1000, 1000)).toBe(1000);
    expect(followPage(1000, 2000, 2003)).toBe(2003);
  });

  it("goes to the playhead when it jumps or goes round the loop out of view", () => {
    expect(followPage(4000, 5000, 0)).toBe(0);
    expect(followPage(0, 1000, 7000)).toBe(7000);
  });
});

describe("Follow", () => {
  function playing(): { clock: PlayheadClock; follow: Follow } {
    const clock = new PlayheadClock();
    clock.report(0, true, 0);
    return { clock, follow: new Follow(clock) };
  }

  it("follows only while playing", () => {
    const { clock, follow } = playing();
    expect(follow.following()).toBe(true);
    clock.report(100, false, 16);
    expect(follow.following()).toBe(false);
  });

  it("pauses on a scroll or edit, and resumes on the next Play", () => {
    const { clock, follow } = playing();
    follow.pause();
    clock.report(10, true, 16);
    expect(follow.following()).toBe(false);
    clock.report(20, false, 32);
    clock.report(0, true, 48);
    expect(follow.following()).toBe(true);
    // And stays on after that.
    clock.report(10, true, 64);
    expect(follow.following()).toBe(true);
  });

  it("paused while stopped, it resumes when playback starts", () => {
    const clock = new PlayheadClock();
    const follow = new Follow(clock);
    follow.pause();
    clock.report(0, true, 0);
    expect(follow.following()).toBe(true);
  });
});
