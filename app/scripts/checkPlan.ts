// Deciding what `scripts/check` runs. Pure functions over a tree's file list,
// so the tests can drive them without git.

import { createHash } from "node:crypto";

export type Job = {
  /** Shown in the summary, and named by other jobs' `after`. */
  name: string;
  /** The files it reads: see checkJobs.ts. */
  inputs: readonly string[];
  /** Where it runs, relative to the checkout. */
  cwd?: string;
  command: readonly string[];
  env?: Readonly<Record<string, string>>;
  /** Jobs it waits for, when they run too, because they share a build. */
  after?: readonly string[];
  /** It needs app/node_modules installed from the current lock file. */
  nodeModules?: boolean;
  /** It builds Rust, so it starts faster from a warm target/ (scripts/warm). */
  cargo?: boolean;
};

/** A file in a git tree: its blob hash and path. */
export type TreeFile = { hash: string; path: string };

export type Status =
  /** Its inputs changed, and haven't passed yet: it runs. */
  | "run"
  /** These exact inputs have passed before. */
  | "passed"
  /** Its inputs are the same as origin/main's, which CI has already checked. */
  | "unchanged";

export type Planned = { job: Job; key: string; status: Status };

const matches = (spec: string, path: string) =>
  spec.endsWith("/") ? path.startsWith(spec) : path === spec;

export function isInput(inputs: readonly string[], path: string): boolean {
  let included = false;
  for (const spec of inputs) {
    if (spec.startsWith("!")) {
      if (included && matches(spec.slice(1), path)) included = false;
    } else if (matches(spec, path)) {
      included = true;
    }
  }
  return included;
}

/**
 * A hash of the job and the content of every input, so it changes when the
 * job's command or any file it reads does, and nothing else.
 */
export function inputKey(job: Job, files: readonly TreeFile[]): string {
  const hash = createHash("sha256");
  hash.update(JSON.stringify([job.command, job.env ?? {}, job.cwd ?? "."]));
  for (const file of files) {
    if (isInput(job.inputs, file.path)) hash.update(`${file.hash} ${file.path}\n`);
  }
  return hash.digest("hex").slice(0, 40);
}

export type PlanOptions = {
  /** Run every job, whether or not its inputs changed. */
  all?: boolean;
  /** Ignore stamps: run again what has already passed. */
  rerun?: boolean;
};

export function plan(
  jobs: readonly Job[],
  tree: readonly TreeFile[],
  base: readonly TreeFile[],
  stamped: (key: string) => boolean,
  options: PlanOptions = {},
): Planned[] {
  return jobs.map((job) => {
    const key = inputKey(job, tree);
    const changed = options.all || key !== inputKey(job, base);
    const status: Status = !changed
      ? "unchanged"
      : !options.rerun && stamped(key)
        ? "passed"
        : "run";
    return { job, key, status };
  });
}
