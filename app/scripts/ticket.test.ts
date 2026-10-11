// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  type Action,
  type Check,
  type Pr,
  type PrState,
  type Project,
  Refusal,
  type Services,
  type Ticket,
  type Todo,
  ciState,
  claim,
  commandLine,
  dependencies,
  matchTodos,
  merge,
  newDecision,
  parseDependsOn,
  parseId,
  readyReport,
  tick,
  verdict,
  whyNotMergeable,
} from "./ticket.ts";

const PROJECT = "project-page";

/** A fake Notion and GitHub: tickets are [id, status, depends on, PR state] with PR n = 10000 + id. */
function fake(
  rows: [number, string, number[], PrState | "none"][],
  extra: { project?: Partial<Project>; todos?: Todo[]; pr?: Partial<Pr> } = {},
): Services {
  const tickets: Ticket[] = rows.map(([id, status, , pr]) => ({
    pageId: `page-${id}`,
    id,
    name: `Ticket ${id}`,
    status,
    type: "Chore",
    pr: pr === "none" ? undefined : `https://github.com/willmcneilly/uta/pull/${10000 + id}`,
    projectId: PROJECT,
  }));
  const bodies = new Map(
    rows.map(([id, , deps]) => [
      `page-${id}`,
      `## Goal\nSomething, unlike UTA-999.\n## Depends on\n${deps.length ? deps.map((d) => `UTA-${d}`).join(" and ") : "Nothing."}\n## Context\nSee UTA-998.`,
    ]),
  );
  const states = new Map<number, PrState>(
    rows.filter(([, , , pr]) => pr !== "none").map(([id, , , pr]) => [10000 + id, pr as PrState]),
  );
  return {
    tickets: () => tickets,
    body: (pageId) => bodies.get(pageId) ?? "",
    project: (pageId) => ({ pageId, name: "Tooling", status: "Active", ...extra.project }),
    todos: () => extra.todos ?? [],
    prStates: () => states,
    pr: (number) => ({
      number,
      url: `https://github.com/willmcneilly/uta/pull/${number}`,
      state: states.get(number) ?? "OPEN",
      branch: "uta-1-thing",
      headSha: "abcdef1234567",
      checks: [{ name: "Rust", status: "COMPLETED", conclusion: "SUCCESS" }],
      reviews: ["🤖 Claude (review):\n\n**Verdict: Ready to merge.**"],
      ...extra.pr,
    }),
  };
}

/** A Run that records each action, as a dry run would print it. */
function recorder(response?: string): { run: (a: Action) => string | undefined; actions: Action[] } {
  const actions: Action[] = [];
  return { actions, run: (a) => (actions.push(a), response) };
}

const summaries = (actions: Action[]) => actions.map((a) => a.summary);

describe("parseId", () => {
  it("takes UTA-n, uta-n or a bare number", () => {
    expect([parseId("UTA-62"), parseId("uta-7"), parseId("12")]).toEqual([62, 7, 12]);
  });

  it("refuses anything else", () => {
    expect(() => parseId("D-3")).toThrow(Refusal);
  });
});

describe("parseDependsOn", () => {
  it("reads the IDs under the Depends on heading only", () => {
    const body = "## Goal\nUnlike UTA-1.\n## Depends on\nUTA-53, UTA-54 and UTA-43, after UTA-53 lands.\n## Context\nUTA-9.";
    expect(parseDependsOn(body)).toEqual([53, 54, 43]);
  });

  it("is empty for Nothing, or no heading", () => {
    expect(parseDependsOn("## Depends on\nNothing.\n## Context")).toEqual([]);
    expect(parseDependsOn("## Goal\nUTA-3")).toEqual([]);
  });

  it("reads to the end when Depends on is the last section", () => {
    expect(parseDependsOn("## Depends on\n- UTA-2\n- UTA-3\n")).toEqual([2, 3]);
  });
});

describe("dependencies", () => {
  const tickets = fake([
    [1, "Done", [], "MERGED"],
    [2, "Done", [], "OPEN"],
    [3, "In Review", [], "OPEN"],
    [4, "Done", [], "none"],
  ]);

  it("is met when Done and merged, or Done with no PR", () => {
    const deps = dependencies([1, 2, 3, 4, 5], tickets.tickets(), tickets.prStates());
    expect(deps.map((d) => [d.id, d.pr, d.met])).toEqual([
      [1, "merged", true],
      [2, "not merged", false],
      [3, "not merged", false],
      [4, "no PR", true],
      [5, "no PR", false], // Not in Notion at all.
    ]);
  });

  it("counts the ticket being merged as Done and merged", () => {
    const [dep] = dependencies([3], tickets.tickets(), tickets.prStates(), 3);
    expect(dep.met).toBe(true);
  });

  it("isn't met when GitHub doesn't know the PR", () => {
    const [dep] = dependencies([1], tickets.tickets(), new Map());
    expect(dep).toMatchObject({ pr: "unknown PR", met: false });
  });
});

