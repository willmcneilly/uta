import { afterEach, describe, expect, it } from "vitest";
import { readFonts, readLineWidths, readPatterns } from "./readTokens";
import { spacing, typography } from "./tokens";

describe("readLineWidths", () => {
  afterEach(() => document.documentElement.removeAttribute("style"));

  it("reads each line weight from its CSS variable", () => {
    document.documentElement.style.setProperty("--stroke-clip-selected", "3.5px");
    expect(readLineWidths(document.body).strokeClipSelected).toBe(3.5);
  });

  it("falls back to DESIGN.md's value without the stylesheet, and reads only line weights", () => {
    const widths = readLineWidths(document.body);
    expect(widths.strokeGrid).toBe(spacing.strokeGrid);
    expect(Object.keys(widths).every((name) => name.startsWith("stroke"))).toBe(true);
  });
});

describe("readPatterns", () => {
  afterEach(() => document.documentElement.removeAttribute("style"));

  it("reads the hatch and millimetre grid gaps, or DESIGN.md's without the stylesheet", () => {
    expect(readPatterns(document.body)).toEqual({
      hatchGap: spacing.hatchGap,
      mmGridGap: spacing.mmGridGap,
    });
    document.documentElement.style.setProperty("--mm-grid-gap", "12px");
    expect(readPatterns(document.body).mmGridGap).toBe(12);
  });
});

describe("readFonts", () => {
  afterEach(() => document.documentElement.removeAttribute("style"));

  it("builds a canvas font from the role's weight, size and family", () => {
    const root = document.documentElement.style;
    root.setProperty("--font-label-weight", "600");
    root.setProperty("--font-label-size", "17px");
    root.setProperty("--font-label-family", '"Commit Mono"');
    expect(readFonts(document.body).label).toBe('600 17px "Commit Mono"');
  });

  it("falls back to DESIGN.md's font without the stylesheet", () => {
    expect(readFonts(document.body).number).toBe(typography.number.font);
  });
});
