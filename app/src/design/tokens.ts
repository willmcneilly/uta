// Generated from DESIGN.md by `npm run tokens`. Don't edit it: change DESIGN.md and run that again.

export interface TypeToken {
  fontFamily: string;
  /** In CSS pixels. */
  fontSize: number;
  fontWeight: number;
  /** A multiple of the font size. */
  lineHeight: number;
  /** For a canvas's `font`, such as "500 13.5px system-ui". */
  font: string;
}

/** Each theme's colours. The stylesheets use them through the variables in `colorVariables`. */
export const colors = {
  light: {
    paper: "#eceee9",
    sheet: "#f5f6f2",
    ink: "#22262a",
    ink2: "#6c726f",
    ink3: "#aeb2ae",
    line: "#22262a0e",
    line2: "#22262a1f",
    mmLine: "#22262a0f",
    hatch: "#22262a4d",
    live: "#ff5a12",
    selected: "#2b59d6",
    selectedWash: "#2b59d61a",
    signal: "#6c726f",
    signalHot: "#22262a",
    error: "#b42318",
    clip: "#b42318",
    scrim: "#22262a4d",
  },
  dark: {
    paper: "#1f2225",
    sheet: "#262a2d",
    ink: "#dcd9d1",
    ink2: "#979892",
    ink3: "#5d6265",
    line: "#dcd9d10b",
    line2: "#dcd9d11a",
    mmLine: "#dcd9d10b",
    hatch: "#dcd9d138",
    live: "#ff7d42",
    selected: "#86a5ff",
    selectedWash: "#86a5ff24",
    signal: "#979892",
    signalHot: "#dcd9d1",
    error: "#ff6b5e",
    clip: "#ff6b5e",
    scrim: "#12141699",
  },
} as const;

export type ColorName = keyof typeof colors.light;

/** The CSS variable that holds each colour. */
export const colorVariables: Record<ColorName, string> = {
  paper: "--paper",
  sheet: "--sheet",
  ink: "--ink",
  ink2: "--ink-2",
  ink3: "--ink-3",
  line: "--line",
  line2: "--line-2",
  mmLine: "--mm-line",
  hatch: "--hatch",
  live: "--live",
  selected: "--selected",
  selectedWash: "--selected-wash",
  signal: "--signal",
  signalHot: "--signal-hot",
  error: "--error",
  clip: "--clip",
  scrim: "--scrim",
};

export const typography = {
  name: {
    fontFamily: "system-ui",
    fontSize: 13.5,
    fontWeight: 500,
    lineHeight: 1.2,
    font: "500 13.5px system-ui",
  },
  text: {
    fontFamily: "system-ui",
    fontSize: 12.5,
    fontWeight: 400,
    lineHeight: 1.4,
    font: "400 12.5px system-ui",
  },
  label: {
    fontFamily: "system-ui",
    fontSize: 11,
    fontWeight: 400,
    lineHeight: 1.2,
    font: "400 11px system-ui",
  },
  number: {
    fontFamily: "ui-monospace",
    fontSize: 10,
    fontWeight: 400,
    lineHeight: 1.2,
    font: "400 10px ui-monospace",
  },
  position: {
    fontFamily: "ui-monospace",
    fontSize: 19,
    fontWeight: 400,
    lineHeight: 1,
    font: "400 19px ui-monospace",
  },
  title: {
    fontFamily: "system-ui",
    fontSize: 15,
    fontWeight: 600,
    lineHeight: 1.2,
    font: "600 15px system-ui",
  },
} as const satisfies Record<string, TypeToken>;

/** Corner radii, in CSS pixels. */
export const rounded = {
  sm: 2,
  md: 6,
  lg: 8,
} as const;

/** Spacing, fixed sizes and line weights (`stroke*`), in CSS pixels. */
export const spacing = {
  space1: 2,
  space2: 4,
  space3: 6,
  space4: 8,
  space5: 12,
  space6: 16,
  space7: 24,
  rulerHeight: 24,
  trackHeight: 96,
  addTrackHeight: 44,
  trackHeadersWidth: 272,
  keyboardWidth: 56,
  velocityLaneHeight: 72,
  strokeClip: 1,
  strokeClipSelected: 1.75,
  strokeNote: 1,
  strokeNoteSelected: 1.75,
  strokeClipNotes: 1.5,
  strokeCurve: 1.5,
  strokePlayhead: 1,
  strokeGrid: 1,
  strokeSelectionBox: 1,
  hatchGap: 5,
  mmGridGap: 20,
} as const;
