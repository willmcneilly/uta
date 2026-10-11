// The real Notion and GitHub behind `scripts/ticket`: Notion through the
// uta-notion skill's ntn.sh (Will's personal workspace), GitHub through gh.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { cleanEnv } from "./git.ts";
import {
  type Action,
  type Check,
  type Pr,
  type PrState,
  type Project,
  type Run,
  type Services,
  type Ticket,
  type Todo,
  commandLine,
} from "./ticket.ts";

type RichText = { plain_text: string }[];
type Paged<T> = { results: T[]; has_more: boolean; next_cursor: string | null };
type PageJson = { id: string; properties: Record<string, Record<string, unknown>> };

const plain = (text: RichText | undefined) => (text ?? []).map((t) => t.plain_text).join("");

export function realServices(checkout: string): Services & { run(dryRun: boolean): Run } {
  const ntnPath = join(checkout, ".claude", "skills", "uta-notion", "ntn.sh");

  const exec = (tool: string, args: string[]) =>
    execFileSync(tool === "ntn.sh" ? ntnPath : tool, args, {
      cwd: checkout,
      encoding: "utf8",
      env: cleanEnv(),
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 64 * 1024 * 1024,
    });
  const ntn = (...args: string[]) => exec("ntn.sh", args);
  const ntnJson = <T>(...args: string[]) => JSON.parse(ntn(...args)) as T;
  const gh = <T>(...args: string[]) => JSON.parse(exec("gh", args)) as T;

  /** Every page of a paged Notion list. `next` gives the arguments for the page after `cursor`. */
  function all<T>(next: (cursor?: string) => string[]): T[] {
    const results: T[] = [];
    let cursor: string | undefined;
    do {
      const page = ntnJson<Paged<T>>(...next(cursor));
      results.push(...page.results);
      cursor = page.has_more && page.next_cursor ? page.next_cursor : undefined;
    } while (cursor);
    return results;
  }

  function ticket(page: PageJson): Ticket {
    const p = page.properties as {
      Name: { title: RichText };
      ID: { unique_id: { number: number } };
      Status: { status: { name: string } | null };
      Type: { select: { name: string } | null };
      PR: { url: string | null };
      Project: { relation: { id: string }[] };
    };
    return {
      pageId: page.id,
      id: p.ID.unique_id.number,
      name: plain(p.Name.title),
      status: p.Status.status?.name ?? "",
      type: p.Type.select?.name ?? "",
      pr: p.PR.url ?? undefined,
      projectId: p.Project.relation[0]?.id,
    };
  }

  let ticketsCache: Ticket[] | undefined;
  const projects = new Map<string, Project>();
  let prStatesCache: Map<number, PrState> | undefined;

  return {
    tickets() {
      ticketsCache ??= all<PageJson>((cursor) => [
        "tickets",
        "query",
        "--json",
        "--limit",
        "100",
        ...(cursor ? ["--start-cursor", cursor] : []),
      ]).map(ticket);
      return ticketsCache;
    },

    body: (pageId) => ntn("pages", "get", pageId),

    project(pageId) {
      if (!projects.has(pageId)) {
        const page = ntnJson<PageJson>("api", `v1/pages/${pageId}`);
        const p = page.properties as { Name: { title: RichText }; Status: { status: { name: string } | null } };
        projects.set(pageId, { pageId, name: plain(p.Name.title), status: p.Status.status?.name ?? "" });
      }
      return projects.get(pageId) as Project;
    },

    todos(pageId): Todo[] {
      type Block = { id: string; type: string; to_do?: { rich_text: RichText; checked: boolean } };
      const blocks = all<Block>((cursor) => [
        "api",
        `v1/blocks/${pageId}/children`,
        "page_size==100",
        ...(cursor ? [`start_cursor==${cursor}`] : []),
      ]);
      return blocks
        .filter((b) => b.type === "to_do" && b.to_do)
        .map((b) => ({ blockId: b.id, text: plain(b.to_do?.rich_text), checked: b.to_do?.checked ?? false }));
    },

    prStates() {
      prStatesCache ??= new Map(
        gh<{ number: number; state: PrState }[]>("pr", "list", "--state", "all", "--limit", "1000", "--json", "number,state").map(
          (p) => [p.number, p.state],
        ),
      );
      return prStatesCache;
    },

    pr(number): Pr {
      // Always --json: plain `gh pr view` fails on this repo (the retired classic Projects field).
      const pr = gh<{
        number: number;
        url: string;
        state: PrState;
        headRefName: string;
        headRefOid: string;
        statusCheckRollup: { name?: string; context?: string; status?: string; conclusion?: string; state?: string }[];
        reviews: { body: string; submittedAt: string }[];
      }>("pr", "view", String(number), "--json", "number,url,state,headRefName,headRefOid,statusCheckRollup,reviews");
      const checks: Check[] = pr.statusCheckRollup.map((c) => ({
        name: c.name ?? c.context ?? "a check",
        status: c.status ?? undefined,
        conclusion: c.conclusion ?? undefined,
        state: c.state ?? undefined,
      }));
      const reviews = [...pr.reviews].sort((a, b) => a.submittedAt.localeCompare(b.submittedAt)).map((r) => r.body);
      return { number: pr.number, url: pr.url, state: pr.state, branch: pr.headRefName, headSha: pr.headRefOid, checks, reviews };
    },

    run: (dryRun) => (action: Action) => {
      if (dryRun) {
        console.log(`[dry run] ${action.summary}\n  ${commandLine(action.call)}`);
        return undefined;
      }
      const out = exec(action.call.tool, action.call.args);
      console.log(`✓ ${action.summary}`);
      return out;
    },
  };
}
