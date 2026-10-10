// The Canvas 2D renderer. Each layer draws only what's in view, and loops
// only over what fits on screen, so its work doesn't grow with the song.

import { drawPlayhead, drawSelectionBox } from "../design/canvasMarks";
import { drawRuler } from "../design/ruler";
import { type Theme, isBlackKey, octaveName, readTheme, velocityAlpha } from "./colours";
import type { PlacedNote } from "./notes";
import type { GridScene, Layers, NotesScene, PianoRollRenderer, TopMarks } from "./renderer";
import {
  RULER_HEIGHT,
  type Rect,
  type Viewport,
  gridStep,
  noteArea,
  rowToY,
  tickToX,
  velocityLane,
  velocityToY,
  visibleRows,
  visibleTicks,
} from "./viewport";

/** Notes past the clip's end, and their velocity stems, are drawn this faint. */
const OUTSIDE_ALPHA = 0.35;
/** Space between the keyboard's right edge and its C labels. */
const KEY_LABEL_INSET = 6;
/** Where the velocity lane's label sits, from the lane's top left. */
const LANE_LABEL_INSET = 6;
/** The square on top of each velocity stem: this wide and tall (provisional: D-4). */
const VELOCITY_HEAD = 5;
/** What an empty clip says, in the middle of the notes (provisional: D-6). */
const EMPTY_HINT = "Click to draw a note";


export function createCanvas2DRenderer(layers: Layers): PianoRollRenderer | null {
  const grid = layers.grid.getContext("2d", { alpha: false });
  const notes = layers.notes.getContext("2d");
  const top = layers.top.getContext("2d");
  if (!grid || !notes || !top) return null;
  return new Canvas2DRenderer(layers, { grid, notes, top }, readTheme(layers.grid));
}

type Contexts = Record<keyof Layers, CanvasRenderingContext2D>;

class Canvas2DRenderer implements PianoRollRenderer {
  private width = 0;
  private height = 0;

  constructor(
    private readonly layers: Layers,
    private readonly contexts: Contexts,
    private readonly theme: Theme,
  ) {}

  resize(width: number, height: number, pixelRatio: number): void {
    this.width = width;
    this.height = height;
    for (const key of ["grid", "notes", "top"] as const) {
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
    loopStart,
    loopEnd,
    loopEnabled,
    clipStart,
    clipEnd,
  }: GridScene): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const area = noteArea(view);
    const { low, high } = visibleRows(view);
    const ticks = visibleTicks(view);
    const bar = ticksPerQuarter * beatsPerBar;
    const line = theme.gridWidth;

    context.fillStyle = theme.background;
    context.fillRect(0, 0, this.width, this.height);

    context.save();
    context.beginPath();
    context.rect(area.x, area.y, area.width, area.height);
    context.clip();

    // Rows: shaded to match the darker keys, a line under each B (the
    // octave boundary). Drum lanes have a line between each two, as faint
    // as the octave lines (provisional: D-23).
    for (let row = low; row <= high; row++) {
      const y = rowToY(view, row);
      if (view.rows.lanes) {
        if (row > 0) {
          context.fillStyle = theme.octaveLine;
          context.fillRect(area.x, Math.round(y + view.keyHeight) - line, area.width, line);
        }
        continue;
      }
      const pitch = view.rows.pitch(row);
      if (isBlackKey(pitch) === theme.shadeBlackKeyRows) {
        context.fillStyle = theme.rowShade;
        context.fillRect(area.x, y, area.width, view.keyHeight);
      }
      if (pitch % 12 === 0) {
        context.fillStyle = theme.octaveLine;
        context.fillRect(area.x, Math.round(y + view.keyHeight) - line, area.width, line);
      }
    }

    // Columns: bars, beats, and finer lines when there's room.
    const step = gridStep(view.pixelsPerTick, ticksPerQuarter, beatsPerBar);
    for (let tick = Math.floor(ticks.start / step) * step; tick < ticks.end; tick += step) {
      context.fillStyle =
        tick % bar === 0
          ? theme.barLine
          : tick % ticksPerQuarter === 0
            ? theme.beatLine
            : theme.subLine;
      context.fillRect(Math.round(tickToX(view, tick)), area.y, line, area.height);
    }

