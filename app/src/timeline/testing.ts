// Test helpers: a timeline renderer that records what it's asked to draw
// instead of drawing.

import type { Rect } from "../pianoRoll/viewport";
import type { Span } from "./editing";
import type {
  DrawnClip,
  TimelineClipsScene,
  TimelineGridScene,
  TimelineRenderer,
  TimelineRendererFactory,
} from "./renderer";
import type { TimelineViewport } from "./viewport";

export class RecordingTimelineRenderer implements TimelineRenderer {
  sizes: [number, number][] = [];
  grids: TimelineGridScene[] = [];
  clips: TimelineClipsScene[] = [];
  tops: number[] = [];
  drawings: (Span | null)[] = [];
  boxes: (Rect | null)[] = [];

  resize(width: number, height: number): void {
    this.sizes.push([width, height]);
  }
  drawGrid(scene: TimelineGridScene): void {
    this.grids.push(scene);
  }
  drawClips(scene: TimelineClipsScene): void {
    this.clips.push(scene);
  }
  drawTop(_view: TimelineViewport, playhead: number, drawing: Span | null, box: Rect | null): void {
    this.tops.push(playhead);
    this.drawings.push(drawing);
    this.boxes.push(box);
  }

  /** The view the clips were last drawn in. */
  lastView(): TimelineViewport {
    const view = this.clips.at(-1)?.view;
    if (!view) throw new Error("no clips drawn yet");
    return view;
  }

  /** The clips drawn most recently. */
  lastClips(): readonly DrawnClip[] {
    return this.clips.at(-1)?.clips ?? [];
  }

  /** The selection box drawn most recently, if any. */
  lastBox(): Rect | null {
    return this.boxes.at(-1) ?? null;
  }

  /** The clip being drawn most recently, if any. */
  lastDrawing(): Span | null {
    return this.drawings.at(-1) ?? null;
  }
}

/** A factory that hands out one recording renderer, for tests to look at. */
export function recordingTimelineFactory(): {
  factory: TimelineRendererFactory;
  renderer: RecordingTimelineRenderer;
} {
  const renderer = new RecordingTimelineRenderer();
  return { factory: () => renderer, renderer };
}
