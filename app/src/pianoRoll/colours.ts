// The piano roll's colours, from the design tokens' CSS variables, so they
// follow light and dark mode.

import { readColours } from "../design/readColours";

export interface Theme {
  background: string;
  blackKeyRow: string;
  /** Over the grid outside the clip. */
  outsideClip: string;
  barLine: string;
  beatLine: string;
  subLine: string;
  octaveLine: string;
  ruler: string;
  rulerText: string;
  whiteKey: string;
  blackKey: string;
  keyText: string;
  playhead: string;
  /**
   * The loop region's band along the bottom of the ruler while the loop is
   * on, and its fill on the timeline's ruler.
   */
  loop: string;
  loopRegion: string;
  /** The outline of a selected note, and its velocity bar. */
  selectedNote: string;
  velocityLane: string;
  /** The selection box's fill and edge. */
  selectionBox: string;
  selectionBoxEdge: string;
  /** A note's fill at each velocity, 0 to 127 (0 is never used). */
  noteByVelocity: string[];
}

/** The piano roll's colours, by what each part means in DESIGN.md. */
export function readTheme(element: Element): Theme {
  const c = readColours(element);
  return {
    background: c.sheet,
    blackKeyRow: c.line,
    outsideClip: c.line2,
    barLine: c.ink3,
    beatLine: c.line2,
    subLine: c.line,
    octaveLine: c.line2,
    ruler: c.paper,
    rulerText: c.ink2,
    whiteKey: c.sheet,
    blackKey: c.ink3,
    keyText: c.ink2,
    playhead: c.live,
    loop: c.ink2,
    loopRegion: c.line2,
    selectedNote: c.selected,
    velocityLane: c.paper,
    selectionBox: c.selectedWash,
    selectionBoxEdge: c.selected,
    noteByVelocity: velocityColours(c.ink3, c.ink),
  };
}

/**
 * A colour for each velocity from 0 to 127, from `soft` to `hard` (both
 * `#rrggbb`), worked out once so drawing a note never builds a string.
 */
export function velocityColours(soft: string, hard: string): string[] {
  const from = parseHex(soft);
  const to = parseHex(hard);
  return Array.from({ length: 128 }, (_, velocity) => {
    const t = Math.max(0, velocity - 1) / 126;
    const channel = (i: number) => Math.round(from[i] + (to[i] - from[i]) * t);
    // eslint-disable-next-line uta/no-raw-colour -- a blend of two tokens, not a new colour
    return `rgb(${channel(0)}, ${channel(1)}, ${channel(2)})`;
  });
}

function parseHex(colour: string): [number, number, number] {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(colour);
  if (!match) return [128, 128, 128];
  return [parseInt(match[1], 16), parseInt(match[2], 16), parseInt(match[3], 16)];
}

/** Whether `pitch` is a black key: C♯, D♯, F♯, G♯ or A♯. */
export function isBlackKey(pitch: number): boolean {
  return [1, 3, 6, 8, 10].includes(pitch % 12);
}

/** "C4" for 60: MIDI's middle C is C4. Only Cs are labelled. */
export function octaveName(pitch: number): string {
  return `C${Math.floor(pitch / 12) - 1}`;
}
