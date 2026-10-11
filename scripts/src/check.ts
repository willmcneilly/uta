// `scripts/check`: runs the checks this change needs. See HELP below, and
// checkJobs.ts for the jobs.

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { join, relative, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { JOBS } from "./checkJobs.ts";
import { type Job, type Planned, plan } from "./checkPlan.ts";
import { commonDir, git, shortSha, topLevel, treeFiles, tryGit, workingTree } from "./git.ts";
import { warm } from "./warm.ts";

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

export type Options = {
  all?: boolean;
  rerun?: boolean;
  plan?: boolean;
  fetch?: boolean;
  push?: string;
};

type Outcome = { ok: boolean; seconds: number; log?: string; note?: string };

const NPM_CI: Job = { name: "frontend: npm ci", inputs: [], cwd: "app", command: ["npm", "ci"] };

/** Stamps older than this are cleared, so the folder doesn't grow forever. */
const STAMP_DAYS = 30;

export function parseArgs(argv: readonly string[]): Options | "help" | string {
  const options: Options = { fetch: true };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") return "help";
    else if (arg === "--all") options.all = true;
    else if (arg === "--rerun") options.rerun = true;
    else if (arg === "--plan") options.plan = true;
    else if (arg === "--no-fetch") options.fetch = false;
    else if (arg === "--push" && argv[i + 1]) options.push = argv[++i];
    else return `Unknown argument: ${arg}`;
  }
  return options;
}

/** Where a job's passes are recorded, shared by every worktree. */
const stampDir = (root: string) => join(commonDir(root), "uta-check", "stamps");

function readStamp(root: string, key: string): { at: string } | undefined {
  try {
    return JSON.parse(readFileSync(join(stampDir(root), key), "utf8"));
  } catch {
    return undefined;
  }
}

function pruneStamps(root: string) {
  const dir = stampDir(root);
  if (!existsSync(dir)) return;
  const cutoff = Date.now() - STAMP_DAYS * 24 * 3600 * 1000;
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).mtimeMs < cutoff) rmSync(path, { force: true });
  }
}

const lockHash = (root: string) =>
  createHash("sha256")
    .update(readFileSync(join(root, "app", "package-lock.json")))
    .digest("hex");

/** Written after `npm ci`, so it only runs again when the lock file changes. */
const lockMarker = (root: string) => join(root, "app", "node_modules", ".uta-lock-sha256");

function nodeModulesCurrent(root: string): boolean {
  try {
    return readFileSync(lockMarker(root), "utf8") === lockHash(root);
  } catch {
    return false;
  }
}

const slug = (name: string) => name.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "");

function runOne(root: string, job: Job, logDir: string): Promise<Outcome> {
  const log = join(logDir, `${slug(job.name)}.log`);
  const fd = openSync(log, "w");
  const started = performance.now();
  return new Promise((done) => {
    let finished = false;
    const finish = (ok: boolean) => {
      if (finished) return;
      finished = true;
      closeSync(fd);
      done({ ok, seconds: (performance.now() - started) / 1000, log });
    };
    const child = spawn(job.command[0], job.command.slice(1), {
      cwd: resolve(root, job.cwd ?? "."),
      env: { ...process.env, ...job.env },
      stdio: ["ignore", fd, fd],
    });
    child.on("error", (error) => {
      writeSync(fd, `${error.message}\n`);
      finish(false);
    });
    child.on("close", (code) => finish(code === 0));
  });
}

const seconds = (s: number) => `${s.toFixed(1)} s`.padStart(8);

function line(width: number, name: string, outcome: Outcome, root: string): string {
  const mark = outcome.ok ? "✓" : "✗";
  const time = outcome.note ?? seconds(outcome.seconds);
  const log = !outcome.ok && outcome.log ? `   ${relative(root, outcome.log)}` : "";
  return `  ${mark} ${name.padEnd(width)}  ${time}${log}`;
}

/** Runs the jobs, each as soon as the jobs it waits for are done. */
async function runJobs(
  root: string,
  toRun: readonly Planned[],
  print: (text: string) => void,
): Promise<Map<string, Outcome>> {
  const logDir = join(root, "target", "check");
  mkdirSync(logDir, { recursive: true });
  const width = Math.max(NPM_CI.name.length, ...toRun.map((p) => p.job.name.length));
  const outcomes = new Map<string, Outcome>();
  const report = (name: string, outcome: Outcome) => {
    outcomes.set(name, outcome);
    print(line(width, name, outcome, root));
  };

  let npm: Promise<Outcome> | undefined;
  if (toRun.some((p) => p.job.nodeModules) && !nodeModulesCurrent(root)) {
    npm = runOne(root, NPM_CI, logDir).then((outcome) => {
      if (outcome.ok) writeFileSync(lockMarker(root), lockHash(root));
      report(NPM_CI.name, outcome);
      return outcome;
    });
  }

  const running = new Map<string, Promise<Outcome>>();
  for (const { job } of toRun) {
    const waits = (job.after ?? []).flatMap((name) => running.get(name) ?? []);
    running.set(
      job.name,
      (async () => {
        if (job.nodeModules && npm && !(await npm).ok) {
          const outcome = { ok: false, seconds: 0, note: "not run: npm ci failed" };
          report(job.name, outcome);
          return outcome;
        }
        await Promise.all(waits);
        const outcome = await runOne(root, job, logDir);
        report(job.name, outcome);
        return outcome;
      })(),
    );
  }
  await Promise.all([...running.values(), npm]);
  return outcomes;
}

