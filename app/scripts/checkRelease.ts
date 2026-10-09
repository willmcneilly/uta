// `npm run check:release`: fails if the built frontend in dist/ contains the
// tuning panel. CI runs it after `tauri build`.

import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { findPanel } from "./releaseBundle.ts";

const dist = fileURLToPath(new URL("../dist", import.meta.url));
if (!existsSync(dist)) {
  console.error("No dist/ to check: build the app first.");
  process.exitCode = 1;
} else {
  const found = findPanel(dist);
  for (const { file, markers } of found) console.error(`${file} contains ${markers.join(", ")}`);
  if (found.length > 0) {
    console.error("The release build contains the tuning panel.");
    process.exitCode = 1;
  } else {
    console.log("The release build contains neither Tweakpane nor the save endpoint's code.");
  }
}
