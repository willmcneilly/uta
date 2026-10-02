// What the timeline draws, and which layers need drawing again. The
// component feeds it the project, the size, scrolling and the selection; the
// frame loop calls `draw` once a screen frame. Kept out of React so that
// scrolling and the playhead never re-render anything.

import type { ClipView, ProjectView } from "../backend";
import { followPage } from "../follow";
import { NoteIndex, type PlacedNote } from "../pianoRoll/notes";
import { sameClip } from "../pianoRoll/scene";
import { type ClipHit, type Span, hitClip } from "./editing";
import type { DrawnClip, TimelineRenderer } from "./renderer";
import {
  type TimelineViewport,
  clampViewport,
  inTracks,
  visibleTicks,
  visibleTracks,
} from "./viewport";

/** When the timeline opens, at least this many bars fill its width. */
const INITIAL_BARS = 16;
/** How far past the song's end the view scrolls, so there's room to draw clips. */
const BARS_PAST_END = 8;

/** A clip's notes, indexed for drawing only those in view, and their range of pitches. */
interface ClipNotes {
  clip: ClipView;
  index: NoteIndex;
  low: number;
  high: number;
}

export class TimelineScene {
  private project: ProjectView | null = null;
  private view: TimelineViewport | null = null;
  private renderer: TimelineRenderer | null = null;
  private size: { width: number; height: number; pixelRatio: number } | null = null;
  /** Each clip's notes by clip ID, rebuilt only when that clip changes. */
  private notes = new Map<string, ClipNotes>();
  private selectedTrack: string | null = null;
  private selectedClip: string | null = null;
  private drawing: Span | null = null;
  private gridDirty = true;
  private clipsDirty = true;
  private topDirty = true;
  private lastPlayhead = NaN;

  /** A new project view from Rust: the tracks or clips may have changed. */
  setProject(project: ProjectView): void {
    const previous = this.project;
    this.project = project;
    // Every view from Rust is a fresh object, so compare what's in it: a
    // volume or tempo change mustn't redraw every clip.
    const notes = new Map<string, ClipNotes>();
    let changed = previous === null || previous.tracks.length !== project.tracks.length;
    project.tracks.forEach((track, index) => {
      const before = previous?.tracks[index];
      if (!before || before.id !== track.id || before.clips.length !== track.clips.length) {
        changed = true;
      }
      track.clips.forEach((clip, i) => {
        const known = this.notes.get(clip.id);
        if (known && sameClip(known.clip, clip)) {
          notes.set(clip.id, known);
        } else {
          notes.set(clip.id, indexNotes(clip));
          changed = true;
        }
        if (before?.clips[i]?.id !== clip.id) changed = true;
      });
    });
    this.notes = notes;
    if (changed) this.clipsDirty = true;
    if (
      !previous ||
      previous.tracks.length !== project.tracks.length ||
      previous.ticksPerQuarter !== project.ticksPerQuarter ||
      previous.beatsPerBar !== project.beatsPerBar ||
      previous.loopStart !== project.loopStart ||
      previous.loopLength !== project.loopLength ||
      previous.loopEnabled !== project.loopEnabled ||
      previous.tracks.some((track, i) => track.id !== project.tracks[i].id)
    ) {
      this.gridDirty = true;
    }
    if (!this.view && this.size) this.view = this.initialView();
    this.clampView();
  }

  /** The timeline's size in CSS pixels. */
  resize(width: number, height: number, pixelRatio: number): void {
    this.size = { width, height, pixelRatio };
    this.renderer?.resize(width, height, pixelRatio);
    if (this.view) this.view = { ...this.view, width, height };
    else this.view = this.initialView();
    this.clampView();
    this.markAllDirty();
  }

  /** Draws with `renderer` from now on, or stops drawing with `null`. */
  setRenderer(renderer: TimelineRenderer | null): void {
    this.renderer = renderer;
    if (renderer && this.size) {
      renderer.resize(this.size.width, this.size.height, this.size.pixelRatio);
    }
    this.markAllDirty();
  }

  /** Scrolls or zooms. The result is kept inside the timeline. */
  changeView(change: (view: TimelineViewport) => TimelineViewport): void {
    const before = this.view;
    if (!before) return;
    this.view = change(before);
    this.clampView();
    if (!sameView(before, this.view)) this.markAllDirty();
  }

  /**
   * Turns a page if `playhead` has run off the view, so it's at the left
   * edge. Nothing is redrawn while it stays in view, or the view can't
   * scroll further.
   */
  follow(playhead: number): void {
    if (!this.view) return;
    const { start, end } = visibleTicks(this.view);
    const to = followPage(start, end, playhead);
    if (to !== null) this.changeView((view) => ({ ...view, scrollTicks: to }));
  }

  getView(): TimelineViewport | null {
    return this.view;
  }

