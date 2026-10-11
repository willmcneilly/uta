// `scripts/mutate`: breaks the code on purpose, runs the tests that should
// catch it, and always puts the code back. mutateCli.ts has the command line
// and its help.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { gitPath } from "./git.ts";

/** Exit codes, as the help and pr-reviewer.md give them. */
export const CAUGHT = 0;
export const MISSED = 1;
export const NOT_BUILT = 2;
export const INTERRUPTED = 130;
export const USAGE = 64;

/** How many times `find` occurs in `text`. */
export function occurrences(text: string, find: string): number {
  if (find === "") return 0;
  let count = 0;
  for (let at = text.indexOf(find); at !== -1; at = text.indexOf(find, at + find.length)) count++;
  return count;
}

/** cargo's line when the mutated code didn't compile, so no test ran. */
export const didNotBuild = (output: string) => /^error: could not compile/m.test(output);

/**
 * A mutation in progress is written down before the file changes, so a run
 * that was killed outright (and couldn't restore it) is put right by the
 * next one, or by `scripts/review-setup`.
 */
type Pending = { pid: number; file: string; original: string; mutated: string };

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}

const pendingDir = (root: string) => gitPath(root, "uta-mutate");

/**
 * Puts back any file a killed run left mutated. A file that has changed
 * since (edited, or reset by git) is left alone, and so is a run that is
 * still going.
 */
export function restorePending(root: string, print: (text: string) => void): void {
  const dir = pendingDir(root);
  if (!existsSync(dir)) return;
  for (const name of readdirSync(dir)) {
    const record = join(dir, name);
    const pending = JSON.parse(readFileSync(record, "utf8")) as Pending;
    if (pending.pid !== process.pid && alive(pending.pid)) continue;
    const shown = relative(root, pending.file);
    if (existsSync(pending.file) && readFileSync(pending.file, "utf8") === pending.mutated) {
      writeFileSync(pending.file, pending.original);
      print(`Restored ${shown}, left mutated by a run that was killed.`);
    } else {
      print(`Not restoring ${shown}: it has changed since a killed run mutated it.`);
    }
    rmSync(record);
  }
}

export type Run = {
  /** The checkout, for the record of mutations in progress. */
  root: string;
  /** Absolute. */
  file: string;
  find: string;
  replace: string;
  command: readonly string[];
  cwd: string;
};

export type Outcome = { code: number; verdict: string };

/** Keeps the end of the command's output, to tell a build failure from a test failure. */
const TAIL = 64 * 1024;

export async function mutate(run: Run, print: (text: string) => void = console.log): Promise<Outcome> {
  restorePending(run.root, print);
  const shown = relative(run.root, run.file);
  if (!existsSync(run.file)) return { code: USAGE, verdict: `No such file: ${shown}` };
  const original = readFileSync(run.file, "utf8");
  const count = occurrences(original, run.find);
  if (count !== 1) {
    return { code: USAGE, verdict: `The text to find occurs ${count} times in ${shown}; it must occur exactly once.` };
  }
  const mutated = original.replace(run.find, () => run.replace);

  mkdirSync(pendingDir(run.root), { recursive: true });
  const record = join(pendingDir(run.root), `${process.pid}.json`);
  writeFileSync(record, JSON.stringify({ pid: process.pid, file: run.file, original, mutated } satisfies Pending));
  let restored = false;
  // Its modified time is left as the restore sets it, not put back: cargo
  // would otherwise take the mutated build as up to date with the original.
  const restore = () => {
    if (restored) return;
    restored = true;
    writeFileSync(run.file, original);
    rmSync(record, { force: true });
  };
  process.once("exit", restore);
  writeFileSync(run.file, mutated);
  print(`Mutated ${shown}. Running: ${run.command.join(" ")}`);

  let interrupted = false;
  let tail = "";
  let child: ReturnType<typeof spawn> | undefined;
  // Ctrl-C reaches the whole process group, the command included; a signal
  // sent to this process alone is passed on. Either way the file is restored
  // once the command has stopped, or at once on a second signal.
  const onSignal = (signal: NodeJS.Signals) => {
    if (interrupted) restore();
    interrupted = true;
    child?.kill(signal);
  };
  const signals: NodeJS.Signals[] = ["SIGINT", "SIGTERM", "SIGHUP"];
  for (const signal of signals) process.on(signal, onSignal);
  try {
    const code = await new Promise<number | null>((resolve, reject) => {
      child = spawn(run.command[0], run.command.slice(1), { cwd: run.cwd, stdio: ["ignore", "pipe", "pipe"] });
      const keep = (chunk: Buffer, to: NodeJS.WriteStream) => {
        to.write(chunk);
        tail = (tail + chunk.toString("utf8")).slice(-TAIL);
      };
      child.stdout?.on("data", (chunk: Buffer) => keep(chunk, process.stdout));
      child.stderr?.on("data", (chunk: Buffer) => keep(chunk, process.stderr));
      child.on("error", reject);
      child.on("close", resolve);
    });
    restore();
    if (interrupted) return { code: INTERRUPTED, verdict: `INTERRUPTED. ${shown} is restored.` };
    if (code === 0) return { code: MISSED, verdict: `MISSED: the command passed with ${shown} broken.` };
    if (didNotBuild(tail)) {
      return { code: NOT_BUILT, verdict: `DIDN'T BUILD: the mutated code didn't compile, so no test judged it.` };
    }
    return { code: CAUGHT, verdict: `CAUGHT: the command failed with ${shown} broken.` };
  } finally {
    restore();
    process.off("exit", restore);
    for (const signal of signals) process.off(signal, onSignal);
  }
}
