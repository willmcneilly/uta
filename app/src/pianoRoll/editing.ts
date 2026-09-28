// Editing notes with the pointer: what's under it, and where a drag puts a
// note. Pure functions; the component turns their results into commands.

import type { NoteView } from "../backend";
import type { PlacedNote } from "./notes";
import { snapNearest } from "./snap";
import { PITCH_COUNT, type Viewport, pitchToY, tickToX, velocityPerPixel } from "./viewport";

/** Which part of a note the pointer is on: its body moves it, its ends resize it. */
export type NotePart = "body" | "start" | "end";

export interface Hit {
  note: PlacedNote;
  part: NotePart;
}

/** How far into a note, from each end, the pointer resizes it rather than moving it. */
export const EDGE_PIXELS = 6;

/**
 * The note under `x`, `y`, and which part of it. `notes` are in drawing
 * order, so where notes overlap, the one drawn last (on top) wins.
 */
export function hitTest(
  view: Viewport,
  notes: readonly PlacedNote[],
  x: number,
  y: number,
): Hit | null {
  for (let i = notes.length - 1; i >= 0; i--) {
    const note = notes[i];
    const left = tickToX(view, note.start);
    // As drawn: never narrower than a pixel.
    const width = Math.max(1, note.length * view.pixelsPerTick);
    const top = pitchToY(view, note.pitch);
    if (x < left || x >= left + width || y < top || y >= top + view.keyHeight) continue;
    // A narrow note keeps a middle third to move it by.
    const edge = Math.min(EDGE_PIXELS, width / 3);
    const part = x < left + edge ? "start" : x >= left + width - edge ? "end" : "body";
    return { note, part };
  }
  return null;
}

/**
 * One drag of one note. Drawing a note is resizing its end, straight after
 * placing it.
 */
export interface Drag {
  kind: "draw" | "move" | "start" | "end";
  /** The note as it was when the drag started. Its start is from its clip's start. */
  from: NoteView;
  /** Where the pointer went down: ticks from the start of the song, and a pitch. */
  tick: number;
  pitch: number;
}

/**
 * Where `drag` puts its note with the pointer at `tick` and `pitch`. Positions
 * snap to multiples of `step` ticks (1 with snapping off), and a note is never
 * shorter than one step or earlier than its clip's start.
 */
export function dragNote(
  drag: Drag,
  tick: number,
  pitch: number,
  step: number,
  clipStart: number,
): NoteView {
  const { from } = drag;
  const moved = tick - drag.tick;
  const start = clipStart + from.start;
  const end = start + from.length;
  switch (drag.kind) {
    case "move":
      return moveNotes([from], drag, tick, pitch, step, clipStart)[0];
    case "start": {
      const latest = Math.max(clipStart, end - step);
      const newStart = clamp(snapNearest(start + moved, step), clipStart, latest);
      return { ...from, start: newStart - clipStart, length: end - newStart };
    }
    case "draw":
    case "end": {
      const newEnd = Math.max(start + step, snapNearest(end + moved, step));
      return { ...from, length: newEnd - start };
    }
  }
}

/**
 * Where a move puts `notes`, all together, when `drag` (a move of one of
 * them) has the pointer at `tick` and `pitch`. The dragged note's start snaps
 * to the grid, and the rest keep their places relative to it. The move stops
 * where any note would go before its clip's start or off the keyboard, so
 * the notes never bunch up.
 */
export function moveNotes(
  notes: readonly NoteView[],
  drag: Drag,
  tick: number,
  pitch: number,
  step: number,
  clipStart: number,
): NoteView[] {
  if (notes.length === 0) return [];
  const start = clipStart + drag.from.start;
  const earliest = Math.min(...notes.map((note) => note.start));
  const lowest = Math.min(...notes.map((note) => note.pitch));
  const highest = Math.max(...notes.map((note) => note.pitch));
  // Clip-relative starts can't go below 0.
  const ticks = Math.max(-earliest, snapNearest(start + tick - drag.tick, step) - start);
  const pitches = clamp(pitch - drag.pitch, -lowest, PITCH_COUNT - 1 - highest);
  return notes.map((note) => ({ ...note, start: note.start + ticks, pitch: note.pitch + pitches }));
}

/** A note's bar in the velocity lane is this close to the pointer to be picked. */
export const VELOCITY_HIT_PIXELS = 4;

/**
 * The note whose velocity bar is under `x`, of `notes` (in drawing order).
 * Where bars overlap, as a chord's do, a selected note wins, then the one
 * drawn last.
 */
export function hitVelocity(
  view: Viewport,
  notes: readonly PlacedNote[],
  selected: ReadonlySet<string>,
  x: number,
): PlacedNote | null {
  let best: PlacedNote | null = null;
  for (const note of notes) {
    if (Math.abs(tickToX(view, note.start) - x) > VELOCITY_HIT_PIXELS) continue;
    if (!best || selected.has(note.id) || !selected.has(best.id)) best = note;
  }
  return best;
}

/**
 * `notes`' velocities after a drag in the velocity lane of `pixels` upwards
 * (negative is down). Every note changes by the same amount, kept from 1 to
 * 127, so a group keeps its shape until it reaches either end.
 */
export function dragVelocities(
  view: Viewport,
  notes: readonly NoteView[],
  pixels: number,
): NoteView[] {
  const change = Math.round(pixels * velocityPerPixel(view));
  return notes.map((note) => ({ ...note, velocity: clamp(note.velocity + change, 1, 127) }));
}

export function sameNote(a: NoteView, b: NoteView): boolean {
  return (
    a.id === b.id &&
    a.pitch === b.pitch &&
    a.velocity === b.velocity &&
    a.start === b.start &&
    a.length === b.length
  );
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
