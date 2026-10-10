import { describe, expect, it } from "vitest";
import { drawPlayhead, drawSelectionBox } from "./canvasMarks";
import { type Mark, recordingContext } from "./canvasTesting";

describe("drawPlayhead", () => {
  it("is a line down the canvas with a marker in the ruler, centred on it, in its colour", () => {
    const marks: Mark[] = [];
    drawPlayhead(recordingContext(marks), 100, 1, 300, "live");
    expect(marks[0]).toEqual({ call: "fillRect", style: "live", args: [100, 0, 1, 300] });
    const path = marks.filter((mark) => mark.call === "moveTo" || mark.call === "lineTo");
    expect(path.map((mark) => mark.args)).toEqual([
      [95, 0],
      [106, 0],
      [100.5, 7],
    ]);
    expect(marks.at(-1)).toEqual({ call: "fill", style: "live", args: [] });
  });
});

describe("drawSelectionBox", () => {
  it("washes the box and edges it inside, so the edge never spills past it", () => {
    const marks: Mark[] = [];
    const context = recordingContext(marks);
    drawSelectionBox(context, { x: 10, y: 20, width: 100, height: 50 }, "wash", "selected", 2);
    expect(marks).toEqual([
      { call: "fillRect", style: "wash", args: [10, 20, 100, 50] },
      { call: "strokeRect", style: "selected", args: [11, 21, 98, 48] },
    ]);
    expect(context.lineWidth).toBe(2);
  });
});
