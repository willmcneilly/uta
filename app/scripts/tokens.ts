// `npm run tokens`: writes app/src/design/tokens.css and tokens.ts from the
// repository's DESIGN.md. CI runs it and fails if the committed files differ.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { DesignTokenError, readDesign, toCss, toTs } from "./designTokens.ts";

const at = (path: string) => fileURLToPath(new URL(path, import.meta.url));
const design = at("../../DESIGN.md");
const css = at("../src/design/tokens.css");
const ts = at("../src/design/tokens.ts");

try {
  const tokens = readDesign(readFileSync(design, "utf8"));
  writeFileSync(css, toCss(tokens));
  writeFileSync(ts, toTs(tokens));
  console.log("Wrote src/design/tokens.css and src/design/tokens.ts from DESIGN.md.");
} catch (error) {
  if (!(error instanceof DesignTokenError)) throw error;
  console.error(error.message);
  process.exitCode = 1;
}
