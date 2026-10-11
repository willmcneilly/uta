// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { PullCommit, Review, ReviewComment } from "./github.ts";
import {
  areas,
  bodyAt,
  checksCovering,
  cutoff,
  formatReviews,
  goldens,
  parseArgs,
  parseNameStatus,
  recordedBefore,
  threads,
  ticketId,
  worktreePath,
} from "./reviewSetup.ts";

const commit = (oid: string, committedDate: string): PullCommit => ({ oid, committedDate, messageHeadline: oid });

const review = (id: number, commit_id: string, submitted_at: string, body = "🤖 Claude (review): fine"): Review => ({
  id,
  commit_id,
  submitted_at,
  state: "COMMENTED",
  body,
  html_url: `https://github.com/r/${id}`,
  user: { login: "will" },
});

function comment(id: number, created_at: string, extra: Partial<ReviewComment> = {}): ReviewComment {
  return {
    id,
    in_reply_to_id: null,
    pull_request_review_id: 1,
    path: "crates/uta-engine/src/processor.rs",
    line: null,
    original_line: 10,
    commit_id: "c1",
    original_commit_id: "c1",
    created_at,
    body: `comment ${id}`,
    user: { login: "will" },
    ...extra,
  };
}

describe("parseArgs", () => {
  it("takes the PR, a round and a commit", () => {
    expect(parseArgs(["57"])).toEqual({ pr: 57, round: 1, commit: undefined });
    expect(parseArgs(["57", "--round", "2", "--commit", "a09d223"])).toEqual({ pr: 57, round: 2, commit: "a09d223" });
    expect(parseArgs(["-h"])).toBe("help");
    expect(parseArgs(["57", "--round", "0"])).toBe("--round takes a round number.");
    expect(parseArgs(["--round", "2"])).toBe("Give the PR number.");
  });
});

describe("ticketId", () => {
  it("finds the Notion page in the PR body, as a UUID", () => {
    const id = "3f53af96-9b6f-8124-bed7-c8c40514d0e6";
    expect(ticketId("Ticket: [UTA-56](https://app.notion.com/p/3f53af969b6f8124bed7c8c40514d0e6)")).toBe(id);
    expect(ticketId("https://www.notion.so/Add-the-909-kick-3f53af969b6f8124bed7c8c40514d0e6?pvs=4")).toBe(id);
    expect(ticketId("No ticket here, just a sha 3f53af969b6f8124bed7c8c40514d0e6")).toBeUndefined();
  });
});

describe("cutoff", () => {
  const commits = [commit("c1", "2026-10-10T18:00:00Z"), commit("c2", "2026-10-10T19:20:00Z")];

  it("is the first review of the commit, when that came before the next push", () => {
    const reviews = [review(1, "c1", "2026-10-10T19:16:00Z"), review(2, "c1", "2026-10-10T19:17:00Z")];
    expect(cutoff("c1", commits, reviews)).toBe("2026-10-10T19:16:00Z");
  });

  it("is the next push, when the commit was never reviewed", () => {
    expect(cutoff("c1", commits, [])).toBe("2026-10-10T19:20:00Z");
  });

  it("is undefined for an unreviewed head, so nothing is left out", () => {
    expect(cutoff("c2", commits, [review(1, "c1", "2026-10-10T19:16:00Z")])).toBeUndefined();
  });
});

describe("recordedBefore", () => {
  it("keeps only what came strictly before the cutoff, so the commit's own first review is left out", () => {
    const at = "2026-10-10T19:16:59Z";
    const history = {
      reviews: [review(1, "c0", "2026-10-10T18:00:00Z"), review(2, "c1", at), review(3, "c2", "2026-10-10T19:30:00Z")],
      comments: [comment(1, "2026-10-10T18:00:00Z"), comment(2, at)],
      conversation: [
        { id: 1, created_at: "2026-10-10T18:30:00Z", body: "fixes", user: { login: "will" } },
        { id: 2, created_at: "2026-10-10T19:20:00Z", body: "later fixes", user: { login: "will" } },
      ],
    };
    const kept = recordedBefore(history, at);
    expect(kept.reviews.map((r) => r.id)).toEqual([1]);
    expect(kept.comments.map((c) => c.id)).toEqual([1]);
    expect(kept.conversation.map((c) => c.id)).toEqual([1]);
  });
});