    // Outside the clip is shaded.
    context.fillStyle = theme.outsideClip;
    const clipLeft = tickToX(view, clipStart);
    const clipRight = tickToX(view, clipEnd);
    if (clipLeft > area.x) context.fillRect(area.x, area.y, clipLeft - area.x, area.height);
    if (clipRight < area.x + area.width) {
      context.fillRect(clipRight, area.y, area.x + area.width - clipRight, area.height);
    }
    context.restore();

    this.drawRuler(view, ticksPerQuarter, bar, loopStart, loopEnd, loopEnabled);
    this.drawKeyboard(view, low, high);
    this.drawVelocityLane(view, ticksPerQuarter, bar);
  }

  /**
   * The velocity lane: the same sheet as the notes, set apart by a faint
   * line, with fainter bar and beat lines than above it, and its label in
   * the corner (provisional: D-5).
   */
  private drawVelocityLane(view: Viewport, ticksPerQuarter: number, bar: number): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const lane = velocityLane(view);
    const ticks = visibleTicks(view);
    const line = theme.gridWidth;
    context.fillStyle = theme.background;
    context.fillRect(0, lane.y, this.width, lane.height);
    context.fillStyle = theme.edge;
    context.fillRect(0, lane.y, this.width, line);
    context.fillRect(view.rows.width - line, lane.y, line, lane.height);

    context.save();
    context.beginPath();
    context.rect(lane.x, lane.y + line, lane.width, lane.height - line);
    context.clip();
    if (ticksPerQuarter * view.pixelsPerTick >= 8) {
      for (
        let tick = Math.floor(ticks.start / ticksPerQuarter) * ticksPerQuarter;
        tick < ticks.end;
        tick += ticksPerQuarter
      ) {
        context.fillStyle = tick % bar === 0 ? theme.beatLine : theme.subLine;
        context.fillRect(Math.round(tickToX(view, tick)), lane.y, line, lane.height);
      }
    }
    context.restore();

    context.fillStyle = theme.velocityText;
    context.font = theme.labelFont;
    context.textBaseline = "top";
    context.fillText("Velocity", LANE_LABEL_INSET, lane.y + LANE_LABEL_INSET);
  }

  /** The ruler, as the timeline's, over the notes and not the keyboard's corner. */
  private drawRuler(
    view: Viewport,
    ticksPerQuarter: number,
    bar: number,
    loopStart: number,
    loopEnd: number,
    loopEnabled: boolean,
  ): void {
    const theme = this.theme;
    const area = noteArea(view);
    drawRuler(this.contexts.grid, {
      left: area.x,
      width: area.width,
      height: RULER_HEIGHT,
      tickToX: (tick) => tickToX(view, tick),
      pixelsPerTick: view.pixelsPerTick,
      ticks: visibleTicks(view),
      ticksPerQuarter,
      bar,
      loop: { start: loopStart, end: loopEnd, enabled: loopEnabled },
      line: theme.gridWidth,
      loopWeight: theme.loopWidth,
      font: theme.labelFont,
      colours: {
        background: theme.background,
        loopRegion: theme.loopRegion,
        edge: theme.edge,
        beatTick: theme.beatTick,
        barTick: theme.barTick,
        text: theme.rulerText,
        loop: theme.loop,
        loopOff: theme.loopOff,
      },
    });
  }

  /**
   * The keys, drawn like rows rather than a real keyboard: every key is
   * the keyboard's full width and one row tall, so each lines up with its
   * row in the notes. Black keys are always the darker ones, a line parts
   * two white keys that meet (E and F, B and C), and each C is named in the
   * second ink beside the notes (provisional: D-5). Beside drum lanes it's
   * the sheet with a line between each two, under the lanes' labels, which
   * are buttons over it (provisional: D-23).
   */
  private drawKeyboard(view: Viewport, low: number, high: number): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const area = noteArea(view);
    const width = view.rows.width;
    const lanes = view.rows.lanes !== null;
    const line = theme.gridWidth;

    context.save();
    context.beginPath();
    context.rect(0, area.y, width, area.height);
    context.clip();
    context.fillStyle = lanes ? theme.background : theme.whiteKey;
    context.fillRect(0, area.y, width, area.height);
    context.font = theme.labelFont;
    context.textBaseline = "middle";
    context.textAlign = "right";
    for (let row = low; row <= high; row++) {
      const y = rowToY(view, row);
      if (lanes) {
        if (row > 0) {
          context.fillStyle = theme.edge;
          context.fillRect(0, Math.round(y + view.keyHeight) - line, width, line);
        }
        continue;
      }
      const pitch = view.rows.pitch(row);
      if (isBlackKey(pitch)) {
        context.fillStyle = theme.blackKey;
        context.fillRect(0, y, width, view.keyHeight);
      } else if (!isBlackKey(pitch - 1)) {
        context.fillStyle = theme.keyLine;
        context.fillRect(0, Math.round(y + view.keyHeight) - line, width, line);
      }
      if (pitch % 12 === 0 && view.keyHeight >= 8) {
        context.fillStyle = theme.keyText;
        context.fillText(octaveName(pitch), width - KEY_LABEL_INSET, y + view.keyHeight / 2);
      }
    }
    context.textAlign = "left";
    context.fillStyle = theme.edge;
    context.fillRect(width - line, area.y, line, area.height);
    context.restore();

    // The corner above the keyboard, closed off by the ruler's foot and the keys' edge.
    context.fillStyle = theme.background;
    context.fillRect(0, 0, width, RULER_HEIGHT);
    context.fillStyle = theme.edge;
    context.fillRect(0, RULER_HEIGHT - line, width, line);
    context.fillRect(width - line, 0, line, RULER_HEIGHT);
  }

  /**
   * Each note filled with its ink as heavily as it's played, and outlined.
   * The outlines are one stroke for each kind of note, so a busy view is a
   * handful of strokes, not one per note (provisional: D-4).
   */
  drawNotes({ view, notes, velocities, selected, empty }: NotesScene): void {
    const context = this.contexts.notes;
    const theme = this.theme;
    const area = noteArea(view);
    context.clearRect(0, 0, this.width, this.height);
    context.save();
    context.beginPath();
    context.rect(area.x, area.y, area.width, area.height);
    context.clip();
    for (const note of notes) {
      const { x, y, width, height } = noteRect(view, note);
      context.globalAlpha = velocityAlpha(note.velocity) * (note.outside ? OUTSIDE_ALPHA : 1);
      context.fillStyle = selected.has(note.id) ? theme.selectedNote : theme.note;
      context.fillRect(x, y, width, height);
    }
    for (const outside of [false, true]) {
      for (const isSelected of [false, true]) {
        context.beginPath();
        let any = false;
        const line = isSelected ? theme.selectedNoteWidth : theme.noteWidth;
        for (const note of notes) {
          if (note.outside !== outside || selected.has(note.id) !== isSelected) continue;
          outline(context, noteRect(view, note), line);
          any = true;
        }
        if (!any) continue;
        context.globalAlpha = outside ? OUTSIDE_ALPHA : 1;
        context.strokeStyle = isSelected ? theme.selectedNote : theme.note;
        context.lineWidth = line;
        context.stroke();
      }
    }
    context.globalAlpha = 1;
    context.restore();

    if (empty) {
      context.fillStyle = theme.hintText;
      context.font = theme.labelFont;
      context.textAlign = "center";
      context.textBaseline = "middle";
      context.fillText(EMPTY_HINT, area.x + area.width / 2, area.y + area.height / 2);
      context.textAlign = "left";
    }
    this.drawVelocities(view, velocities, selected);
  }

  /**
   * A stem for each note at its start, as tall as its velocity, with a small
   * square on top, in ink. Selected notes' stems are heavier, in the selected
   * ink, and go on top (provisional: D-4).
   */
  private drawVelocities(
    view: Viewport,
    notes: readonly PlacedNote[],
    selected: ReadonlySet<string>,
  ): void {
    const context = this.contexts.notes;
    const theme = this.theme;
    const lane = velocityLane(view);
    context.save();
    context.beginPath();
    context.rect(lane.x, lane.y + theme.gridWidth, lane.width, lane.height - theme.gridWidth);
    context.clip();
    for (const isSelected of [false, true]) {
      for (const outside of [false, true]) {
        const line = isSelected ? theme.selectedNoteWidth : theme.noteWidth;
        context.beginPath();
        let any = false;
        for (const note of notes) {
          if (note.outside !== outside || selected.has(note.id) !== isSelected) continue;
          stem(context, view, note, line);
          any = true;
        }
        if (!any) continue;
        context.globalAlpha = outside ? OUTSIDE_ALPHA : 1;
        context.fillStyle = isSelected ? theme.selectedNote : theme.note;
        context.fill();
      }
    }
    context.globalAlpha = 1;
    context.restore();
  }

  drawTop(view: Viewport, playhead: number, box: Rect | null, marks: TopMarks): void {
    const context = this.contexts.top;
    const theme = this.theme;
    const area = noteArea(view);
    context.clearRect(0, 0, this.width, this.height);
    if (marks.sounding.length > 0 || marks.hovered) this.drawMarks(view, marks);
    if (box) {
      drawSelectionBox(context, box, theme.selectionBox, theme.selectionBoxEdge, theme.selectionBoxWidth);
    }
    const x = Math.round(tickToX(view, playhead));
    if (x < area.x || x > area.x + area.width) return;
    drawPlayhead(context, x, theme.playheadWidth, this.height, theme.playhead);
  }

  /**
   * The notes sounding now, filled and outlined in the live ink (a selected
   * one keeps its selected outline), and the note under the pointer
   * outlined heavier, with its velocity stem (provisional: D-4).
   */
  private drawMarks(view: Viewport, { sounding, hovered, selected }: TopMarks): void {
    const context = this.contexts.top;
    const theme = this.theme;
    const area = noteArea(view);
    context.save();
    context.beginPath();
    context.rect(area.x, area.y, area.width, area.height);
    context.clip();
    context.fillStyle = theme.soundingNote;
    for (const note of sounding) {
      const { x, y, width, height } = noteRect(view, note);
      context.fillRect(x, y, width, height);
    }
    // One stroke for the outlines of each kind, as on the notes layer.
    for (const isSelected of [false, true]) {
      const line = isSelected ? theme.selectedNoteWidth : theme.noteWidth;
      context.beginPath();
      let any = false;
      for (const note of sounding) {
        if (selected.has(note.id) !== isSelected) continue;
        outline(context, noteRect(view, note), line);
        any = true;
      }
      if (!any) continue;
      context.strokeStyle = isSelected ? theme.selectedNote : theme.soundingNote;
      context.lineWidth = line;
      context.stroke();
    }
    // A selected note is already drawn as heavy as a note gets.
    const hover = hovered && !selected.has(hovered.id) ? hovered : null;
    if (hover) {
      const line = theme.selectedNoteWidth;
      context.globalAlpha = hover.outside ? OUTSIDE_ALPHA : 1;
      context.beginPath();
      outline(context, noteRect(view, hover), line);
      context.strokeStyle = sounding.includes(hover) ? theme.soundingNote : theme.note;
      context.lineWidth = line;
      context.stroke();
    }
    context.restore();

    if (!hover) return;
    const lane = velocityLane(view);
    context.save();
    context.beginPath();
    context.rect(lane.x, lane.y + theme.gridWidth, lane.width, lane.height - theme.gridWidth);
    context.clip();
    context.globalAlpha = hover.outside ? OUTSIDE_ALPHA : 1;
    context.beginPath();
    stem(context, view, hover, theme.selectedNoteWidth);
    context.fillStyle = theme.note;
    context.fill();
    context.restore();
  }
}

