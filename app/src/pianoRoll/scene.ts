// What the piano roll draws, and which layers need drawing again. The
// component feeds it the project, the size and scrolling; the drawing loop
// calls `draw` once a screen frame. Kept out of React so that scrolling and
// the playhead never re-render anything.

import type { ClipView, ProjectView } from "../backend";
import { type Hit, VELOCITY_HIT_PIXELS, hitTest, hitVelocity } from "./editing";
import { NoteIndex, type PlacedNote } from "./notes";
import type { PianoRollRenderer } from "./renderer";
import {
  PITCH_COUNT,
  type Rect,
  type Viewport,
  clampViewport,
  inRect,
  noteArea,
  velocityLane,
  visiblePitches,
  visibleTicks,
  xToTick,
  yToPitch,
} from "./viewport";

/** The pitch shown at the top when the piano roll opens: C6, so C3 to C6 or so is in view. */
const INITIAL_TOP_PITCH = 84;
const INITIAL_KEY_HEIGHT = 12;

export class PianoRollScene {
  private project: ProjectView | null = null;
  private index: NoteIndex | null = null;
  private view: Viewport | null = null;
  private renderer: PianoRollRenderer | null = null;
  private size: { width: number; height: number; pixelRatio: number } | null = null;
  private gridDirty = true;
  private notesDirty = true;
  private topDirty = true;
  private lastPlayhead = NaN;
  private selected: ReadonlySet<string> = new Set();
  private box: Rect | null = null;

  /** A new project view from Rust: the notes or the loop may have changed. */
  setProject(project: ProjectView): void {
    const loopChanged =
      this.project?.loopStart !== project.loopStart ||
      this.project.loopLength !== project.loopLength;
    // Every view from Rust is a fresh object, so compare what's in it: a
    // volume or tempo change mustn't re-sort and redraw thousands of notes.
    if (!this.project || !sameClip(this.project.track.clip, project.track.clip)) {
      this.index = new NoteIndex(project.track.clip);
      this.notesDirty = true;
    }
    this.project = project;
    if (loopChanged) this.gridDirty = true;
    this.clampView();
  }

  /** The piano roll's size in CSS pixels. */
  resize(width: number, height: number, pixelRatio: number): void {
    this.size = { width, height, pixelRatio };
    this.renderer?.resize(width, height, pixelRatio);
    if (this.view) this.view = { ...this.view, width, height };
    else if (this.project) this.view = initialView(this.project, width, height);
    this.clampView();
    this.markAllDirty();
  }

  /** Draws with `renderer` from now on, or stops drawing with `null`. */
  setRenderer(renderer: PianoRollRenderer | null): void {
    this.renderer = renderer;
    if (renderer && this.size) {
      renderer.resize(this.size.width, this.size.height, this.size.pixelRatio);
    }
    this.markAllDirty();
  }

  /** Scrolls or zooms. The result is kept inside the piano roll. */
  changeView(change: (view: Viewport) => Viewport): void {
    if (!this.view) return;
    this.view = change(this.view);
    this.clampView();
    this.markAllDirty();
  }

  getView(): Viewport | null {
    return this.view;
  }

  /** Highlights the notes with these IDs. */
  setSelection(ids: ReadonlySet<string>): void {
    if (ids.size === this.selected.size && [...ids].every((id) => this.selected.has(id))) return;
    this.selected = new Set(ids);
    this.notesDirty = true;
  }

  /** Shows the selection box being dragged out, in CSS pixels, or hides it with `null`. */
  setBox(box: Rect | null): void {
    this.box = box;
    this.topDirty = true;
  }

  /** Whether `x`, `y` (CSS pixels from the top left) is where notes are drawn. */
  inNoteArea(x: number, y: number): boolean {
    return this.view !== null && inRect(noteArea(this.view), x, y);
  }

  /** Whether `x`, `y` is in the velocity lane. */
  inVelocityLane(x: number, y: number): boolean {
    return this.view !== null && inRect(velocityLane(this.view), x, y);
  }

  /** The note drawn under `x`, `y`, and which part of it. */
  hitTest(x: number, y: number): Hit | null {
    const { view, index } = this;
    if (!view || !index || !this.inNoteArea(x, y)) return null;
    const tick = xToTick(view, x);
    const pitch = yToPitch(view, y);
    // A pixel either side, for notes drawn wider than they are long.
    const slack = 1 / view.pixelsPerTick;
    return hitTest(view, index.visible(tick - slack, tick + slack, pitch, pitch), x, y);
  }

