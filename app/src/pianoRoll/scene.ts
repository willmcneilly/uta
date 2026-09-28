// What the piano roll draws, and which layers need drawing again. The
// component feeds it the project, the size and scrolling; the drawing loop
// calls `draw` once a screen frame. Kept out of React so that scrolling and
// the playhead never re-render anything.

import type { ClipView, ProjectView } from "../backend";
import { type Hit, hitTest } from "./editing";
import { NoteIndex } from "./notes";
import type { PianoRollRenderer } from "./renderer";
import {
  PITCH_COUNT,
  type Viewport,
  clampViewport,
  noteArea,
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
  private selected: string | null = null;

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

  /** Highlights the note with this ID, or none. */
  setSelection(id: string | null): void {
    if (id === this.selected) return;
    this.selected = id;
    this.notesDirty = true;
  }

  /** Whether `x`, `y` (CSS pixels from the top left) is where notes are drawn. */
  inNoteArea(x: number, y: number): boolean {
    if (!this.view) return false;
    const area = noteArea(this.view);
    return x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height;
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
      renderer.drawNotes(
        view,
        index.visible(ticks.start, ticks.end, pitches.low, pitches.high),
        this.selected,
      );
      this.notesDirty = false;
    }
    if (this.topDirty || playhead !== this.lastPlayhead) {
      renderer.drawTop(view, playhead);
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
