// `scripts/warm`: gives a new worktree a warm Rust build. See HELP below.

import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { mainCheckout, topLevel } from "./git.ts";

const HELP = `Usage: scripts/warm

Gives this worktree a warm Rust build by cloning the main checkout's target/
with APFS copy-on-write (cp -c -R). The clone takes no extra disk until cargo
changes a file, and cargo then rebuilds only what differs: usually just the
workspace's own crates.

It clones only the parts of target/ this worktree doesn't have yet (debug,
release, and so on), so it never touches a build that's already here. The
post-checkout hook runs it for every new worktree, and scripts/check runs it
before the Rust jobs.
`;

/** Folders in target/ that aren't builds. */
const SKIP = new Set(["check", "tmp"]);

/** Folders a build's `build/` folder can't be under, skipped to keep the walk short. */
const NOT_PROFILES = new Set(["deps", "incremental", ".fingerprint", "examples", "build", "bundle"]);

/**
 * Build scripts' recorded output names absolute paths into the target/ they
 * ran in (link search paths, generated files). Cargo replays it without
 * rerunning the script, so a clone would keep reading the main checkout's
 * target/. Point those paths at the clone instead.
 */
export function relocate(dir: string, from: string, to: string, depth = 0): number {
  let changed = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const path = join(dir, entry.name);
    if (entry.name === "build") {
      for (const unit of readdirSync(path)) {
        for (const name of ["output", "root-output"]) {
          const file = join(path, unit, name);
          if (!existsSync(file)) continue;
          const text = readFileSync(file, "utf8");
          const moved = text.replaceAll(`${from}/`, `${to}/`);
          if (moved !== text) {
            writeFileSync(file, moved);
            changed++;
          }
        }
      }
    } else if (depth < 3 && !NOT_PROFILES.has(entry.name)) {
      changed += relocate(path, from, to, depth + 1);
    }
  }
  return changed;
}

export function warm(root: string, print: (text: string) => void = console.log): void {
  const main = mainCheckout(root);
  if (!main || resolve(main) === resolve(root)) return;
  const from = join(main, "target");
  const to = join(root, "target");
  if (!existsSync(from)) return;
  const missing = readdirSync(from).filter(
    (entry) => !SKIP.has(entry) && !existsSync(join(to, entry)) && statSync(join(from, entry)).isDirectory(),
  );
  if (missing.length === 0) return;

  const started = performance.now();
  mkdirSync(to, { recursive: true });
  const cloned: string[] = [];
  for (const entry of missing) {
    // Cloned beside it and renamed into place, so a cargo that starts
    // meanwhile keeps its own build, and the clone is dropped.
    const staging = join(to, `.warming-${entry}`);
    rmSync(staging, { recursive: true, force: true });
    execFileSync("cp", ["-c", "-R", join(from, entry), staging]);
    relocate(staging, from, to);
    try {
      renameSync(staging, join(to, entry));
      cloned.push(entry);
    } catch {
      rmSync(staging, { recursive: true, force: true });
    }
  }
  if (cloned.length > 0) {
    const took = ((performance.now() - started) / 1000).toFixed(1);
    print(`Warmed the build: cloned ${cloned.map((c) => `target/${c}`).join(", ")} from ${main} (${took} s).`);
  }
}

if (import.meta.main) {
  if (process.argv.includes("--help") || process.argv.includes("-h")) console.log(HELP);
  else warm(topLevel(process.cwd()));
}
