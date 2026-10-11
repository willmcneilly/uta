// scripts/check against real git repositories in a temporary folder, with
// small jobs in place of cargo and npm.

import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
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
    expect(code).toBe(0);
    expect(out).toMatch(/with uncommitted changes/);
    expect(out).toMatch(/✓ frontend/);
    expect(out).not.toMatch(/✓ rust/);
    expect(out).toMatch(/Not needed, same files as origin\/main: rust/);
  });

  it("says so, and runs nothing, when the tree has already passed", async () => {
    write(repo, "crates/a.rs", "fn b() {}\n");
    expect((await run()).code).toBe(0);
    const started = performance.now();
    const { code, out } = await run();
    expect(code).toBe(0);
    expect(out).toMatch(/This tree already passed every check it needs: rust\.\nNothing to run\./);
    expect(performance.now() - started < 2000).toBe(true);
  });

  it("finds a stamp from another worktree with the same files", async () => {
    write(repo, "crates/a.rs", "fn b() {}\n");
    commit(repo, "Change a");
    expect((await run()).code).toBe(0);
    const other = join(dir, "other");
    git(repo, ["worktree", "add", "--quiet", "--detach", other, "HEAD"]);
    const lines: string[] = [];
    await check(other, { fetch: false }, JOBS, (text) => lines.push(text));
    expect(lines.join("\n")).toMatch(/already passed every check it needs/);
  });

  it("runs a job again when its files change after it passed", async () => {
    write(repo, "crates/a.rs", "fn b() {}\n");
    await run();
    write(repo, "crates/a.rs", "fn c() {}\n");
    expect((await run()).out).toMatch(/✓ rust/);
  });

  it("fails, names the log and stamps nothing when a job fails", async () => {
    write(repo, "app/broken", "");
    const { code, out } = await run();
    expect(code).toBe(1);
    expect(out).toMatch(/✗ frontend .* target\/check\/frontend\.log/);
    expect(out).toMatch(/1 failed: frontend/);
    expect((await run()).out).toMatch(/✗ frontend/);
  });

  it("records no pass when files change while it runs", async () => {
    const jobs: Job[] = [
      { name: "rust", inputs: ["crates/"], command: ["sh", "-c", "echo x >> crates/a.rs"] },
    ];
    write(repo, "crates/a.rs", "fn b() {}\n");
    const lines: string[] = [];
    await check(repo, { fetch: false }, jobs, (text) => lines.push(text));
    expect(lines.join("\n")).toMatch(/Files changed while the checks ran/);
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
    expect(out).toMatch(/origin\/main has 1 commit this branch doesn't:\n {4}[0-9a-f]{7} UTA-2: Something that merged meanwhile/);
  });

  it("with --all, runs every job", async () => {
    const { out } = await run({ fetch: false, all: true });
    expect(out).toMatch(/✓ rust/);
    expect(out).toMatch(/✓ frontend/);
  });

  it("says when there's nothing to check", async () => {
    write(repo, "docs/notes.md", "Notes\n");
    expect((await run()).out).toMatch(/Nothing to check/);
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
      expect(code).toBe(0);
      expect(out).toMatch(/already passed every check it needs/);
      expect(out).not.toMatch(/✓ rust/);
    });

    it("checks a commit that hasn't passed, when it's the working copy", async () => {
      const { code, out } = await run({ fetch: false, push: head() });
      expect(code).toBe(0);
      expect(out).toMatch(/✓ rust/);
    });

    it("blocks a commit that hasn't passed and isn't the working copy", async () => {
      write(repo, "crates/a.rs", "fn uncommitted() {}\n");
      const { code, out } = await run({ fetch: false, push: head() });
      expect(code).toBe(1);
      expect(out).toMatch(/hasn't passed: rust/);
      expect(out).toMatch(/working copy differs from it/);
    });

    it("blocks a commit whose check fails", async () => {
      write(repo, "app/broken", "");
      commit(repo, "Break it");
      const { code } = await run({ fetch: false, push: head() });
      expect(code).toBe(1);
    });
  });
});

describe("parseArgs", () => {
  it("reads the options", () => {
    expect(parseArgs(["--all", "--no-fetch"])).toEqual({ fetch: false, all: true });
    expect(parseArgs(["--push", "abc"])).toEqual({ fetch: true, push: "abc" });
    expect(parseArgs(["--help"])).toBe("help");
    expect(parseArgs(["--nope"]) as string).toMatch(/Unknown argument/);
  });
});
