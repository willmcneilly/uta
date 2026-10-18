// The jobs `scripts/check` runs: CLAUDE.md's checks, each with the files it
// reads. A job runs when its inputs differ from origin/main's, and a pass is
// stamped with their content, so the same inputs never run twice.
//
// An input is a file (`Cargo.toml`) or a folder (`crates/`). One starting
// with `!` takes a folder back out of the ones before it.

import type { Job } from "./checkPlan.ts";

/** Everything the Rust workspace reads, including the files its tests load. */
const RUST = [
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain.toml",
  "crates/",
  "app/src-tauri/",
  // Command lists the tests include, and the filter fixture the engine
  // checks itself against.
  "examples/",
  "app/src/synth/filter-response.json",
];

/** What the RTSan run builds: the engine, its project core and the CLI. */
const RTSAN = [
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain.toml",
  "crates/uta-core/",
  "crates/uta-engine/",
  "crates/uta-cli/",
  "examples/",
  "app/src/synth/filter-response.json",
];

/**
 * What the demo songs' render test reads: the songs, and the crates that
 * compile, build, play and measure them.
 */
const DEMO_RENDERS = [
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain.toml",
  "crates/uta-core/",
  "crates/uta-engine/",
  "crates/uta-analysis/",
  "crates/uta-song/",
  "examples/demos/",
];

/**
 * The frontend: the app without its Rust side, and the design tokens'
 * source. app/scripts/ is in it, so these jobs check scripts/check itself.
 */
const FRONTEND = ["app/", "!app/src-tauri/", "DESIGN.md"];

/**
 * The release build only breaks locally in ways CI wouldn't catch first when
 * the Tauri side or the npm manifest changed, so it runs only then. CI builds
 * it on every PR (Will, 11 Oct).
 */
const RELEASE = ["app/src-tauri/", "app/package.json"];

const TOKENS = "src/design/tokens.css src/design/tokens.ts";

export const JOBS: readonly Job[] = [
  { name: "rust: fmt", inputs: RUST, command: ["cargo", "fmt", "--all", "--check"] },
  {
    name: "rust: clippy",
    inputs: RUST,
    command: ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    cargo: true,
  },
  // After clippy: both build in target/debug, and cargo locks it.
  {
    name: "rust: test",
    inputs: RUST,
    command: ["cargo", "test", "--workspace"],
    after: ["rust: clippy"],
    cargo: true,
  },
  // Its own target directory: RTSAN_ENABLE changes how the engine builds, so
  // sharing target/debug would rebuild it back and forth, and wait on the lock.
  {
    name: "rtsan",
    inputs: RTSAN,
    command: ["cargo", "test", "-p", "uta-engine", "-p", "uta-cli"],
    env: { RTSAN_ENABLE: "1", CARGO_TARGET_DIR: "target/rtsan" },
    cargo: true,
  },
  // CI checks the committed token files match DESIGN.md. Here the tree may
  // not be committed yet, so compare the files before and after generating.
  {
    name: "frontend: tokens",
    inputs: FRONTEND,
    cwd: "app",
    command: [
      "sh",
      "-c",
      `before=$(cat ${TOKENS} | shasum) && npm run tokens && ` +
        `if [ "$before" != "$(cat ${TOKENS} | shasum)" ]; then ` +
        `echo "The token files didn't match DESIGN.md. npm run tokens has regenerated them: commit them."; exit 1; fi`,
    ],
    nodeModules: true,
  },
  {
    name: "frontend: designmd",
    inputs: FRONTEND,
    cwd: "app",
    command: ["npx", "designmd", "lint", "../DESIGN.md"],
    nodeModules: true,
  },
  {
    name: "frontend: typecheck",
    inputs: FRONTEND,
    cwd: "app",
    command: ["npm", "run", "typecheck"],
    nodeModules: true,
  },
  {
    name: "frontend: lint",
    inputs: FRONTEND,
    cwd: "app",
    command: ["npm", "run", "lint"],
    nodeModules: true,
  },
  {
    name: "frontend: test",
    inputs: FRONTEND,
    cwd: "app",
    command: ["npm", "test"],
    nodeModules: true,
  },
  // After typecheck: tauri build runs `tsc -b` too, which writes the same
  // build info files.
  {
    name: "app: release build",
    inputs: RELEASE,
    cwd: "app",
    command: ["sh", "-c", "npm run tauri build -- --ci && npm run check:release"],
    after: ["frontend: typecheck"],
    nodeModules: true,
    cargo: true,
  },
  // Ignored in `rust: test`, since rendering every song takes minutes in a
  // debug build; CI runs it in release too, in its "Timing reports and demo
  // renders" job. Last, after the release build: both build in
  // target/release, and cargo locks it.
  {
    name: "rust: demo renders",
    inputs: DEMO_RENDERS,
    command: ["cargo", "test", "-p", "uta-song", "--release", "--test", "demos", "--", "--ignored"],
    after: ["app: release build"],
    cargo: true,
  },
];
