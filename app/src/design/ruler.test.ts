import { describe, expect, it } from "vitest";
import { type Mark, recordingContext } from "./canvasTesting";
import { type Ruler, drawRuler } from "./ruler";

const QUARTER = 480;
const BAR = 4 * QUARTER;

/** A ruler from x = 50, 400 px wide, at 0.05 px a tick: a bar is 96 px. */
function ruler(overrides: Partial<Ruler> = {}): Ruler {
  return {
    left: 50,
    width: 400,
    height: 24,
    tickToX: (tick) => 50 + tick * 0.05,
    pixelsPerTick: 0.05,
    ticks: { start: 0, end: 8000 },
    ticksPerQuarter: QUARTER,
    bar: BAR,
    loop: { start: 0, end: 2 * BAR, enabled: true },
    line: 1,
    loopWeight: 1,
    font: "10px mono",
    colours: {
      background: "sheet",
      loopRegion: "wash",
      edge: "line-2",
      beatTick: "beat",
      barTick: "bar",
      text: "ink-2",
      loop: "ink",
      loopOff: "ink-3",
    },
    ...overrides,
  };
}

function draw(overrides: Partial<Ruler> = {}): Mark[] {
  const marks: Mark[] = [];
  drawRuler(recordingContext(marks), ruler(overrides));
  return marks;
}

const fills = (marks: Mark[], style: string) =>
  marks.filter((mark) => mark.call === "fillRect" && mark.style === style).map((mark) => mark.args);

describe("drawRuler", () => {
  it("draws only inside its own box, and leaves the context as it found it", () => {
    const marks = draw();
    expect(marks[0].call).toBe("save");
    expect(marks.find((mark) => mark.call === "rect")?.args).toEqual([50, 0, 400, 24]);
    expect(marks.findIndex((mark) => mark.call === "clip")).toBeLessThan(
      marks.findIndex((mark) => mark.call === "fillRect"),
    );
    expect(marks.at(-1)?.call).toBe("restore");
  });

  it("lays its sheet, with a line along its foot", () => {
    const marks = draw();
    expect(fills(marks, "sheet")).toEqual([[50, 0, 400, 24]]);
    expect(fills(marks, "line-2")).toEqual([[50, 23, 400, 1]]);
  });

  it("numbers each bar beside a tick standing on its foot, with shorter beat ticks between", () => {
    const marks = draw();
    const texts = marks.filter((mark) => mark.call === "fillText");
    expect(texts.map((mark) => mark.args)).toEqual([
      ["1", 54, 4],
      ["2", 150, 4],
      ["3", 246, 4],
      ["4", 342, 4],
      ["5", 438, 4],
    ]);
    expect(texts.every((mark) => mark.style === "ink-2")).toBe(true);
    expect(fills(marks, "bar")[1]).toEqual([146, 15, 1, 8]);
    expect(fills(marks, "beat")[0]).toEqual([74, 19, 1, 4]);
    // Three beats between each pair of bars.
    expect(fills(marks, "beat")).toHaveLength(12);
  });

  it("leaves out beat ticks when they'd crowd, and numbers fewer bars when they would", () => {
    const pixelsPerTick = 0.01; // a beat is 4.8 px and a bar 19.2 px.
    const marks = draw({
      pixelsPerTick,
      tickToX: (tick) => 50 + tick * pixelsPerTick,
      ticks: { start: 0, end: 40_000 },
    });
    expect(fills(marks, "beat")).toEqual([]);
    const numbers = marks.filter((mark) => mark.call === "fillText").map((mark) => mark.args[0]);
    expect(numbers.slice(0, 3)).toEqual(["1", "3", "5"]);
  });

  it("washes the loop region while the loop is on, and draws it as a dimension line", () => {
    const marks = draw();
    expect(fills(marks, "wash")).toEqual([[50, 0, 192, 24]]);
    // The line, then an end tick at each end, then both arrowheads.
    expect(fills(marks, "ink")).toEqual([
      [50, 19, 192, 1],
      [50, 14, 1, 10],
      [241, 14, 1, 10],
    ]);
    expect(marks.filter((mark) => mark.call === "fill" && mark.style === "ink")).toHaveLength(1);
  });

  it("draws the loop faint and unwashed while it's off", () => {
    const marks = draw({ loop: { start: 0, end: 2 * BAR, enabled: false } });
    expect(fills(marks, "wash")).toEqual([]);
    expect(fills(marks, "ink")).toEqual([]);
    expect(fills(marks, "ink-3")[0]).toEqual([50, 19, 192, 1]);
  });

  it("leaves the arrowheads off a loop too short for them", () => {
    const marks = draw({ loop: { start: 0, end: 300, enabled: true } }); // 15 px
    expect(marks.filter((mark) => mark.call === "fill")).toEqual([]);
    expect(fills(marks, "ink")).toHaveLength(3);
  });

  it("draws the part of the loop line in view, and nothing of a loop out of view", () => {
    const scrolled = (start: number) => (tick: number) => 50 + (tick - start) * 0.05;
    const partly = draw({ tickToX: scrolled(BAR), ticks: { start: BAR, end: 9000 } });
    expect(fills(partly, "wash")).toEqual([[50, 0, 96, 24]]);
    expect(fills(partly, "ink")[0]).toEqual([50, 19, 96, 1]);
    const gone = draw({ tickToX: scrolled(3 * BAR), ticks: { start: 3 * BAR, end: 13_000 } });
    expect(fills(gone, "wash")).toEqual([]);
    expect(fills(gone, "ink")).toEqual([]);
  });
});
