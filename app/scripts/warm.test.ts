import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { git } from "./git.ts";
import { relocate, warm } from "./warm.ts";

let dir: string;

beforeEach(() => {
  dir = realpathSync(mkdtempSync(join(tmpdir(), "uta-warm-")));
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

function write(path: string, text: string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
}

describe("relocate", () => {
  it("points build scripts' recorded paths at the new target/, in every profile", () => {
    const from = "/main/target";
    const to = "/worktree/target";
    const debug = join(dir, "debug", "build", "objc2-1", "output");
    const rtsan = join(dir, "rtsan", "debug", "build", "tauri-2", "root-output");
    const unrelated = join(dir, "debug", "build", "plain-3", "output");
    write(debug, `cargo:rustc-link-search=native=${from}/debug/build/objc2-1/out\n`);
    write(rtsan, `${from}/rtsan/debug/build/tauri-2/out`);
    write(unrelated, "cargo:rerun-if-changed=build.rs\n");

    expect(relocate(dir, from, to)).toBe(2);
    expect(readFileSync(debug, "utf8")).toBe(`cargo:rustc-link-search=native=${to}/debug/build/objc2-1/out\n`);
    expect(readFileSync(rtsan, "utf8")).toBe(`${to}/rtsan/debug/build/tauri-2/out`);
    expect(readFileSync(unrelated, "utf8")).toBe("cargo:rerun-if-changed=build.rs\n");
  });
});

describe("warm", () => {
  let main: string;
  let worktree: string;

  beforeEach(() => {
    main = join(dir, "main");
    worktree = join(dir, "worktree");
    git(dir, ["init", "--quiet", "--initial-branch=main", main]);
    git(main, ["-c", "user.email=t@example.com", "-c", "user.name=T", "commit", "--quiet", "--allow-empty", "-m", "Start"]);
    git(main, ["worktree", "add", "--quiet", "--detach", worktree]);
    write(join(main, "target", "debug", "deps", "libfoo.rlib"), "foo");
    write(join(main, "target", "debug", "build", "foo-1", "output"), `${main}/target/debug/build/foo-1/out`);
    write(join(main, "target", "release", "deps", "libfoo.rlib"), "foo release");
  });

  it("clones the main checkout's build into a worktree that has none", () => {
    const lines: string[] = [];
    warm(worktree, (text) => lines.push(text));
    expect(readFileSync(join(worktree, "target", "debug", "deps", "libfoo.rlib"), "utf8")).toBe("foo");
    expect(readFileSync(join(worktree, "target", "release", "deps", "libfoo.rlib"), "utf8")).toBe("foo release");
    expect(readFileSync(join(worktree, "target", "debug", "build", "foo-1", "output"), "utf8")).toBe(`${worktree}/target/debug/build/foo-1/out`);
    // The main checkout's own copy is untouched.
    expect(readFileSync(join(main, "target", "debug", "build", "foo-1", "output"), "utf8")).toBe(`${main}/target/debug/build/foo-1/out`);
    expect(lines.join("\n")).toMatch(/cloned target\/debug, target\/release/);
  });

  it("leaves a build that's already there alone", () => {
    write(join(worktree, "target", "debug", "deps", "libfoo.rlib"), "mine");
    warm(worktree, () => {});
    expect(readFileSync(join(worktree, "target", "debug", "deps", "libfoo.rlib"), "utf8")).toBe("mine");
    expect(readFileSync(join(worktree, "target", "release", "deps", "libfoo.rlib"), "utf8")).toBe("foo release");
  });

  it("does nothing in the main checkout itself", () => {
    const lines: string[] = [];
    warm(main, (text) => lines.push(text));
    expect(lines).toEqual([]);
  });
});
