import { afterEach, describe, expect, it } from "vitest";
import { colors, spacing } from "../design/tokens";
import type { PlacedNote } from "../pianoRoll/notes";
import { createTimelineRenderer } from "./canvasRenderer";
import type { DrawnClip, TimelineRenderer } from "./renderer";
import { readTimelineTheme } from "./theme";
import type { TimelineViewport } from "./viewport";

// The timeline draws by ink weight: faint structure, clips and their notes
// in ink, the selected clip heaviest in the selected ink, the playhead in
// the live ink. Each is checked on what the real renderer asks the canvas
// to draw.

/** One thing drawn: how, and with which colour and line weight. */
interface Mark {
  call: "fillRect" | "strokeRect" | "stroke" | "fill";
  style: string;
  width: number;
  args: number[];
}

/** A 2D context that records what it's asked to draw. */
function recordingContext(marks: Mark[]): CanvasRenderingContext2D {
  const state = { fillStyle: "", strokeStyle: "", lineWidth: 1 };
  const record = (call: Mark["call"], style: string) => (...args: number[]) =>
    marks.push({ call, style, width: state.lineWidth, args });
  const context = {
    fillRect: (...args: number[]) => record("fillRect", state.fillStyle)(...args),
    strokeRect: (...args: number[]) => record("strokeRect", state.strokeStyle)(...args),
    stroke: () => record("stroke", state.strokeStyle)(),
    fill: () => record("fill", state.fillStyle)(),
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
const view: TimelineViewport = {
  width: 800,
  height: 400,
  scrollTicks: 0,
  scrollY: 0,
  pixelsPerTick: 800 / (8 * BAR),
};

function clip(id: string, start: number, overrides: Partial<DrawnClip> = {}): DrawnClip {
  const notes: PlacedNote[] = [
    { id: `${id}-n`, pitch: 60, velocity: 100, start, length: BAR / 4, outside: false },
  ];
  return {
    id,
    track: 0,
    start,
    length: BAR,
    selected: false,
    hovered: false,
    notes,
    low: 60,
    high: 60,
    ...overrides,
  };
}

describe("the timeline's canvas renderer", () => {
  let grid: Mark[];
  let clips: Mark[];
  let top: Mark[];
  let renderer: TimelineRenderer;

  const setUp = () => {
    grid = [];
    clips = [];
    top = [];
    const canvas = (marks: Mark[]) => {
      // In the page, so it inherits the tokens' CSS variables.
      const element = document.body.appendChild(document.createElement("canvas"));
      element.getContext = (() => recordingContext(marks)) as unknown as HTMLCanvasElement["getContext"];
      return element;
    };
    const made = createTimelineRenderer({ grid: canvas(grid), clips: canvas(clips), top: canvas(top) });
    if (!made) throw new Error("no renderer");
    renderer = made;
    renderer.resize(view.width, view.height, 1);
  };

  afterEach(() => {
    document.documentElement.removeAttribute("style");
    document.body.replaceChildren();
  });

  it("draws the grid and the ruler's ticks faintly", () => {
    setUp();
    renderer.drawGrid({
      view,
      ticksPerQuarter: 960,
      beatsPerBar: 4,
      trackCount: 2,
      selectedTrack: null,
      loop: { start: 0, end: 4 * BAR, enabled: true },
    });
    const styles = new Set(grid.map((mark) => mark.style));
    for (const faint of [colors.light.line, colors.light.line2, colors.light.ink3]) {
      expect(styles).toContain(faint);
    }
    // Nothing on the grid layer is drawn in the coloured inks.
    expect(styles).not.toContain(colors.light.live);
    expect(styles).not.toContain(colors.light.selected);
    // Grid lines and ticks are all at the grid's line weight.
    const lines = grid.filter((mark) => mark.style === colors.light.line2);
    expect(lines.every((mark) => mark.args[2] === spacing.strokeGrid || mark.args[3] === spacing.strokeGrid)).toBe(true);
  });

  it("draws the loop in ink while it's on, and faintly while it's off", () => {
    setUp();
    const loop = (enabled: boolean) => {
      grid.length = 0;
      renderer.drawGrid({
        view,
        ticksPerQuarter: 960,
        beatsPerBar: 4,
        trackCount: 1,
        selectedTrack: null,
        loop: { start: BAR, end: 3 * BAR, enabled },
      });
      return grid.filter((mark) => mark.call === "fill").map((mark) => mark.style);
    };
    // The arrowheads at each end of its dimension line.
    expect(loop(true)).toEqual([colors.light.ink]);
    expect(loop(false)).toEqual([colors.light.ink3]);
  });

  it("draws clips in ink, their notes in the darkest ink, and a selected clip heaviest in the selected ink", () => {
    setUp();
    renderer.drawClips({ view, clips: [clip("a", 0), clip("b", BAR, { selected: true })] });
    const edges = clips.filter((mark) => mark.call === "strokeRect");
    expect(edges.map((mark) => [mark.style, mark.width])).toEqual([
      [colors.light.ink2, spacing.strokeClip],
      [colors.light.selected, spacing.strokeClipSelected],
    ]);
    expect(spacing.strokeClipSelected).toBeGreaterThan(spacing.strokeClip);
    const notes = clips.filter((mark) => mark.call === "stroke");
    expect(notes.map((mark) => [mark.style, mark.width])).toEqual([
      [colors.light.ink, spacing.strokeClipNotes],
      [colors.light.ink, spacing.strokeClipNotes],
    ]);
  });

  it("shades the clip under the pointer", () => {
    setUp();
    renderer.drawClips({ view, clips: [clip("a", 0), clip("b", BAR, { hovered: true })] });
    const fills = clips.filter((mark) => mark.call === "fillRect").map((mark) => mark.style);
    expect(fills).toEqual([colors.light.line, colors.light.line2]);
  });

  it("draws the playhead in the live ink", () => {
    setUp();
    renderer.drawTop(view, 2 * BAR, null, null);
    expect(top.map((mark) => [mark.call, mark.style])).toEqual([
      ["fillRect", colors.light.live],
      ["fill", colors.light.live],
    ]);
  });

  it("takes its line weights from the tokens, so the tuning panel reaches it", () => {
    document.documentElement.style.setProperty("--stroke-clip-selected", "3px");
    expect(readTimelineTheme(document.documentElement).selectedClipWidth).toBe(3);
    setUp();
    renderer.drawClips({ view, clips: [clip("b", 0, { selected: true })] });
    expect(clips.find((mark) => mark.call === "strokeRect")?.width).toBe(3);
  });
});
