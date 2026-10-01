// The playhead between the engine's reports. Rust sends where the playhead
// is once a frame; to move smoothly on screen, the piano roll works out where
// it should be now from the time since, at the project's tempo (RFC-002,
// "The shared model", point 8).

export interface Timing {
  bpm: number;
  ticksPerQuarter: number;
  loopStart: number;
  loopLength: number;
  loopEnabled: boolean;
}

/**
 * Keeps guessing for at most this long after the last report, so the
 * playhead stops rather than runs on if reports stop.
 */
export const MAX_GUESS_MS = 100;

/**
 * A report within this much of the guess nudges the guess towards it rather
 * than jumping, so jitter in when reports arrive doesn't shake the playhead.
 */
export const SMOOTHING_WINDOW_MS = 30;

/** How much of the difference each nearby report corrects. */
export const CORRECTION = 0.25;

export class PlayheadClock {
  private timing: Timing = {
    bpm: 120,
    ticksPerQuarter: 960,
    loopStart: 0,
    loopLength: 0,
    loopEnabled: true,
  };
  private playing = false;
  /** Where the playhead was at `anchorTime`, in ticks. */
  private anchorTicks = 0;
  private anchorTime = 0;
  private startCount = 0;

  setTiming(timing: Timing): void {
    this.timing = timing;
  }

  /** Whether the engine last said it was playing. */
  isPlaying(): boolean {
    return this.playing;
  }

  /** How many times playback has started: it goes up on every Play or Continue. */
  starts(): number {
    return this.startCount;
  }

  /** Takes a report from the engine, received at `now` (in milliseconds). */
  report(ticks: number, playing: boolean, now: number): void {
    if (playing && this.playing) {
      const guess = this.at(now);
      const error = this.wrapDifference(ticks - guess);
      if (Math.abs(error) <= this.ticksIn(SMOOTHING_WINDOW_MS)) {
        this.anchor(this.wrap(guess + error * CORRECTION), now);
        return;
      }
    }
    if (playing && !this.playing) this.startCount++;
    this.playing = playing;
    this.anchor(ticks, now);
  }

  /** Where the playhead is at `now`, in ticks. */
  at(now: number): number {
    if (!this.playing) return this.anchorTicks;
    const elapsed = Math.min(Math.max(0, now - this.anchorTime), MAX_GUESS_MS);
    return this.wrap(this.anchorTicks + this.ticksIn(elapsed));
  }

  private anchor(ticks: number, now: number): void {
    this.anchorTicks = ticks;
    this.anchorTime = now;
  }

  private ticksIn(ms: number): number {
    return (ms / 60_000) * this.timing.bpm * this.timing.ticksPerQuarter;
  }

  /**
   * Whether playback goes round the loop: it does when the loop is on and
   * it was before the loop's end, as in the engine.
   */
  private looping(): boolean {
    const { loopStart, loopLength, loopEnabled } = this.timing;
    return loopEnabled && loopLength > 0 && this.anchorTicks < loopStart + loopLength;
  }

  /**
   * Brings `ticks` past the loop's end back round it, as the engine does.
   * Before the loop, it's on its way in.
   */
  private wrap(ticks: number): number {
    const { loopStart, loopLength } = this.timing;
    if (!this.looping() || ticks < loopStart + loopLength) return ticks;
    return loopStart + ((ticks - loopStart) % loopLength);
  }

  /**
   * The shortest way from one loop position to another: a report just after
   * the loop start is slightly ahead of a guess just before the loop end.
   */
  private wrapDifference(difference: number): number {
    const { loopLength } = this.timing;
    if (!this.looping()) return difference;
    if (difference > loopLength / 2) return difference - loopLength;
    if (difference < -loopLength / 2) return difference + loopLength;
    return difference;
  }
}
