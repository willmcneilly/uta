/* eslint-disable uta/no-raw-colour -- the colours here are test data */
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { type Server, createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DESIGN_ENDPOINT, FONTS_ENDPOINT, type ServedDesign } from "./designEndpoints";
import { designRequests, fontFromFile, listFonts, saveDesign } from "./designServer";
import { readDesign, toCss } from "./designTokens";
import type { DesignFiles } from "./writeTokens";

const at = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");

describe("fontFromFile", () => {
  it("reads the family, weight and style from the file's name", () => {
    expect(fontFromFile("CommitMono-Bold.otf")).toEqual({
      file: "CommitMono-Bold.otf",
      family: "CommitMono",
      weight: "700",
      style: "normal",
    });
    expect(fontFromFile("Martian-SemiBoldItalic.woff2")).toMatchObject({ weight: "600", style: "italic" });
    expect(fontFromFile("Inter-ExtraBold.ttf")).toMatchObject({ weight: "800" });
    expect(fontFromFile("Plain.woff")).toMatchObject({ family: "Plain", weight: "400" });
  });

  it("gives a variable font every weight", () => {
    expect(fontFromFile("Martian[wdth,wght].ttf")).toMatchObject({ family: "Martian", weight: "100 900" });
    expect(fontFromFile("Foo-VF.woff2")).toMatchObject({ family: "Foo", weight: "100 900" });
  });

  it("skips files that aren't fonts", () => {
    expect(fontFromFile("OFL.txt")).toBeNull();
    expect(fontFromFile(".DS_Store")).toBeNull();
  });
});

describe("the dev server's design endpoints", () => {
  let root: string;
  let files: DesignFiles;
  let repo: string;
  let trial: string;
  let server: Server;
  let base: string;

  beforeEach(async () => {
    root = mkdtempSync(join(tmpdir(), "uta-design-"));
    files = { design: join(root, "DESIGN.md"), css: join(root, "tokens.css"), ts: join(root, "tokens.ts") };
    writeFileSync(files.design, at("../../DESIGN.md"));
    repo = join(root, "repo-fonts");
    trial = join(root, "trial-fonts");
    mkdirSync(repo);
    mkdirSync(trial);
    writeFileSync(join(repo, "Free-Regular.woff2"), "free font bytes");
    writeFileSync(join(trial, "Trial-Bold.otf"), "trial font bytes");
    writeFileSync(join(root, "secret.otf"), "not offered");
    const handle = designRequests({ files, fonts: { repo, trial } });
    server = createServer((request, response) =>
      handle(request, response, () => {
        response.writeHead(404);
        response.end();
      }),
    );
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });

  afterEach(async () => {
    await new Promise((resolve) => server.close(resolve));
    rmSync(root, { recursive: true, force: true });
  });

  const post = (body: unknown, headers: Record<string, string> = {}) =>
    fetch(`${base}${DESIGN_ENDPOINT}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", ...headers },
      body: JSON.stringify(body),
    });

  it("hands the panel DESIGN.md and the fonts from both folders", async () => {
    const served = (await (await fetch(`${base}${DESIGN_ENDPOINT}`)).json()) as ServedDesign;
    expect(served.markdown).toBe(readFileSync(files.design, "utf8"));
    expect(served.fonts).toEqual([
      { family: "Free", weight: "400", style: "normal", url: `${FONTS_ENDPOINT}/repo/Free-Regular.woff2` },
      { family: "Trial", weight: "700", style: "normal", url: `${FONTS_ENDPOINT}/trial/Trial-Bold.otf` },
    ]);
  });

  it("serves a trial font from its folder without copying it anywhere", async () => {
    const before = readdirSync(root, { recursive: true }).sort();
    const response = await fetch(`${base}${FONTS_ENDPOINT}/trial/Trial-Bold.otf`);
    expect(response.headers.get("content-type")).toBe("font/otf");
    expect(await response.text()).toBe("trial font bytes");
    expect(readdirSync(root, { recursive: true }).sort()).toEqual(before);
  });

  it("serves only fonts it lists", async () => {
    for (const path of ["trial/..%2Fsecret.otf", "repo/secret.otf", "other/Trial-Bold.otf"]) {
      expect((await fetch(`${base}${FONTS_ENDPOINT}/${path}`)).status).toBe(404);
    }
  });

  it("saves the values into DESIGN.md and writes the token files again", async () => {
    const response = await post({ edits: { "colors.ink": "#101418", "spacing.stroke-grid": "1.5px" } });
    expect(await response.json()).toMatchObject({ changed: true });
    const saved = readFileSync(files.design, "utf8");
    expect(saved).toContain('  ink: "#101418"');
    expect(readFileSync(files.css, "utf8")).toBe(toCss(readDesign(saved)));
    expect(readFileSync(files.ts, "utf8")).toContain("strokeGrid: 1.5,");
  });

  it("changes nothing when the same values are saved twice", async () => {
    await post({ edits: { "colors.ink": "#101418" } });
    const once = readFileSync(files.design, "utf8");
    const again = await post({ edits: { "colors.ink": "#101418" } });
    expect(await again.json()).toMatchObject({ changed: false });
    expect(readFileSync(files.design, "utf8")).toBe(once);
  });

  it("refuses a value the token script would reject, and leaves the file alone", async () => {
    const before = readFileSync(files.design, "utf8");
    const response = await post({ edits: { "spacing.stroke-grid": "thick" } });
    expect(response.status).toBe(400);
    expect(((await response.json()) as { error: string }).error).toMatch(/stroke-grid/);
    expect(readFileSync(files.design, "utf8")).toBe(before);
  });

  it("refuses a save from another page", async () => {
    const response = await post({ edits: { "colors.ink": "#000000" } }, { Origin: "https://example.com" });
    expect(response.status).toBe(403);
    const form = await fetch(`${base}${DESIGN_ENDPOINT}`, {
      method: "POST",
      headers: { "Content-Type": "text/plain" },
      body: JSON.stringify({ edits: { "colors.ink": "#000000" } }),
    });
    expect(form.status).toBe(415);
    expect(readFileSync(files.design, "utf8")).not.toContain("#000000");
  });

  it("passes on anything else", async () => {
    expect((await fetch(`${base}/src/main.tsx`)).status).toBe(404);
  });
});

describe("saveDesign and listFonts", () => {
  it("lists nothing for a folder that doesn't exist", () => {
    expect(listFonts({ repo: "/no/such/folder" })).toEqual([]);
  });

  it("doesn't write when nothing changed", () => {
    const root = mkdtempSync(join(tmpdir(), "uta-design-"));
    const files = { design: join(root, "DESIGN.md"), css: join(root, "tokens.css"), ts: join(root, "tokens.ts") };
    writeFileSync(files.design, at("../../DESIGN.md"));
    expect(saveDesign({ "colors.ink": "#22262a" }, files).changed).toBe(false);
    expect(readdirSync(root)).toEqual(["DESIGN.md"]);
    rmSync(root, { recursive: true, force: true });
  });
});
