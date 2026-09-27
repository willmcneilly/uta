import { describe, expect, it } from "vitest";
import type { ClipView, NoteView } from "../backend";
import { NoteIndex } from "./notes";

let nextId = 0;
const note = (start: number, length: number, pitch = 60, velocity = 100): NoteView => ({
  id: `n${nextId++}`,
  pitch,
  velocity,
  start,
  length,
});

const clip = (notes: NoteView[], start = 0, length = 15_360): ClipView => ({
  id: "clip",
  start,
  length,
  notes,
});

const starts = (index: NoteIndex, from: number, to: number, low = 0, high = 127) =>
  index.visible(from, to, low, high).map((n) => n.start);

describe("NoteIndex", () => {
  it("finds the notes overlapping a range of ticks, in order of start", () => {
    const index = new NoteIndex(clip([note(2000, 100), note(0, 100), note(960, 1200)]));
    expect(starts(index, 0, 15_360)).toEqual([0, 960, 2000]);
    // Starts before the range but runs into it.
    expect(starts(index, 1500, 1600)).toEqual([960]);
    // A note ending exactly at the range's start isn't in it; one starting at its end isn't either.
    expect(starts(index, 100, 960)).toEqual([]);
  });

  it("finds a long note that starts well before the range", () => {
    const index = new NoteIndex(
      clip([note(0, 15_000), ...Array.from({ length: 50 }, (_, i) => note(i * 240, 60))]),
    );
    expect(starts(index, 14_000, 14_500)).toEqual([0]);
  });

  it("leaves out pitches outside the range", () => {
    const index = new NoteIndex(clip([note(0, 100, 59), note(0, 100, 60), note(0, 100, 72)]));
    expect(index.visible(0, 100, 60, 71).map((n) => n.pitch)).toEqual([60]);
  });

  it("places notes after the clip's start, and marks those past its end", () => {
    const index = new NoteIndex(clip([note(0, 10), note(4000, 10)], 1000, 3840));
    const placed = index.visible(0, 1e9, 0, 127);
    expect(placed.map((n) => [n.start, n.outside])).toEqual([
      [1000, false],
      [5000, true],
    ]);
    expect(index.end).toBe(5010);
  });

  it("matches a plain scan over thousands of notes", () => {
    let seed = 1;
    const random = () => (seed = (seed * 16_807) % 2_147_483_647) / 2_147_483_647;
    const notes = Array.from({ length: 3000 }, () =>
      note(Math.floor(random() * 61_440), 1 + Math.floor(random() * 3840), Math.floor(random() * 128)),
    );
    const index = new NoteIndex(clip(notes, 0, 61_440));
    for (const [from, to, low, high] of [
      [0, 61_440, 0, 127],
      [10_000, 12_000, 50, 70],
      [60_000, 70_000, 0, 127],
      [3000, 3001, 100, 127],
    ]) {
      const expected = notes
        .filter((n) => n.start < to && n.start + n.length > from && n.pitch >= low && n.pitch <= high)
        .map((n) => n.id)
        .sort();
      const found = index
        .visible(from, to, low, high)
        .map((n) => n.id)
        .sort();
      expect(found).toEqual(expected);
    }
  });
});
