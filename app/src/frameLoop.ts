// One drawing loop for every canvas view, once a screen frame. The piano
// roll and the timeline each add their drawing to it, and the frame readout
// times them together, so it shows what both canvases cost (RFC-003, "Risks
// & unknowns": two busy canvases).

import type { FrameStats } from "./pianoRoll/frameStats";

/** Draws a view for the screen frame at `now` (from `requestAnimationFrame`). */
export type Draw = (now: number) => void;

export class FrameLoop {
  private readonly draws = new Set<Draw>();
  private request = 0;

  constructor(private readonly stats: FrameStats) {}

  /** Calls `draw` once a screen frame until the returned function is called. */
  add(draw: Draw): () => void {
    this.draws.add(draw);
    if (this.draws.size === 1) this.start();
    return () => {
      if (this.draws.delete(draw) && this.draws.size === 0) this.stop();
    };
  }

  private readonly frame = (now: number) => {
    const started = performance.now();
    for (const draw of this.draws) draw(now);
    this.stats.record(now, performance.now() - started);
    this.request = requestAnimationFrame(this.frame);
  };

  // Frames stop while the window is hidden; that gap isn't a slow frame.
  private readonly onVisibility = () => this.stats.pause();

  private start(): void {
    this.request = requestAnimationFrame(this.frame);
    document.addEventListener("visibilitychange", this.onVisibility);
  }

  private stop(): void {
    cancelAnimationFrame(this.request);
    document.removeEventListener("visibilitychange", this.onVisibility);
    // Nor is the time until something draws again.
    this.stats.pause();
  }
}
