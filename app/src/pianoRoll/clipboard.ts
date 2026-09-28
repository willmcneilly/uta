// Copying, pasting and duplicating notes: where the new notes go. Pure
// functions; the component sends the result as one `AddNotes`.

import type { NoteView } from "../backend";

/** Makes a new note's permanent ID. Tests pass their own. */
export type NewId = () => string;

/**
 * `notes` pasted with the earliest of them at `at` ticks from the start of
 * the song (already snapped), keeping their places relative to each other.
 * Each gets a new ID. Nothing lands before the clip's start.
 */
export function pasteNotes(
  notes: readonly NoteView[],
  at: number,
  clipStart: number,
  newId: NewId,
): NoteView[] {
  if (notes.length === 0) return [];
  const earliest = Math.min(...notes.map((note) => note.start));
  const shift = Math.max(0, at - clipStart) - earliest;
  return notes.map((note) => ({ ...note, id: newId(), start: note.start + shift }));
}

/**
 * A copy of `notes` right after them: moved on by the time they span, from
 * the first start to the last end, rounded up to whole beats. So a bar
 * whose last note ends early still repeats on the next bar.
 */
export function duplicateNotes(
  notes: readonly NoteView[],
  ticksPerQuarter: number,
  newId: NewId,
): NoteView[] {
  if (notes.length === 0) return [];
  const first = Math.min(...notes.map((note) => note.start));
  const last = Math.max(...notes.map((note) => note.start + note.length));
  const shift = Math.ceil((last - first) / ticksPerQuarter) * ticksPerQuarter;
  return notes.map((note) => ({ ...note, id: newId(), start: note.start + shift }));
}
