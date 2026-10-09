/* eslint-disable uta/no-raw-colour -- the colours here are test data */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { DesignTokenError, readDesign, toCss, toTs } from "./designTokens";

const DESIGN = `---
version: alpha
name: Test
colors:
  primary: "{colors.ink}"
  paper: "#eceee9"
  ink: "#22262a"
  ink-2: "#6c726f"
  signal: "{colors.ink-2}"
  dark-paper: "#1f2225"
  dark-ink: "#dcd9d1"
  dark-ink-2: "#979892"
  dark-signal: "{colors.dark-ink-2}"
typography:
  name:
    fontFamily: system-ui
    fontSize: 13.5px
    fontWeight: 500
    lineHeight: 1.2
  trial:
    fontFamily: Commit Mono
    fontSize: 10px
    fontWeight: "400"
    lineHeight: 1
rounded:
  sm: 2px
spacing:
  space-1: 2px
  track-height: 96px
  stroke-clip-selected: 1.75px
---

## Overview

Prose with a --- line in it.

---
`;

/** DESIGN.md with one line of the front matter replaced. */
function withLine(from: string, to: string): string {
  if (!DESIGN.includes(from)) throw new Error(`No line ${from}`);
  return DESIGN.replace(from, to);
}

function errorFrom(markdown: string): string {
  try {
    readDesign(markdown);
  } catch (error) {
    expect(error).toBeInstanceOf(DesignTokenError);
    return (error as Error).message;
  }
  throw new Error("readDesign didn't throw");
}

/** The declarations in the first block that `selector` opens. */
function block(css: string, selector: string): string {
  const start = css.indexOf(`${selector} {`);
  expect(start).toBeGreaterThanOrEqual(0);
  return css.slice(start, css.indexOf("}", start));
}

