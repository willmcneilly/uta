// The project as the UI holds it: the latest outline Rust sent, and the
// notes it sent for each clip, kept until Rust says they changed (RFC-004,
// "Where it lives in the UI"). It's like an HTTP cache with ETags: the
// outline is the index, each clip's notes revision is the ETag.
//
// The reducer only merges what Rust sent. It never knows what a command did,
// and never works out project data itself, so adding a command never
// touches it.

import type {
  ClipNotes,
  ClipOutline,
  ClipView,
  NoteView,
  Outline,
  ProjectView,
  Update,
} from "./backend";

export interface ProjectCache {
  /** The sequence number of the latest update taken, or -1 before the first. */
  sequence: number;
  outline: Outline | null;
  /** The notes held for each clip in the outline, by clip ID, at the revision they were sent. */
  notes: ReadonlyMap<string, ClipNotes>;
  /** What the UI draws: the outline, with each clip's notes if they're held at its revision. */
  project: ProjectView | null;
}

/** What Rust sent: an update, or clips' notes, fetched with `getNotes`. */
export type Received = { type: "update"; update: Update } | { type: "notes"; notes: ClipNotes[] };

export const EMPTY_CACHE: ProjectCache = {
  sequence: -1,
  outline: null,
  notes: new Map(),
  project: null,
};

/** A clip's notes while the UI doesn't hold them at its revision. */
const NO_NOTES: NoteView[] = [];

/** Merges what Rust sent into `cache`. */
export function receive(cache: ProjectCache, received: Received): ProjectCache {
  return received.type === "update"
    ? takeUpdate(cache, received.update)
    : takeNotes(cache, received.notes);
}

/**
 * The merge rule:
 * 1. ignore an update older than the latest one taken;
 * 2. take its outline whole;
 * 3. for each clip, use the notes just sent, or keep the ones held (drawn
 *    only if their revision matches, and fetched again otherwise);
 * 4. drop clips the outline no longer has.
 */
function takeUpdate(cache: ProjectCache, update: Update): ProjectCache {
  if (update.sequence <= cache.sequence) return cache;
  const sent = new Map(update.notes.map((notes) => [notes.clip, notes]));
  const notes = new Map<string, ClipNotes>();
  for (const clip of clips(update.outline)) {
    const held = sent.get(clip.id) ?? cache.notes.get(clip.id);
    if (held) notes.set(clip.id, held);
  }
  return build(update.sequence, update.outline, notes);
}

/**
 * Keeps fetched notes for each clip still in the outline, if they're newer
 * than any held. They may be newer than the outline too, if a change
 * happened since: they're drawn once an update names their revision.
 */
function takeNotes(cache: ProjectCache, fetched: ClipNotes[]): ProjectCache {
  if (!cache.outline) return cache;
  const inOutline = new Set(clips(cache.outline).map((clip) => clip.id));
  const notes = new Map(cache.notes);
  let changed = false;
  for (const clip of fetched) {
    const held = notes.get(clip.clip);
    if (!inOutline.has(clip.clip) || (held && held.revision >= clip.revision)) continue;
    notes.set(clip.clip, clip);
    changed = true;
  }
  return changed ? build(cache.sequence, cache.outline, notes) : cache;
}

function build(
  sequence: number,
  outline: Outline,
  notes: ReadonlyMap<string, ClipNotes>,
): ProjectCache {
  const project: ProjectView = {
    ...outline,
    tracks: outline.tracks.map((track) => ({
      ...track,
      clips: track.clips.map((clip): ClipView => {
        const held = notes.get(clip.id);
        return {
          id: clip.id,
          start: clip.start,
          length: clip.length,
          // The same array as before when unchanged, so views can skip it.
          notes: held?.revision === clip.notesRevision ? held.notes : NO_NOTES,
        };
      }),
    })),
  };
  return { sequence, outline, notes, project };
}

/** The clips in the outline whose notes aren't held at its revision: to fetch with `getNotes`. */
export function missingNotes(cache: ProjectCache): ClipOutline[] {
  if (!cache.outline) return [];
  return clips(cache.outline).filter(
    (clip) => cache.notes.get(clip.id)?.revision !== clip.notesRevision,
  );
}

/** Whether `clip` is in the cache's outline. */
export function inOutline(cache: ProjectCache, clip: string): boolean {
  return cache.outline !== null && clips(cache.outline).some((c) => c.id === clip);
}

function clips(outline: Outline): ClipOutline[] {
  return outline.tracks.flatMap((track) => track.clips);
}
