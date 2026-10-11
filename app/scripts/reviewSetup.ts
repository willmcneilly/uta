// `scripts/review-setup`: everything a review round starts from, in one go.
// reviewSetupCli.ts has the command line and its help. The functions above
// `reviewSetup` are pure, so the tests can drive them without GitHub or git.

import { existsSync } from "node:fs";
import { join } from "node:path";
import { nodeModulesCurrent } from "./check.ts";
import { JOBS } from "./checkJobs.ts";
import { isInput } from "./checkPlan.ts";
import { judge, latestPerName, table } from "./ciWait.ts";
import { git, mainCheckout, shortSha, topLevel, tryGit } from "./git.ts";
import * as github from "./github.ts";
import type { BodyEdit, IssueComment, PullCommit, Review, ReviewComment } from "./github.ts";
import { restorePending } from "./mutate.ts";
import { warm } from "./warm.ts";

export type Args = { pr: number; round: number; commit?: string };

export function parseArgs(argv: readonly string[]): Args | "help" | string {
  if (argv.includes("--help") || argv.includes("-h")) return "help";
  let pr: number | undefined;
  let round = 1;
  let commit: string | undefined;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--round") round = Number(argv[++i]);
    else if (arg === "--commit") commit = argv[++i];
    else if (/^\d+$/.test(arg) && pr === undefined) pr = Number(arg);
    else return `Unknown argument: ${arg}`;
  }
  if (pr === undefined) return "Give the PR number.";
  if (!Number.isInteger(round) || round < 1) return "--round takes a round number.";
  if (commit !== undefined && !/^[0-9a-f]{7,40}$/.test(commit)) return "--commit takes a commit SHA.";
  return { pr, round, commit };
}

/** The Notion page ID of the ticket the PR body links to, as a UUID. */
export function ticketId(body: string): string | undefined {
  const hex = /notion\.(?:so|com|site)\/\S*?([0-9a-f]{32})/i.exec(body)?.[1].toLowerCase();
  if (!hex) return undefined;
  return [hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16), hex.slice(16, 20), hex.slice(20)].join("-");
}

/**
 * When a review of `commit` would have started: the first review of it, or
 * failing that the next push. Anything recorded from then on (later
 * reviews, replies, fixes) is left out, so the review sees only what came
 * before. Undefined when nothing came after.
 */
export function cutoff(commit: string, commits: readonly PullCommit[], reviews: readonly Review[]): string | undefined {
  const times: string[] = reviews.filter((review) => review.commit_id === commit).map((review) => review.submitted_at);
  const at = commits.findIndex((c) => c.oid === commit);
  if (at !== -1 && at + 1 < commits.length) times.push(commits[at + 1].committedDate);
  return times.sort()[0];
}

/**
 * The PR's body as it was at `cutoff`. GitHub keeps every version; the first
 * is the body the PR was opened with, used even when it was opened later.
 */
export function bodyAt(body: string, edits: readonly BodyEdit[], at: string | undefined): string {
  if (at === undefined || edits.length === 0) return body;
  const before = edits.filter((edit) => edit.editedAt < at);
  return (before.at(-1) ?? edits[0]).body;
}

export type Thread = { root: ReviewComment; replies: ReviewComment[] };

/** Inline comments grouped into threads, each reply under the comment it answers. */
export function threads(comments: readonly ReviewComment[]): Thread[] {
  const byId = new Map(comments.map((comment) => [comment.id, comment]));
  const rootOf = (comment: ReviewComment): ReviewComment => {
    const parent = comment.in_reply_to_id ? byId.get(comment.in_reply_to_id) : undefined;
    return parent ? rootOf(parent) : comment;
  };
  const found = new Map<number, Thread>();
  const sorted = [...comments].sort((a, b) => a.created_at.localeCompare(b.created_at));
  for (const comment of sorted) {
    const root = rootOf(comment);
    const thread = found.get(root.id) ?? { root, replies: [] };
    if (comment !== root) thread.replies.push(comment);
    found.set(root.id, thread);
  }
  return [...found.values()];
}

