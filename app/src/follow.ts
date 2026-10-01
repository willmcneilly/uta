// Following the playhead: while playing, a view that the playhead runs off
// turns a page, so the playhead is back at its left edge. Scrolling or
// editing pauses it until the next Play (RFC-003, "Playing a song").

import type { PlayheadClock } from "./pianoRoll/playhead";

/**
 * Where a view showing ticks `start` to `end` (exclusive) scrolls to, to
 * follow the playhead at `playhead`: a page on when it reaches the right
 * edge, so it's at the left edge; there too when it jumps or goes round the
 * loop out of view. `null` while it's in view: the view stays put.
 */
export function followPage(start: number, end: number, playhead: number): number | null {
  if (playhead >= start && playhead < end) return null;
  return playhead;
}

/** Whether one view follows the playhead: while playing, unless paused since the last Play. */
export class Follow {
  /** The clock's count of starts when following was paused, or `null` if it isn't. */
  private pausedAt: number | null = null;

  constructor(private readonly clock: PlayheadClock) {}

  /** Stops following until playback next starts: on a scroll or an edit. */
  pause(): void {
    this.pausedAt = this.clock.starts();
  }

  /** Whether to follow the playhead now. */
  following(): boolean {
    if (!this.clock.isPlaying()) return false;
    if (this.pausedAt === this.clock.starts()) return false;
    this.pausedAt = null;
    return true;
  }
}
