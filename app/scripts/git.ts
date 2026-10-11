// The git the scripts need, run synchronously in a checkout.

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, statSync, utimesSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import type { TreeFile } from "./checkPlan.ts";

export function git(cwd: string, args: readonly string[], env: NodeJS.ProcessEnv = {}): string {
  return execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 64 * 1024 * 1024,
  }).trimEnd();
}

/** Like `git`, but undefined instead of throwing when git fails. */
export function tryGit(cwd: string, args: readonly string[]): string | undefined {
  try {
    return git(cwd, args);
  } catch {
    return undefined;
  }
}

/** The top of the checkout (worktree) that `cwd` is in. */
export const topLevel = (cwd: string) => git(cwd, ["rev-parse", "--show-toplevel"]);

/** A path inside this worktree's git directory, made absolute. */
export const gitPath = (root: string, name: string) =>
  resolve(root, git(root, ["rev-parse", "--git-path", name]));

/** The git directory every worktree shares. */
export const commonDir = (root: string) => resolve(root, git(root, ["rev-parse", "--git-common-dir"]));

/** The main checkout, which the other worktrees hang off, or undefined for a bare repository. */
export function mainCheckout(root: string): string | undefined {
  const common = commonDir(root);
  return basename(common) === ".git" ? dirname(common) : undefined;
}

/**
 * The tree the working copy would commit, untracked files included and
 * ignored ones left out. It goes through a copy of the index, so the real
 * one is untouched. The copy keeps the index's modified time, which git
 * compares with each file's to spot an edit made in the same instant as the
 * last `git add` (a "racily clean" entry).
 */
export function workingTree(root: string): string {
  const index = gitPath(root, "uta-check-index");
  const real = gitPath(root, "index");
  if (existsSync(real)) {
    copyFileSync(real, index);
    const { atime, mtime } = statSync(real);
    utimesSync(index, atime, mtime);
  }
  const env = { GIT_INDEX_FILE: index };
  git(root, ["add", "--all"], env);
  return git(root, ["write-tree"], env);
}

/** Every file in a tree, sorted by path. */
export function treeFiles(root: string, treeish: string): TreeFile[] {
  const out = git(root, ["ls-tree", "-r", "-z", "--full-tree", treeish]);
  return out
    .split("\0")
    .filter(Boolean)
    .map((line) => {
      const tab = line.indexOf("\t");
      return { hash: line.slice(0, tab).split(" ")[2], path: line.slice(tab + 1) };
    });
}

export const shortSha = (sha: string) => sha.slice(0, 7);
