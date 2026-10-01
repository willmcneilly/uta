// The piano roll's colours, from the CSS custom properties in App.css, so
// they follow light and dark mode.

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
  /** The outline of a selected note, and its velocity bar. */
  selectedNote: string;
  velocityLane: string;
  /** The selection box's fill and edge. */
  selectionBox: string;
  selectionBoxEdge: string;
  /** A note's fill at each velocity, 0 to 127 (0 is never used). */
  noteByVelocity: string[];
}

export function readTheme(element: Element): Theme {
  const style = getComputedStyle(element);
  const read = (name: string, fallback: string) =>
    style.getPropertyValue(name).trim() || fallback;
  return {
    background: read("--roll-background", "#ffffff"),
    blackKeyRow: read("--roll-black-key-row", "#f1f1ef"),
    outsideClip: read("--roll-outside-clip", "rgba(0, 0, 0, 0.06)"),
    barLine: read("--roll-bar-line", "#b8b8b4"),
    beatLine: read("--roll-beat-line", "#d9d9d6"),
    subLine: read("--roll-sub-line", "#ebebe8"),
    octaveLine: read("--roll-octave-line", "#cfcfcb"),
    ruler: read("--roll-ruler", "#efefec"),
    rulerText: read("--roll-ruler-text", "#6e6e73"),
    whiteKey: read("--roll-white-key", "#ffffff"),
    blackKey: read("--roll-black-key", "#2c2c2e"),
    keyText: read("--roll-key-text", "#6e6e73"),
    playhead: read("--roll-playhead", "#e5484d"),
    selectedNote: read("--roll-note-selected", "#1c1c1e"),
    velocityLane: read("--roll-velocity-lane", "#f7f7f5"),
    selectionBox: read("--roll-selection-box", "rgba(31, 95, 191, 0.12)"),
    selectionBoxEdge: read("--roll-selection-box-edge", "#1f5fbf"),
    noteByVelocity: velocityColours(
      read("--roll-note-soft", "#b9d4f5"),
      read("--roll-note-hard", "#1f5fbf"),
    ),
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
