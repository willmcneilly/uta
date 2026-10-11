import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { JOBS } from "./checkJobs.ts";
import { type Job, type TreeFile, inputKey, isInput, plan } from "./checkPlan.ts";

/** A small tree shaped like Uta's. */
const BASE: TreeFile[] = [
  { hash: "1", path: "Cargo.toml" },
  { hash: "2", path: "Cargo.lock" },
  { hash: "3", path: "DESIGN.md" },
  { hash: "4", path: "app/package.json" },
  { hash: "5", path: "app/src/App.tsx" },
  { hash: "6", path: "app/src-tauri/src/lib.rs" },
  { hash: "7", path: "crates/uta-core/src/lib.rs" },
  { hash: "8", path: "crates/uta-engine/src/lib.rs" },
  { hash: "9", path: "docs/plans/agent-tooling.md" },
  { hash: "a", path: "examples/demo-loop.json" },
  { hash: "b", path: "scripts/src/check.ts" },
  { hash: "c", path: "app/src/synth/filter-response.json" },
];

/** BASE with the given files' content changed. */
const changing = (...paths: string[]) =>
  BASE.map((file) => (paths.includes(file.path) ? { ...file, hash: `${file.hash}-new` } : file));

const running = (tree: TreeFile[], stamped = (_key: string) => false) =>
  plan(JOBS, tree, BASE, stamped)
    .filter((p) => p.status === "run")
    .map((p) => p.job.name);

const RUST = ["rust: fmt", "rust: clippy", "rust: test"];
const FRONTEND = [
  "frontend: tokens",
  "frontend: designmd",
  "frontend: typecheck",
  "frontend: lint",
  "frontend: test",
];

describe("isInput", () => {
  it("matches a folder by prefix and a file exactly", () => {
    assert.ok(isInput(["crates/"], "crates/uta-core/src/lib.rs"));
    assert.ok(isInput(["Cargo.toml"], "Cargo.toml"));
    assert.ok(!isInput(["Cargo.toml"], "app/src-tauri/Cargo.toml"));
    assert.ok(!isInput(["crates/"], "crates"));
  });

  it("takes an excluded folder back out", () => {
    const frontend = ["app/", "!app/src-tauri/", "DESIGN.md"];
    assert.ok(isInput(frontend, "app/src/App.tsx"));
    assert.ok(!isInput(frontend, "app/src-tauri/src/lib.rs"));
    assert.ok(isInput(frontend, "DESIGN.md"));
  });
});

describe("plan", () => {
  it("runs no Rust jobs for a frontend-only change", () => {
    assert.deepEqual(running(changing("app/src/App.tsx")), FRONTEND);
  });

  it("runs no frontend jobs for a Rust-only change", () => {
    assert.deepEqual(running(changing("crates/uta-core/src/lib.rs")), [...RUST, "rtsan"]);
  });

  it("runs RTSan only when what its tests build changed", () => {
    assert.deepEqual(running(changing("app/src-tauri/src/lib.rs")), [...RUST, "app: release build"]);
  });

  it("runs the release build only for the Tauri side or the npm manifest", () => {
    assert.ok(!running(changing("app/src/App.tsx")).includes("app: release build"));
    assert.ok(!running(changing("Cargo.lock")).includes("app: release build"));
    assert.deepEqual(running(changing("app/package.json")), [...FRONTEND, "app: release build"]);
  });

  it("runs both for a file both sides read", () => {
    assert.deepEqual(running(changing("app/src/synth/filter-response.json")), [
      ...RUST,
      "rtsan",
      ...FRONTEND,
    ]);
  });

  it("runs the Rust jobs when a command list the tests include changed", () => {
    assert.deepEqual(running(changing("examples/demo-loop.json")), [...RUST, "rtsan"]);
  });

  it("runs the frontend jobs for DESIGN.md", () => {
    assert.deepEqual(running(changing("DESIGN.md")), FRONTEND);
  });

  it("runs the scripts' own checks for a script change", () => {
    assert.deepEqual(running(changing("scripts/src/check.ts")), ["scripts: typecheck", "scripts: test"]);
  });

  it("runs nothing for a docs change", () => {
    assert.deepEqual(running(changing("docs/plans/agent-tooling.md")), []);
  });

  it("runs everything with --all", () => {
    const all = plan(JOBS, BASE, BASE, () => false, { all: true });
    assert.deepEqual(
      all.map((p) => p.status),
      JOBS.map(() => "run"),
    );
  });

  it("skips what has passed with the same inputs, unless asked to rerun", () => {
    const tree = changing("crates/uta-core/src/lib.rs");
    const fmt = JOBS.find((job) => job.name === "rust: fmt")!;
    const stamped = (key: string) => key === inputKey(fmt, tree);
    const planned = plan(JOBS, tree, BASE, stamped);
    assert.equal(planned.find((p) => p.job === fmt)?.status, "passed");
    assert.ok(!running(tree, stamped).includes("rust: fmt"));
    const rerun = plan(JOBS, tree, BASE, stamped, { rerun: true });
    assert.equal(rerun.find((p) => p.job === fmt)?.status, "run");
  });
});

describe("inputKey", () => {
  const job: Job = { name: "a", inputs: ["a/"], command: ["true"] };
  const tree: TreeFile[] = [
    { hash: "1", path: "a/x" },
    { hash: "2", path: "b/y" },
  ];

  it("changes with an input's content, and not with anything else", () => {
    const key = inputKey(job, tree);
    assert.notEqual(inputKey(job, [{ hash: "9", path: "a/x" }, tree[1]]), key);
    assert.equal(inputKey(job, [tree[0], { hash: "9", path: "b/y" }]), key);
  });

  it("changes with the job's command or environment", () => {
    const key = inputKey(job, tree);
    assert.notEqual(inputKey({ ...job, command: ["false"] }, tree), key);
    assert.notEqual(inputKey({ ...job, env: { A: "1" } }, tree), key);
  });
});
