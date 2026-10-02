import { describe, expect, it } from "vitest";
import type { ClipView, NoteView } from "../backend";
import { copyClips, duplicateClips, pasteClips } from "./clipboard";

const BAR = 3840;

const note: NoteView = {
  id: "n",
  pitch: 60,
  velocity: 100,
  start: 0,
  length: 480,
};

const c = (id: string, start: number, length = BAR): ClipView => ({
  id,
  start,
  length,
  notes: [note],
});

/** IDs "new-1", "new-2"… */
const ids = () => {
  let count = 0;
  return () => `new-${++count}`;
};

/** Each result as its track, ID and start. */
const placed = (clips: { track: number; clip: ClipView }[]) =>
  clips.map(({ track, clip }) => [track, clip.id, clip.start]);

describe("copying", () => {
  it("keeps each clip's distance below the top track copied from", () => {
    expect(
      copyClips([
        { track: 3, clip: c("a", 0) },
        { track: 1, clip: c("b", BAR) },
      ]),
    ).toEqual([
      { below: 2, clip: c("a", 0) },
      { below: 0, clip: c("b", BAR) },
    ]);
    expect(copyClips([])).toEqual([]);
  });
});

describe("pasting", () => {
  it("puts the earliest clip at the paste point on the track, keeping the rest in place", () => {
    const copied = copyClips([
      { track: 0, clip: c("a", BAR) },
      { track: 0, clip: c("b", 3 * BAR) },
    ]);
    // The first clip on the track comes last, as the one to show.
    expect(placed(pasteClips(copied, 8 * BAR, 2, 4, ids()))).toEqual([
      [2, "new-1", 10 * BAR],
      [2, "new-2", 8 * BAR],
    ]);
  });

  it("keeps the layout across tracks, the top track's clips on the selected track", () => {
    const copied = copyClips([
      { track: 1, clip: c("a", 0) },
      { track: 2, clip: c("b", BAR) },
      { track: 3, clip: c("c", 0) },
    ]);
    expect(placed(pasteClips(copied, 4 * BAR, 0, 4, ids()))).toEqual([
      [2, "new-1", 4 * BAR],
      [1, "new-2", 5 * BAR],
      [0, "new-3", 4 * BAR],
    ]);
  });

  it("puts any that would fall past the last track on the last track", () => {
    const copied = copyClips([
      { track: 0, clip: c("a", 0) },
      { track: 1, clip: c("b", 0) },
      { track: 2, clip: c("c", 0) },
    ]);
    expect(placed(pasteClips(copied, 0, 2, 4, ids())).map(([track]) => track)).toEqual([3, 3, 2]);
  });

  it("keeps each clip's length and notes, as copied", () => {
    const [pasted] = pasteClips(
      copyClips([{ track: 0, clip: c("a", 0, 2 * BAR) }]),
      BAR,
      0,
      1,
      ids(),
    );
    expect(pasted.clip).toEqual({
      id: "new-1",
      start: BAR,
      length: 2 * BAR,
      notes: [note],
    });
  });

  it("pastes nothing when nothing was copied", () => {
    expect(pasteClips([], 0, 0, 1, ids())).toEqual([]);
  });
});

describe("duplicating", () => {
  it("puts a copy straight after the original", () => {
    expect(placed(duplicateClips([{ track: 1, clip: c("a", 2 * BAR, 3 * BAR) }], ids()))).toEqual([
      [1, "new-1", 5 * BAR],
    ]);
  });

  it("moves a group on by the time it spans, on the same tracks", () => {
    const group = [
      { track: 0, clip: c("a", BAR, BAR) },
      { track: 2, clip: c("b", 2 * BAR, 2 * BAR) },
    ];
    // From bar 2 to the end of bar 4: three bars.
    expect(placed(duplicateClips(group, ids()))).toEqual([
      [0, "new-1", 4 * BAR],
      [2, "new-2", 5 * BAR],
    ]);
    expect(duplicateClips([], ids())).toEqual([]);
  });
});