describe("claim", () => {
  it("refuses a Backlog ticket", () => {
    const { run, actions } = recorder();
    const services = fake([[5, "Backlog", [], "none"]]);
    expect(() => claim(services, run, 5, "uta-5-x")).toThrow("UTA-5 is Backlog, not Ready.");
    expect(actions).toEqual([]);
  });

  it("refuses a ticket with a dependency that's Done but not merged", () => {
    const { run, actions } = recorder();
    const services = fake([
      [1, "Done", [], "OPEN"],
      [5, "Ready", [1], "none"],
    ]);
    expect(() => claim(services, run, 5, "uta-5-x")).toThrow("UTA-5 depends on UTA-1  Done, PR not merged");
    expect(actions).toEqual([]);
  });

  it("sets In Progress, the project Active if it was Planned, and comments", () => {
    const { run, actions } = recorder();
    const services = fake(
      [
        [1, "Done", [], "MERGED"],
        [5, "Ready", [1], "none"],
      ],
      { project: { status: "Planned" } },
    );
    claim(services, run, 5, "uta-5-thing");
    expect(summaries(actions)).toEqual([
      "Set UTA-5 to In Progress.",
      "Set the project Tooling to Active.",
      'Comment "🤖 Claude: Started on branch uta-5-thing."',
    ]);
    expect(actions[0].call.args).toEqual([
      "api",
      "v1/pages/page-5",
      "-X",
      "PATCH",
      "-d",
      '{"properties":{"Status":{"status":{"name":"In Progress"}}}}',
    ]);
    expect(JSON.parse(actions[2].call.args[3])).toEqual({
      parent: { page_id: "page-5" },
      rich_text: [{ text: { content: "🤖 Claude: Started on branch uta-5-thing." } }],
    });
  });

  it("leaves an Active project alone", () => {
    const { run, actions } = recorder();
    claim(fake([[5, "Ready", [], "none"]]), run, 5, "b");
    expect(summaries(actions)).not.toContain("Set the project Tooling to Active.");
  });
});

describe("matchTodos", () => {
  const todos: Todo[] = [
    { blockId: "a", text: "`show` and `ready` give the right answers", checked: false },
    { blockId: "b", text: "Process log entry added", checked: false },
    { blockId: "c", text: "Process log entry added, with evidence.", checked: false },
    { blockId: "d", text: "Unit tests cover dependency checking", checked: true },
  ];

  it("matches by part of the text, ignoring case, backticks and spacing", () => {
    expect(matchTodos(todos, ["SHOW and  ready give"]).map((t) => t.blockId)).toEqual(["a"]);
    expect(matchTodos(todos, ["`show` and `ready`"]).map((t) => t.blockId)).toEqual(["a"]);
  });

  it("takes the exact match when several contain it", () => {
    expect(todos.filter((t) => t.text.toLowerCase().includes("process log entry added"))).toHaveLength(2);
    expect(matchTodos(todos, ["process log entry added"]).map((t) => t.blockId)).toEqual(["b"]);
  });

  it("refuses an ambiguous criterion, naming the candidates", () => {
    expect(() => matchTodos(todos, ["Process log"])).toThrow(/"Process log" matches 2 criteria:\n {4}- Process log entry added\n/);
  });

  it("refuses a criterion that matches nothing, listing the criteria", () => {
    expect(() => matchTodos(todos, ["golden WAVs"])).toThrow(/No criterion matches "golden WAVs"\.[\s\S]*\[x\] Unit tests/);
  });

  it("refuses an empty criterion", () => {
    expect(() => matchTodos(todos, [" "])).toThrow(Refusal);
  });

  it("names a checkbox once when two criteria match it", () => {
    expect(matchTodos(todos, ["show", "ready give"])).toHaveLength(1);
  });
});

describe("tick", () => {
  const todos: Todo[] = [
    { blockId: "a", text: "First criterion", checked: false },
    { blockId: "b", text: "Second criterion", checked: true },
  ];

  it("ticks unticked matches, and says which were already ticked", () => {
    const { run, actions } = recorder();
    const out = tick(fake([[5, "Done", [], "none"]], { todos }), run, 5, ["first", "second"]);
    expect(actions.map((a) => a.call.args)).toEqual([["api", "v1/blocks/a", "-X", "PATCH", "-d", '{"to_do":{"checked":true}}']]);
    expect(out).toEqual(["Already ticked: Second criterion"]);
  });

  it("ticks nothing if any criterion fails", () => {
    const { run, actions } = recorder();
    expect(() => tick(fake([[5, "Done", [], "none"]], { todos }), run, 5, ["first", "third"])).toThrow(Refusal);
    expect(actions).toEqual([]);
  });
});

