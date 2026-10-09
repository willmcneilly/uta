// Under Node, as `npm run lint` runs: jsdom changes how Stylelint sees colours.
// @vitest-environment node
/* eslint-disable uta/no-raw-colour -- the colours here are test data */
import { ESLint } from "eslint";
import { join } from "node:path";
import stylelint from "stylelint";
import { describe, expect, it } from "vitest";
import { rawColour } from "./noRawColour";

const app = join(import.meta.dirname, "../..");
const fixture = (name: string) => join(app, "scripts/lint/fixtures", name);

describe("rawColour", () => {
  it("finds hex, colour functions and named colours", () => {
    expect(rawColour("#22262a")).toBe("#22262a");
    expect(rawColour("#fff")).toBe("#fff");
    expect(rawColour("1px solid #22262a80")).toBe("#22262a80");
    expect(rawColour("rgba(0, 0, 0, 0.5)")).toBe("rgba(");
    expect(rawColour("hsl(210deg 10% 15%)")).toBe("hsl(");
    expect(rawColour("oklch(0.5 0.1 200)")).toBe("oklch(");
    expect(rawColour("Red")).toBe("red");
  });

  it("leaves tokens and ordinary text alone", () => {
    expect(rawColour("var(--ink)")).toBeUndefined();
    expect(rawColour("Track #1")).toBeUndefined();
    expect(rawColour("&#123;")).toBeUndefined();
    expect(rawColour("the red one")).toBeUndefined();
    expect(rawColour("black keys")).toBeUndefined();
  });
});

describe("the colour check", () => {
  it("catches each colour planted in a TypeScript fixture", async () => {
    // `ignore: false` because the app's own lint skips the fixtures.
    const eslint = new ESLint({ cwd: app, ignore: false });
    const [result] = await eslint.lintFiles([fixture("rawColour.ts")]);
    const caught = result.messages
      .filter((m) => m.ruleId === "uta/no-raw-colour")
      .map((m) => m.line);
    // hex, rgb(), hsl(), a named colour and a template; not the escaped line,
    // the token or the words.
    expect(caught).toEqual([2, 3, 4, 5, 6]);
    expect(result.messages.filter((m) => m.ruleId !== "uta/no-raw-colour")).toEqual([]);
  });

  it("catches each colour planted in a CSS fixture", async () => {
    const { results } = await stylelint.lint({ cwd: app, files: [fixture("rawColour.css")] });
    const caught = results[0].warnings
      .toSorted((a, b) => a.line - b.line)
      .map((w) => [w.line, w.rule]);
    expect(caught).toEqual([
      [3, "color-no-hex"],
      [7, "function-disallowed-list"],
      [11, "function-disallowed-list"],
      [15, "color-named"],
    ]);
  });

  it("allows the generated token files", async () => {
    const eslint = new ESLint({ cwd: app });
    const [ts] = await eslint.lintFiles([join(app, "src/design/tokens.ts")]);
    expect(ts.messages).toEqual([]);
    const { results } = await stylelint.lint({
      cwd: app,
      files: [join(app, "src/design/tokens.css")],
      allowEmptyInput: true,
    });
    expect(results.flatMap((r) => r.warnings)).toEqual([]);
  });
});
