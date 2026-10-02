// Copying, pasting and duplicating clips: where the copies go. Pure
// functions; the component sends the result as one `paste_clips`, and Rust
// gives every note a new ID.

import type { ClipView } from "../backend";
import type { NewId } from "../pianoRoll/clipboard";

/** A clip and the index of its track in the order. */
export interface ClipOnTrack {
  track: number;
  clip: ClipView;
}

/** A copied clip, and how many tracks below the top one of those copied it was. */
export interface CopiedClip {
  below: number;
  clip: ClipView;
}

/** What was copied, keeping how the clips were laid out across tracks. */
export function copyClips(clips: readonly ClipOnTrack[]): CopiedClip[] {
  if (clips.length === 0) return [];
  const top = Math.min(...clips.map((c) => c.track));
  return clips.map(({ track, clip }) => ({ below: track - top, clip }));
}

/**
 * `copied` pasted on track `track` (an index of `trackCount`), with the
 * earliest of them at `at` ticks (already snapped). The top track's clips
 * land on `track`, and the rest keep their distance below it, or go on the
 * last track if that would be past it. Each gets a new ID.
 *
 * The first clip on `track` comes last, as the one to show in the Notes tab.
 */
export function pasteClips(
  copied: readonly CopiedClip[],
  at: number,
  track: number,
  trackCount: number,
  newId: NewId,
): ClipOnTrack[] {
  if (copied.length === 0) return [];
  const shift = at - Math.min(...copied.map((c) => c.clip.start));
  return [...copied]
    .sort((a, b) => b.below - a.below || b.clip.start - a.clip.start)
    .map(({ below, clip }) => ({
      track: Math.min(track + below, trackCount - 1),
      clip: { ...clip, id: newId(), start: clip.start + shift },
    }));
}

/**
 * A copy of `clips` straight after them, on the same tracks: moved on by
 * the time they span, from the first start to the last end. Each gets a new
 * ID, and they keep their order.
 */
export function duplicateClips(clips: readonly ClipOnTrack[], newId: NewId): ClipOnTrack[] {
  if (clips.length === 0) return [];
  const first = Math.min(...clips.map((c) => c.clip.start));
  const last = Math.max(...clips.map((c) => c.clip.start + c.clip.length));
  return clips.map(({ track, clip }) => ({
    track,
    clip: { ...clip, id: newId(), start: clip.start + last - first },
  }));
}