describe("bodyAt", () => {
  const edits = [
    { editedAt: "2026-10-10T13:51:00Z", body: "opened" },
    { editedAt: "2026-10-10T14:15:00Z", body: "after round 1" },
  ];

  it("is the body as it was at the cutoff", () => {
    expect(bodyAt("now", edits, "2026-10-10T14:00:00Z")).toBe("opened");
    expect(bodyAt("now", edits, "2026-10-10T15:00:00Z")).toBe("after round 1");
  });

  it("is the body the PR was opened with, when it was opened after the cutoff", () => {
    expect(bodyAt("now", edits, "2026-10-10T12:00:00Z")).toBe("opened");
  });

  it("is today's body with no cutoff, or when it was never edited", () => {
    expect(bodyAt("now", edits, undefined)).toBe("now");
    expect(bodyAt("now", [], "2026-10-10T12:00:00Z")).toBe("now");
  });
});

describe("threads", () => {
  it("puts each reply under the comment it answers, in order", () => {
    const root = comment(1, "2026-10-10T19:00:00Z");
    const other = comment(2, "2026-10-10T19:01:00Z", { path: "app/src/App.tsx" });
    const reply = comment(3, "2026-10-10T19:05:00Z", { in_reply_to_id: 1, pull_request_review_id: 9 });
    const second = comment(4, "2026-10-10T19:06:00Z", { in_reply_to_id: 3, pull_request_review_id: 10 });
    expect(threads([second, other, reply, root])).toEqual([
      { root, replies: [reply, second] },
      { root: other, replies: [] },
    ]);
  });
});

describe("formatReviews", () => {
  it("shows each review with its threads, and leaves out the empty reviews replies make", () => {
    const reviews = [review(1, "a09d2230", "2026-10-10T19:16:59Z", "Verdict"), review(9, "f654ced0", "2026-10-10T19:20:00Z", "")];
    const comments = [
      comment(1, "2026-10-10T19:16:59Z", { body: "Should fix: this" }),
      comment(3, "2026-10-10T19:20:00Z", { in_reply_to_id: 1, pull_request_review_id: 9, body: "Fixed" }),
    ];
    expect(formatReviews(reviews, comments)).toBe(
      [
        "### Review 1: of a09d223, 2026-10-10 19:16Z, https://github.com/r/1",
        "",
        "Verdict",
        "",
        "#### crates/uta-engine/src/processor.rs:10 (on c1)",
        "> Should fix: this",
        "",
        "Reply, 2026-10-10 19:20Z:",
        "> Fixed",
      ].join("\n"),
    );
    expect(formatReviews([], [])).toBe("None.");
  });
});

describe("what changed", () => {
  const changed = parseNameStatus(
    [
      "M\tcrates/uta-engine/src/processor.rs",
      "A\tcrates/uta-engine/tests/golden/kick909.wav",
      "M\tapp/src/DrumPanel.tsx",
      "R100\tdocs/old.md\tdocs/new.md",
      "M\t.claude/agents/pr-reviewer.md",
    ].join("\n"),
  );
  const paths = changed.map((change) => change.path);

  it("reads git's name-status, renames by their new path", () => {
    expect(changed[3]).toEqual({ status: "R", path: "docs/new.md" });
  });

  it("groups the paths into areas", () => {
    expect(areas(paths)).toEqual([
      "Rust: engine, core, CLI (2)",
      "App: frontend (1)",
      "Docs (1)",
      "Skills and agents (1)",
    ]);
  });

  it("names the checks whose files changed", () => {
    expect(checksCovering(["docs/new.md"])).toEqual([]);
    expect(checksCovering(["app/src/DrumPanel.tsx"])).toContain("frontend: test");
    expect(checksCovering(["crates/uta-engine/src/processor.rs"])).toEqual(["rust: fmt", "rust: clippy", "rust: test", "rtsan"]);
  });

  it("finds the golden WAVs", () => {
    expect(goldens(changed)).toEqual([{ status: "A", path: "crates/uta-engine/tests/golden/kick909.wav" }]);
  });
});

describe("worktreePath", () => {
  it("is in the main checkout, whichever worktree the reviewer starts in", () => {
    expect(worktreePath("/Users/will/Sites/uta", 57)).toBe("/Users/will/Sites/uta/.claude/worktrees/review-pr-57");
  });
});
