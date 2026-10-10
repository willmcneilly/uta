// The piano roll's colours, line weights and fonts, from the design tokens'
// CSS variables, so they follow light and dark mode and the tuning panel.
// They're ranked by ink weight (DESIGN.md, "Elevation & Depth"): the rows
// and finer grid lines are faintest, bar lines are ink-3, notes are
// ink, and selected notes are drawn heaviest, in the selected ink. The
// notes sounding now are in the live ink. Its own, not the timeline's, so
// restyling one never moves the other.

import { readColours } from "../design/readColours";
import { readFonts, readLineWidths } from "../design/readTokens";

export interface Theme {
  /** The drawing surface: the notes, the keys, the ruler and the velocity lane. */
  background: string;
  /**
   * Whether the rows shaded with `rowShade` are the black keys' (on light
   * paper) or the white keys' (on dark), to match the darker keys
   * (provisional: D-5).
   */
  shadeBlackKeyRows: boolean;
  rowShade: string;
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
  /**
   * The keys. Black keys are always the darker ones: ink-3 on the sheet in
   * light, and the sheet among ink-3 white keys in dark, where ink-3 is the
   * lighter (provisional: D-5).
   */
  whiteKey: string;
  blackKey: string;
  /** The line between two white keys that meet. */
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
  const dark = luminance(c.sheet) < luminance(c.ink);
  const widths = readLineWidths(element);
  return {
    background: c.sheet,
    shadeBlackKeyRows: !dark,
    rowShade: c.line,
    outsideClip: c.line2,
    barLine: c.ink3,
    beatLine: c.line2,
    subLine: c.line,
    octaveLine: c.line2,
    edge: c.line2,
    barTick: c.ink3,
    beatTick: c.line2,
    rulerText: c.ink2,
    whiteKey: dark ? c.ink3 : c.sheet,
    blackKey: dark ? c.sheet : c.ink3,
    keyLine: dark ? c.sheet : c.line2,
    // Dark text on dark theme's lighter white keys, so the names stay legible.
    keyText: dark ? c.sheet : c.ink2,
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

/**
 * How light a `#rrggbb` (or `#rrggbbaa`) colour is, from 0 to 1, to tell
 * which theme's tokens these are. Unparseable colours count as light.
 */
export function luminance(colour: string): number {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})/i.exec(colour.trim());
  if (!match) return 1;
  const [r, g, b] = match.slice(1).map((hex) => parseInt(hex, 16) / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Whether `pitch` is a black key: C♯, D♯, F♯, G♯ or A♯. */
export function isBlackKey(pitch: number): boolean {
  return [1, 3, 6, 8, 10].includes(pitch % 12);
}

/** "C4" for 60: MIDI's middle C is C4. Only Cs are labelled. */
export function octaveName(pitch: number): string {
  return `C${Math.floor(pitch / 12) - 1}`;
}