/** The commits on origin/main since `base`, one line each. */
function mainCommits(root: string, base: string, until = "origin/main"): string[] {
  const out = tryGit(root, ["log", "--oneline", "--no-decorate", `${base}..${until}`]);
  return out ? out.split("\n") : [];
}

function warnMoved(print: (text: string) => void, commits: readonly string[], heading: string) {
  if (commits.length === 0) return;
  print(`\n⚠ ${heading}`);
  for (const commit of commits.slice(0, 20)) print(`    ${commit}`);
  if (commits.length > 20) print(`    … and ${commits.length - 20} more`);
  print("  Bring main in before you open or update the PR, then run scripts/check again.");
}

function fetchMain(root: string, print: (text: string) => void) {
  try {
    git(root, ["fetch", "--quiet", "origin", "main"]);
  } catch {
    print("⚠ Couldn't fetch origin/main, so this compares against the copy already here.");
  }
}

function resolveBase(root: string, commit: string): string | undefined {
  return (
    tryGit(root, ["merge-base", "origin/main", commit]) ??
    tryGit(root, ["merge-base", "main", commit])
  );
}

export async function check(
  root: string,
  options: Options,
  jobs: readonly Job[] = JOBS,
  print: (text: string) => void = console.log,
): Promise<number> {
  if (options.fetch) fetchMain(root, print);
  const head = git(root, ["rev-parse", "HEAD"]);
  const tree = workingTree(root);
  const commit = options.push ?? head;
  const checkedTree = options.push ? git(root, ["rev-parse", `${options.push}^{tree}`]) : tree;
  const base = resolveBase(root, commit);
  if (!base) {
    print("No origin/main or main to compare against.");
    return 1;
  }
  const mainAtStart = tryGit(root, ["rev-parse", "origin/main"]);
  const planned = plan(
    jobs,
    treeFiles(root, checkedTree),
    treeFiles(root, base),
    (key) => readStamp(root, key) !== undefined,
    options,
  );
  const toRun = planned.filter((p) => p.status === "run");
  const passedBefore = planned.filter((p) => p.status === "passed");
  const unchanged = planned.filter((p) => p.status === "unchanged");

  const dirty = !options.push && tree !== git(root, ["rev-parse", "HEAD^{tree}"]);
  print(
    `scripts/check: ${shortSha(commit)}${dirty ? " with uncommitted changes" : ""}, ` +
      `against its merge base with origin/main, ${shortSha(base)}`,
  );
  const moved = mainCommits(root, base);
  warnMoved(print, moved, `origin/main has ${moved.length} commit${moved.length === 1 ? "" : "s"} this branch doesn't:`);

  if (options.push && toRun.length > 0 && tree !== checkedTree) {
    print(`\n${shortSha(commit)} hasn't passed: ${toRun.map((p) => p.job.name).join(", ")}.`);
    print("Your working copy differs from it, so it can't be checked here. Commit or stash");
    print("the difference and push again, or run scripts/check on it first.");
    return 1;
  }

  if (options.plan) {
    for (const p of planned) print(`  ${p.status.padEnd(9)} ${p.job.name}`);
    return 0;
  }

  if (toRun.length === 0) {
    print("");
    if (passedBefore.length > 0) {
      print(`This tree already passed every check it needs: ${passedBefore.map((p) => p.job.name).join(", ")}.`);
      print("Nothing to run.");
    } else {
      print("Nothing to check: no job's files differ from origin/main.");
    }
    return 0;
  }

  if (toRun.some((p) => p.job.cargo)) warm(root, print);

  print("");
  const outcomes = await runJobs(root, toRun, print);

  const failed = [...outcomes].filter(([, o]) => !o.ok).map(([name]) => name);
  const treeAfter = workingTree(root);
  if (treeAfter !== tree) {
    print("\n⚠ Files changed while the checks ran, so no pass is recorded. Run it again.");
  } else {
    mkdirSync(stampDir(root), { recursive: true });
    const at = new Date().toISOString();
    for (const p of toRun) {
      if (outcomes.get(p.job.name)?.ok) {
        writeFileSync(join(stampDir(root), p.key), JSON.stringify({ job: p.job.name, tree, at }));
      }
    }
  }
  pruneStamps(root);

  const width = Math.max(...planned.map((p) => p.job.name.length), NPM_CI.name.length);
  print("\nSummary:");
  for (const [name, outcome] of outcomes) print(line(width, name, outcome, root));
  for (const p of passedBefore) {
    const at = readStamp(root, p.key)?.at;
    const when = at ? `, ${new Date(at).toLocaleString("en-GB", { dateStyle: "short", timeStyle: "short" })}` : "";
    print(line(width, p.job.name, { ok: true, seconds: 0, note: `passed before${when}` }, root));
  }
  if (unchanged.length > 0) {
    print(`  Not needed, same files as origin/main: ${unchanged.map((p) => p.job.name).join(", ")}`);
  }

  if (options.fetch) fetchMain(root, print);
  const mainAtEnd = tryGit(root, ["rev-parse", "origin/main"]);
  if (mainAtStart && mainAtEnd && mainAtStart !== mainAtEnd) {
    warnMoved(print, mainCommits(root, mainAtStart, mainAtEnd), "origin/main moved while the checks ran:");
  }

  print(
    failed.length === 0
      ? `\nAll ${toRun.length} checks this change needed passed.`
      : `\n${failed.length} failed: ${failed.join(", ")}. The summary gives each one's log.`,
  );
  return failed.length === 0 ? 0 : 1;
}

/** Points git at the committed hooks, once per repository. */
function useCommittedHooks(root: string) {
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
  useCommittedHooks(root);
  process.exitCode = await check(root, options);
}

if (import.meta.main) await main();
