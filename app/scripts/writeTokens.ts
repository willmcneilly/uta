// Writes app/src/design/tokens.css and tokens.ts from the repository's
// DESIGN.md. `npm run tokens` runs it, and so does the tuning panel's Save.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { readDesign, toCss, toTs } from "./designTokens.ts";

export interface DesignFiles {
  design: string;
  css: string;
  ts: string;
}

const at = (path: string) => fileURLToPath(new URL(path, import.meta.url));

/** Where DESIGN.md and the token files it generates live. */
export const DESIGN_FILES: DesignFiles = {
  design: at("../../DESIGN.md"),
  css: at("../src/design/tokens.css"),
  ts: at("../src/design/tokens.ts"),
};

/** Reads DESIGN.md and writes both token files. Throws a `DesignTokenError` if a token is wrong. */
export function writeTokens(files: DesignFiles = DESIGN_FILES): void {
  const tokens = readDesign(readFileSync(files.design, "utf8"));
  writeFileSync(files.css, toCss(tokens));
  writeFileSync(files.ts, toTs(tokens));
}