describe("verdict", () => {
  it.each([
    ["🤖 Claude (review):\n\n**Verdict: Ready to merge.** No blocking findings.", "Ready to merge"],
    ["🤖 Claude (re-review of 67d1d6c): **Ready to merge.** 0 blocking.", "Ready to merge"],
    ["🤖 Claude (review):\n\n**Verdict: Changes needed.** It's not ready to merge until …", "Changes needed"],
    ["🤖 Claude (review): Not ready to merge yet.\n\n**Verdict: Changes needed.**", "Changes needed"],
    ["🤖 Claude (review): changes needed, then ready to merge", "Changes needed"],
    ["🤖 Claude (review):\n\n**Verdict: Not ready to merge.**", "Changes needed"],
    ["🤖 Claude (review): not ready to merge, two blocking.", "Changes needed"],
    ["🤖 Claude (review): nothing to say", undefined],
  ])("reads %j", (body, expected) => {
    expect(verdict(body)).toBe(expected);
  });
});

describe("ciState", () => {
  const run = (conclusion: string, status = "COMPLETED"): Check => ({ name: conclusion, status, conclusion });
  it.each<[Check[], string]>([
    [[run("SUCCESS"), run("SKIPPED"), { name: "status", state: "SUCCESS" }], "passing"],
    [[run("SUCCESS"), run("", "IN_PROGRESS")], "pending"],
    [[run("SUCCESS"), { name: "status", state: "PENDING" }], "pending"],
    [[run("FAILURE"), run("", "QUEUED")], "failed"],
    [[{ name: "status", state: "ERROR" }], "failed"],
    [[], "none"],
  ])("%j is %s", (checks, expected) => {
    expect(ciState(checks)).toBe(expected);
  });
});

describe("whyNotMergeable", () => {
  const pr = fake([[1, "In Review", [], "OPEN"]]).pr(10001);

  it("is undefined when the newest review says Ready to merge and CI is green", () => {
    expect(whyNotMergeable(pr)).toBeUndefined();
  });

  it("goes by the newest reviewer review, skipping replies", () => {
    const reviews = ["🤖 Claude (review): **Ready to merge.**", "🤖 Claude (re-review of abc): **Changes needed.**", "Fixed in 123."];
    expect(whyNotMergeable({ ...pr, reviews })).toBe("The newest review of PR #10001 says Changes needed.");
    expect(whyNotMergeable({ ...pr, reviews: reviews.slice(1).reverse() })).toBe("The newest review of PR #10001 says Changes needed.");
    expect(whyNotMergeable({ ...pr, reviews: [...reviews, "🤖 Claude (re-review of def): Ready to merge."] })).toBeUndefined();
  });

  it("refuses with no review, or CI not green", () => {
    expect(whyNotMergeable({ ...pr, reviews: ["LGTM"] })).toMatch(/has no review yet/);
    expect(whyNotMergeable({ ...pr, checks: [{ name: "Rust", status: "IN_PROGRESS" }] })).toBe(
      "PR #10001 has CI still running (Rust) on abcdef1.",
    );
    expect(whyNotMergeable({ ...pr, checks: [{ name: "Lint", status: "COMPLETED", conclusion: "FAILURE" }] })).toBe(
      "PR #10001 has CI failing (Lint) on abcdef1.",
    );
  });
});

