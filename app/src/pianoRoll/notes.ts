import type { ClipView } from "../backend";
import type { Rows } from "./viewport";

/** A note placed on the song's timeline, ready to draw. */
export interface PlacedNote {
  id: string;
  pitch: number;
  velocity: number;
  /** In ticks from the start of the song. */
  start: number;
  length: number;
  /** The piano roll's row it's on: its pitch beside the keyboard, or its drum lane. */
  row: number;
  /** Past the clip's end: kept, but not played. */
  outside: boolean;
}

/**
 * A clip's notes sorted by start, so the ones in view can be found without
 * looking at them all, each on its row of `rows`.
 */
export class NoteIndex {
  private readonly notes: PlacedNote[];
  /** The longest note, which bounds how far back a note in view can start. */
  private readonly maxLength: number;

  constructor(clip: ClipView, rows: Rows) {
    this.notes = clip.notes
      .map((note) => ({
        id: note.id,
        pitch: note.pitch,
        velocity: note.velocity,
        start: clip.start + note.start,
        length: note.length,
        row: rows.row(note.pitch),
        outside: note.start >= clip.length,
      }))
      .sort((a, b) => a.start - b.start);
    this.maxLength = this.notes.reduce((longest, note) => Math.max(longest, note.length), 0);
  }

  get size(): number {
    return this.notes.length;
  }

  /** The last tick any note reaches. */
  get end(): number {
    return this.notes.reduce((end, note) => Math.max(end, note.start + note.length), 0);
  }

  /**
   * The notes with any part between the ticks `start` and `end` (exclusive)
   * on a row from `low` to `high`, in order of start.
   */
  visible(start: number, end: number, low: number, high: number): PlacedNote[] {
    // Nothing starting before this can reach `start`.
    let index = firstAtOrAfter(this.notes, start - this.maxLength);
    const found: PlacedNote[] = [];
    for (; index < this.notes.length; index++) {
      const note = this.notes[index];
      if (note.start >= end) break;
      if (note.start + note.length > start && note.row >= low && note.row <= high) {
        found.push(note);
      }
    }
    return found;
  }

  /**
   * The notes sounding at the tick `at` on a row from `low` to `high`:
   * those that have started and not yet ended. Notes past the clip's end
   * aren't played, so they never sound.
   */
  sounding(at: number, low: number, high: number): PlacedNote[] {
    let index = firstAtOrAfter(this.notes, at - this.maxLength);
    const found: PlacedNote[] = [];
    for (; index < this.notes.length; index++) {
      const note = this.notes[index];
      if (note.start > at) break;
      if (note.start + note.length > at && !note.outside && note.row >= low && note.row <= high) {
        found.push(note);
      }
    }
    return found;
  }
}

/** The index of the first note starting at or after `ticks`. */
function firstAtOrAfter(notes: readonly PlacedNote[], ticks: number): number {
  let low = 0;
  let high = notes.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (notes[middle].start < ticks) low = middle + 1;
    else high = middle;
  }
  return low;
}
