// `npm run tokens`: writes app/src/design/tokens.css and tokens.ts from the
// repository's DESIGN.md. CI runs it and fails if the committed files differ.

import { DesignTokenError } from "./designTokens.ts";
import { writeTokens } from "./writeTokens.ts";

try {
  writeTokens();
  console.log("Wrote src/design/tokens.css and src/design/tokens.ts from DESIGN.md.");
} catch (error) {
  if (!(error instanceof DesignTokenError)) throw error;
  console.error(error.message);
  process.exitCode = 1;
}