describe("merge", () => {
  // 7 is being merged. 8 waits on 7 alone; 9 on 7 and 6, which isn't merged;
  // 10 doesn't need 7; 11 was Ready already.
  const rows: [number, string, number[], PrState | "none"][] = [
    [6, "Done", [], "OPEN"],
    [7, "In Review", [], "OPEN"],
    [8, "Backlog", [7], "none"],
    [9, "Backlog", [7, 6], "none"],
    [10, "Backlog", [], "none"],
  ];

  it("merges, deletes the branch, sets Done and readies only what 7 unblocks", () => {
    const { run, actions } = recorder();
    const out = merge(fake(rows), run, 7);
    expect(summaries(actions)).toEqual([
      "Squash merge PR #10007 at abcdef1.",
      "Delete the remote branch uta-1-thing.",
      "Set UTA-7 to Done.",
      "Set UTA-8 to Ready (Ticket 8).",
    ]);
    expect(actions.slice(0, 2).map((a) => commandLine(a.call))).toEqual([
      "gh pr merge 10007 --squash --match-head-commit abcdef1234567",
      "git push origin --delete uta-1-thing",
    ]);
    // What's ready afterwards counts 7 as merged and 8 as Ready.
    expect(out).toContain("Ready to start (Ready, with every dependency Done and merged):");
    expect(out).toContain("  UTA-8  Ticket 8  (Tooling)");
  });

  it("sets the project Done when every other ticket is", () => {
    const { run, actions } = recorder();
    merge(fake([[6, "Done", [], "MERGED"], [7, "In Review", [], "OPEN"]]), run, 7);
    expect(summaries(actions).at(-1)).toBe("Set the project Tooling to Done: every ticket is.");
  });

  it("skips the merge for a PR that's already merged, and a ticket already Done", () => {
    const { run, actions } = recorder();
    const out = merge(fake([[7, "Done", [], "MERGED"], [8, "Backlog", [7], "none"]], { project: { status: "Done" } }), run, 7);
    expect(out[0]).toBe("PR #10007 is already merged.");
    expect(summaries(actions)).toEqual(["Set UTA-8 to Ready (Ticket 8)."]);
  });

  it("refuses when the newest review isn't Ready to merge, changing nothing", () => {
    const { run, actions } = recorder();
    const services = fake(rows, { pr: { reviews: ["🤖 Claude (review): **Verdict: Changes needed.**"] } });
    expect(() => merge(services, run, 7)).toThrow("Not merging: The newest review of PR #10007 says Changes needed.");
    expect(actions).toEqual([]);
  });

  it("refuses a ticket that isn't In Review, or has no PR", () => {
    const { run } = recorder();
    expect(() => merge(fake([[7, "In Progress", [], "OPEN"]]), run, 7)).toThrow("UTA-7 is In Progress, not In Review.");
    expect(() => merge(fake([[7, "In Review", [], "none"]]), run, 7)).toThrow("UTA-7 has no PR link.");
  });
});

describe("readyReport", () => {
  it("splits Ready tickets into startable and waiting, and lists what's running", () => {
    const report = readyReport(
      fake([
        [1, "Done", [], "MERGED"],
        [2, "In Progress", [], "none"],
        [3, "Ready", [1], "none"],
        [4, "Ready", [2], "none"],
        [5, "Ready", [], "none"],
        [6, "In Review", [], "OPEN"],
      ]),
    );
    expect(report.startable.map((t) => t.id)).toEqual([3, 5]);
    expect(report.waiting.map(({ ticket, deps }) => [ticket.id, deps.map((d) => d.id)])).toEqual([[4, [2]]]);
    expect(report.running.map((t) => t.id)).toEqual([2, 6]);
  });
});

describe("newDecision", () => {
  const decision = {
    name: "Kick model choice",
    needed: "n".repeat(2500),
    chosen: "c",
    principles: "p",
    alternatives: "a",
    ticket: 5,
  };

  it("creates the row linked to the ticket and prints the D-n Notion gave it", () => {
    const response = JSON.stringify({ url: "https://www.notion.so/x", properties: { ID: { unique_id: { number: 31 } } } });
    const { run, actions } = recorder(response);
    const out = newDecision(fake([[5, "In Progress", [], "none"]]), run, decision);
    expect(out).toEqual(["D-31  https://www.notion.so/x", "Mark the code with: provisional: D-31"]);
    const [, , flag, body] = actions[0].call.args;
    expect([actions[0].call.args[0], actions[0].call.args[1], flag]).toEqual(["decisions", "create", "-d"]);
    const { properties } = JSON.parse(body);
    expect(properties.Status).toEqual({ status: { name: "Open" } });
    expect(properties.Ticket).toEqual({ relation: [{ id: "page-5" }] });
    // Notion takes at most 2000 characters in one rich text item.
    expect(properties.Needed.rich_text.map((t: { text: { content: string } }) => t.text.content.length)).toEqual([2000, 500]);
  });

  it("says Notion numbers it, in a dry run", () => {
    const { run } = recorder(undefined);
    expect(newDecision(fake([[5, "In Progress", [], "none"]]), run, decision)).toEqual([
      "Notion gives it its D-n when it's created.",
    ]);
  });
});

describe("commandLine", () => {
  it("quotes what the shell would split, and names ntn.sh by its path", () => {
    expect(commandLine({ tool: "ntn.sh", args: ["api", "v1/comments", "-d", `{"a":"it's"}`] })).toBe(
      `.claude/skills/uta-notion/ntn.sh api v1/comments -d '{"a":"it'\\''s"}'`,
    );
  });
});
