// What the review scripts read from GitHub, through the `gh` CLI. Each call
// returns plain data, so the logic built on it can be tested without GitHub.

import { execFileSync } from "node:child_process";

export const REPO = "willmcneilly/uta";

/** Runs `gh` and returns its output. */
export function gh(args: readonly string[]): string {
  return execFileSync("gh", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 64 * 1024 * 1024,
  });
}

/** Every page of a REST call, as an array of pages. */
function pages<T>(path: string): T[] {
  return JSON.parse(gh(["api", "--paginate", "--slurp", path])) as T[];
}

export type PullCommit = { oid: string; committedDate: string; messageHeadline: string };

export type Pull = {
  number: number;
  title: string;
  url: string;
  state: string;
  headRefName: string;
  headRefOid: string;
  body: string;
  createdAt: string;
  commits: PullCommit[];
};

export function pull(pr: number): Pull {
  const fields = "number,title,url,state,headRefName,headRefOid,body,createdAt,commits";
  // `gh pr view` without --json asks for the retired classic Projects field and fails.
  return JSON.parse(gh(["pr", "view", String(pr), "--repo", REPO, "--json", fields])) as Pull;
}

/** Just the head commit, for polling. */
export function pullHead(pr: number): string {
  return gh(["pr", "view", String(pr), "--repo", REPO, "--json", "headRefOid", "-q", ".headRefOid"]).trim();
}

/** Every version of the PR's body, oldest first: GitHub keeps the full text of each edit. */
export type BodyEdit = { editedAt: string; body: string };

export function bodyEdits(pr: number): BodyEdit[] {
  const query = `query($pr: Int!) { repository(owner: "willmcneilly", name: "uta") {
    pullRequest(number: $pr) { userContentEdits(first: 100) { nodes { editedAt diff } } } } }`;
  const out = JSON.parse(gh(["api", "graphql", "-f", `query=${query}`, "-F", `pr=${pr}`])) as {
    data: { repository: { pullRequest: { userContentEdits: { nodes: { editedAt: string; diff: string | null }[] } } } };
  };
  return out.data.repository.pullRequest.userContentEdits.nodes
    .map((node) => ({ editedAt: node.editedAt, body: node.diff ?? "" }))
    .sort((a, b) => a.editedAt.localeCompare(b.editedAt));
}

export type Review = {
  id: number;
  commit_id: string;
  submitted_at: string;
  state: string;
  body: string;
  html_url: string;
  user: { login: string };
};

export const reviews = (pr: number) => pages<Review[]>(`repos/${REPO}/pulls/${pr}/reviews`).flat();

/** An inline comment on the diff. A reply has `in_reply_to_id`. */
export type ReviewComment = {
  id: number;
  in_reply_to_id?: number | null;
  pull_request_review_id: number | null;
  path: string;
  line: number | null;
  original_line: number | null;
  commit_id: string;
  original_commit_id: string;
  created_at: string;
  body: string;
  user: { login: string };
};

export const reviewComments = (pr: number) => pages<ReviewComment[]>(`repos/${REPO}/pulls/${pr}/comments`).flat();

/** A comment on the PR's conversation, not on a line. */
export type IssueComment = { id: number; created_at: string; body: string; user: { login: string } };

export const issueComments = (pr: number) => pages<IssueComment[]>(`repos/${REPO}/issues/${pr}/comments`).flat();

export type CheckRun = {
  name: string;
  head_sha: string;
  status: string;
  conclusion: string | null;
  started_at: string | null;
  completed_at: string | null;
  html_url: string;
};

export function checkRuns(sha: string): CheckRun[] {
  return pages<{ check_runs: CheckRun[] }>(`repos/${REPO}/commits/${sha}/check-runs?per_page=100`).flatMap(
    (page) => page.check_runs,
  );
}
