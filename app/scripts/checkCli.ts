// The command line for `scripts/check`, which runs it with Node. The work is
// in check.ts.

import { existsSync } from "node:fs";
import { join } from "node:path";
import { check, parseArgs } from "./check.ts";
import { git, topLevel, tryGit } from "./git.ts";

const HELP = `Usage: scripts/check [--all] [--rerun] [--plan] [--no-fetch]
       scripts/check --push <commit>

Runs the checks from CLAUDE.md that this change needs, in parallel where they
don't share a build, and prints one line per job with its time and, if it
failed, its log.

A job runs only when the files it reads differ from origin/main's (fetched
first), so a frontend change runs no Rust jobs and a Rust change no frontend
jobs. The release build runs only when app/src-tauri/ or app/package.json
changed; CI builds it on every PR. Each pass is stamped with the content it
checked, so running again on the same files is instant.

It checks the working copy as it is, uncommitted and untracked files
included. If origin/main has moved on since the branch's merge base, it says
so and lists the commits.

  --all        run every job, whether or not its files changed
  --rerun      run again even what has already passed
  --plan       say what would run, and run nothing
  --no-fetch   compare against the origin/main already fetched
  --push <c>   for the pre-push hook: pass if commit <c> has already passed
               what it needs, otherwise check it if it's the working copy
  --help       show this

Exits 0 when everything it needed passed, 1 when anything failed.
`;

/** Points git at the committed hooks, once per repository. */
function pointGitAtHooks(root: string) {
  if (!existsSync(join(root, ".githooks"))) return;
  if (tryGit(root, ["config", "--get", "core.hooksPath"]) !== undefined) return;
  git(root, ["config", "core.hooksPath", ".githooks"]);
  console.log("Set core.hooksPath to .githooks, so git push runs scripts/check (once per repository).");
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options === "help") {
    console.log(HELP);
    return;
  }
  if (typeof options === "string") {
    console.error(`${options}\n\n${HELP}`);
    process.exitCode = 2;
    return;
  }
  const root = topLevel(process.cwd());
  pointGitAtHooks(root);
  process.exitCode = await check(root, options);
}

await main();
