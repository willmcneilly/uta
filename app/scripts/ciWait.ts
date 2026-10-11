// `scripts/ci-wait`: waits for a PR's CI on one commit and says how it went.
// ciWaitCli.ts has the command line and its help.

import type { CheckRun } from "./github.ts";
import { shortSha } from "./git.ts";

/** Exit codes, as the help and pr-reviewer.md give them. */
export const GREEN = 0;
export const RED = 1;
export const OTHER_COMMIT = 2;
export const TIMED_OUT = 3;

/** Conclusions that don't fail a PR. */
const PASSING = new Set(["success", "neutral", "skipped"]);

/** A re-run job is a new check run with the same name: keep the newest of each. */
export function latestPerName(runs: readonly CheckRun[]): CheckRun[] {
  const latest = new Map<string, CheckRun>();
  for (const run of runs) {
    const seen = latest.get(run.name);
    if (!seen || (run.started_at ?? "") > (seen.started_at ?? "")) latest.set(run.name, run);
  }
  return [...latest.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export type State = "none" | "running" | "passed" | "failed";

/**
 * Red as soon as any check fails, even with others still running: the
 * reviewer then runs the checks locally, so waiting for the rest gains
 * nothing.
 */
export function judge(runs: readonly CheckRun[]): State {
  if (runs.length === 0) return "none";
  if (runs.some((run) => run.status === "completed" && !PASSING.has(run.conclusion ?? ""))) return "failed";
  if (runs.some((run) => run.status !== "completed")) return "running";
  return "passed";
}

function result(run: CheckRun): string {
  if (run.status !== "completed") return run.status === "in_progress" ? "running" : run.status;
  return PASSING.has(run.conclusion ?? "") ? (run.conclusion ?? "") : `FAILED (${run.conclusion})`;
}

function minutes(run: CheckRun): string {
  if (!run.started_at || !run.completed_at) return "";
  const seconds = (Date.parse(run.completed_at) - Date.parse(run.started_at)) / 1000;
  return `${Math.floor(seconds / 60)}m${String(Math.round(seconds % 60)).padStart(2, "0")}s`;
}

/** One line per check: its name, result, commit, time and link. */
export function table(runs: readonly CheckRun[]): string {
  const width = Math.max(0, ...runs.map((run) => run.name.length));
  const resultWidth = Math.max(0, ...runs.map((run) => result(run).length));
  const timeWidth = Math.max(0, ...runs.map((run) => minutes(run).length));
  return runs
    .map((run) =>
      [run.name.padEnd(width), result(run).padEnd(resultWidth), shortSha(run.head_sha), minutes(run).padEnd(timeWidth), run.html_url]
        .join("  ")
        .trimEnd(),
    )
    .join("\n");
}

export type WaitOptions = {
  /** Wait for this commit's checks instead of the PR's head. */
  commit?: string;
  timeoutMs: number;
  /** How long a commit may go without any checks before it counts as having none. */
  graceMs: number;
  intervalMs: number;
};

export type WaitDeps = {
  head: () => string;
  /** The PR's commits, oldest first. */
  commits: () => string[];
  runs: (sha: string) => CheckRun[];
  sleep: (ms: number) => Promise<void>;
  now: () => number;
  print: (text: string) => void;
};

/** The newest of the PR's other commits that has checks, if any. */
function newestWithChecks(deps: WaitDeps, except: string): { sha: string; runs: CheckRun[] } | undefined {
  for (const sha of [...deps.commits()].reverse()) {
    if (sha === except) continue;
    const runs = latestPerName(deps.runs(sha));
    if (runs.length > 0) return { sha, runs };
  }
  return undefined;
}

export async function ciWait(pr: number, options: WaitOptions, deps: WaitDeps): Promise<number> {
  const started = deps.now();
  const target = options.commit ?? deps.head();
  deps.print(`PR #${pr}: waiting for CI on ${shortSha(target)}${options.commit ? "" : " (its head)"}.`);
  let progress = "";
  for (;;) {
    if (!options.commit) {
      const head = deps.head();
      if (head !== target) {
        deps.print(`The PR's head moved to ${shortSha(head)} while waiting; these checks are for ${shortSha(target)}.`);
        deps.print(table(latestPerName(deps.runs(target))));
        return OTHER_COMMIT;
      }
    }
    const runs = latestPerName(deps.runs(target));
    const state = judge(runs);
    const elapsed = deps.now() - started;
    if (state === "passed" || state === "failed") {
      deps.print(table(runs));
      deps.print(state === "passed" ? `All ${runs.length} checks passed on ${shortSha(target)}.` : `CI failed on ${shortSha(target)}.`);
      return state === "passed" ? GREEN : RED;
    }
    if (state === "none" && elapsed >= Math.min(options.graceMs, options.timeoutMs)) {
      deps.print(`No checks have started for ${shortSha(target)}.`);
      const other = newestWithChecks(deps, target);
      if (other) {
        deps.print(`The newest checks on this PR are for ${shortSha(other.sha)}:`);
        deps.print(table(other.runs));
      }
      return OTHER_COMMIT;
    }
    if (elapsed >= options.timeoutMs) {
      deps.print(table(runs));
      deps.print(`Timed out after ${Math.round(elapsed / 60000)} minutes; CI is still running. Run it again to keep waiting.`);
      return TIMED_OUT;
    }
    const running = runs.filter((run) => run.status !== "completed").map((run) => run.name);
    const now =
      state === "none" ? "waiting for checks to start" : `${runs.length - running.length} of ${runs.length} done; running: ${running.join(", ")}`;
    if (now !== progress) deps.print(now);
    progress = now;
    await deps.sleep(options.intervalMs);
  }
}

export type Args = { pr: number; commit?: string; timeoutMinutes: number };

export function parseArgs(argv: readonly string[]): Args | "help" | string {
  if (argv.includes("--help") || argv.includes("-h")) return "help";
  let pr: number | undefined;
  let commit: string | undefined;
  let timeoutMinutes = 9;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--commit") commit = argv[++i];
    else if (arg === "--timeout") timeoutMinutes = Number(argv[++i]);
    else if (/^\d+$/.test(arg) && pr === undefined) pr = Number(arg);
    else return `Unknown argument: ${arg}`;
  }
  if (pr === undefined) return "Give the PR number.";
  if (commit !== undefined && !/^[0-9a-f]{7,40}$/.test(commit)) return "--commit takes a commit SHA.";
  if (!(timeoutMinutes > 0)) return "--timeout takes a number of minutes.";
  return { pr, commit, timeoutMinutes };
}
