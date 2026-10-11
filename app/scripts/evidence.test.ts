// @vitest-environment node
import { describe, expect, it } from "vitest";
import { type Tree, classify, evidenceProblems, quoted, totals } from "./evidence.ts";

/** A fake checkout: these files, whose text is what `read` and `wordsFound` look in. */
function tree(files: Record<string, string>, ignored: string[] = []): Tree {
  return {
    files: () => Object.keys(files),
    wordsFound: (words) =>
      new Set(words.filter((w) => Object.values(files).some((text) => new RegExp(`(^|\\W)${w}(\\W|$)`).test(text)))),
    read: (path) => files[path] ?? "",
    ignored: (path) => ignored.some((i) => path === i || path.startsWith(`${i}/`)),
  };
}

const CHECKOUT = tree(
  {
    "Cargo.toml": "[workspace]\nmembers = []",
    "crates/uta-engine/Cargo.toml": '[package]\nname = "uta-engine"\nversion = "0.1.0"',
    "crates/uta-engine/tests/balance.rs": "#[test]\nfn the_kit_balances() {}\nconst REFERENCE_PEAK: f32 = 0.25;",
    "crates/uta-engine/src/snapshot.rs": "pub struct Snapshot;",
    "app/package.json": JSON.stringify({ scripts: { typecheck: "tsc -b", test: "vitest run" } }),
    "app/scripts/ticket.ts": "export function parseDependsOn() {}",
    "scripts/check": "#!/bin/sh",
    "examples/demo-loop.json": "[]",
  },
  ["target", "app/node_modules"],
);

const problems = (markdown: string) => evidenceProblems(markdown, CHECKOUT).map((p) => p.ref);

describe("quoted", () => {
  it("takes inline spans outside fences, and the lines of shell blocks", () => {
    const body = [
      "Run `scripts/check` and see `the_kit_balances`.",
      "```bash",
      "cargo test -p uta-engine \\",
      "  --test balance",
      "# a comment",
      "```",
      "```",
      "`not_a_span` in output",
      "```",
    ].join("\n");
    const { spans, shell, prose } = quoted(body);
    expect(spans).toEqual(["scripts/check", "the_kit_balances"]);
    expect(shell).toEqual(["cargo test -p uta-engine --test balance"]);
    expect(prose).not.toContain("the_kit_balances");
  });

  it("leaves out names that are gone: struck through, or renamed with an arrow", () => {
    const { spans } = quoted("~~`lastDrag`, `THUMB_GRAB`~~ are gone, and `sliderScale` → `numberScale`, `old_name` -> `new_name`.");
    expect(spans).toEqual(["numberScale", "new_name"]);
  });
});

describe("classify", () => {
  it("tells names, paths and commands apart, and skips prose and placeholders", () => {
    expect(classify("the_kit_balances")).toEqual({ kind: "name", ref: "the_kit_balances", names: ["the_kit_balances"] });
    expect(classify("common::unity_kit")).toMatchObject({ kind: "name", names: ["unity_kit"] });
    expect(classify("parseDependsOn()")).toMatchObject({ kind: "name", names: ["parseDependsOn"] });
    expect(classify("snapshot.rs:42")).toMatchObject({ kind: "path", path: "snapshot.rs" });
    expect(classify("app/scripts/")).toMatchObject({ kind: "path", path: "app/scripts" });
    expect(classify("RTSAN_ENABLE=1 cargo test -p uta-engine")).toMatchObject({ kind: "command" });
    for (const skipped of ["merge", "Ready", "In Review", "UTA-<n>", "PATCH v1/blocks/<id>", "a … b", "https://x.y/z", "~/.claude", "0.18.x"]) {
      expect(classify(skipped)).toBeUndefined();
    }
  });
});

describe("evidenceProblems", () => {
  it("passes a body whose names, paths and commands are all there", () => {
    const body = [
      "- [x] **The kit balances.** `the_kit_balances` in `balance.rs` uses `REFERENCE_PEAK`; see `crates/uta-engine/src/snapshot.rs:12` and `app/scripts/`.",
      "Run `npm run typecheck`, `scripts/check` and `cargo test -p uta-engine --test balance`.",
      "Build output in `target/listen/kit.wav`; deps in `app/node_modules`. `parseDependsOn` reads it.",
    ].join("\n");
    expect(evidenceProblems(body, CHECKOUT)).toEqual([]);
  });

  it("names a test that was renamed, saying it isn't in the code", () => {
    expect(evidenceProblems("Proved by `the_old_test_name`.", CHECKOUT)).toEqual([
      { ref: "the_old_test_name", why: "isn't in the code" },
    ]);
  });

  it("names a missing file, script, npm script, package and test target", () => {
    expect(problems("`crates/uta-engine/src/gone.rs` and `moved.rs`")).toEqual(["crates/uta-engine/src/gone.rs", "moved.rs"]);
    const why = evidenceProblems("`scripts/nope --help`, `npm run lint`, `cargo test -p uta-gone --test sound`", CHECKOUT).map((p) => p.why);
    expect(why).toEqual(["no scripts/nope script", 'no npm script "lint" in app/package.json', "no package uta-gone", "no test target sound"]);
  });

  it("checks paths in shell blocks, but not output blocks", () => {
    const body = "```bash\ncargo run -p uta-engine -- render-all examples/listen/kit target/listen/kit\n```\n```\nexamples/missing.json\n```";
    expect(evidenceProblems(body, CHECKOUT)).toEqual([
      { ref: "cargo run -p uta-engine -- render-all examples/listen/kit target/listen/kit", why: "no examples/listen/kit" },
    ]);
  });

  it("reports each reference once", () => {
    expect(problems("`gone_test` and again `gone_test`")).toEqual(["gone_test"]);
  });

  it("refuses totals of tests, pointing at the rule", () => {
    const found = evidenceProblems("All 509 tests pass (`the_kit_balances` among them).", CHECKOUT);
    expect(found).toHaveLength(1);
    expect(found[0].ref).toBe("509 tests");
    expect(found[0].why).toContain("name the tests");
  });
});

describe("totals", () => {
  it("finds counts of tests and passes in prose", () => {
    expect(totals("46 tests, 6 unit tests, 27 shared tests and 1,024 passed; 3 rounds; two tests")).toEqual([
      "46 tests",
      "6 unit tests",
      "27 shared tests",
      "1,024 passed",
    ]);
  });
});
