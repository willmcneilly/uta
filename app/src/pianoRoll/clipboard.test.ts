import { describe, expect, it } from "vitest";
import type { NoteView } from "../backend";
import { duplicateNotes, pasteNotes } from "./clipboard";

const n = (id: string, pitch: number, start: number, length = 240): NoteView => ({
  id,
  pitch,
  velocity: 90,
  start,
  length,
});

/** IDs "new-1", "new-2"… */
const ids = () => {
  let count = 0;
  return () => `new-${++count}`;
};

describe("pasting", () => {
  const copied = [n("a", 60, 1200), n("b", 64, 960)];

  it("puts the earliest note at the paste point, keeping the rest in place around it", () => {
    expect(pasteNotes(copied, 7680, 0, ids())).toEqual([
      n("new-1", 60, 7920),
      n("new-2", 64, 7680),
    ]);
  });

  it("measures the paste point from the song's start, and keeps it inside the clip", () => {
    expect(pasteNotes(copied, 3840 + 480, 3840, ids()).map((note) => note.start)).toEqual([
      720, 480,
    ]);
    expect(pasteNotes(copied, 0, 3840, ids()).map((note) => note.start)).toEqual([240, 0]);
  });

  it("pastes nothing when nothing was copied", () => {
    expect(pasteNotes([], 960, 0, ids())).toEqual([]);
  });
});

describe("duplicating", () => {
  it("repeats a bar's notes on the next bar, even if the last ends early", () => {
    // From 0 to 3360: rounded up to 4 beats.
    const bar = [n("a", 60, 0, 960), n("b", 62, 1920, 480), n("c", 64, 2880, 480)];
    expect(duplicateNotes(bar, 960, ids())).toEqual([
      n("new-1", 60, 3840, 960),
      n("new-2", 62, 5760, 480),
      n("new-3", 64, 6720, 480),
    ]);
  });

  it("moves on at least a beat", () => {
    expect(duplicateNotes([n("a", 60, 1000, 100)], 960, ids())).toEqual([
      n("new-1", 60, 1960, 100),
    ]);
  });

  it("duplicates nothing when nothing is selected", () => {
    expect(duplicateNotes([], 960, ids())).toEqual([]);
  });
});
