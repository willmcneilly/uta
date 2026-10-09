// The timeline's Canvas 2D renderer. Each layer draws only what's in view,
// and loops only over what fits on screen, so its work doesn't grow with the
// song.

import { readColours } from "../design/readColours";
import { readLineWidths } from "../design/readTokens";
import { type Theme, readTheme } from "../pianoRoll/colours";
import type { Rect } from "../pianoRoll/viewport";
import type { Span } from "./editing";
import { timelineGridStep } from "./snap";
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

/** Bar numbers are at least this far apart. */
const MIN_BAR_LABEL_SPACING = 32;
/** Space between a clip and the edges of its track's row. */
const CLIP_INSET = 3;
/** Space between a clip's edge and its notes. */
const PREVIEW_PADDING = 6;
/** The tallest a note is drawn in a preview, however few pitches the clip spans. */
const MAX_PREVIEW_ROW = 6;

interface ClipTheme {
  fill: string;
  edge: string;
  note: string;
  selectedEdge: string;
  selectedTrack: string;
  belowTracks: string;
  /** Line weights for a clip's edge, in CSS pixels. */
  edgeWidth: number;
  selectedEdgeWidth: number;
}

function readClipTheme(element: Element): ClipTheme {
  const c = readColours(element);
  const widths = readLineWidths(element);
  return {
    fill: c.line,
    edge: c.ink2,
    note: c.ink,
    selectedEdge: c.selected,
    selectedTrack: c.selectedWash,
    belowTracks: c.paper,
    edgeWidth: widths.strokeClip,
    selectedEdgeWidth: widths.strokeClipSelected,
  };
}

export function createTimelineRenderer(layers: TimelineLayers): TimelineRenderer | null {
  const grid = layers.grid.getContext("2d", { alpha: false });
  const clips = layers.clips.getContext("2d");
  const top = layers.top.getContext("2d");
  if (!grid || !clips || !top) return null;
  return new Canvas2DTimelineRenderer(
    layers,
    { grid, clips, top },
    readTheme(layers.grid),
    readClipTheme(layers.grid),
  );
}

type Contexts = Record<keyof TimelineLayers, CanvasRenderingContext2D>;

class Canvas2DTimelineRenderer implements TimelineRenderer {
  private width = 0;
  private height = 0;

  constructor(
    private readonly layers: TimelineLayers,
    private readonly contexts: Contexts,
    private readonly theme: Theme,
    private readonly clipTheme: ClipTheme,
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
      context.fillStyle = this.clipTheme.belowTracks;
      context.fillRect(0, tracksBottom, this.width, this.height - tracksBottom);
    }
    if (selectedTrack !== null && selectedTrack >= tracks.first && selectedTrack <= tracks.last) {
      context.fillStyle = this.clipTheme.selectedTrack;
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
    context.fillStyle = theme.barLine;
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
    const context = this.contexts.grid;
    const theme = this.theme;
    const ticks = visibleTicks(view);
    const line = theme.gridWidth;
    context.fillStyle = theme.ruler;
    context.fillRect(0, 0, this.width, RULER_HEIGHT);

    // The loop region: shaded, with a band along the bottom, while it's on;
    // just a grey band while it's off.
    const loopLeft = Math.max(0, Math.round(tickToX(view, loop.start)));
    const loopRight = Math.min(this.width, Math.round(tickToX(view, loop.end)));
    if (loopRight > loopLeft) {
      if (loop.enabled) {
        context.fillStyle = theme.loopRegion;
        context.fillRect(loopLeft, 0, loopRight - loopLeft, RULER_HEIGHT);
      }
      context.fillStyle = loop.enabled ? theme.loop : theme.barLine;
      context.fillRect(loopLeft, RULER_HEIGHT - 4, loopRight - loopLeft, 4);
    }
    context.fillStyle = theme.barLine;
    context.fillRect(0, RULER_HEIGHT - line, this.width, line);

    // Beat ticks when they're far enough apart, and bar numbers.
    context.fillStyle = theme.rulerText;
    context.font = theme.labelFont;
    context.textBaseline = "top";
    if (ticksPerQuarter * view.pixelsPerTick >= 6) {
      for (
        let tick = Math.floor(ticks.start / ticksPerQuarter) * ticksPerQuarter;
        tick < ticks.end;
        tick += ticksPerQuarter
      ) {
        if (tick % bar !== 0) {
          context.fillRect(Math.round(tickToX(view, tick)), RULER_HEIGHT - 9, line, 5);
        }
      }
    }
    const labelEvery = Math.max(1, Math.ceil(MIN_BAR_LABEL_SPACING / (bar * view.pixelsPerTick)));
    const barStep = bar * labelEvery;
    for (
      let tick = Math.floor(ticks.start / barStep) * barStep;
      tick < ticks.end;
      tick += barStep
    ) {
      const x = Math.round(tickToX(view, tick));
      context.fillRect(x, 4, line, RULER_HEIGHT - 8);
      context.fillText(String(tick / bar + 1), x + 4, 4);
    }
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

  /** A clip's box, with its notes drawn small inside, scaled to its range of pitches. */
  private drawClip(view: TimelineViewport, clip: DrawnClip): void {
    const context = this.contexts.clips;
    const theme = this.clipTheme;
    const x = tickToX(view, clip.start);
    const y = trackToY(view, clip.track) + CLIP_INSET;
    const width = Math.max(1, clip.length * view.pixelsPerTick);
    const height = TRACK_HEIGHT - 2 * CLIP_INSET - this.theme.gridWidth;

    context.fillStyle = theme.fill;
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
      const top = y + PREVIEW_PADDING + (inner - row * pitches) / 2;
      context.fillStyle = theme.note;
      for (const note of clip.notes) {
        const noteX = tickToX(view, note.start);
        const noteY = top + (clip.high - note.pitch) * row;
        const noteWidth = Math.max(1, note.length * view.pixelsPerTick - 1);
        context.fillRect(noteX, noteY, noteWidth, Math.max(1, row - (row > 3 ? 1 : 0)));
      }
      context.restore();
    }

    context.strokeStyle = clip.selected ? theme.selectedEdge : theme.edge;
    context.lineWidth = clip.selected ? theme.selectedEdgeWidth : theme.edgeWidth;
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
      context.fillStyle = this.theme.selectionBox;
      context.fillRect(x, y, width, height);
      const line = this.theme.selectionBoxWidth;
      context.strokeStyle = this.theme.selectionBoxEdge;
      context.lineWidth = line;
      context.strokeRect(x + line / 2, y + line / 2, width - line, height - line);
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
    context.fillStyle = this.theme.playhead;
    context.fillRect(x, 0, this.theme.playheadWidth, this.height);
    // A marker in the ruler, centred on the line.
    const middle = x + this.theme.playheadWidth / 2;
    context.beginPath();
    context.moveTo(middle - 5.5, 0);
    context.lineTo(middle + 5.5, 0);
    context.lineTo(middle, 7);
    context.closePath();
    context.fill();
  }
}
