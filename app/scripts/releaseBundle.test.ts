import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { findPanel } from "./releaseBundle";

describe("findPanel", () => {
  let dist: string;

  beforeEach(() => {
    dist = mkdtempSync(join(tmpdir(), "uta-dist-"));
    mkdirSync(join(dist, "assets"));
    writeFileSync(join(dist, "index.html"), "<div id=root></div>");
    writeFileSync(join(dist, "assets", "index.js"), "console.log('the app')");
  });

  afterEach(() => rmSync(dist, { recursive: true, force: true }));

  it("finds nothing in a bundle without the panel", () => {
    expect(findPanel(dist)).toEqual([]);
  });

  it("finds Tweakpane and the save endpoint in any file", () => {
    writeFileSync(join(dist, "assets", "panel.js"), 'fetch("/__uta/design")');
    writeFileSync(join(dist, "assets", "panel.css"), ".tp-rotv{color:red}");
    expect(findPanel(dist)).toEqual([
      { file: join(dist, "assets", "panel.css"), markers: ["tp-rotv"] },
      { file: join(dist, "assets", "panel.js"), markers: ["/__uta/design"] },
    ]);
  });
});
