// Checks that the frontend a release build ships has no tuning panel in it:
// neither Tweakpane nor the code that talks to the dev server's save
// endpoint (RFC-005, "How we'll verify it"). `tauri build` embeds dist/.

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { DESIGN_ENDPOINT, FONTS_ENDPOINT } from "./designEndpoints.ts";

/** Strings that only the panel's code contains. */
export const PANEL_MARKERS = [
  // Tweakpane's root class name, in its code and its stylesheet.
  "tp-rotv",
  DESIGN_ENDPOINT,
  FONTS_ENDPOINT,
];

/** Each file under `folder` that contains a marker, with the markers it contains. */
export function findPanel(folder: string): { file: string; markers: string[] }[] {
  const found: { file: string; markers: string[] }[] = [];
  for (const file of readdirSync(folder, { recursive: true, withFileTypes: true })) {
    if (!file.isFile()) continue;
    const path = join(file.parentPath, file.name);
    const text = readFileSync(path, "latin1");
    const markers = PANEL_MARKERS.filter((marker) => text.includes(marker));
    if (markers.length > 0) found.push({ file: path, markers });
  }
  return found.sort((a, b) => a.file.localeCompare(b.file));
}