/** Where a note is drawn: its row, less a pixel's gap between neighbours unless that would hide it. */
function noteRect(view: Viewport, note: PlacedNote): Rect {
  const gap = view.keyHeight > 3 ? 1 : 0;
  return {
    x: tickToX(view, note.start),
    y: rowToY(view, note.row) + gap,
    width: Math.max(1, note.length * view.pixelsPerTick - gap),
    height: view.keyHeight - gap,
  };
}

/** Adds the outline of `rect` to the path, drawn inside its edge at `line` pixels. */
function outline(context: CanvasRenderingContext2D, rect: Rect, line: number): void {
  context.rect(
    rect.x + line / 2,
    rect.y + line / 2,
    Math.max(0, rect.width - line),
    Math.max(0, rect.height - line),
  );
}

/** Adds a note's velocity stem and the square on top of it to the path, to fill. */
function stem(context: CanvasRenderingContext2D, view: Viewport, note: PlacedNote, line: number): void {
  const x = Math.round(tickToX(view, note.start));
  const y = Math.round(velocityToY(view, note.velocity));
  const bottom = velocityToY(view, 0);
  context.rect(x, y, line, bottom - y);
  const head = VELOCITY_HEAD + (line - 1);
  context.rect(x + line / 2 - head / 2, y - head / 2, head, head);
}