  /** The note whose velocity bar is under `x`, if any. */
  hitVelocity(x: number): PlacedNote | null {
    const { view, index } = this;
    if (!view || !index) return null;
    const tick = xToTick(view, x);
    // Bars are drawn at notes' starts; look at any starting near the pointer.
    const slack = (VELOCITY_HIT_PIXELS + 1) / view.pixelsPerTick;
    return hitVelocity(
      view,
      index.visible(tick - slack, tick + slack, 0, PITCH_COUNT - 1),
      this.selected,
      x,
    );
  }

  /**
   * The IDs of the notes with any part inside `box` (CSS pixels), which is
   * kept to where notes are drawn.
   */
  notesIn(box: Rect): string[] {
    const { view, index } = this;
    if (!view || !index) return [];
    const area = noteArea(view);
    const left = Math.max(area.x, box.x);
    const right = Math.min(area.x + area.width, box.x + box.width);
    const top = Math.max(area.y, box.y);
    const bottom = Math.min(area.y + area.height, box.y + box.height);
    if (right <= left || bottom <= top) return [];
    return index
      .visible(xToTick(view, left), xToTick(view, right), yToPitch(view, bottom - 1e-6), yToPitch(view, top))
      .map((note) => note.id);
  }

  /** Redraws the layers that need it, with the playhead at `playhead` ticks. */
  draw(playhead: number): void {
    const { renderer, view, project, index } = this;
    if (!renderer || !view || !project || !index) return;
    if (this.gridDirty) {
      renderer.drawGrid({
        view,
        ticksPerQuarter: project.ticksPerQuarter,
        beatsPerBar: project.beatsPerBar,
        loopStart: project.loopStart,
        loopEnd: project.loopStart + project.loopLength,
      });
      this.gridDirty = false;
    }
    if (this.notesDirty) {
      const ticks = visibleTicks(view);
      const pitches = visiblePitches(view);
      renderer.drawNotes({
        view,
        notes: index.visible(ticks.start, ticks.end, pitches.low, pitches.high),
        velocities: index.visible(ticks.start, ticks.end, 0, PITCH_COUNT - 1),
        selected: this.selected,
      });
      this.notesDirty = false;
    }
    if (this.topDirty || playhead !== this.lastPlayhead) {
      renderer.drawTop(view, playhead, this.box);
      this.lastPlayhead = playhead;
      this.topDirty = false;
    }
  }

  private markAllDirty(): void {
    this.gridDirty = this.notesDirty = this.topDirty = true;
  }

  private clampView(): void {
    if (!this.view || !this.project || !this.index) return;
    const clamped = clampViewport(this.view, contentTicks(this.project, this.index));
    if (!sameView(clamped, this.view)) this.markAllDirty();
    this.view = clamped;
  }
}

/** How far right the view can scroll: past the loop and every note, plus a bar. */
function contentTicks(project: ProjectView, index: NoteIndex): number {
  const bar = project.ticksPerQuarter * project.beatsPerBar;
  const clip = project.track.clip;
  return Math.max(project.loopStart + project.loopLength, clip.start + clip.length, index.end) + bar;
}

/** The loop fills the width, with C3 to C6 or so in view. */
function initialView(project: ProjectView, width: number, height: number): Viewport {
  const view: Viewport = {
    width,
    height,
    scrollTicks: project.loopStart,
    scrollY: (PITCH_COUNT - 1 - INITIAL_TOP_PITCH) * INITIAL_KEY_HEIGHT,
    pixelsPerTick: 1,
    keyHeight: INITIAL_KEY_HEIGHT,
  };
  const area = noteArea(view);
  return { ...view, pixelsPerTick: area.width > 0 ? area.width / project.loopLength : 1 };
}

function sameView(a: Viewport, b: Viewport): boolean {
  return (
    a.width === b.width &&
    a.height === b.height &&
    a.scrollTicks === b.scrollTicks &&
    a.scrollY === b.scrollY &&
    a.pixelsPerTick === b.pixelsPerTick &&
    a.keyHeight === b.keyHeight
  );
}

/** Whether two clips have the same position, length and notes, in the same order. */
export function sameClip(a: ClipView, b: ClipView): boolean {
  if (a === b) return true;
  if (a.id !== b.id || a.start !== b.start || a.length !== b.length) return false;
  if (a.notes.length !== b.notes.length) return false;
  return a.notes.every((note, i) => {
    const other = b.notes[i];
    return (
      note.id === other.id &&
      note.pitch === other.pitch &&
      note.velocity === other.velocity &&
      note.start === other.start &&
      note.length === other.length
    );
  });
}