/** The paths a change touched, with git's status letter (A, M, D, R…). */
export type Changed = { status: string; path: string };

export function parseNameStatus(out: string): Changed[] {
  return out
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const parts = line.split("\t");
      return { status: parts[0][0], path: parts.at(-1) ?? "" };
    });
}

const AREAS: readonly [string, (path: string) => boolean][] = [
  ["Rust: engine, core, CLI", (p) => p.startsWith("crates/") || /^(Cargo\.(toml|lock)|rust-toolchain\.toml)$/.test(p)],
  ["Rust: the app's side", (p) => p.startsWith("app/src-tauri/")],
  ["App: frontend", (p) => p.startsWith("app/src/") || /^app\/[^/]+$/.test(p)],
  ["Scripts", (p) => p.startsWith("scripts/") || p.startsWith("app/scripts/") || p.startsWith(".githooks/")],
  ["Design", (p) => p === "DESIGN.md"],
  ["Examples and listening renders", (p) => p.startsWith("examples/")],
  ["CI", (p) => p.startsWith(".github/")],
  ["Skills and agents", (p) => p.startsWith(".claude/")],
  ["Docs", (p) => p.startsWith("docs/") || /^[^/]+\.md$/.test(p)],
];

/** Which parts of the repository changed, and how many files in each. */
export function areas(paths: readonly string[]): string[] {
  const counts = new Map<string, number>();
  for (const path of paths) {
    const area = AREAS.find(([, test]) => test(path))?.[0] ?? "Other";
    counts.set(area, (counts.get(area) ?? 0) + 1);
  }
  return [...counts].map(([area, count]) => `${area} (${count})`);
}

/** The local checks (scripts/check's jobs) whose files changed, which CI's jobs cover. */
export const checksCovering = (paths: readonly string[]) =>
  JOBS.filter((job) => paths.some((path) => isInput(job.inputs, path))).map((job) => job.name);

export const goldens = (changed: readonly Changed[]) =>
  changed.filter((change) => /(^|\/)golden\/[^/]+\.wav$/.test(change.path));

const when = (iso: string) => iso.replace("T", " ").replace(/:\d\dZ$/, "Z");

const quote = (text: string) =>
  text
    .trim()
    .split("\n")
    .map((line) => `> ${line}`.trimEnd())
    .join("\n");

// The report's own sections are marked "==== Name ====", since the PR body
// and reviews it quotes have Markdown headings of their own.

/** Earlier reviews, each with the inline threads it started. */
export function formatReviews(reviews: readonly Review[], comments: readonly ReviewComment[]): string {
  const all = threads(comments);
  // A reply is posted as a review of its own, with no body: leave those out.
  const shown = reviews.filter((review) => review.body.trim() || all.some((t) => t.root.pull_request_review_id === review.id));
  if (shown.length === 0) return "None.";
  const parts = shown.map((review, i) => {
    const lines = [
      `### Review ${i + 1}: of ${shortSha(review.commit_id)}, ${when(review.submitted_at)}, ${review.html_url}`,
      "",
      review.body.trim() || "(no body)",
    ];
    for (const thread of all.filter((t) => t.root.pull_request_review_id === review.id)) {
      lines.push("", `#### ${thread.root.path}:${thread.root.line ?? thread.root.original_line ?? "?"} (on ${shortSha(thread.root.original_commit_id)})`);
      lines.push(quote(thread.root.body));
      for (const reply of thread.replies) lines.push("", `Reply, ${when(reply.created_at)}:`, quote(reply.body));
    }
    return lines.join("\n");
  });
  // Threads started outside a review shown above.
  const ids = new Set(shown.map((review) => review.id));
  const loose = all.filter((t) => !ids.has(t.root.pull_request_review_id ?? -1));
  for (const thread of loose) {
    const lines = [`### Comment on ${thread.root.path}, ${when(thread.root.created_at)}`, quote(thread.root.body)];
    for (const reply of thread.replies) lines.push("", `Reply, ${when(reply.created_at)}:`, quote(reply.body));
    parts.push(lines.join("\n"));
  }
  return parts.join("\n\n");
}

