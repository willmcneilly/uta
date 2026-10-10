// The timeline's Canvas 2D renderer. Each layer draws only what's in view,
// and loops only over what fits on screen, so its work doesn't grow with the
// song.

import { drawPlayhead, drawSelectionBox } from "../design/canvasMarks";
import { drawRuler } from "../design/ruler";
import type { Rect } from "../pianoRoll/viewport";
import type { Span } from "./editing";
import { timelineGridStep } from "./snap";
import { type TimelineTheme, readTimelineTheme } from "./theme";
import type {
  DrawnClip,
  TimelineClipsScene,
  TimelineGridScene,
  TimelineLayers,
  TimelineRenderer,
} from "./renderer";
import {
  RULER_HEIGHT,
  TRACK_HEIGHT,
  type TimelineViewport,
  tickToX,
  trackToY,
  visibleTicks,
  visibleTracks,
} from "./viewport";

/** Space between a clip and the edges of its track's row. */
const CLIP_INSET = 3;
/**
 * Space drawn either side of a clip, so clips that touch keep their own
 * edges rather than doubling into one heavy line. Pressing still finds the
 * clip's full width (provisional: D-1).
 */
const CLIP_GAP = 1;
/** Space between a clip's edge and its notes. */
const PREVIEW_PADDING = 6;
/** The tallest a note is drawn in a preview, however few pitches the clip spans. */
const MAX_PREVIEW_ROW = 6;


export function createTimelineRenderer(layers: TimelineLayers): TimelineRenderer | null {
  const grid = layers.grid.getContext("2d", { alpha: false });
  const clips = layers.clips.getContext("2d");
  const top = layers.top.getContext("2d");
  if (!grid || !clips || !top) return null;
  return new Canvas2DTimelineRenderer(
    layers,
    { grid, clips, top },
    readTimelineTheme(layers.grid),
  );
}

type Contexts = Record<keyof TimelineLayers, CanvasRenderingContext2D>;

class Canvas2DTimelineRenderer implements TimelineRenderer {
  private width = 0;
  private height = 0;

  constructor(
    private readonly layers: TimelineLayers,
    private readonly contexts: Contexts,
    private readonly theme: TimelineTheme,
  ) {}

  resize(width: number, height: number, pixelRatio: number): void {
    this.width = width;
    this.height = height;
    for (const key of ["grid", "clips", "top"] as const) {
      const canvas = this.layers[key];
      canvas.width = Math.round(width * pixelRatio);
      canvas.height = Math.round(height * pixelRatio);
      this.contexts[key].setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    }
  }

  drawGrid({
    view,
    ticksPerQuarter,
    beatsPerBar,
    trackCount,
    selectedTrack,
    loop,
  }: TimelineGridScene): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const ticks = visibleTicks(view);
    const tracks = visibleTracks(view, trackCount);
    const bar = ticksPerQuarter * beatsPerBar;
    const tracksBottom = Math.min(this.height, trackToY(view, trackCount));
    const line = theme.gridWidth;

    context.fillStyle = theme.background;
    context.fillRect(0, 0, this.width, this.height);

    context.save();
    context.beginPath();
    context.rect(0, RULER_HEIGHT, this.width, this.height - RULER_HEIGHT);
    context.clip();

    // Below the last track there's nothing to draw on.
    if (tracksBottom < this.height) {
      context.fillStyle = theme.belowTracks;
      context.fillRect(0, tracksBottom, this.width, this.height - tracksBottom);
    }
    if (selectedTrack !== null && selectedTrack >= tracks.first && selectedTrack <= tracks.last) {
      context.fillStyle = theme.selectedTrack;
      context.fillRect(0, trackToY(view, selectedTrack), this.width, TRACK_HEIGHT);
    }

    // Columns: bars, beats, and finer lines when there's room. Clips snap
    // to them by default.
    const step = timelineGridStep(view.pixelsPerTick, ticksPerQuarter, beatsPerBar);
    const gridBottom = Math.max(RULER_HEIGHT, tracksBottom);
    for (let tick = Math.floor(ticks.start / step) * step; tick < ticks.end; tick += step) {
      context.fillStyle =
        tick % bar === 0
          ? theme.barLine
          : tick % ticksPerQuarter === 0
            ? theme.beatLine
            : theme.subLine;
      context.fillRect(Math.round(tickToX(view, tick)), RULER_HEIGHT, line, gridBottom - RULER_HEIGHT);
    }

    // Rows: a line under each track, as under each header.
    context.fillStyle = theme.rowLine;
    for (let track = tracks.first; track <= tracks.last; track++) {
      context.fillRect(0, Math.round(trackToY(view, track + 1)) - line, this.width, line);
    }
    context.restore();

