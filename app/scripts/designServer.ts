// The tuning panel's side of the Vite dev server. It hands the panel
// DESIGN.md and the fonts to try, and saves the panel's changes back into
// DESIGN.md. It's a Vite plugin that only applies to the dev server, so a
// release build has nothing to save to (RFC-005, part 3).

import { createReadStream, readFileSync, readdirSync, writeFileSync } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { extname, join } from "node:path";
import type { Plugin } from "vite";
import { type TokenEdits, applyEdits } from "./designEdits.ts";
import { DESIGN_ENDPOINT, FONTS_ENDPOINT, type ServedDesign } from "./designEndpoints.ts";
import { DesignTokenError, readDesign } from "./designTokens.ts";
import { DESIGN_FILES, type DesignFiles, writeTokens } from "./writeTokens.ts";

/** The folder outside the repo for trial fonts, whose files are never copied into it. */
export const TRIAL_FONTS_VARIABLE = "UTA_TRIAL_FONTS";

export interface FontFolders {
  /** Free fonts kept in the repo. */
  repo: string;
  /** Trial fonts, from outside the repo, if a folder was named. */
  trial?: string;
}

export interface FontFile {
  source: "repo" | "trial";
  file: string;
  family: string;
  /** A CSS font-weight, or a range such as "100 900" for a variable font. */
  weight: string;
  style: "normal" | "italic";
}

const FONT_TYPES: Record<string, string> = {
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".ttf": "font/ttf",
  ".otf": "font/otf",
};

const WEIGHTS: [RegExp, number][] = [
  [/hairline|thin/i, 100],
  [/extra-?light|ultra-?light/i, 200],
  [/light/i, 300],
  [/medium/i, 500],
  [/semi-?bold|demi-?bold/i, 600],
  [/extra-?bold|ultra-?bold/i, 800],
  [/bold/i, 700],
  [/black|heavy/i, 900],
];

/**
 * The family, weight and style a font file's name gives: `CommitMono-Bold.otf`
 * is CommitMono at 700, and a variable font (`Martian[wght].ttf`, `Foo-VF.woff2`)
 * covers every weight. Null if it isn't a font file.
 */
export function fontFromFile(file: string): Omit<FontFile, "source"> | null {
  const extension = extname(file).toLowerCase();
  if (!(extension in FONT_TYPES) || file.startsWith(".")) return null;
  const name = file.slice(0, -extension.length);
  const family = name.split(/[-_[]/)[0].trim();
  if (!family) return null;
  const variant = name.slice(family.length);
  const variable = /\[[^\]]*wght[^\]]*\]|\bVF\b|variable/i.test(variant);
  const weight = variable
    ? "100 900"
    : String(WEIGHTS.find(([pattern]) => pattern.test(variant))?.[1] ?? 400);
  return { file, family, weight, style: /italic|oblique/i.test(variant) ? "italic" : "normal" };
}

/** Every font file in the repo's folder and the trial folder, sorted by family. */
export function listFonts(folders: FontFolders): FontFile[] {
  const read = (source: FontFile["source"], folder: string | undefined): FontFile[] => {
    if (!folder) return [];
    let files: string[];
    try {
      files = readdirSync(folder);
    } catch {
      return [];
    }
    return files.flatMap((file) => {
      const font = fontFromFile(file);
      return font ? [{ source, ...font }] : [];
    });
  };
  return [...read("repo", folders.repo), ...read("trial", folders.trial)].sort(
    (a, b) => a.family.localeCompare(b.family) || a.file.localeCompare(b.file),
  );
}

/**
 * Saves `edits` into DESIGN.md, changing only those values in its front
 * matter, then writes the token files again. Nothing is written if the values
 * are already there, or if the result has a token the script would reject.
 */
export function saveDesign(edits: TokenEdits, files: DesignFiles = DESIGN_FILES): { markdown: string; changed: boolean } {
  const before = readFileSync(files.design, "utf8");
  const after = applyEdits(before, edits);
  if (after === before) return { markdown: before, changed: false };
  readDesign(after);
  writeFileSync(files.design, after);
  writeTokens(files);
  return { markdown: after, changed: true };
}

