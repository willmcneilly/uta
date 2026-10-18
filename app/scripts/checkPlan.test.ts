import { describe, expect, it } from "vitest";
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
  { hash: "b", path: "app/scripts/check.ts" },
  { hash: "c", path: "app/src/synth/filter-response.json" },
  { hash: "d", path: "examples/demos/loop.toml" },
  { hash: "e", path: "crates/uta-song/src/lib.rs" },
];

/** BASE with the given files' content changed. */
const changing = (...paths: string[]) =>
  BASE.map((file) => (paths.includes(file.path) ? { ...file, hash: `${file.hash}-new` } : file));

const running = (tree: TreeFile[], stamped: (key: string) => boolean = () => false) =>
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
    expect(isInput(["crates/"], "crates/uta-core/src/lib.rs")).toBe(true);
    expect(isInput(["Cargo.toml"], "Cargo.toml")).toBe(true);
    expect(isInput(["Cargo.toml"], "app/src-tauri/Cargo.toml")).toBe(false);
    expect(isInput(["crates/"], "crates")).toBe(false);
  });

  it("takes an excluded folder back out", () => {
    const frontend = ["app/", "!app/src-tauri/", "DESIGN.md"];
    expect(isInput(frontend, "app/src/App.tsx")).toBe(true);
    expect(isInput(frontend, "app/src-tauri/src/lib.rs")).toBe(false);
    expect(isInput(frontend, "DESIGN.md")).toBe(true);
  });
});

describe("plan", () => {
  it("runs no Rust jobs for a frontend-only change", () => {
    expect(running(changing("app/src/App.tsx"))).toEqual(FRONTEND);
  });

  it("runs no frontend jobs for a Rust-only change", () => {
    expect(running(changing("crates/uta-core/src/lib.rs"))).toEqual([
      ...RUST,
      "rtsan",
      "rust: demo renders",
    ]);
  });

  it("runs RTSan only when what its tests build changed", () => {
    expect(running(changing("app/src-tauri/src/lib.rs"))).toEqual([...RUST, "app: release build"]);
  });

  it("runs the release build only for the Tauri side or the npm manifest", () => {
    expect(running(changing("app/src/App.tsx")).includes("app: release build")).toBe(false);
    expect(running(changing("Cargo.lock")).includes("app: release build")).toBe(false);
    expect(running(changing("app/package.json"))).toEqual([...FRONTEND, "app: release build"]);
  });

  it("renders the demo songs when a song or what plays it changed", () => {
    expect(running(changing("examples/demos/loop.toml"))).toEqual([
      ...RUST,
      "rtsan",
      "rust: demo renders",
    ]);
    expect(running(changing("crates/uta-song/src/lib.rs"))).toEqual([
      ...RUST,
      "rust: demo renders",
    ]);
    expect(running(changing("app/src-tauri/src/lib.rs")).includes("rust: demo renders")).toBe(
      false,
    );
    expect(running(changing("examples/demo-loop.json")).includes("rust: demo renders")).toBe(
      false,
    );
  });

  it("runs both for a file both sides read", () => {
    expect(running(changing("app/src/synth/filter-response.json"))).toEqual([
      ...RUST,
      "rtsan",
      ...FRONTEND,
    ]);
  });

  it("runs the Rust jobs when a command list the tests include changed", () => {
    expect(running(changing("examples/demo-loop.json"))).toEqual([...RUST, "rtsan"]);
  });

  it("runs the frontend jobs for DESIGN.md", () => {
    expect(running(changing("DESIGN.md"))).toEqual(FRONTEND);
  });

  it("checks scripts/check itself with the frontend jobs", () => {
    expect(running(changing("app/scripts/check.ts"))).toEqual(FRONTEND);
  });

  it("runs nothing for a docs change", () => {
    expect(running(changing("docs/plans/agent-tooling.md"))).toEqual([]);
  });

  it("runs everything with --all", () => {
    const all = plan(JOBS, BASE, BASE, () => false, { all: true });
    expect(all.map((p) => p.status)).toEqual(JOBS.map(() => "run"));
  });

  it("skips what has passed with the same inputs, unless asked to rerun", () => {
    const tree = changing("crates/uta-core/src/lib.rs");
    const fmt = JOBS.find((job) => job.name === "rust: fmt")!;
    const stamped = (key: string) => key === inputKey(fmt, tree);
    const planned = plan(JOBS, tree, BASE, stamped);
    expect(planned.find((p) => p.job === fmt)?.status).toBe("passed");
    expect(running(tree, stamped).includes("rust: fmt")).toBe(false);
    const rerun = plan(JOBS, tree, BASE, stamped, { rerun: true });
    expect(rerun.find((p) => p.job === fmt)?.status).toBe("run");
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
    expect(inputKey(job, [{ hash: "9", path: "a/x" }, tree[1]])).not.toBe(key);
    expect(inputKey(job, [tree[0], { hash: "9", path: "b/y" }])).toBe(key);
  });

  it("changes with the job's command or environment", () => {
    const key = inputKey(job, tree);
    expect(inputKey({ ...job, command: ["false"] }, tree)).not.toBe(key);
    expect(inputKey({ ...job, env: { A: "1" } }, tree)).not.toBe(key);
  });
});
