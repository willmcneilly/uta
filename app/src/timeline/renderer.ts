// The timeline's one drawing interface, like the piano roll's. The component
// decides what to draw and when; a renderer only draws it (RFC-003, "Clips
// on the timeline").

import type { PlacedNote } from "../pianoRoll/notes";
import type { Span } from "./editing";
import type { TimelineViewport } from "./viewport";

/** The three stacked canvases, bottom to top. */
export interface TimelineLayers {
  /**
   * The ruler, the loop region, the track rows and the grid. Redrawn on
   * scroll, zoom, or a change of tracks or the loop.
   */
  grid: HTMLCanvasElement;
  /** The clips and their notes. Redrawn when the clips, the selection or the view change. */
  clips: HTMLCanvasElement;
  /** The playhead and the clip being drawn. Redrawn when either moves. */
  top: HTMLCanvasElement;
}

/** What the grid layer shows. */
export interface TimelineGridScene {
  view: TimelineViewport;
  ticksPerQuarter: number;
  beatsPerBar: number;
  trackCount: number;
  /** The selected track's index, if any. Its row is highlighted. */
  selectedTrack: number | null;
  /**
   * The loop region, in ticks, drawn on the ruler: shaded while it's on,
   * greyed out while it's off.
   */
  loop: { start: number; end: number; enabled: boolean };
}

/** A clip, ready to draw. */
export interface DrawnClip {
  id: string;
  /** Its track's index in the order. */
  track: number;
  /** In ticks from the start of the song. */
  start: number;
  length: number;
  selected: boolean;
  /** Its notes inside it and in view, in song time, for the preview. */
  notes: readonly PlacedNote[];
  /**
   * The lowest and highest pitch of all its notes inside it, not just those
   * in view, so the preview keeps its scale while scrolling.
   */
  low: number;
  high: number;
}

/** What the clips layer shows. */
export interface TimelineClipsScene {
  view: TimelineViewport;
  /** Only those in view, in the order to draw them: each track's by start, then ID. */
  clips: readonly DrawnClip[];
}

export interface TimelineRenderer {
  /** Sizes every layer, in CSS pixels, at `pixelRatio` device pixels to each. */
  resize(width: number, height: number, pixelRatio: number): void;
  drawGrid(scene: TimelineGridScene): void;
  drawClips(scene: TimelineClipsScene): void;
  /**
   * `playhead` is in ticks from the start of the song. `drawing` is the clip
   * being dragged out on empty space, if any: it's only added once the drag
   * ends.
   */
  drawTop(view: TimelineViewport, playhead: number, drawing: Span | null): void;
}

/** Makes a renderer for `layers`, or `null` if this one can't draw here. */
export type TimelineRendererFactory = (layers: TimelineLayers) => TimelineRenderer | null;