export interface DesignServerOptions {
  files?: DesignFiles;
  fonts?: FontFolders;
}

/** Handles the panel's requests, or passes anything else on. */
export function designRequests(options: DesignServerOptions = {}) {
  const files = options.files ?? DESIGN_FILES;
  const folders = options.fonts ?? defaultFontFolders();
  return (request: IncomingMessage, response: ServerResponse, next: () => void): void => {
    const path = (request.url ?? "").split("?")[0];
    if (path === DESIGN_ENDPOINT) {
      if (request.method === "GET") {
        const served: ServedDesign = {
          markdown: readFileSync(files.design, "utf8"),
          fonts: listFonts(folders).map(({ source, file, family, weight, style }) => ({
            family,
            weight,
            style,
            url: `${FONTS_ENDPOINT}/${source}/${encodeURIComponent(file)}`,
          })),
        };
        json(response, 200, served);
      } else if (request.method === "POST") {
        void save(request, response, files);
      } else {
        json(response, 405, { error: "Use GET or POST." });
      }
      return;
    }
    const font = new RegExp(`^${FONTS_ENDPOINT}/(repo|trial)/([^/]+)$`).exec(path);
    if (font && request.method === "GET") {
      const [, source, encoded] = font;
      const file = safeDecode(encoded);
      // Only a file the listing offers, so nothing else on disk can be read.
      const found = listFonts(folders).find((f) => f.source === source && f.file === file);
      const folder = source === "repo" ? folders.repo : folders.trial;
      if (!found || !folder) {
        json(response, 404, { error: "No such font." });
        return;
      }
      response.writeHead(200, {
        "Content-Type": FONT_TYPES[extname(file).toLowerCase()],
        "Cache-Control": "no-store",
      });
      createReadStream(join(folder, found.file)).pipe(response);
      return;
    }
    next();
  };
}

/** The Vite plugin: the panel's endpoints, on the dev server only. */
export function designTuning(options: DesignServerOptions = {}): Plugin {
  return {
    name: "uta-design-tuning",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(designRequests(options));
    },
  };
}

function defaultFontFolders(): FontFolders {
  return {
    repo: join(DESIGN_FILES.css, "..", "fonts"),
    trial: process.env[TRIAL_FONTS_VARIABLE] || undefined,
  };
}

const MAX_BODY = 1 << 20;

async function save(request: IncomingMessage, response: ServerResponse, files: DesignFiles) {
  // Only the app's own page may save: a JSON body and a matching origin keep
  // other web pages from posting to the dev server.
  const origin = request.headers.origin;
  if (origin !== undefined && origin !== `http://${request.headers.host}`) {
    json(response, 403, { error: "Only the app can save." });
    return;
  }
  if (!request.headers["content-type"]?.startsWith("application/json")) {
    json(response, 415, { error: "Send JSON." });
    return;
  }
  let body = "";
  for await (const chunk of request) {
    body += chunk;
    if (body.length > MAX_BODY) {
      json(response, 413, { error: "Too much to save." });
      return;
    }
  }
  try {
    const { edits } = JSON.parse(body) as { edits?: unknown };
    if (typeof edits !== "object" || edits === null || Array.isArray(edits)) {
      json(response, 400, { error: "Send { edits: { path: value } }." });
      return;
    }
    json(response, 200, saveDesign(edits as TokenEdits, files));
  } catch (error) {
    if (error instanceof DesignTokenError || error instanceof SyntaxError) {
      json(response, 400, { error: error.message });
    } else {
      json(response, 500, { error: String(error) });
    }
  }
}

function json(response: ServerResponse, status: number, value: unknown): void {
  response.writeHead(status, { "Content-Type": "application/json" });
  response.end(JSON.stringify(value));
}

function safeDecode(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return "";
  }
}
