import { describe, expect, it } from "vitest";
import type { ClipNotes, NoteView, Update } from "./backend";
import { trackView } from "./pianoRoll/testing";
import { EMPTY_CACHE, type ProjectCache, missingNotes, receive } from "./projectCache";

// The merge rule, as RFC-004 sets it out ("Where it lives in the UI").

const note = (id: string): NoteView => ({ id, pitch: 60, velocity: 100, start: 0, length: 480 });

/** A clip's notes at `revision`: one note, named after both. */
const notes = (clip: string, revision: number): ClipNotes => ({
  clip,
  revision,
  notes: [note(`${clip}-${revision}`)],
});

/**
 * An update whose one track holds `clips`, each at its revision, carrying
 * `sent`. The volume tells updates apart.
 */
function update(
  sequence: number,
  clips: Record<string, number>,
  sent: ClipNotes[] = [],
  volumeDb = -12,
): Update {
  const track = trackView("track-1", "Synth 1");
  return {
    sequence,
    outline: {
      volumeDb,
      minVolumeDb: -60,
      maxVolumeDb: 6,
      canUndo: false,
      canRedo: false,
      bpm: 120,
      minBpm: 20,
      maxBpm: 300,
      loopStart: 0,
      loopLength: 3840,
      loopEnabled: true,
      songEnd: 3840,
      ticksPerQuarter: 960,
      beatsPerBar: 4,
      synthLimits: {
        cutoffHz: [20, 20_000],
        resonance: [0, 1],
        envelopeSeconds: [0.001, 10],
        sustain: [0, 1],
      },
      mixerLimits: { volumeDb: [-60, 6], pan: [-1, 1] },
      maxTracks: 32,
      tracks: [
        {
          ...track,
          clips: Object.entries(clips).map(([id, notesRevision], index) => ({
            id,
            start: index * 3840,
            length: 3840,
            notesRevision,
          })),
        },
      ],
    },
    notes: sent,
  };
}

const take = (cache: ProjectCache, sent: Update) =>
  receive(cache, { type: "update", update: sent });
const fetched = (cache: ProjectCache, sent: ClipNotes[]) =>
  receive(cache, { type: "notes", notes: sent });

/** Each clip's drawn notes, by clip ID. */
function drawn(cache: ProjectCache): Record<string, NoteView[]> {
  const clips = cache.project?.tracks.flatMap((track) => track.clips) ?? [];
  return Object.fromEntries(clips.map((clip) => [clip.id, clip.notes]));
}

/** A cache holding clips a and b at revision 1. */
const started = () => take(EMPTY_CACHE, update(1, { a: 1, b: 1 }, [notes("a", 1), notes("b", 1)]));

describe("the project cache", () => {
  it("draws the outline with each clip's notes", () => {
    const cache = started();
    expect(cache.project?.volumeDb).toBe(-12);
    expect(drawn(cache)).toEqual({ a: [note("a-1")], b: [note("b-1")] });
  });

  it("keeps every held notes array, by identity, when an update sends none", () => {
    const before = started();
    const after = take(before, update(2, { a: 1, b: 1 }, [], -6));
    expect(after.project?.volumeDb).toBe(-6);
    expect(drawn(after).a).toBe(drawn(before).a);
    expect(drawn(after).b).toBe(drawn(before).b);
    expect(missingNotes(after)).toEqual([]);
  });

  it("replaces only the clip whose notes were sent", () => {
    const before = started();
    const after = take(before, update(2, { a: 2, b: 1 }, [notes("a", 2)]));
    expect(drawn(after).a).toEqual([note("a-2")]);
    expect(drawn(after).b).toBe(drawn(before).b);
  });

  it("drops clips the outline no longer has", () => {
    const after = take(started(), update(2, { b: 1 }));
    expect(drawn(after)).toEqual({ b: [note("b-1")] });
    expect([...after.notes.keys()]).toEqual(["b"]);
  });

  it("ignores an update older than the latest one taken", () => {
    const newer = take(started(), update(3, { a: 2, b: 1 }, [notes("a", 2)], -3));
    const late = take(newer, update(2, { a: 1, b: 1 }, [], -20));
    expect(late).toBe(newer);
    expect(late.project?.volumeDb).toBe(-3);
  });

  it("doesn't draw held notes whose revision isn't the outline's, and wants them fetched", () => {
    // The update that sent a's revision 2 was ignored, because this one,
    // built after it, arrived first.
    const cache = take(started(), update(3, { a: 2, b: 1 }));
    expect(drawn(cache).a).toEqual([]);
    expect(missingNotes(cache).map((clip) => [clip.id, clip.notesRevision])).toEqual([["a", 2]]);

    const fixed = fetched(cache, [notes("a", 2)]);
    expect(drawn(fixed).a).toEqual([note("a-2")]);
    expect(drawn(fixed).b).toBe(drawn(cache).b);
    expect(missingNotes(fixed)).toEqual([]);
  });

  it("wants every clip fetched after a reload, when Rust thinks it sent them all", () => {
    const cache = take(EMPTY_CACHE, update(7, { a: 4, b: 5 }));
    expect(drawn(cache)).toEqual({ a: [], b: [] });
    expect(missingNotes(cache).map((clip) => clip.id)).toEqual(["a", "b"]);
    const fixed = fetched(cache, [notes("a", 4), notes("b", 5)]);
    expect(drawn(fixed)).toEqual({ a: [note("a-4")], b: [note("b-5")] });
  });

  it("ignores fetched notes for a clip that's gone, or older than the ones held", () => {
    const cache = take(started(), update(2, { a: 2, b: 1 }, [notes("a", 2)]));
    expect(fetched(cache, [notes("gone", 9), notes("a", 1)])).toBe(cache);
  });

  it("holds fetched notes newer than the outline until an update names their revision", () => {
    // A change happened after this outline was built, and before the fetch.
    const cache = take(started(), update(2, { a: 2, b: 1 }));
    const ahead = fetched(cache, [notes("a", 3)]);
    expect(drawn(ahead).a).toEqual([]);
    const caughtUp = take(ahead, update(3, { a: 3, b: 1 }));
    expect(drawn(caughtUp).a).toEqual([note("a-3")]);
    expect(missingNotes(caughtUp)).toEqual([]);
  });
});
