// How long the piano roll's frames take: the time between one screen frame
// and the next, and the time spent drawing each. The stress test uses it to
// decide whether Canvas 2D is smooth enough (RFC-002, "Risks & unknowns").

/** How many recent frames the figures cover: 4 seconds at 60 frames a second. */
export const FRAME_WINDOW = 240;

export interface FrameSummary {
  /** Time between frames, in milliseconds. */
  intervalP50: number;
  intervalP99: number;
  /** Time spent drawing a frame, in milliseconds. */
  drawP50: number;
  drawP99: number;
}

export class FrameStats {
  private readonly intervals = new Float64Array(FRAME_WINDOW);
  private readonly draws = new Float64Array(FRAME_WINDOW);
  private count = 0;
  private last: number | null = null;

  /** Records a frame that started at `now` and took `drawMs` to draw. */
  record(now: number, drawMs: number): void {
    if (this.last !== null) {
      const slot = this.count % FRAME_WINDOW;
      this.intervals[slot] = now - this.last;
      this.draws[slot] = drawMs;
      this.count += 1;
    }
    this.last = now;
  }

  /** Forgets the previous frame, so a pause (a hidden window) isn't counted. */
  pause(): void {
    this.last = null;
  }

  /** The figures over the recent frames, or `null` before there are any. */
  summary(): FrameSummary | null {
    const filled = Math.min(this.count, FRAME_WINDOW);
    if (filled === 0) return null;
    const intervals = this.intervals.slice(0, filled).sort();
    const draws = this.draws.slice(0, filled).sort();
    return {
      intervalP50: percentile(intervals, 0.5),
      intervalP99: percentile(intervals, 0.99),
      drawP50: percentile(draws, 0.5),
      drawP99: percentile(draws, 0.99),
    };
  }
}

/** The value `fraction` of the way through `sorted`, by the nearest-rank method. */
export function percentile(sorted: ArrayLike<number>, fraction: number): number {
  if (sorted.length === 0) return NaN;
  const rank = Math.ceil(fraction * sorted.length);
  return sorted[Math.min(sorted.length, Math.max(1, rank)) - 1];
}