describe("readDesign", () => {
  it("splits colours into light and dark by the dark- prefix, and resolves references", () => {
    const tokens = readDesign(DESIGN);
    expect(tokens.colors.light).toEqual({
      paper: "#eceee9",
      ink: "#22262a",
      "ink-2": "#6c726f",
      signal: "#6c726f",
    });
    expect(tokens.colors.dark).toEqual({
      paper: "#1f2225",
      ink: "#dcd9d1",
      "ink-2": "#979892",
      signal: "#979892",
    });
  });

  it("reads type, corners and spacing as numbers of pixels", () => {
    const tokens = readDesign(DESIGN);
    expect(tokens.typography.name).toEqual({
      fontFamily: "system-ui",
      fontSize: 13.5,
      fontWeight: 500,
      lineHeight: 1.2,
    });
    expect(tokens.typography.trial.fontWeight).toBe(400);
    expect(tokens.rounded).toEqual({ sm: 2 });
    expect(tokens.spacing).toEqual({ "space-1": 2, "track-height": 96, "stroke-clip-selected": 1.75 });
  });

  it("resolves a reference to a reference", () => {
    const tokens = readDesign(
      withLine('  signal: "{colors.ink-2}"', '  signal: "{colors.signal-2}"\n  signal-2: "{colors.ink-2}"').replace(
        '  dark-signal: "{colors.dark-ink-2}"',
        '  dark-signal: "{colors.dark-ink-2}"\n  dark-signal-2: "#000000"',
      ),
    );
    expect(tokens.colors.light.signal).toBe("#6c726f");
  });

  it("explains a missing or malformed token", () => {
    expect(errorFrom("# No front matter")).toMatch(/no front matter/);
    expect(errorFrom("---\ncolors: [\n")).toMatch(/no closing ---/);
    expect(errorFrom("---\ncolors: [\n---\n")).toMatch(/isn't valid YAML/);
    expect(errorFrom(withLine('  dark-ink: "#dcd9d1"\n', ""))).toBe(
      "colors.ink has no dark twin: add colors.dark-ink for the dark theme.",
    );
    expect(errorFrom(withLine('  ink: "#22262a"\n', ""))).toMatch(/colors\.dark-ink has no light twin/);
    expect(errorFrom(withLine('  ink: "#22262a"', '  ink: "#22262"'))).toBe(
      'colors.ink should be a colour such as "#22262a", but it\'s "#22262".',
    );
    expect(errorFrom(withLine('  signal: "{colors.ink-2}"', '  signal: "{colors.ink-9}"'))).toBe(
      "colors.signal refers to {colors.ink-9}, which doesn't exist.",
    );
    expect(errorFrom(withLine('  signal: "{colors.ink-2}"', '  signal: "{colors.signal}"'))).toMatch(
      /colors\.signal refers to itself/,
    );
    expect(errorFrom(withLine("    fontSize: 13.5px\n", ""))).toBe("typography.name is missing fontSize.");
    expect(errorFrom(withLine("    fontSize: 13.5px", "    fontSize: 13.5"))).toBe(
      'typography.name.fontSize should be a size in pixels, such as "24px", but it\'s 13.5.',
    );
    expect(errorFrom(withLine("    lineHeight: 1.2", "    lineHeight: 16px"))).toMatch(
      /typography\.name\.lineHeight should be a multiple of the font size/,
    );
    expect(errorFrom(withLine("  track-height: 96px", "  track-height: 96"))).toMatch(
      /spacing\.track-height should be a size in pixels/,
    );
    expect(errorFrom(withLine("spacing:\n", "padding:\n"))).toBe(
      "DESIGN.md's front matter has no spacing group.",
    );
    expect(errorFrom(withLine("  track-height: 96px", "  trackHeight: 96px"))).toMatch(
      /spacing\.trackHeight isn't a usable name/,
    );
    expect(errorFrom(withLine("  space-1: 2px", "  space-1: 2px\n  paper: 2px"))).toBe(
      "Two tokens give the same CSS variable, --paper: rename one.",
    );
  });
});

describe("toCss", () => {
  const css = toCss(readDesign(DESIGN));

  it("puts light colours in :root and dark ones under the system's dark mode", () => {
    expect(block(css, ":root")).toContain("--ink: #22262a;");
    expect(block(css, ":root")).toContain("--signal: #6c726f;");
    const dark = css.slice(css.indexOf("@media (prefers-color-scheme: dark)"));
    expect(block(dark, ":root")).toContain("--ink: #dcd9d1;");
    expect(block(dark, ":root")).toContain("color-scheme: dark;");
    expect(block(dark, ":root")).not.toContain("--track-height");
    expect(css).not.toContain("--dark-");
    expect(css).not.toContain("--primary");
  });

  it("writes both themes again under data-theme", () => {
    expect(block(css, ':root[data-theme="light"]')).toContain("--ink: #22262a;");
    expect(block(css, ':root[data-theme="dark"]')).toContain("--ink: #dcd9d1;");
    expect(block(css, ':root[data-theme="dark"]')).toContain("color-scheme: dark;");
  });

  it("writes type, corners, spacing and line weights as variables", () => {
    const root = block(css, ":root");
    expect(root).toContain("--font-name: 500 13.5px/1.2 system-ui;");
    expect(root).toContain("--font-name-size: 13.5px;");
    expect(root).toContain("--font-name-weight: 500;");
    expect(root).toContain('--font-trial-family: "Commit Mono";');
    expect(root).toContain("--rounded-sm: 2px;");
    expect(root).toContain("--track-height: 96px;");
    expect(root).toContain("--stroke-clip-selected: 1.75px;");
  });
});

describe("toTs", () => {
  const ts = toTs(readDesign(DESIGN));

  it("writes each theme's colours and their variables, in camelCase", () => {
    expect(ts).toContain('light: {\n    paper: "#eceee9",\n    ink: "#22262a",\n    ink2: "#6c726f",');
    expect(ts).toContain('dark: {\n    paper: "#1f2225",\n    ink: "#dcd9d1",\n    ink2: "#979892",');
    expect(ts).toContain('ink2: "--ink-2",');
  });

  it("writes type, corners, spacing and line weights as numbers", () => {
    expect(ts).toContain(
      'name: {\n    fontFamily: "system-ui",\n    fontSize: 13.5,\n    fontWeight: 500,\n    lineHeight: 1.2,\n    font: "500 13.5px system-ui",\n  },',
    );
    expect(ts).toContain('font: "400 10px \\"Commit Mono\\"",');
    expect(ts).toContain("sm: 2,");
    expect(ts).toContain("trackHeight: 96,");
    expect(ts).toContain("strokeClipSelected: 1.75,");
  });
});

describe("the committed token files", () => {
  const at = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");

  it("match DESIGN.md", () => {
    const tokens = readDesign(at("../../DESIGN.md"));
    expect(at("../src/design/tokens.css")).toBe(toCss(tokens));
    expect(at("../src/design/tokens.ts")).toBe(toTs(tokens));
  });
});
