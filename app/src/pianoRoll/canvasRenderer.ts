// The Canvas 2D renderer. Each layer draws only what's in view, and loops
// only over what fits on screen, so its work doesn't grow with the song.

import { type Theme, isBlackKey, octaveName, readTheme, velocityAlpha } from "./colours";
import type { PlacedNote } from "./notes";
import type { GridScene, Layers, NotesScene, PianoRollRenderer, TopMarks } from "./renderer";
import {
  KEYBOARD_WIDTH,
  RULER_HEIGHT,
  type Rect,
  type Viewport,
  gridStep,
  noteArea,
  pitchToY,
  tickToX,
  velocityLane,
  velocityToY,
  visiblePitches,
  visibleTicks,
} from "./viewport";

/** Bar numbers are at least this far apart. */
const MIN_BAR_LABEL_SPACING = 32;
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

/** The loop's dimension line, as on the timeline (provisional: D-2): this far above the ruler's foot. */
const LOOP_LINE_RAISE = 5;
/** Its end ticks run from here to the bottom of the ruler. */
const LOOP_TICK_HEIGHT = 10;
/** Its arrowheads, and the narrowest loop that has room for them. */
const LOOP_ARROW_LENGTH = 5;
const LOOP_ARROW_HALF_WIDTH = 2.5;
const LOOP_ARROWS_MIN_WIDTH = 4 * LOOP_ARROW_LENGTH;
/** The ruler's bar ticks and beat ticks, up from its bottom edge. */
const BAR_TICK_HEIGHT = 8;
const BEAT_TICK_HEIGHT = 4;

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
    const { low, high } = visiblePitches(view);
    const ticks = visibleTicks(view);
    const bar = ticksPerQuarter * beatsPerBar;
    const line = theme.gridWidth;

    context.fillStyle = theme.background;
    context.fillRect(0, 0, this.width, this.height);

    context.save();
    context.beginPath();
    context.rect(area.x, area.y, area.width, area.height);
    context.clip();

    // Rows: black keys shaded, a line under each B (the octave boundary).
    for (let pitch = low; pitch <= high; pitch++) {
      const y = pitchToY(view, pitch);
      if (isBlackKey(pitch)) {
        context.fillStyle = theme.blackKeyRow;
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
    context.fillRect(KEYBOARD_WIDTH - line, lane.y, line, lane.height);

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

  /**
   * The ruler, as the timeline's: ticks standing on its foot, faint for
   * beats and ink-3 for bars, bar numbers in the second ink, and the loop
   * as a dimension line.
   */
  private drawRuler(
    view: Viewport,
    ticksPerQuarter: number,
    bar: number,
    loopStart: number,
    loopEnd: number,
    loopEnabled: boolean,
  ): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const area = noteArea(view);
    const ticks = visibleTicks(view);
    const line = theme.gridWidth;

    context.save();
    context.beginPath();
    context.rect(area.x, 0, area.width, RULER_HEIGHT);
    context.clip();
    context.fillStyle = theme.background;
    context.fillRect(area.x, 0, area.width, RULER_HEIGHT);
    const loopLeft = Math.max(area.x, Math.round(tickToX(view, loopStart)));
    const loopRight = Math.min(area.x + area.width, Math.round(tickToX(view, loopEnd)));
    if (loopEnabled && loopRight > loopLeft) {
      context.fillStyle = theme.loopRegion;
      context.fillRect(loopLeft, 0, loopRight - loopLeft, RULER_HEIGHT);
    }
    context.fillStyle = theme.edge;
    context.fillRect(area.x, RULER_HEIGHT - line, area.width, line);

    // Beat ticks when they're far enough apart, and bar ticks with their
    // numbers, all standing on the ruler's bottom edge.
    const bottom = RULER_HEIGHT - line;
    if (ticksPerQuarter * view.pixelsPerTick >= 6) {
      context.fillStyle = theme.beatTick;
      for (
        let tick = Math.floor(ticks.start / ticksPerQuarter) * ticksPerQuarter;
        tick < ticks.end;
        tick += ticksPerQuarter
      ) {
        if (tick % bar !== 0) {
          context.fillRect(Math.round(tickToX(view, tick)), bottom - BEAT_TICK_HEIGHT, line, BEAT_TICK_HEIGHT);
        }
      }
    }
    context.font = theme.labelFont;
    context.textBaseline = "top";
    const labelEvery = Math.max(1, Math.ceil(MIN_BAR_LABEL_SPACING / (bar * view.pixelsPerTick)));
    const barStep = bar * labelEvery;
    for (let tick = Math.floor(ticks.start / barStep) * barStep; tick < ticks.end; tick += barStep) {
      const x = Math.round(tickToX(view, tick));
      context.fillStyle = theme.barTick;
      context.fillRect(x, bottom - BAR_TICK_HEIGHT, line, BAR_TICK_HEIGHT);
      context.fillStyle = theme.rulerText;
      context.fillText(String(tick / bar + 1), x + 4, 4);
    }
    this.drawLoop(view, loopStart, loopEnd, loopEnabled);
    context.restore();
  }

  /**
   * The loop region, drawn like a dimension line on a drawing: a line from
   * its start to its end with an arrowhead and a tick at each, in ink while
   * the loop is on and faint while it's off, as on the timeline
   * (provisional: D-2). The ruler's clip keeps it off the keyboard's corner.
   */
  private drawLoop(view: Viewport, loopStart: number, loopEnd: number, enabled: boolean): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const start = Math.round(tickToX(view, loopStart));
    const end = Math.round(tickToX(view, loopEnd));
    if (end <= start) return;
    const weight = theme.loopWidth;
    const y = RULER_HEIGHT - LOOP_LINE_RAISE;
    const middle = y + weight / 2;
    context.fillStyle = enabled ? theme.loop : theme.loopOff;
    context.fillRect(start, y, end - start, weight);
    // The end ticks stand on the bar lines the loop starts and ends on, inside it.
    context.fillRect(start, RULER_HEIGHT - LOOP_TICK_HEIGHT, weight, LOOP_TICK_HEIGHT);
    context.fillRect(end - weight, RULER_HEIGHT - LOOP_TICK_HEIGHT, weight, LOOP_TICK_HEIGHT);
    if (end - start < LOOP_ARROWS_MIN_WIDTH) return;
    context.beginPath();
    context.moveTo(start + weight, middle);
    context.lineTo(start + weight + LOOP_ARROW_LENGTH, middle - LOOP_ARROW_HALF_WIDTH);
    context.lineTo(start + weight + LOOP_ARROW_LENGTH, middle + LOOP_ARROW_HALF_WIDTH);
    context.closePath();
    context.moveTo(end - weight, middle);
    context.lineTo(end - weight - LOOP_ARROW_LENGTH, middle - LOOP_ARROW_HALF_WIDTH);
    context.lineTo(end - weight - LOOP_ARROW_LENGTH, middle + LOOP_ARROW_HALF_WIDTH);
    context.closePath();
    context.fill();
  }

  /**
   * The keys, drawn like rows rather than a real keyboard: every key is
   * the keyboard's full width and one row tall, so each lines up with its
   * row in the notes. Black keys are ink-3, a faint line parts two white
   * keys that meet (E and F, B and C), and each C is named in the second
   * ink beside the notes (provisional: D-5).
   */
  private drawKeyboard(view: Viewport, low: number, high: number): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const area = noteArea(view);
    const line = theme.gridWidth;

    context.save();
    context.beginPath();
    context.rect(0, area.y, KEYBOARD_WIDTH, area.height);
    context.clip();
    context.fillStyle = theme.background;
    context.fillRect(0, area.y, KEYBOARD_WIDTH, area.height);
    context.font = theme.labelFont;
    context.textBaseline = "middle";
    context.textAlign = "right";
    for (let pitch = low; pitch <= high; pitch++) {
      const y = pitchToY(view, pitch);
      if (isBlackKey(pitch)) {
        context.fillStyle = theme.blackKey;
        context.fillRect(0, y, KEYBOARD_WIDTH, view.keyHeight);
      } else if (!isBlackKey(pitch - 1)) {
        context.fillStyle = theme.keyLine;
        context.fillRect(0, Math.round(y + view.keyHeight) - line, KEYBOARD_WIDTH, line);
      }
      if (pitch % 12 === 0 && view.keyHeight >= 8) {
        context.fillStyle = theme.keyText;
        context.fillText(octaveName(pitch), KEYBOARD_WIDTH - KEY_LABEL_INSET, y + view.keyHeight / 2);
      }
    }
    context.textAlign = "left";
    context.fillStyle = theme.edge;
    context.fillRect(KEYBOARD_WIDTH - line, area.y, line, area.height);
    context.restore();

    // The corner above the keyboard, closed off by the ruler's foot and the keys' edge.
    context.fillStyle = theme.background;
    context.fillRect(0, 0, KEYBOARD_WIDTH, RULER_HEIGHT);
    context.fillStyle = theme.edge;
    context.fillRect(0, RULER_HEIGHT - line, KEYBOARD_WIDTH, line);
    context.fillRect(KEYBOARD_WIDTH - line, 0, line, RULER_HEIGHT);
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
      context.fillStyle = theme.selectionBox;
      context.fillRect(box.x, box.y, box.width, box.height);
      const line = theme.selectionBoxWidth;
      context.strokeStyle = theme.selectionBoxEdge;
      context.lineWidth = line;
      context.strokeRect(box.x + line / 2, box.y + line / 2, box.width - line, box.height - line);
    }
    const x = Math.round(tickToX(view, playhead));
    if (x < area.x || x > area.x + area.width) return;
    context.fillStyle = theme.playhead;
    context.fillRect(x, 0, theme.playheadWidth, this.height);
    // A marker in the ruler, centred on the line.
    const middle = x + theme.playheadWidth / 2;
    context.beginPath();
    context.moveTo(middle - 5.5, 0);
    context.lineTo(middle + 5.5, 0);
    context.lineTo(middle, 7);
    context.closePath();
    context.fill();
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
    y: pitchToY(view, note.pitch) + gap,
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
