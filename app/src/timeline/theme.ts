// The timeline's colours, line weights and fonts, from the design tokens'
// CSS variables, so they follow light and dark mode and the tuning panel.
// They're ranked by ink weight (DESIGN.md, "Elevation & Depth"): the grid
// and the ruler are faint, clips and their notes are ink, and a selected
// clip is drawn heaviest, in the selected ink. Its own, not the piano roll's,
// so restyling one never moves the other.

import { readColours } from "../design/readColours";
import { readFonts, readLineWidths } from "../design/readTokens";

export interface TimelineTheme {
  /** The drawing surface, under the ruler and the tracks. */
  background: string;
  /** Below the last track, where there's nothing to draw on. */
  belowTracks: string;
  selectedTrack: string;
  /**
   * The grid's columns, a step fainter than the piano roll's, so the clips
   * stand out (provisional: D-1).
   */
  barLine: string;
  beatLine: string;
  subLine: string;
  /** The line under each track and under the ruler, as under each header. */
  rowLine: string;
  /** The ruler's bar ticks and beat ticks, and its bar numbers. */
  barTick: string;
  beatTick: string;
  rulerText: string;
  /**
   * The loop region's dimension line along the bottom of the ruler, in ink
   * while the loop is on and faint while it's off, and the wash over the
   * ruler while it's on (provisional: D-2).
   */
  loop: string;
  loopOff: string;
  loopRegion: string;
  clipFill: string;
  /** A clip's fill under the pointer. */
  clipHover: string;
  /**
   * A clip's edge is the second ink, so its notes, in ink, are the darkest
   * thing in it (provisional: D-1).
   */
  clipEdge: string;
  clipNote: string;
  selectedClipEdge: string;
  playhead: string;
  /** The selection box and the clip being drawn: fill and edge. */
  selectionBox: string;
  selectionBoxEdge: string;
  /** Line weights, in CSS pixels. */
  gridWidth: number;
  clipWidth: number;
  selectedClipWidth: number;
  clipNoteWidth: number;
  playheadWidth: number;
  selectionBoxWidth: number;
  /** The canvas font for the bar numbers. */
  labelFont: string;
}

/** The timeline's colours, line weights and fonts, by what each part means in DESIGN.md. */
export function readTimelineTheme(element: Element): TimelineTheme {
  const c = readColours(element);
  const widths = readLineWidths(element);
  return {
    background: c.sheet,
    belowTracks: c.paper,
    selectedTrack: c.selectedWash,
    barLine: c.line2,
    beatLine: c.line,
    subLine: c.line,
    rowLine: c.line2,
    barTick: c.ink3,
    beatTick: c.line2,
    rulerText: c.ink2,
    loop: c.ink,
    loopOff: c.ink3,
    loopRegion: c.line,
    clipFill: c.line,
    clipHover: c.line2,
    clipEdge: c.ink2,
    clipNote: c.ink,
    selectedClipEdge: c.selected,
    playhead: c.live,
    selectionBox: c.selectedWash,
    selectionBoxEdge: c.selected,
    gridWidth: widths.strokeGrid,
    clipWidth: widths.strokeClip,
    selectedClipWidth: widths.strokeClipSelected,
    clipNoteWidth: widths.strokeClipNotes,
    playheadWidth: widths.strokePlayhead,
    selectionBoxWidth: widths.strokeSelectionBox,
    labelFont: readFonts(element).label,
  };
}
