// @vitest-environment node
import { describe, expect, it } from "vitest";
import { GREEN, OTHER_COMMIT, RED, TIMED_OUT, ciWait, judge, latestPerName, parseArgs, table } from "./ciWait.ts";
import type { WaitDeps } from "./ciWait.ts";
import type { CheckRun } from "./github.ts";

const HEAD = "b".repeat(40);
const OLDER = "a".repeat(40);

function run(name: string, conclusion: string | null, sha = HEAD, started = "2026-10-11T10:00:00Z"): CheckRun {
  return {
    name,
    head_sha: sha,
    status: conclusion === null ? "in_progress" : "completed",
    conclusion,
    started_at: started,
    completed_at: conclusion === null ? null : "2026-10-11T10:02:05Z",
    html_url: `https://example.com/${name}`,
  };
}

describe("latestPerName", () => {
  it("keeps only the newest run of a re-run job", () => {
    const first = run("Rust", "failure", HEAD, "2026-10-11T10:00:00Z");
    const rerun = run("Rust", "success", HEAD, "2026-10-11T10:30:00Z");
    expect(latestPerName([rerun, run("App", "success"), first])).toEqual([run("App", "success"), rerun]);
  });
});

describe("judge", () => {
  it("is none with no checks, pending while any runs, red on any failure, green when all pass", () => {
    expect(judge([])).toBe("none");
    expect(judge([run("Rust", null), run("App", "success")])).toBe("running");
    expect(judge([run("Rust", null), run("App", "failure")])).toBe("failed");
    expect(judge([run("Rust", "cancelled")])).toBe("failed");
    expect(judge([run("Rust", "success"), run("Docs", "skipped")])).toBe("passed");
  });
});

describe("table", () => {
  it("gives each check's result, commit, time and link", () => {
    expect(table([run("Rust", "failure")])).toBe("Rust  FAILED (failure)  bbbbbbb  2m05s  https://example.com/Rust");
  });
});

/** Fake GitHub: `runs` is called once per poll, and returns the next answer for each commit. */
function fake(answers: Record<string, CheckRun[][]>, heads: string[] = [HEAD]) {
  const printed: string[] = [];
  let clock = 0;
  const polls: Record<string, number> = {};
  const deps: WaitDeps = {
    head: () => heads[Math.min(Math.floor(clock / 15_000), heads.length - 1)],
    commits: () => [OLDER, HEAD],
    runs: (sha) => {
      const list = answers[sha] ?? [[]];
      const at = polls[sha] ?? 0;
      polls[sha] = at + 1;
      return list[Math.min(at, list.length - 1)];
    },
    sleep: async (ms) => {
      clock += ms;
    },
    now: () => clock,
    print: (text) => printed.push(text),
  };
  return { deps, printed, elapsed: () => clock };
}

const OPTIONS = { timeoutMs: 600_000, graceMs: 90_000, intervalMs: 15_000 };

describe("ciWait", () => {
  it("exits 0 when every check passes on the head", async () => {
    const { deps, printed } = fake({ [HEAD]: [[run("Rust", "success"), run("App", "success")]] });
    expect(await ciWait(57, OPTIONS, deps)).toBe(GREEN);
    expect(printed.at(-1)).toBe("All 2 checks passed on bbbbbbb.");
  });

  it("waits while checks run, then reports how they ended", async () => {
    const { deps, elapsed } = fake({
      [HEAD]: [[], [run("Rust", null)], [run("Rust", null)], [run("Rust", "success")]],
    });
    expect(await ciWait(57, OPTIONS, deps)).toBe(GREEN);
    expect(elapsed()).toBe(45_000);
  });

  it("exits 1 at the first failure, without waiting for the rest", async () => {
    const { deps } = fake({ [HEAD]: [[run("Rust", null), run("App", "failure")]] });
    expect(await ciWait(57, OPTIONS, deps)).toBe(RED);
  });

  it("exits 2 when the newest push has no checks, and shows the older commit's", async () => {
    const { deps, printed, elapsed } = fake({ [OLDER]: [[run("Rust", "success", OLDER)]] });
    expect(await ciWait(57, OPTIONS, deps)).toBe(OTHER_COMMIT);
    expect(elapsed()).toBe(90_000);
    expect(printed).toContain("No checks have started for bbbbbbb.");
    expect(printed).toContain("The newest checks on this PR are for aaaaaaa:");
  });

  it("exits 2 when the head moves while it waits", async () => {
    const { deps, printed } = fake({ [HEAD]: [[run("Rust", null)]] }, [HEAD, HEAD, "c".repeat(40)]);
    expect(await ciWait(57, OPTIONS, deps)).toBe(OTHER_COMMIT);
    expect(printed.join("\n")).toContain("The PR's head moved to ccccccc");
  });

  it("waits for a given commit's checks, whatever the head", async () => {
    const { deps } = fake({ [OLDER]: [[run("Rust", "success", OLDER)]] }, ["c".repeat(40)]);
    expect(await ciWait(57, { ...OPTIONS, commit: OLDER }, deps)).toBe(GREEN);
  });

  it("exits 3 when CI is still running at the timeout", async () => {
    const { deps, elapsed } = fake({ [HEAD]: [[run("Rust", null)]] });
    expect(await ciWait(57, { ...OPTIONS, timeoutMs: 60_000 }, deps)).toBe(TIMED_OUT);
    expect(elapsed()).toBe(60_000);
  });

  it("counts a commit as having no checks by the timeout, even when that's shorter than the grace", async () => {
    const { deps } = fake({});
    expect(await ciWait(57, { ...OPTIONS, timeoutMs: 30_000 }, deps)).toBe(OTHER_COMMIT);
  });
});

describe("parseArgs", () => {
  it("takes the PR, a commit and a timeout", () => {
    expect(parseArgs(["57"])).toEqual({ pr: 57, commit: undefined, timeoutMinutes: 9 });
    expect(parseArgs(["57", "--commit", "a09d223", "--timeout", "2"])).toEqual({
      pr: 57,
      commit: "a09d223",
      timeoutMinutes: 2,
    });
    expect(parseArgs(["--help"])).toBe("help");
    expect(parseArgs([])).toBe("Give the PR number.");
    expect(parseArgs(["57", "--commit", "HEAD"])).toBe("--commit takes a commit SHA.");
    expect(parseArgs(["57", "--wait"])).toBe("Unknown argument: --wait");
  });
});
