// The piano roll's colours, line weights and fonts, from the design tokens'
// CSS variables, so they follow light and dark mode and the tuning panel.
// They're ranked by ink weight (DESIGN.md, "Elevation & Depth"): the
// millimetre grid and the rows are faintest, bar lines are ink-3, notes are
// ink, and selected notes are drawn heaviest, in the selected ink. The
// notes sounding now are in the live ink. Its own, not the timeline's, so
// restyling one never moves the other.

import { readColours } from "../design/readColours";
import { readFonts, readLineWidths, readPatterns } from "../design/readTokens";

export interface Theme {
  /** The drawing surface: the notes, the keys, the ruler and the velocity lane. */
  background: string;
  /** The millimetre grid behind the bar and beat lines, and its gap. */
  mmLine: string;
  mmGap: number;
  blackKeyRow: string;
  /** Over the grid outside the clip. */
  outsideClip: string;
  barLine: string;
  beatLine: string;
  subLine: string;
  octaveLine: string;
  /** The lines that set the keys, the ruler and the velocity lane apart from the notes. */
  edge: string;
  /** The ruler's bar ticks and beat ticks, and its bar numbers. */
  barTick: string;
  beatTick: string;
  rulerText: string;
  blackKey: string;
  /** The line under each white key. */
  keyLine: string;
  keyText: string;
  playhead: string;
  /**
   * The loop region's dimension line along the bottom of the ruler, in ink
   * while the loop is on and faint while it's off, and the wash over the
   * ruler while it's on, as on the timeline (provisional: D-2).
   */
  loop: string;
  loopOff: string;
  loopRegion: string;
  /**
   * A note is outlined in ink, and filled with ink as heavily as it's
   * played (see `velocityAlpha`). A selected note is the same in the
   * selected ink, outlined heavier, and a note sounding now is filled and
   * outlined in the live ink (provisional: D-4).
   */
  note: string;
  selectedNote: string;
  soundingNote: string;
  /** The velocity lane's stems, and the label in its corner. */
  velocityText: string;
  /** The empty clip's hint (provisional: D-6). */
  hintText: string;
  /** The selection box's fill and edge. */
  selectionBox: string;
  selectionBoxEdge: string;
  /** Line weights, in CSS pixels. */
  gridWidth: number;
  noteWidth: number;
  selectedNoteWidth: number;
  playheadWidth: number;
  selectionBoxWidth: number;
  /** The loop's dimension line, as on the timeline. */
  loopWidth: number;
  /** The canvas font for the rulers' numbers, key names, the lane's label and the hint. */
  labelFont: string;
}

/** The piano roll's colours, line weights and fonts, by what each part means in DESIGN.md. */
export function readTheme(element: Element): Theme {
  const c = readColours(element);
  const widths = readLineWidths(element);
  return {
    background: c.sheet,
    mmLine: c.mmLine,
    mmGap: readPatterns(element).mmGridGap,
    blackKeyRow: c.line,
    outsideClip: c.line2,
    barLine: c.ink3,
    beatLine: c.line2,
    subLine: c.line,
    octaveLine: c.line2,
    edge: c.line2,
    barTick: c.ink3,
    beatTick: c.line2,
    rulerText: c.ink2,
    blackKey: c.ink3,
    keyLine: c.line2,
    keyText: c.ink2,
    playhead: c.live,
    loop: c.ink,
    loopOff: c.ink3,
    loopRegion: c.line,
    note: c.ink,
    selectedNote: c.selected,
    soundingNote: c.live,
    velocityText: c.ink2,
    hintText: c.ink2,
    selectionBox: c.selectedWash,
    selectionBoxEdge: c.selected,
    gridWidth: widths.strokeGrid,
    noteWidth: widths.strokeNote,
    selectedNoteWidth: widths.strokeNoteSelected,
    playheadWidth: widths.strokePlayhead,
    selectionBoxWidth: widths.strokeSelectionBox,
    loopWidth: widths.strokeClip,
    labelFont: readFonts(element).label,
  };
}

/** How opaque a note's fill is at the softest velocity, and at the hardest (provisional: D-4). */
export const SOFTEST_FILL = 0.08;
export const HARDEST_FILL = 0.6;

/**
 * How heavily a note played at `velocity` (1 to 127) is filled with its
 * ink: barely at all when it's soft, well over half when it's hard, so a
 * hard note reads as heavier ink without hiding its outline (provisional: D-4).
 */
export function velocityAlpha(velocity: number): number {
  const t = Math.min(1, Math.max(0, (velocity - 1) / 126));
  return SOFTEST_FILL + (HARDEST_FILL - SOFTEST_FILL) * t;
}

/** Whether `pitch` is a black key: C♯, D♯, F♯, G♯ or A♯. */
export function isBlackKey(pitch: number): boolean {
  return [1, 3, 6, 8, 10].includes(pitch % 12);
}

/** "C4" for 60: MIDI's middle C is C4. Only Cs are labelled. */
export function octaveName(pitch: number): string {
  return `C${Math.floor(pitch / 12) - 1}`;
}
