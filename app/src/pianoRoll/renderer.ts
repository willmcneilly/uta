// The piano roll's one drawing interface. The component decides what to draw
// and when; a renderer only draws it. Canvas 2D is the only one for now, and
// a WebGL one can take its place without the component changing (RFC-002,
// "The shared model", point 8).

import type { PlacedNote } from "./notes";
import type { Rect, Viewport } from "./viewport";

/** The three stacked canvases, bottom to top. */
export interface Layers {
  /**
   * The keyboard, the ruler, the grid and the velocity lane's background.
   * Redrawn on scroll, zoom, or a change to the loop or the clip's place.
   */
  grid: HTMLCanvasElement;
  /** The notes and their velocity bars. Redrawn when the notes, the selection or the view change. */
  notes: HTMLCanvasElement;
  /** The playhead and the selection box. Redrawn when either moves. */
  top: HTMLCanvasElement;
}

/** What the grid layer shows. */
export interface GridScene {
  view: Viewport;
  ticksPerQuarter: number;
  beatsPerBar: number;
  loopStart: number;
  loopEnd: number;
  /** Whether the loop is on. While it's off, the loop region is drawn greyed out. */
  loopEnabled: boolean;
  /** The clip's place in the song. Outside it is shaded. */
  clipStart: number;
  clipEnd: number;
}

/** What the notes layer shows. */
export interface NotesScene {
  view: Viewport;
  /** Only those in view (see `NoteIndex.visible`), in the order to draw them. */
  notes: readonly PlacedNote[];
  /** The notes whose velocity bars are in view: those in view in time, at any pitch. */
  velocities: readonly PlacedNote[];
  /** The selected notes' IDs. */
  selected: ReadonlySet<string>;
}

export interface PianoRollRenderer {
  /** Sizes every layer, in CSS pixels, at `pixelRatio` device pixels to each. */
  resize(width: number, height: number, pixelRatio: number): void;
  drawGrid(scene: GridScene): void;
  drawNotes(scene: NotesScene): void;
  /**
   * `playhead` is in ticks from the start of the song. `box` is the
   * selection box being dragged out, in CSS pixels, if any.
   */
  drawTop(view: Viewport, playhead: number, box: Rect | null): void;
}

/** Makes a renderer for `layers`, or `null` if this one can't draw here. */
export type RendererFactory = (layers: Layers) => PianoRollRenderer | null;
