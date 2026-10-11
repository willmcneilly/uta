// @vitest-environment node
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { git, gitPath } from "./git.ts";
import { CAUGHT, INTERRUPTED, MISSED, NOT_BUILT, USAGE, didNotBuild, mutate, occurrences } from "./mutate.ts";
import { MUTATIONS } from "./mutations.ts";

const REPO = resolve(import.meta.dirname, "../..");
const CLI = join(import.meta.dirname, "mutateCli.ts");
const ORIGINAL = "fn process() {\n    let started = now();\n}\n";

let dir: string;
let file: string;

beforeEach(() => {
  dir = realpathSync(mkdtempSync(join(tmpdir(), "uta-mutate-")));
  git(dir, ["init", "--quiet"]);
  file = join(dir, "processor.rs");
  writeFileSync(file, ORIGINAL);
});

afterEach(() => rmSync(dir, { recursive: true, force: true }));

/** A command that fails when the file has the mutation in it, as a test would. */
const testFor = (text: string) => ["sh", "-c", `! grep -q '${text}' processor.rs`];

// The command's output is kept out of the test run's own: a test failing
// with cargo's build error in the log would read as DIDN'T BUILD to a
// reviewer's mutate.
const quiet = () => {};

const run = (find: string, replace: string, command: readonly string[]) =>
  mutate({ root: dir, cwd: dir, file, find, replace, command, echo: quiet }, quiet);

describe("occurrences", () => {
  it("counts non-overlapping matches", () => {
    expect(occurrences("aaaa", "aa")).toBe(2);
    expect(occurrences("abc", "x")).toBe(0);
    expect(occurrences("abc", "")).toBe(0);
  });
});

describe("didNotBuild", () => {
  it("spots a compile or parse failure, not a test failure", () => {
    expect(didNotBuild("error[E0425]: cannot find value\nerror: could not compile `uta-engine`\n")).toBe(true);
    expect(didNotBuild("FAIL  scripts/warm.test.ts\nError: Transform failed with 1 error:\n[PARSE_ERROR]")).toBe(true);
    expect(didNotBuild("scripts/git.ts(107,56): error TS1005: ')' expected.")).toBe(true);
    expect(didNotBuild("test result: FAILED. 3 passed; 1 failed\nerror: test failed")).toBe(false);
    expect(didNotBuild("FAIL  DrumPanel.test.tsx > shows the knobs\nAssertionError: expected 2 to be 3")).toBe(false);
    // vitest quoting a failing test's source, which mentions the patterns.
    expect(didNotBuild('  42|     expect(didNotBuild("Error: Transform failed with 1 error")).toBe(true);')).toBe(false);
  });
});

describe("mutate", () => {
  it("reports CAUGHT when the command fails, and restores the file", async () => {
    const outcome = await run("now()", "alloc()", testFor("alloc"));
    expect(outcome).toMatchObject({ code: CAUGHT });
    expect(outcome.verdict).toMatch(/^CAUGHT/);
    expect(readFileSync(file, "utf8")).toBe(ORIGINAL);
  });

  it("runs the command with the mutation in place", async () => {
    const outcome = await run("now()", "alloc()", ["grep", "-qF", "alloc()", "processor.rs"]);
    expect(outcome.code).toBe(MISSED);
  });

  it("reports MISSED when the command passes, and restores the file", async () => {
    const outcome = await run("now()", "later()", testFor("alloc"));
    expect(outcome).toMatchObject({ code: MISSED });
    expect(outcome.verdict).toMatch(/^MISSED/);
    expect(readFileSync(file, "utf8")).toBe(ORIGINAL);
  });

  it("tells a build failure from a caught mutation", async () => {
    const outcome = await run("now()", "nope()", ["sh", "-c", "echo 'error: could not compile `uta-engine`'; exit 101"]);
    expect(outcome.code).toBe(NOT_BUILT);
    expect(readFileSync(file, "utf8")).toBe(ORIGINAL);
  });

  it("refuses text that doesn't occur exactly once, and leaves the file alone", async () => {
    expect((await run("missing", "x", ["true"])).code).toBe(USAGE);
    expect((await run("()", "x", ["true"])).verdict).toMatch(/occurs 2 times/);
    expect(readFileSync(file, "utf8")).toBe(ORIGINAL);
  });

  it("restores the file when the command can't start", async () => {
    await expect(run("now()", "x()", ["no-such-command-uta"])).rejects.toThrow();
    expect(readFileSync(file, "utf8")).toBe(ORIGINAL);
    expect(readdirSync(gitPath(dir, "uta-mutate"))).toEqual([]);
  });

  it("puts a replacement with $ patterns in literally", async () => {
    const outcome = await run("now()", "$&$&", ["grep", "-qF", "$&$&", "processor.rs"]);
    expect(outcome.code).toBe(MISSED);
  });
});

/** Runs the command line, and resolves once the command it runs has started. */
function start(command: string) {
  const child = spawn(
    process.execPath,
    ["--disable-warning=ExperimentalWarning", CLI, "processor.rs", "now()", "alloc()", "--", "sh", "-c", command],
    { cwd: dir, stdio: ["ignore", "pipe", "pipe"] },
  );
  let output = "";
  const started = new Promise<string>((resolve) =>
    child.stdout.on("data", (chunk: Buffer) => {
      output += chunk.toString();
      const match = /started (\d+)/.exec(output);
      if (match) resolve(match[1]);
    }),
  );
  const exited = new Promise<number | null>((resolve) => child.on("close", resolve));
  return { child, started, exited, output: () => output };
}

describe("the command line", () => {
  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    it(`restores the file when it gets ${signal} mid-run`, async () => {
      const run = start("echo started $$; exec sleep 30");
      await run.started;
      expect(readFileSync(file, "utf8")).toContain("alloc()");
      run.child.kill(signal);
      expect(await run.exited).toBe(INTERRUPTED);
      expect(run.output()).toContain("INTERRUPTED");
      expect(readFileSync(file, "utf8")).toBe(ORIGINAL);
    });
  }

  it("leaves a record when killed outright, so the next run restores the file", async () => {
    const run = start("echo started $$; exec sleep 30");
    const sleeper = Number(await run.started);
    run.child.kill("SIGKILL");
    await run.exited;
    process.kill(sleeper, "SIGKILL");
    expect(readFileSync(file, "utf8")).toContain("alloc()");

    const lines: string[] = [];
    const outcome = await mutate(
      { root: dir, cwd: dir, file, find: "now()", replace: "later()", command: ["true"], echo: quiet },
      (text) => lines.push(text),
    );
    expect(lines[0]).toBe("Restored processor.rs, left mutated by a run that was killed.");
    expect(outcome.code).toBe(MISSED);
    expect(readFileSync(file, "utf8")).toBe(ORIGINAL);
    expect(existsSync(gitPath(dir, "uta-mutate")) && readdirSync(gitPath(dir, "uta-mutate"))).toEqual([]);
  });
});

describe("the named mutations", () => {
  for (const mutation of MUTATIONS) {
    it(`${mutation.name} matches its file exactly once`, () => {
      expect(occurrences(readFileSync(join(REPO, mutation.file), "utf8"), mutation.find)).toBe(1);
    });
  }
});