  /** Highlights the track and the clip with these IDs. */
  setSelection(track: string | null, clip: string | null): void {
    if (track !== this.selectedTrack) this.gridDirty = true;
    if (track !== this.selectedTrack || clip !== this.selectedClip) this.clipsDirty = true;
    this.selectedTrack = track;
    this.selectedClip = clip;
  }

  /** Shows the clip being drawn, or hides it with `null`. */
  setDrawing(drawing: Span | null): void {
    this.drawing = drawing;
    this.topDirty = true;
  }

  /** The clip under `x`, `y` (CSS pixels from the top left), and which part of it. */
  hitTest(x: number, y: number): ClipHit | null {
    const { view, project } = this;
    if (!view || !project || !inTracks(view, y)) return null;
    return hitClip(view, project.tracks, x, y);
  }

  /** Redraws the layers that need it, with the playhead at `playhead` ticks. */
  draw(playhead: number): void {
    const { renderer, view, project } = this;
    if (!renderer || !view || !project) return;
    if (this.gridDirty) {
      const selected = project.tracks.findIndex((track) => track.id === this.selectedTrack);
      renderer.drawGrid({
        view,
        ticksPerQuarter: project.ticksPerQuarter,
        beatsPerBar: project.beatsPerBar,
        trackCount: project.tracks.length,
        selectedTrack: selected >= 0 ? selected : null,
        loop: {
          start: project.loopStart,
          end: project.loopStart + project.loopLength,
          enabled: project.loopEnabled,
        },
      });
      this.gridDirty = false;
    }
    if (this.clipsDirty) {
      renderer.drawClips({ view, clips: this.clipsInView(view, project) });
      this.clipsDirty = false;
    }
    if (this.topDirty || playhead !== this.lastPlayhead) {
      renderer.drawTop(view, playhead, this.drawing);
      this.lastPlayhead = playhead;
      this.topDirty = false;
    }
  }

  /** The clips with any part in view, each with its notes in view. */
  private clipsInView(view: TimelineViewport, project: ProjectView): DrawnClip[] {
    const ticks = visibleTicks(view);
    const tracks = visibleTracks(view, project.tracks.length);
    const drawn: DrawnClip[] = [];
    for (let track = tracks.first; track <= tracks.last; track++) {
      for (const clip of project.tracks[track].clips) {
        const end = clip.start + clip.length;
        if (end <= ticks.start || clip.start >= ticks.end) continue;
        const notes = this.notes.get(clip.id);
        drawn.push({
          id: clip.id,
          track,
          start: clip.start,
          length: clip.length,
          selected: clip.id === this.selectedClip,
          notes: notes
            ? notesInside(notes, Math.max(ticks.start, clip.start), Math.min(ticks.end, end))
            : [],
          low: notes?.low ?? 0,
          high: notes?.high ?? 0,
        });
      }
    }
    return drawn;
  }

  private markAllDirty(): void {
    this.gridDirty = this.clipsDirty = this.topDirty = true;
  }

  /** At least 16 bars, or the whole song, fill the width. */
  private initialView(): TimelineViewport | null {
    const { size, project } = this;
    if (!size || !project) return null;
    const bar = project.ticksPerQuarter * project.beatsPerBar;
    const ticks = Math.max(INITIAL_BARS * bar, project.songEnd);
    return {
      width: size.width,
      height: size.height,
      scrollTicks: 0,
      scrollY: 0,
      pixelsPerTick: size.width > 0 ? size.width / ticks : 1,
    };
  }

  private clampView(): void {
    const { view, project } = this;
    if (!view || !project) return;
    const bar = project.ticksPerQuarter * project.beatsPerBar;
    const content =
      Math.max(project.songEnd, project.loopStart + project.loopLength) + BARS_PAST_END * bar;
    const clamped = clampViewport(view, content, project.tracks.length);
    if (!sameView(clamped, view)) this.markAllDirty();
    this.view = clamped;
  }
}

function indexNotes(clip: ClipView): ClipNotes {
  let low = 127;
  let high = 0;
  for (const note of clip.notes) {
    // Notes past the clip's end don't play, and aren't shown.
    if (note.start >= clip.length) continue;
    low = Math.min(low, note.pitch);
    high = Math.max(high, note.pitch);
  }
  return { clip, index: new NoteIndex(clip), low: Math.min(low, high), high };
}

/** A clip's notes with any part from `start` to `end`, leaving out those past its end. */
function notesInside(notes: ClipNotes, start: number, end: number): PlacedNote[] {
  return notes.index.visible(start, end, 0, 127).filter((note) => !note.outside);
}

function sameView(a: TimelineViewport, b: TimelineViewport): boolean {
  return (
    a.width === b.width &&
    a.height === b.height &&
    a.scrollTicks === b.scrollTicks &&
    a.scrollY === b.scrollY &&
    a.pixelsPerTick === b.pixelsPerTick
  );
}
