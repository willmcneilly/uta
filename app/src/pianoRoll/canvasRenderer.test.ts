import { afterEach, describe, expect, it } from "vitest";
import { type ColorName, colorVariables, colors, spacing } from "../design/tokens";
import { createCanvas2DRenderer } from "./canvasRenderer";
import { velocityAlpha } from "./colours";
import type { PlacedNote } from "./notes";
import type { PianoRollRenderer, TopMarks } from "./renderer";
import {
  KEYBOARD,
  KEYBOARD_WIDTH,
  RULER_HEIGHT,
  type Viewport,
  noteArea,
  pitchToY,
  tickToX,
  velocityLane,
} from "./viewport";

// The piano roll draws by ink weight: faint rows and grid lines, notes in ink filled as heavily as they're
// played, selected notes heaviest in the selected ink, and the playhead and
// the notes sounding now in the live ink. Each is checked on what the real
// renderer asks the canvas to draw.

/** One thing drawn: how, with which colour, line weight and opacity. */
interface Mark {
  call: "fillRect" | "strokeRect" | "stroke" | "fill" | "fillText";
  style: string;
  width: number;
  alpha: number;
  args: (number | string)[];
  /** For a stroke or fill: the rectangles in its path. */
  rects: number[][];
}

/** A 2D context that records what it's asked to draw. */
function recordingContext(marks: Mark[]): CanvasRenderingContext2D {
  const state = { fillStyle: "", strokeStyle: "", lineWidth: 1, globalAlpha: 1 };
  let path: number[][] = [];
  const record =
    (call: Mark["call"], style: () => string) =>
    (...args: (number | string)[]) =>
      marks.push({ call, style: style(), width: state.lineWidth, alpha: state.globalAlpha, args, rects: path });
  const context = {
    fillRect: record("fillRect", () => state.fillStyle),
    strokeRect: record("strokeRect", () => state.strokeStyle),
    stroke: record("stroke", () => state.strokeStyle),
    fill: record("fill", () => state.fillStyle),
    fillText: record("fillText", () => state.fillStyle),
    beginPath: () => {
      path = [];
    },
    rect: (...args: number[]) => {
      path.push(args);
    },
  };
  return new Proxy(context, {
    get: (target, key) =>
      key in target
        ? target[key as keyof typeof target]
        : key in state
          ? state[key as keyof typeof state]
          : () => {},
    set: (_target, key, value) => {
      if (key in state) Object.assign(state, { [key]: value });
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

const BAR = 3840;
const view: Viewport = {
  width: KEYBOARD_WIDTH + 800,
  height: 480,
  scrollTicks: 0,
  scrollY: 60 * 12,
  pixelsPerTick: 800 / (4 * BAR),
  keyHeight: 12,
  rows: KEYBOARD,
};
const light = colors.light;

function note(id: string, start: number, overrides: Partial<PlacedNote> = {}): PlacedNote {
  const placed = { id, pitch: 60, velocity: 100, start, length: BAR / 4, outside: false, ...overrides };
  return { row: placed.pitch, ...placed };
}

const noMarks: TopMarks = { sounding: [], hovered: null, selected: new Set() };

describe("the piano roll's canvas renderer", () => {
  let grid: Mark[];
  let notes: Mark[];
  let top: Mark[];
  let renderer: PianoRollRenderer;

  const setUp = () => {
    grid = [];
    notes = [];
    top = [];
    const canvas = (marks: Mark[]) => {
      // In the page, so it inherits the tokens' CSS variables.
      const element = document.body.appendChild(document.createElement("canvas"));
      element.getContext = (() => recordingContext(marks)) as unknown as HTMLCanvasElement["getContext"];
      return element;
    };
    const made = createCanvas2DRenderer({ grid: canvas(grid), notes: canvas(notes), top: canvas(top) });
    if (!made) throw new Error("no renderer");
    renderer = made;
    renderer.resize(view.width, view.height, 1);
  };

  afterEach(() => {
    document.documentElement.removeAttribute("style");
    document.body.replaceChildren();
  });

  const drawGrid = (loop = { start: 0, end: 2 * BAR, enabled: true }, at: Viewport = view) => {
    grid.length = 0;
    renderer.drawGrid({
      view: at,
      ticksPerQuarter: 960,
      beatsPerBar: 4,
      loopStart: loop.start,
      loopEnd: loop.end,
      loopEnabled: loop.enabled,
      clipStart: 0,
      clipEnd: 4 * BAR,
    });
  };
  const area = noteArea(view);
  /** The full-height columns drawn in the note area, in order. */
  const columns = (marks: Mark[]) =>
    marks.filter(
      (m) => m.call === "fillRect" && m.args[1] === area.y && m.args[3] === area.height && m.args[2] === spacing.strokeGrid,
    );

  it("draws the bar and beat lines with DESIGN.md's mapping, and no other columns", () => {
    setUp();
    drawGrid();
    expect(new Set(columns(grid).map((m) => m.style))).toEqual(new Set([light.ink3, light.line2, light.line]));
  });

  it("draws every key full width and one row tall, black keys in ink-3, and names each C", () => {
    setUp();
    drawGrid();
    const keys = grid.filter((m) => m.call === "fillRect" && m.args[0] === 0 && (m.args[1] as number) >= RULER_HEIGHT);
    // C#4 is a full-width bar exactly on its row.
    const cSharp4 = keys.find((m) => m.style === light.ink3 && m.args[1] === pitchToY(view, 61));
    expect(cSharp4?.args).toEqual([0, pitchToY(view, 61), KEYBOARD_WIDTH, view.keyHeight]);
    expect(keys.filter((m) => m.style === light.ink3).every((m) => m.args[2] === KEYBOARD_WIDTH)).toBe(true);
    // A line parts two white keys that meet: under F4 (above E4), not under D4 (above C#4).
    const lineUnder = (pitch: number) =>
      keys.some(
        (m) =>
          m.style === light.line2 &&
          m.args[3] === spacing.strokeGrid &&
          m.args[1] === Math.round(pitchToY(view, pitch) + view.keyHeight) - spacing.strokeGrid,
      );
    expect(lineUnder(65)).toBe(true);
    expect(lineUnder(62)).toBe(false);
    // C4 sits in the keys, right-aligned against the notes.
    const c4 = grid.find((m) => m.call === "fillText" && m.args[0] === "C4");
    expect(c4?.style).toBe(light.ink2);
    expect(c4?.args[1]).toBeLessThan(KEYBOARD_WIDTH);
    expect(c4?.args[2]).toBe(pitchToY(view, 60) + view.keyHeight / 2);
  });

  it("keeps black keys and their rows the darker ones in both themes", () => {
    const rowAt = (pitch: number) =>
      grid.find(
        (m) =>
          m.call === "fillRect" &&
          m.args[0] === noteArea(view).x &&
          m.args[1] === pitchToY(view, pitch) &&
          m.args[3] === view.keyHeight,
      )?.style;
    const keyAt = (pitch: number) =>
      grid
        .filter(
          (m) =>
            m.call === "fillRect" && m.args[0] === 0 && m.args[1] === pitchToY(view, pitch) && m.args[3] === view.keyHeight,
        )
        .at(-1)?.style;

    setUp();
    drawGrid();
    // Light: C#4's key and row are shaded, D4's aren't.
    expect(keyAt(61)).toBe(light.ink3);
    expect(rowAt(61)).toBe(light.line);
    expect(rowAt(62)).toBeUndefined();

    // Dark: ink-3 is the lighter, so the white keys take it and their rows are shaded.
    const dark = colors.dark;
    for (const name of Object.keys(colorVariables) as ColorName[]) {
      document.documentElement.style.setProperty(colorVariables[name], dark[name]);
    }
    setUp();
    drawGrid();
    expect(keyAt(61)).toBe(dark.sheet);
    const whiteKeys = grid.find((m) => m.call === "fillRect" && m.args[0] === 0 && m.args[1] === RULER_HEIGHT);
    expect(whiteKeys?.style).toBe(dark.ink3);
    expect(rowAt(62)).toBe(dark.line);
    expect(rowAt(61)).toBeUndefined();
  });

  it("draws the loop as the timeline's dimension line, faint while it's off", () => {
    setUp();
    const left = Math.round(tickToX(view, BAR));
    const right = Math.round(tickToX(view, 3 * BAR));
    const line = (ink: string) =>
      grid.filter((m) => m.style === ink && m.args[0] === left && m.args[2] === right - left && m.args[3] === spacing.strokeClip);
    const wash = () => grid.filter((m) => m.style === light.line && m.args[3] === RULER_HEIGHT);
    drawGrid({ start: BAR, end: 3 * BAR, enabled: true });
    expect(line(light.ink)).toHaveLength(1);
    expect(wash().map((m) => [m.args[0], m.args[2]])).toEqual([[left, right - left]]);
    drawGrid({ start: BAR, end: 3 * BAR, enabled: false });
    expect(line(light.ink3)).toHaveLength(1);
    expect(wash()).toEqual([]);
  });

  it("draws the grid layer with no ink and neither coloured ink", () => {
    setUp();
    drawGrid();
    const allowed = [light.sheet, light.line, light.line2, light.ink3, light.ink2, light.ink];
    for (const mark of grid) expect(allowed).toContain(mark.style);
    // Ink only for the loop's dimension line while it's on (its arrowheads are a path).
    const inked = grid.filter((m) => m.style === light.ink && m.call === "fillRect");
    expect(inked.length).toBeGreaterThan(0);
    expect(inked.every((m) => (m.args[1] as number) < RULER_HEIGHT)).toBe(true);
  });

  it("fills notes in ink as heavily as they're played, and outlines them in ink", () => {
    setUp();
    const soft = note("soft", 0, { velocity: 20 });
    const hard = note("hard", BAR, { velocity: 120 });
    renderer.drawNotes({ view, notes: [soft, hard], velocities: [], selected: new Set(), empty: false });
    const fills = notes.filter((m) => m.call === "fillRect");
    expect(fills.map((m) => [m.style, m.alpha])).toEqual([
      [light.ink, velocityAlpha(20)],
      [light.ink, velocityAlpha(120)],
    ]);
    expect(velocityAlpha(120)).toBeGreaterThan(velocityAlpha(20));
    // No blue for a note that isn't selected.
    expect(notes.some((m) => m.style === light.selected)).toBe(false);
    const outlines = notes.filter((m) => m.call === "stroke");
    expect(outlines.map((m) => [m.style, m.width, m.alpha, m.rects.length])).toEqual([
      [light.ink, spacing.strokeNote, 1, 2],
    ]);
  });

  it("draws selected notes heaviest, in the selected ink, still filled by velocity", () => {
    setUp();
    const plain = note("plain", 0);
    const chosen = note("chosen", BAR, { velocity: 50 });
    renderer.drawNotes({ view, notes: [plain, chosen], velocities: [], selected: new Set(["chosen"]), empty: false });
    const fills = notes.filter((m) => m.call === "fillRect");
    expect(fills[1]).toMatchObject({ style: light.selected, alpha: velocityAlpha(50) });
    const outlines = notes.filter((m) => m.call === "stroke");
    expect(outlines.map((m) => [m.style, m.width])).toEqual([
      [light.ink, spacing.strokeNote],
      [light.selected, spacing.strokeNoteSelected],
    ]);
    expect(spacing.strokeNoteSelected).toBeGreaterThan(spacing.strokeNote);
  });

  it("draws notes past the clip's end faintly", () => {
    setUp();
    renderer.drawNotes({
      view,
      notes: [note("past", 3 * BAR, { outside: true, velocity: 127 })],
      velocities: [],
      selected: new Set(),
      empty: false,
    });
    expect(notes.find((m) => m.call === "fillRect")?.alpha).toBeCloseTo(velocityAlpha(127) * 0.35);
    expect(notes.find((m) => m.call === "stroke")?.alpha).toBe(0.35);
  });

  it("draws velocity stems in ink, and selected ones heavier in the selected ink, on top", () => {
    setUp();
    const a = note("a", 0, { pitch: 10 });
    const b = note("b", BAR);
    renderer.drawNotes({ view, notes: [], velocities: [b, a], selected: new Set(["b"]), empty: false });
    const stems = notes.filter((m) => m.call === "fill");
    expect(stems.map((m) => m.style)).toEqual([light.ink, light.selected]);
    // A stem down to velocity 0 and a square on top, for each.
    const [plain, chosen] = stems;
    expect(plain.rects).toHaveLength(2);
    expect(plain.rects[0][2]).toBe(spacing.strokeNote);
    expect(chosen.rects[0][2]).toBe(spacing.strokeNoteSelected);
    expect(plain.rects[0][1]).toBeGreaterThanOrEqual(velocityLane(view).y);
  });

  it("says how to add a note when the clip is empty, and only then", () => {
    setUp();
    renderer.drawNotes({ view, notes: [], velocities: [], selected: new Set(), empty: true });
    const hint = notes.filter((m) => m.call === "fillText");
    expect(hint.map((m) => [m.args[0], m.style])).toEqual([["Click to draw a note", light.ink2]]);
    notes.length = 0;
    renderer.drawNotes({ view, notes: [], velocities: [], selected: new Set(), empty: false });
    expect(notes.some((m) => m.call === "fillText")).toBe(false);
  });

  it("draws the playhead in the live ink, and nothing else when nothing sounds", () => {
    setUp();
    renderer.drawTop(view, BAR, null, noMarks);
    expect(top.map((m) => [m.call, m.style])).toEqual([
      ["fillRect", light.live],
      ["fill", light.live],
    ]);
  });

  it("draws the notes sounding now in the live ink, keeping a selected one's outline", () => {
    setUp();
    const a = note("a", 0, { pitch: 60 });
    const b = note("b", 0, { pitch: 64 });
    renderer.drawTop(view, 100, null, { sounding: [a, b], hovered: null, selected: new Set(["b"]) });
    const lit = top.filter((m) => m.call === "fillRect" && m.args[1] !== 0);
    const rect = (n: PlacedNote) => [tickToX(view, n.start), pitchToY(view, n.pitch) + 1];
    expect(lit.map((m) => [m.style, m.args[0], m.args[1]])).toEqual([
      [light.live, ...rect(a)],
      [light.live, ...rect(b)],
    ]);
    expect(top.filter((m) => m.call === "stroke").map((m) => [m.style, m.width])).toEqual([
      [light.live, spacing.strokeNote],
      [light.selected, spacing.strokeNoteSelected],
    ]);
  });

  it("outlines the note under the pointer heavier, with its velocity stem", () => {
    setUp();
    const a = note("a", 0);
    renderer.drawTop(view, -BAR, null, { sounding: [], hovered: a, selected: new Set() });
    expect(top.filter((m) => m.call === "stroke").map((m) => [m.style, m.width])).toEqual([
      [light.ink, spacing.strokeNoteSelected],
    ]);
    const stem = top.find((m) => m.call === "fill");
    expect(stem?.style).toBe(light.ink);
    expect(stem?.rects[0][2]).toBe(spacing.strokeNoteSelected);

    // A selected note is already drawn as heavy as a note gets.
    top.length = 0;
    renderer.drawTop(view, -BAR, null, { sounding: [], hovered: a, selected: new Set(["a"]) });
    expect(top).toEqual([]);
  });

  it("takes its line weights from the tokens, so the tuning panel reaches it", () => {
    document.documentElement.style.setProperty("--stroke-note-selected", "3px");
    setUp();
    renderer.drawNotes({ view, notes: [note("a", 0)], velocities: [], selected: new Set(["a"]), empty: false });
    expect(notes.find((m) => m.call === "stroke")?.width).toBe(3);
  });
});
