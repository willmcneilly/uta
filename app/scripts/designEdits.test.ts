import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { applyEdits, frontMatterRange } from "./designEdits";
import { DesignTokenError, readDesign } from "./designTokens";

const at = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");
const REAL = at("../../DESIGN.md");

const body = (markdown: string) => markdown.slice(frontMatterRange(markdown).end);

const EDITS = {
  "colors.ink": "#101418",
  "colors.dark-selected-wash": "#86a5ff40",
  "typography.name.fontFamily": "Commit Mono",
  "typography.name.fontSize": "14px",
  "typography.name.fontWeight": 600,
  "typography.position.lineHeight": 1.1,
  "spacing.stroke-clip": "1.25px",
  "spacing.hatch-gap": "6px",
};

describe("applyEdits", () => {
  it("leaves the prose byte for byte as it was", () => {
    const saved = applyEdits(REAL, EDITS);
    expect(body(saved)).toBe(body(REAL));
    expect(saved.endsWith(body(REAL))).toBe(true);
  });

  it("changes only the lines of the tokens that changed, keeping their quoting", () => {
    const before = REAL.split("\n");
    const after = applyEdits(REAL, EDITS).split("\n");
    expect(after).toHaveLength(before.length);
    const changed = after.filter((line, i) => line !== before[i]);
    expect(changed).toEqual([
      '  ink: "#101418"',
      '  dark-selected-wash: "#86a5ff40"',
      "    fontFamily: Commit Mono",
      "    fontSize: 14px",
      "    fontWeight: 600",
      "    lineHeight: 1.1",
      "  stroke-clip: 1.25px",
      "  hatch-gap: 6px",
    ]);
  });

  it("writes values the token script reads back", () => {
    const tokens = readDesign(applyEdits(REAL, EDITS));
    expect(tokens.colors.light.ink).toBe("#101418");
    // References follow what they point at.
    expect(tokens.colors.light["signal-hot"]).toBe("#101418");
    expect(tokens.typography.name).toEqual({
      fontFamily: "Commit Mono",
      fontSize: 14,
      fontWeight: 600,
      lineHeight: 1.2,
    });
    expect(tokens.spacing["stroke-clip"]).toBe(1.25);
  });

  it("changes nothing when the values are already there", () => {
    const once = applyEdits(REAL, EDITS);
    expect(applyEdits(once, EDITS)).toBe(once);
    expect(applyEdits(REAL, {})).toBe(REAL);
    expect(applyEdits(REAL, { "colors.ink": "#22262a", "spacing.track-height": "96px" })).toBe(REAL);
  });

  it("quotes a value that wouldn't read back without quotes", () => {
    const saved = applyEdits(REAL, { "typography.label.fontFamily": "#weird: name" });
    expect(readDesign(saved).typography.label.fontFamily).toBe("#weird: name");
  });

  it("refuses a path that isn't a token", () => {
    expect(() => applyEdits(REAL, { "colors.nope": "#000000" })).toThrow(DesignTokenError);
    expect(() => applyEdits(REAL, { typography: "x" })).toThrow(/isn't a token/);
  });

  it("finds the front matter's end, not a --- in the prose", () => {
    const markdown = "---\na: 1\n---\n\nProse\n---\nmore\n";
    expect(frontMatterRange(markdown)).toEqual({ start: 4, end: 9 });
    expect(applyEdits(markdown, { a: 2 })).toBe("---\na: 2\n---\n\nProse\n---\nmore\n");
  });
});
