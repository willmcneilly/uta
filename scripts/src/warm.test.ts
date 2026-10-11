import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
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

    assert.equal(relocate(dir, from, to), 2);
    assert.equal(readFileSync(debug, "utf8"), `cargo:rustc-link-search=native=${to}/debug/build/objc2-1/out\n`);
    assert.equal(readFileSync(rtsan, "utf8"), `${to}/rtsan/debug/build/tauri-2/out`);
    assert.equal(readFileSync(unrelated, "utf8"), "cargo:rerun-if-changed=build.rs\n");
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
    assert.equal(readFileSync(join(worktree, "target", "debug", "deps", "libfoo.rlib"), "utf8"), "foo");
    assert.equal(readFileSync(join(worktree, "target", "release", "deps", "libfoo.rlib"), "utf8"), "foo release");
    assert.equal(
      readFileSync(join(worktree, "target", "debug", "build", "foo-1", "output"), "utf8"),
      `${worktree}/target/debug/build/foo-1/out`,
    );
    // The main checkout's own copy is untouched.
    assert.equal(
      readFileSync(join(main, "target", "debug", "build", "foo-1", "output"), "utf8"),
      `${main}/target/debug/build/foo-1/out`,
    );
    assert.match(lines.join("\n"), /cloned target\/debug, target\/release/);
  });

  it("leaves a build that's already there alone", () => {
    write(join(worktree, "target", "debug", "deps", "libfoo.rlib"), "mine");
    warm(worktree, () => {});
    assert.equal(readFileSync(join(worktree, "target", "debug", "deps", "libfoo.rlib"), "utf8"), "mine");
    assert.equal(readFileSync(join(worktree, "target", "release", "deps", "libfoo.rlib"), "utf8"), "foo release");
  });

  it("does nothing in the main checkout itself", () => {
    const lines: string[] = [];
    warm(main, (text) => lines.push(text));
    assert.deepEqual(lines, []);
  });
});