export function formatConversation(comments: readonly IssueComment[]): string {
  if (comments.length === 0) return "None.";
  return comments.map((comment) => `### ${when(comment.created_at)}\n\n${comment.body.trim()}`).join("\n\n");
}

/** This checkout's scripts, which the worktree may be too old to have. */
const SCRIPTS = join(import.meta.dirname, "..", "..", "scripts");
const CI_WAIT = join(SCRIPTS, "ci-wait");

/** The fixed place a PR's review worktree lives, whichever checkout the reviewer starts in. */
export const worktreePath = (main: string, pr: number) => join(main, ".claude", "worktrees", `review-pr-${pr}`);

/** Creates the review worktree at `sha`, or resets the one earlier rounds used, so its build stays warm. */
function prepareWorktree(root: string, path: string, sha: string): string {
  const registered = git(root, ["worktree", "list", "--porcelain"])
    .split("\n")
    .some((line) => line === `worktree ${path}`);
  if (!registered) {
    if (existsSync(path)) throw new Error(`${path} exists but isn't a git worktree. Move it away and run this again.`);
    git(root, ["worktree", "add", "--quiet", "--detach", path, sha]);
    return `created at ${shortSha(sha)}`;
  }
  restorePending(path, () => {});
  const was = git(path, ["rev-parse", "HEAD"]);
  const dirty = git(path, ["status", "--porcelain"]);
  // Ignored files stay: target/ and node_modules/ are the warm build.
  git(path, ["reset", "--quiet", "--hard"]);
  git(path, ["clean", "--quiet", "-fd"]);
  git(path, ["checkout", "--quiet", "--detach", sha]);
  const discarded = dirty ? ` (discarded its local changes: ${dirty.split("\n").length} files)` : "";
  return was === sha ? `already at ${shortSha(sha)}${discarded}` : `reset from ${shortSha(was)} to ${shortSha(sha)}${discarded}`;
}

/** Makes sure the PR's commits and origin/main are here. */
function fetch(root: string, pr: number, sha: string) {
  git(root, ["fetch", "--quiet", "origin", "main", `+refs/pull/${pr}/head:refs/remotes/origin/pr/${pr}`]);
  if (tryGit(root, ["cat-file", "-e", `${sha}^{commit}`]) === undefined) git(root, ["fetch", "--quiet", "origin", sha]);
}

