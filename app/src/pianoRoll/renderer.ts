// The piano roll's one drawing interface. The component decides what to draw
// and when; a renderer only draws it. Canvas 2D is the only one for now, and
// a WebGL one can take its place without the component changing (RFC-002,
// "The shared model", point 8).

import type { PlacedNote } from "./notes";
import type { Viewport } from "./viewport";

/** The three stacked canvases, bottom to top. */
export interface Layers {
  /** The keyboard, the ruler and the grid. Redrawn on scroll, zoom or a loop change. */
  grid: HTMLCanvasElement;
  /** The notes. Redrawn when the notes or the view change. */
  notes: HTMLCanvasElement;
  /** The playhead, and later the selection box and drag previews. Redrawn every frame. */
  top: HTMLCanvasElement;
}

/** What the grid layer shows. */
export interface GridScene {
  view: Viewport;
  ticksPerQuarter: number;
  beatsPerBar: number;
  loopStart: number;
  loopEnd: number;
}

export interface PianoRollRenderer {
  /** Sizes every layer, in CSS pixels, at `pixelRatio` device pixels to each. */
  resize(width: number, height: number, pixelRatio: number): void;
  drawGrid(scene: GridScene): void;
  /**
   * `notes` are only those in view (see `NoteIndex.visible`), in the order to
   * draw them. `selected` is the selected note's ID, if any.
   */
  drawNotes(view: Viewport, notes: readonly PlacedNote[], selected: string | null): void;
  /** `playhead` is in ticks from the start of the song. */
  drawTop(view: Viewport, playhead: number): void;
}

/** Makes a renderer for `layers`, or `null` if this one can't draw here. */
export type RendererFactory = (layers: Layers) => PianoRollRenderer | null;
