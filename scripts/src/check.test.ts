// scripts/check against real git repositories in a temporary folder, with
// small jobs in place of cargo and npm.

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { check, parseArgs } from "./check.ts";
import type { Job } from "./checkPlan.ts";
import { git } from "./git.ts";

const JOBS: Job[] = [
  { name: "rust", inputs: ["crates/"], command: ["sh", "-c", "echo rust ran"] },
  { name: "frontend", inputs: ["app/"], command: ["sh", "-c", "test ! -e app/broken"] },
];

let dir: string;
let origin: string;
let repo: string;

function write(root: string, path: string, text: string) {
  mkdirSync(dirname(join(root, path)), { recursive: true });
  writeFileSync(join(root, path), text);
}

function commit(root: string, message: string) {
  git(root, ["add", "--all"]);
  git(root, ["commit", "--quiet", "-m", message]);
}

/** Runs check, and returns its exit code and what it printed. */
async function run(options: Parameters<typeof check>[1] = { fetch: true }) {
  const lines: string[] = [];
  const code = await check(repo, options, JOBS, (text) => lines.push(text));
  return { code, out: lines.join("\n") };
}

beforeEach(() => {
  dir = realpathSync(mkdtempSync(join(tmpdir(), "uta-check-")));
  origin = join(dir, "origin.git");
  repo = join(dir, "repo");
  git(dir, ["init", "--quiet", "--bare", "--initial-branch=main", origin]);
  git(dir, ["clone", "--quiet", origin, repo]);
  git(repo, ["config", "user.email", "test@example.com"]);
  git(repo, ["config", "user.name", "Test"]);
  write(repo, "crates/a.rs", "fn a() {}\n");
  write(repo, "app/a.ts", "export {};\n");
  write(repo, ".gitignore", "/target/\n");
  commit(repo, "Start");
  git(repo, ["push", "--quiet", "origin", "HEAD:main"]);
  git(repo, ["switch", "--quiet", "-c", "uta-1-test"]);
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

describe("scripts/check", () => {
  it("runs only the jobs whose files changed, uncommitted ones included", async () => {
    write(repo, "app/a.ts", "export const a = 1;\n");
    const { code, out } = await run();
    assert.equal(code, 0);
    assert.match(out, /with uncommitted changes/);
    assert.match(out, /✓ frontend/);
    assert.doesNotMatch(out, /✓ rust/);
    assert.match(out, /Not needed, same files as origin\/main: rust/);
  });

  it("says so, and runs nothing, when the tree has already passed", async () => {
    write(repo, "crates/a.rs", "fn b() {}\n");
    assert.equal((await run()).code, 0);
    const started = performance.now();
    const { code, out } = await run();
    assert.equal(code, 0);
    assert.match(out, /This tree already passed every check it needs: rust\.\nNothing to run\./);
    assert.ok(performance.now() - started < 2000);
  });

  it("finds a stamp from another worktree with the same files", async () => {
    write(repo, "crates/a.rs", "fn b() {}\n");
    commit(repo, "Change a");
    assert.equal((await run()).code, 0);
    const other = join(dir, "other");
    git(repo, ["worktree", "add", "--quiet", "--detach", other, "HEAD"]);
    const lines: string[] = [];
    await check(other, { fetch: false }, JOBS, (text) => lines.push(text));
    assert.match(lines.join("\n"), /already passed every check it needs/);
  });

  it("runs a job again when its files change after it passed", async () => {
    write(repo, "crates/a.rs", "fn b() {}\n");
    await run();
    write(repo, "crates/a.rs", "fn c() {}\n");
    assert.match((await run()).out, /✓ rust/);
  });

  it("fails, names the log and stamps nothing when a job fails", async () => {
    write(repo, "app/broken", "");
    const { code, out } = await run();
    assert.equal(code, 1);
    assert.match(out, /✗ frontend .* target\/check\/frontend\.log/);
    assert.match(out, /1 failed: frontend/);
    assert.match((await run()).out, /✗ frontend/);
  });

  it("records no pass when files change while it runs", async () => {
    const jobs: Job[] = [
      { name: "rust", inputs: ["crates/"], command: ["sh", "-c", "echo x >> crates/a.rs"] },
    ];
    write(repo, "crates/a.rs", "fn b() {}\n");
    const lines: string[] = [];
    await check(repo, { fetch: false }, jobs, (text) => lines.push(text));
    assert.match(lines.join("\n"), /Files changed while the checks ran/);
  });

  it("warns when origin/main has moved on, and lists the commits", async () => {
    const other = join(dir, "other");
    git(dir, ["clone", "--quiet", origin, other]);
    git(other, ["config", "user.email", "test@example.com"]);
    git(other, ["config", "user.name", "Test"]);
    write(other, "docs/new.md", "New\n");
    commit(other, "UTA-2: Something that merged meanwhile");
    git(other, ["push", "--quiet", "origin", "HEAD:main"]);

    write(repo, "app/a.ts", "export const a = 2;\n");
    const { out } = await run();
    assert.match(out, /origin\/main has 1 commit this branch doesn't:\n {4}[0-9a-f]{7} UTA-2: Something that merged meanwhile/);
  });

  it("with --all, runs every job", async () => {
    const { out } = await run({ fetch: false, all: true });
    assert.match(out, /✓ rust/);
    assert.match(out, /✓ frontend/);
  });

  it("says when there's nothing to check", async () => {
    write(repo, "docs/notes.md", "Notes\n");
    assert.match((await run()).out, /Nothing to check/);
  });

  describe("--push, for the pre-push hook", () => {
    beforeEach(() => {
      write(repo, "crates/a.rs", "fn b() {}\n");
      commit(repo, "Change a");
    });

    const head = () => git(repo, ["rev-parse", "HEAD"]);

    it("passes a commit that has already passed, without running anything", async () => {
      await run();
      const { code, out } = await run({ fetch: false, push: head() });
      assert.equal(code, 0);
      assert.match(out, /already passed every check it needs/);
      assert.doesNotMatch(out, /✓ rust/);
    });

    it("checks a commit that hasn't passed, when it's the working copy", async () => {
      const { code, out } = await run({ fetch: false, push: head() });
      assert.equal(code, 0);
      assert.match(out, /✓ rust/);
    });

    it("blocks a commit that hasn't passed and isn't the working copy", async () => {
      write(repo, "crates/a.rs", "fn uncommitted() {}\n");
      const { code, out } = await run({ fetch: false, push: head() });
      assert.equal(code, 1);
      assert.match(out, /hasn't passed: rust/);
      assert.match(out, /working copy differs from it/);
    });

    it("blocks a commit whose check fails", async () => {
      write(repo, "app/broken", "");
      commit(repo, "Break it");
      const { code } = await run({ fetch: false, push: head() });
      assert.equal(code, 1);
    });
  });
});

describe("parseArgs", () => {
  it("reads the options", () => {
    assert.deepEqual(parseArgs(["--all", "--no-fetch"]), { fetch: false, all: true });
    assert.deepEqual(parseArgs(["--push", "abc"]), { fetch: true, push: "abc" });
    assert.equal(parseArgs(["--help"]), "help");
    assert.match(parseArgs(["--nope"]) as string, /Unknown argument/);
  });
});