export function reviewSetup(cwd: string, args: Args): string {
  const root = topLevel(cwd);
  const main = mainCheckout(root) ?? root;
  const pull = github.pull(args.pr);
  const head = pull.headRefOid;
  const commit = args.commit ? pull.commits.find((c) => c.oid.startsWith(args.commit ?? ""))?.oid : head;
  if (!commit) throw new Error(`${args.commit} isn't one of PR #${args.pr}'s commits.`);

  let reviews = github.reviews(args.pr);
  let comments = github.reviewComments(args.pr);
  let conversation = github.issueComments(args.pr);
  let body = pull.body;
  let leftOut = "";
  const at = args.commit ? cutoff(commit, pull.commits, reviews) : undefined;
  if (at) {
    const count = reviews.length + comments.length + conversation.length;
    reviews = reviews.filter((review) => review.submitted_at < at);
    comments = comments.filter((comment) => comment.created_at < at);
    conversation = conversation.filter((comment) => comment.created_at < at);
    body = bodyAt(body, github.bodyEdits(args.pr), at);
    const dropped = count - reviews.length - comments.length - conversation.length;
    leftOut = `Left out everything recorded from ${when(at)} on (${dropped} of its reviews and comments), and the PR body is as it was then.`;
  }

  fetch(root, args.pr, commit);
  const path = worktreePath(main, args.pr);
  const worktree = prepareWorktree(root, path, commit);
  // Here rather than by the post-checkout hook, which an older commit doesn't have.
  const warmed: string[] = [];
  warm(path, (text) => warmed.push(text));
  const base = git(path, ["merge-base", "origin/main", commit]);
  const changed = parseNameStatus(git(path, ["diff", "--name-status", base, commit]));
  const paths = changed.map((change) => change.path);
  const golden = goldens(changed);

  const reviewed = reviews.filter((review) => review.body.trim()).at(-1)?.commit_id;
  const runs = latestPerName(github.checkRuns(commit));
  const state = judge(runs);
  const waiting = state === "running" ? " Wait for it with ci-wait." : "";
  let ci = runs.length ? `${table(runs)}\n\nState: ${state}.${waiting}` : `No checks have run on ${shortSha(commit)}. Wait for them with ci-wait.`;
  if (!runs.length) {
    const older = [...pull.commits].reverse().find((c) => c.oid !== commit && github.checkRuns(c.oid).length);
    if (older) ci += ` The newest are for ${shortSha(older.oid)}:\n\n${table(latestPerName(github.checkRuns(older.oid)))}`;
  }

  const ticket = ticketId(body);
  const out: string[] = [
    `==== PR #${pull.number}: ${pull.title} ====`,
    "",
    `${pull.url} (${pull.state.toLowerCase()})`,
    `Branch: ${pull.headRefName}`,
    `Reviewing: ${shortSha(commit)}${commit === head ? " (the head)" : `, given with --commit (the head is ${shortSha(head)})`}`,
    `Round: ${args.round}${args.round > 1 && !reviewed ? ". No earlier review was found, so review it as a first round." : ""}`,
    `Ticket: ${ticket ? `${ticket} (.claude/skills/uta-notion/ntn.sh pages get ${ticket})` : "no Notion link found in the PR body"}`,
    `Worktree: ${path}, ${worktree}. cd there and work there; keep it at the end of the round.`,
    ...warmed.map((line) => `  ${line}`),
    `  Run from it (they work at any commit): ${CI_WAIT} ${args.pr}, ${join(SCRIPTS, "mutate")}`,
    `  app/node_modules: ${nodeModulesCurrent(path) ? "current" : "missing or out of date: run npm ci in app/ before any frontend test"}`,
  ];
  if (leftOut) out.push("", leftOut);
  out.push(
    "",
    "==== Changes ====",
    "",
    `Against the merge base with main (${shortSha(base)}): git diff ${shortSha(base)} ${shortSha(commit)}`,
    "",
    git(path, ["diff", "--stat=120", base, commit]),
    "",
    `Areas: ${areas(paths).join(", ") || "none"}`,
    `Local checks covering them: ${checksCovering(paths).join(", ") || "none"}`,
    `Golden WAVs changed: ${golden.length ? golden.map((g) => `${g.status} ${g.path}`).join(", ") : "none"}`,
  );
  if (reviewed && reviewed !== commit) {
    const since = tryGit(path, ["log", "--oneline", `${reviewed}..${commit}`]);
    out.push(
      "",
      `Since the last review (of ${shortSha(reviewed)}): git diff ${shortSha(reviewed)} ${shortSha(commit)}`,
      "",
      since ?? "(that commit is no longer in the branch's history: it was rebased)",
      "",
      tryGit(path, ["diff", "--stat=120", reviewed, commit]) ?? "",
    );
  }
  out.push(
    "",
    `==== CI on ${shortSha(commit)} ====`,
    "",
    ci,
    "",
    "==== PR body ====",
    "",
    body.trim(),
    "",
    "==== Earlier reviews ====",
    "",
    formatReviews(reviews, comments),
    "",
    "==== Conversation ====",
    "",
    formatConversation(conversation),
  );
  return out.join("\n");
}