    this.drawRuler(view, ticksPerQuarter, bar, loop);
  }

  private drawRuler(
    view: TimelineViewport,
    ticksPerQuarter: number,
    bar: number,
    loop: TimelineGridScene["loop"],
  ): void {
    const theme = this.theme;
    drawRuler(this.contexts.grid, {
      left: 0,
      width: this.width,
      height: RULER_HEIGHT,
      tickToX: (tick) => tickToX(view, tick),
      pixelsPerTick: view.pixelsPerTick,
      ticks: visibleTicks(view),
      ticksPerQuarter,
      bar,
      loop,
      line: theme.gridWidth,
      loopWeight: theme.clipWidth,
      font: theme.labelFont,
      colours: {
        background: theme.background,
        loopRegion: theme.loopRegion,
        edge: theme.rowLine,
        beatTick: theme.beatTick,
        barTick: theme.barTick,
        text: theme.rulerText,
        loop: theme.loop,
        loopOff: theme.loopOff,
      },
    });
  }

  drawClips({ view, clips }: TimelineClipsScene): void {
    const context = this.contexts.clips;
    context.clearRect(0, 0, this.width, this.height);
    context.save();
    context.beginPath();
    context.rect(0, RULER_HEIGHT, this.width, this.height - RULER_HEIGHT);
    context.clip();
    for (const clip of clips) this.drawClip(view, clip);
    context.restore();
  }

  /**
   * A clip's box, with its notes drawn small inside as lines, scaled to its
   * range of pitches. Its edge is ink, or the selected ink, heavier, while
   * it's selected.
   */
  private drawClip(view: TimelineViewport, clip: DrawnClip): void {
    const context = this.contexts.clips;
    const theme = this.theme;
    const full = clip.length * view.pixelsPerTick;
    const gap = full > 4 * CLIP_GAP ? CLIP_GAP : 0;
    const x = tickToX(view, clip.start) + gap;
    const y = trackToY(view, clip.track) + CLIP_INSET;
    const width = Math.max(1, full - 2 * gap);
    const height = TRACK_HEIGHT - 2 * CLIP_INSET - theme.gridWidth;

    context.fillStyle = clip.hovered ? theme.clipHover : theme.clipFill;
    context.fillRect(x, y, width, height);

    if (clip.notes.length > 0) {
      context.save();
      context.beginPath();
      context.rect(x, y, width, height);
      context.clip();
      const pitches = clip.high - clip.low + 1;
      const inner = height - 2 * PREVIEW_PADDING;
      const row = Math.min(MAX_PREVIEW_ROW, inner / pitches);
      // A narrow range sits in the middle rather than at the top.
      const top = y + PREVIEW_PADDING + (inner - row * pitches) / 2 + row / 2;
      // One path for all of them, so a busy clip is a single stroke.
      context.beginPath();
      for (const note of clip.notes) {
        const noteX = tickToX(view, note.start);
        const noteY = top + (clip.high - note.pitch) * row;
        context.moveTo(noteX, noteY);
        context.lineTo(noteX + Math.max(1, note.length * view.pixelsPerTick - 1), noteY);
      }
      context.strokeStyle = theme.clipNote;
      context.lineWidth = theme.clipNoteWidth;
      context.stroke();
      context.restore();
    }

    context.strokeStyle = clip.selected ? theme.selectedClipEdge : theme.clipEdge;
    context.lineWidth = clip.selected ? theme.selectedClipWidth : theme.clipWidth;
    const inset = context.lineWidth / 2;
    context.strokeRect(x + inset, y + inset, Math.max(0, width - 2 * inset), height - 2 * inset);
  }

  drawTop(view: TimelineViewport, playhead: number, drawing: Span | null, box: Rect | null): void {
    const context = this.contexts.top;
    context.clearRect(0, 0, this.width, this.height);
    // The clip being drawn and the selection box look alike, and stay below
    // the ruler.
    const shade = (x: number, y: number, width: number, height: number) => {
      context.save();
      context.beginPath();
      context.rect(0, RULER_HEIGHT, this.width, this.height - RULER_HEIGHT);
      context.clip();
      const { selectionBox, selectionBoxEdge, selectionBoxWidth } = this.theme;
      drawSelectionBox(context, { x, y, width, height }, selectionBox, selectionBoxEdge, selectionBoxWidth);
      context.restore();
    };
    if (drawing) {
      const x = tickToX(view, drawing.start);
      const y = trackToY(view, drawing.track) + CLIP_INSET;
      const width = Math.max(1, drawing.length * view.pixelsPerTick);
      shade(x, y, width, TRACK_HEIGHT - 2 * CLIP_INSET - this.theme.gridWidth);
    }
    if (box) shade(box.x, box.y, box.width, box.height);
    const x = Math.round(tickToX(view, playhead));
    if (x < 0 || x > this.width) return;
    drawPlayhead(context, x, this.theme.playheadWidth, this.height, this.theme.playhead);
  }
}
