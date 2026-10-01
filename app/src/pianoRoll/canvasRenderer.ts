// The Canvas 2D renderer. Each layer draws only what's in view, and loops
// only over what fits on screen, so its work doesn't grow with the song.

import { type Theme, isBlackKey, octaveName, readTheme } from "./colours";
import type { PlacedNote } from "./notes";
import type { GridScene, Layers, NotesScene, PianoRollRenderer } from "./renderer";
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
        context.fillRect(area.x, Math.round(y + view.keyHeight) - 1, area.width, 1);
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
      context.fillRect(Math.round(tickToX(view, tick)), area.y, 1, area.height);
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

  /** The velocity lane's background, with bar and beat lines, and its label. */
  private drawVelocityLane(view: Viewport, ticksPerQuarter: number, bar: number): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const lane = velocityLane(view);
    const ticks = visibleTicks(view);
    context.fillStyle = theme.ruler;
    context.fillRect(0, lane.y, KEYBOARD_WIDTH, lane.height);
    context.fillStyle = theme.velocityLane;
    context.fillRect(lane.x, lane.y, lane.width, lane.height);
    context.fillStyle = theme.barLine;
    context.fillRect(0, lane.y, this.width, 1);

    context.save();
    context.beginPath();
    context.rect(lane.x, lane.y + 1, lane.width, lane.height - 1);
    context.clip();
    if (ticksPerQuarter * view.pixelsPerTick >= 8) {
      for (
        let tick = Math.floor(ticks.start / ticksPerQuarter) * ticksPerQuarter;
        tick < ticks.end;
        tick += ticksPerQuarter
      ) {
        context.fillStyle = tick % bar === 0 ? theme.barLine : theme.beatLine;
        context.fillRect(Math.round(tickToX(view, tick)), lane.y, 1, lane.height);
      }
    }
    context.restore();

    context.fillStyle = theme.rulerText;
    context.font = "10px -apple-system, BlinkMacSystemFont, sans-serif";
    context.textBaseline = "top";
    context.fillText("Velocity", 6, lane.y + 6);
  }

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

    context.save();
    context.beginPath();
    context.rect(area.x, 0, area.width, RULER_HEIGHT);
    context.clip();
    context.fillStyle = theme.ruler;
    context.fillRect(area.x, 0, area.width, RULER_HEIGHT);

    // The loop, as a band along the bottom of the ruler: grey while it's off.
    context.fillStyle = loopEnabled ? theme.loop : theme.barLine;
    const loopLeft = Math.max(area.x, tickToX(view, loopStart));
    const loopRight = Math.min(area.x + area.width, tickToX(view, loopEnd));
    if (loopRight > loopLeft) context.fillRect(loopLeft, RULER_HEIGHT - 4, loopRight - loopLeft, 4);

    // Beat ticks when they're far enough apart, and bar numbers.
    context.fillStyle = theme.rulerText;
    context.font = "11px -apple-system, BlinkMacSystemFont, sans-serif";
    context.textBaseline = "top";
    const beatPixels = ticksPerQuarter * view.pixelsPerTick;
    const labelEvery = Math.max(1, Math.ceil(MIN_BAR_LABEL_SPACING / (bar * view.pixelsPerTick)));
    if (beatPixels >= 6) {
      for (
        let tick = Math.floor(ticks.start / ticksPerQuarter) * ticksPerQuarter;
        tick < ticks.end;
        tick += ticksPerQuarter
      ) {
        if (tick % bar !== 0) {
          context.fillRect(Math.round(tickToX(view, tick)), RULER_HEIGHT - 9, 1, 5);
        }
      }
    }
    const barStep = bar * labelEvery;
    for (let tick = Math.floor(ticks.start / barStep) * barStep; tick < ticks.end; tick += barStep) {
      const x = Math.round(tickToX(view, tick));
      context.fillRect(x, 4, 1, RULER_HEIGHT - 8);
      context.fillText(String(tick / bar + 1), x + 4, 4);
    }
    context.restore();
  }

  private drawKeyboard(view: Viewport, low: number, high: number): void {
    const context = this.contexts.grid;
    const theme = this.theme;
    const area = noteArea(view);

    context.save();
    context.beginPath();
    context.rect(0, area.y, KEYBOARD_WIDTH, area.height);
    context.clip();
    context.fillStyle = theme.whiteKey;
    context.fillRect(0, area.y, KEYBOARD_WIDTH, area.height);
    context.font = "10px -apple-system, BlinkMacSystemFont, sans-serif";
    context.textBaseline = "middle";
    const blackWidth = Math.round(KEYBOARD_WIDTH * 0.6);
    for (let pitch = low; pitch <= high; pitch++) {
      const y = pitchToY(view, pitch);
      if (isBlackKey(pitch)) {
        context.fillStyle = theme.blackKey;
        context.fillRect(0, y, blackWidth, view.keyHeight);
      } else {
        context.fillStyle = theme.octaveLine;
        context.fillRect(0, Math.round(y + view.keyHeight) - 1, KEYBOARD_WIDTH, 1);
      }
      if (pitch % 12 === 0 && view.keyHeight >= 8) {
        context.fillStyle = theme.keyText;
        context.fillText(octaveName(pitch), blackWidth + 2, y + view.keyHeight / 2);
      }
    }
    context.fillStyle = theme.octaveLine;
    context.fillRect(KEYBOARD_WIDTH - 1, area.y, 1, area.height);
    context.restore();

    // The corner above the keyboard.
    context.fillStyle = theme.ruler;
    context.fillRect(0, 0, KEYBOARD_WIDTH, RULER_HEIGHT);
  }

  drawNotes({ view, notes, velocities, selected }: NotesScene): void {
    const context = this.contexts.notes;
    const colours = this.theme.noteByVelocity;
    const area = noteArea(view);
    context.clearRect(0, 0, this.width, this.height);
    context.save();
    context.beginPath();
    context.rect(area.x, area.y, area.width, area.height);
    context.clip();
    // A pixel's gap between neighbours, unless that would hide the note.
    const gap = view.keyHeight > 3 ? 1 : 0;
    for (const note of notes) {
      const x = tickToX(view, note.start);
      const y = pitchToY(view, note.pitch);
      const width = Math.max(1, note.length * view.pixelsPerTick - gap);
      context.globalAlpha = note.outside ? 0.35 : 1;
      context.fillStyle = colours[note.velocity];
      context.fillRect(x, y + gap, width, view.keyHeight - gap);
      if (selected.has(note.id)) {
        context.strokeStyle = this.theme.selectedNote;
        context.lineWidth = 2;
        context.strokeRect(x + 1, y + gap + 1, Math.max(0, width - 2), view.keyHeight - gap - 2);
      }
    }
    context.restore();
    this.drawVelocities(view, velocities, selected);
  }

  /** A bar for each note at its start, as tall as its velocity. Selected notes' bars go on top. */
  private drawVelocities(
    view: Viewport,
    notes: readonly PlacedNote[],
    selected: ReadonlySet<string>,
  ): void {
    const context = this.contexts.notes;
    const lane = velocityLane(view);
    const bottom = velocityToY(view, 0);
    context.save();
    context.beginPath();
    context.rect(lane.x, lane.y + 1, lane.width, lane.height - 1);
    context.clip();
    for (const onTop of [false, true]) {
      for (const note of notes) {
        if (selected.has(note.id) !== onTop) continue;
        const x = Math.round(tickToX(view, note.start));
        const y = Math.round(velocityToY(view, note.velocity));
        context.globalAlpha = note.outside ? 0.35 : 1;
        context.fillStyle = onTop
          ? this.theme.selectedNote
          : this.theme.noteByVelocity[note.velocity];
        context.fillRect(x, y, 2, bottom - y);
        context.fillRect(x - 2, y - 2, 6, 4);
      }
    }
    context.restore();
  }

  drawTop(view: Viewport, playhead: number, box: Rect | null): void {
    const context = this.contexts.top;
    const area = noteArea(view);
    context.clearRect(0, 0, this.width, this.height);
    if (box) {
      context.fillStyle = this.theme.selectionBox;
      context.fillRect(box.x, box.y, box.width, box.height);
      context.strokeStyle = this.theme.selectionBoxEdge;
      context.lineWidth = 1;
      context.strokeRect(box.x + 0.5, box.y + 0.5, box.width - 1, box.height - 1);
    }
    const x = Math.round(tickToX(view, playhead));
    if (x < area.x || x > area.x + area.width) return;
    context.fillStyle = this.theme.playhead;
    context.fillRect(x, 0, 1, this.height);
    // A marker in the ruler.
    context.beginPath();
    context.moveTo(x - 5, 0);
    context.lineTo(x + 6, 0);
    context.lineTo(x + 0.5, 7);
    context.closePath();
    context.fill();
  }
}
