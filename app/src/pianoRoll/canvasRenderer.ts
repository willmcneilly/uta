// The Canvas 2D renderer. Each layer draws only what's in view, and loops
// only over what fits on screen, so its work doesn't grow with the song.

import { type Theme, isBlackKey, octaveName, readTheme } from "./colours";
import type { PlacedNote } from "./notes";
import type { GridScene, Layers, PianoRollRenderer } from "./renderer";
import {
  KEYBOARD_WIDTH,
  RULER_HEIGHT,
  type Viewport,
  gridStep,
  noteArea,
  pitchToY,
  tickToX,
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

  drawGrid({ view, ticksPerQuarter, beatsPerBar, loopStart, loopEnd }: GridScene): void {
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

    // Outside the loop is shaded.
    context.fillStyle = theme.outsideLoop;
    const loopLeft = tickToX(view, loopStart);
    const loopRight = tickToX(view, loopEnd);
    if (loopLeft > area.x) context.fillRect(area.x, area.y, loopLeft - area.x, area.height);
    if (loopRight < area.x + area.width) {
      context.fillRect(loopRight, area.y, area.x + area.width - loopRight, area.height);
    }
    context.restore();

    this.drawRuler(view, ticksPerQuarter, bar, loopStart, loopEnd);
    this.drawKeyboard(view, low, high);
  }

  private drawRuler(
    view: Viewport,
    ticksPerQuarter: number,
    bar: number,
    loopStart: number,
    loopEnd: number,
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

    // The loop, as a band along the bottom of the ruler.
    context.fillStyle = theme.barLine;
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

  drawNotes(view: Viewport, notes: readonly PlacedNote[]): void {
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
    }
    context.restore();
  }

  drawTop(view: Viewport, playhead: number): void {
    const context = this.contexts.top;
    const area = noteArea(view);
    context.clearRect(0, 0, this.width, this.height);
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
