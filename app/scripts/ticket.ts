// `scripts/ticket`: a ticket's Notion and GitHub steps, each one command. The
// commands are here, written against `Services`, so the tests can hand them a
// fake Notion and GitHub. ticketServices.ts has the real ones, and
// ticketCli.ts the command line.

export type Ticket = {
  pageId: string;
  id: number;
  name: string;
  status: string;
  type: string;
  pr?: string;
  projectId?: string;
};

export type Project = { pageId: string; name: string; status: string };

export type Todo = { blockId: string; text: string; checked: boolean };

export type PrState = "OPEN" | "MERGED" | "CLOSED";

export type Check = { name: string; status?: string; conclusion?: string; state?: string };

export type Pr = {
  number: number;
  url: string;
  state: PrState;
  branch: string;
  headSha: string;
  checks: Check[];
  /** Every review's body, oldest first. */
  reviews: string[];
};

/** What the commands read. Reads happen in a dry run too: they decide what it would do. */
export type Services = {
  tickets(): Ticket[];
  /** A page's body as Markdown. */
  body(pageId: string): string;
  project(pageId: string): Project;
  todos(pageId: string): Todo[];
  /** Every PR's state, by number. */
  prStates(): Map<number, PrState>;
  pr(number: number): Pr;
};

/** A command that changes something: a Notion call through ntn.sh, or gh or git. */
export type Call = { tool: "ntn.sh" | "gh" | "git"; args: string[] };

/** One change: what it does, in words, and the call that does it. */
export type Action = { summary: string; call: Call };

/** Runs an action and returns its output. In a dry run it prints the call instead and returns undefined. */
export type Run = (action: Action) => string | undefined;

export class Refusal extends Error {}

const STARTED = (branch: string) => `🤖 Claude: Started on branch ${branch}.`;

// ---------------------------------------------------------------- reading

/** "UTA-62", "uta-62" or "62" as 62. */
export function parseId(text: string): number {
  const match = /^(?:uta-)?(\d+)$/i.exec(text.trim());
  if (!match) throw new Refusal(`Not a ticket ID: ${text}. Use UTA-n.`);
  return Number(match[1]);
}

export const label = (id: number) => `UTA-${id}`;

/** The UTA-n IDs listed under a ticket body's "Depends on" heading, in order, without repeats. */
export function parseDependsOn(markdown: string): number[] {
  const lines = markdown.split("\n");
  const start = lines.findIndex((line) => /^#{1,6}\s+Depends on\s*$/i.test(line.trim()));
  if (start === -1) return [];
  const section: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (/^#{1,6}\s/.test(line.trim())) break;
    section.push(line);
  }
  const ids = [...section.join("\n").matchAll(/\bUTA-(\d+)\b/gi)].map((m) => Number(m[1]));
  return [...new Set(ids)];
}

/** The number of a GitHub PR URL. */
export function prNumber(url: string | undefined): number | undefined {
  const match = url && /\/pull\/(\d+)/.exec(url);
  return match ? Number(match[1]) : undefined;
}

// ---------------------------------------------------------------- dependencies

export type Dependency = {
  id: number;
  ticket?: Ticket;
  /** "merged", "not merged" (open or closed), "no PR", or "unknown PR" when GitHub doesn't have it. */
  pr: "merged" | "not merged" | "no PR" | "unknown PR";
  /** Done, and its PR merged (or it never had one). */
  met: boolean;
};

/**
 * Where each dependency stands. `assumeMerged` is a ticket to count as Done
 * and merged whatever Notion and GitHub say: the one being merged, so a dry
 * run shows what the real run would do.
 */
export function dependencies(
  ids: number[],
  tickets: Ticket[],
  prStates: Map<number, PrState>,
  assumeMerged?: number,
): Dependency[] {
  return ids.map((id) => {
    const ticket = tickets.find((t) => t.id === id);
    if (id === assumeMerged) return { id, ticket, pr: "merged", met: true };
    const number = prNumber(ticket?.pr);
    const state = number === undefined ? undefined : prStates.get(number);
    const pr =
      number === undefined ? "no PR" : state === undefined ? "unknown PR" : state === "MERGED" ? "merged" : "not merged";
    const met = ticket?.status === "Done" && (pr === "merged" || pr === "no PR");
    return { id, ticket, pr, met };
  });
}

export function describeDependency(dep: Dependency): string {
  if (!dep.ticket) return `${label(dep.id)}  not in Notion`;
  const pr = dep.pr === "no PR" ? "no PR" : `PR ${dep.pr}`;
  return `${label(dep.id)}  ${dep.ticket.status}, ${pr}  ${dep.ticket.name}`;
}

/** Why a ticket can't start, or undefined if it can. */
export function whyNotStartable(ticket: Ticket, deps: Dependency[]): string | undefined {
  const reasons: string[] = [];
  if (ticket.status !== "Ready") reasons.push(`is ${ticket.status}, not Ready`);
  const unmet = deps.filter((d) => !d.met);
  if (unmet.length > 0) reasons.push(`depends on ${unmet.map(describeDependency).join("; ")}`);
  return reasons.length === 0 ? undefined : `${label(ticket.id)} ${reasons.join(", and ")}.`;
}

// ---------------------------------------------------------------- ticking

/** Lower case, without Markdown marks, with plain quotes and single spaces, for matching criteria by text. */
export function normalise(text: string): string {
  return text
    .replace(/[`*_]/g, "")
    .replace(/[‘’]/g, "'")
    .replace(/[“”]/g, '"')
    .replace(/…/g, "...")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
}

/**
 * The checkbox each criterion names: the one whose text contains it, or, if
 * several do, the one whose text is exactly it. Throws a Refusal listing every
 * criterion that matched none or several, and the checkboxes there are.
 */
export function matchTodos(todos: Todo[], criteria: string[]): Todo[] {
  const problems: string[] = [];
  const found: Todo[] = [];
  for (const criterion of criteria) {
    const wanted = normalise(criterion);
    if (!wanted) {
      problems.push("An empty criterion matches every checkbox.");
      continue;
    }
    const matches = todos.filter((t) => normalise(t.text).includes(wanted));
    const exact = matches.filter((t) => normalise(t.text) === wanted);
    if (matches.length === 1) found.push(matches[0]);
    else if (exact.length === 1) found.push(exact[0]);
    else if (matches.length === 0) problems.push(`No criterion matches "${criterion}".`);
    else problems.push(`"${criterion}" matches ${matches.length} criteria:\n${matches.map((t) => `    - ${t.text}`).join("\n")}`);
  }
  if (problems.length > 0) {
    const list = todos.map((t) => `    [${t.checked ? "x" : " "}] ${t.text}`).join("\n");
    throw new Refusal(`${problems.join("\n")}\n  The ticket's criteria:\n${list || "    (none)"}`);
  }
  return [...new Set(found)];
}

// ---------------------------------------------------------------- reviews and CI

export type Verdict = "Ready to merge" | "Changes needed";

/** A reviewer's review: the pr-reviewer starts its body "🤖 Claude (review…" or "🤖 Claude (re-review…". */
export const isReview = (body: string) => /^🤖 Claude \((re-)?review/.test(body.trimStart());

/**
 * The verdict a review gives. The reviewer puts it in bold ("**Verdict: Ready
 * to merge.**" or "**Changes needed.**"), so the first bold one counts, and
 * otherwise whichever phrase comes first. "Not ready to merge" is Changes needed.
 */
export function verdict(body: string): Verdict | undefined {
  const match =
    /\*\*(?:verdict:\s*)?(not\s+)?(ready to merge|changes needed)/i.exec(body) ??
    /(not\s+)?(ready to merge|changes needed)/i.exec(body);
  if (!match) return undefined;
  return !match[1] && match[2].toLowerCase() === "ready to merge" ? "Ready to merge" : "Changes needed";
}

const PASSED = new Set(["SUCCESS", "SKIPPED", "NEUTRAL"]);
const WAITING = new Set(["PENDING", "EXPECTED"]);

/** How a check ended, or PENDING. A check run has status and conclusion; a commit status has only state. */
const outcome = (c: Check) => (c.status !== undefined ? (c.status === "COMPLETED" ? (c.conclusion ?? "") : "PENDING") : (c.state ?? ""));

/** "passing", "pending" or "failed", for the checks on a PR's head commit, or "none" when it has none. */
export function ciState(checks: Check[]): "passing" | "pending" | "failed" | "none" {
  if (checks.length === 0) return "none";
  const outcomes = checks.map(outcome);
  if (outcomes.some((o) => !PASSED.has(o) && !WAITING.has(o))) return "failed";
  if (outcomes.some((o) => WAITING.has(o))) return "pending";
  return "passing";
}

/** Why a PR can't merge, or undefined if it can. */
export function whyNotMergeable(pr: Pr): string | undefined {
  if (pr.state === "CLOSED") return `PR #${pr.number} is closed without merging.`;
  const reviews = pr.reviews.filter(isReview);
  const newest = reviews.at(-1);
  if (!newest) return `PR #${pr.number} has no review yet. Run the review-round workflow.`;
  const said = verdict(newest);
  if (said !== "Ready to merge") {
    return `The newest review of PR #${pr.number} says ${said ?? "neither Ready to merge nor Changes needed"}.`;
  }
  const ci = ciState(pr.checks);
  if (ci !== "passing") {
    const names = pr.checks
      .filter((c) => !PASSED.has(outcome(c)))
      .map((c) => c.name)
      .join(", ");
    const what = ci === "none" ? "no CI checks" : ci === "pending" ? `CI still running (${names})` : `CI failing (${names})`;
    return `PR #${pr.number} has ${what} on ${pr.headSha.slice(0, 7)}.`;
  }
  return undefined;
}

// ---------------------------------------------------------------- calls

const json = (value: unknown) => JSON.stringify(value);

const setStatus = (pageId: string, status: string): Call => ({
  tool: "ntn.sh",
  args: ["api", `v1/pages/${pageId}`, "-X", "PATCH", "-d", json({ properties: { Status: { status: { name: status } } } })],
});

const comment = (pageId: string, text: string): Call => ({
  tool: "ntn.sh",
  args: ["api", "v1/comments", "-d", json({ parent: { page_id: pageId }, rich_text: [{ text: { content: text } }] })],
});

const tickCall = (blockId: string): Call => ({
  tool: "ntn.sh",
  args: ["api", `v1/blocks/${blockId}`, "-X", "PATCH", "-d", json({ to_do: { checked: true } })],
});

/** Notion caps a rich text item at 2000 characters, so longer text goes in several. */
function richText(text: string) {
  const parts = [];
  for (let at = 0; at < text.length; at += 2000) parts.push({ text: { content: text.slice(at, at + 2000) } });
  return parts;
}

/** A shell-quoted command line to run from the repository root, for printing. */
export function commandLine(call: Call): string {
  const quote = (arg: string) => (/^[\w@%+=:,./-]+$/.test(arg) ? arg : `'${arg.replace(/'/g, `'\\''`)}'`);
  const tool = call.tool === "ntn.sh" ? ".claude/skills/uta-notion/ntn.sh" : call.tool;
  return [tool, ...call.args].map(quote).join(" ");
}

// ---------------------------------------------------------------- commands

function find(services: Services, id: number): { tickets: Ticket[]; ticket: Ticket } {
  const tickets = services.tickets();
  const ticket = tickets.find((t) => t.id === id);
  if (!ticket) throw new Refusal(`${label(id)} isn't in the Tickets database.`);
  return { tickets, ticket };
}

export function show(services: Services, id: number): string[] {
  const { tickets, ticket } = find(services, id);
  const deps = dependencies(parseDependsOn(services.body(ticket.pageId)), tickets, services.prStates());
  const project = ticket.projectId ? services.project(ticket.projectId) : undefined;
  return [
    `${label(id)}  ${ticket.name}`,
    `  Page     ${ticket.pageId}`,
    `  Status   ${ticket.status}`,
    `  Type     ${ticket.type || "none"}`,
    `  Project  ${project ? `${project.name} (${project.status})` : "none"}`,
    `  PR       ${ticket.pr ?? "none"}`,
    `  Depends on${deps.length === 0 ? "  nothing" : ""}`,
    ...deps.map((d) => `    ${d.met ? "✓" : "✗"} ${describeDependency(d)}`),
  ];
}

export type ReadyReport = { startable: Ticket[]; waiting: { ticket: Ticket; deps: Dependency[] }[]; running: Ticket[] };

export function readyReport(services: Services): ReadyReport {
  const tickets = services.tickets();
  const prStates = services.prStates();
  const startable: Ticket[] = [];
  const waiting: ReadyReport["waiting"] = [];
  for (const ticket of tickets.filter((t) => t.status === "Ready").sort((a, b) => a.id - b.id)) {
    const deps = dependencies(parseDependsOn(services.body(ticket.pageId)), tickets, prStates);
    if (deps.every((d) => d.met)) startable.push(ticket);
    else waiting.push({ ticket, deps: deps.filter((d) => !d.met) });
  }
  const running = tickets.filter((t) => t.status === "In Progress" || t.status === "In Review").sort((a, b) => a.id - b.id);
  return { startable, waiting, running };
}

export function ready(services: Services): string[] {
  const { startable, waiting, running } = readyReport(services);
  const names = new Map<string, string>();
  const project = (t: Ticket) => {
    if (!t.projectId) return "no project";
    if (!names.has(t.projectId)) names.set(t.projectId, services.project(t.projectId).name);
    return names.get(t.projectId);
  };
  const line = (t: Ticket) => `  ${label(t.id)}  ${t.name}  (${project(t)})`;
  const out: string[] = [];
  if (startable.length === 0) out.push("Nothing is ready to start.");
  else {
    out.push("Ready to start (Ready, with every dependency Done and merged):", ...startable.map(line));
    // A dependency that's Done can't be another ticket here, so none of these waits for another.
    if (startable.length > 1) out.push("None depends on another, so they can all run alongside each other.");
  }
  if (waiting.length > 0) {
    out.push("", "Marked Ready, but waiting:");
    for (const { ticket, deps } of waiting) out.push(`${line(ticket)}  on ${deps.map(describeDependency).join("; ")}`);
  }
  if (running.length > 0) {
    out.push("", "Running now:", ...running.map((t) => `${line(t)}  ${t.status}`));
  }
  return out;
}

export function claim(services: Services, run: Run, id: number, branch: string): string[] {
  const { tickets, ticket } = find(services, id);
  const deps = dependencies(parseDependsOn(services.body(ticket.pageId)), tickets, services.prStates());
  const why = whyNotStartable(ticket, deps);
  if (why) throw new Refusal(`Not claiming: ${why}`);
  const actions: Action[] = [{ summary: `Set ${label(id)} to In Progress.`, call: setStatus(ticket.pageId, "In Progress") }];
  if (ticket.projectId) {
    const project = services.project(ticket.projectId);
    if (project.status === "Planned") {
      actions.push({ summary: `Set the project ${project.name} to Active.`, call: setStatus(project.pageId, "Active") });
    }
  }
  actions.push({ summary: `Comment "${STARTED(branch)}"`, call: comment(ticket.pageId, STARTED(branch)) });
  for (const action of actions) run(action);
  return [];
}

export function tick(services: Services, run: Run, id: number, criteria: string[]): string[] {
  const { ticket } = find(services, id);
  const out: string[] = [];
  for (const todo of matchTodos(services.todos(ticket.pageId), criteria)) {
    if (todo.checked) out.push(`Already ticked: ${todo.text}`);
    else run({ summary: `Tick: ${todo.text}`, call: tickCall(todo.blockId) });
  }
  return out;
}

/**
 * The Backlog tickets that merging `id` unblocks: those that list it under
 * Depends on, and whose other dependencies are all Done and merged.
 */
export function unblockedBy(services: Services, tickets: Ticket[], prStates: Map<number, PrState>, id: number): Ticket[] {
  return tickets
    .filter((t) => t.status === "Backlog")
    .filter((t) => {
      const ids = parseDependsOn(services.body(t.pageId));
      return ids.includes(id) && dependencies(ids, tickets, prStates, id).every((d) => d.met);
    })
    .sort((a, b) => a.id - b.id);
}

export function merge(services: Services, run: Run, id: number): string[] {
  const { tickets, ticket } = find(services, id);
  if (ticket.status !== "In Review" && ticket.status !== "Done") {
    throw new Refusal(`Not merging: ${label(id)} is ${ticket.status}, not In Review.`);
  }
  const number = prNumber(ticket.pr);
  if (number === undefined) throw new Refusal(`Not merging: ${label(id)} has no PR link.`);
  const pr = services.pr(number);
  const why = whyNotMergeable(pr);
  if (why) throw new Refusal(`Not merging: ${why}`);

  const out: string[] = [];
  if (pr.state === "MERGED") out.push(`PR #${number} is already merged.`);
  else {
    // Only the commit whose CI was checked: gh refuses if a push has landed since.
    run({
      summary: `Squash merge PR #${number} at ${pr.headSha.slice(0, 7)}.`,
      call: { tool: "gh", args: ["pr", "merge", String(number), "--squash", "--match-head-commit", pr.headSha] },
    });
    // Not gh's --delete-branch: from a worktree it tries to switch to main, which another checkout has.
    run({ summary: `Delete the remote branch ${pr.branch}.`, call: { tool: "git", args: ["push", "origin", "--delete", pr.branch] } });
  }

  if (ticket.status !== "Done") run({ summary: `Set ${label(id)} to Done.`, call: setStatus(ticket.pageId, "Done") });
  const prStates = services.prStates();
  const unblocked = unblockedBy(services, tickets, prStates, id);
  for (const t of unblocked) {
    run({ summary: `Set ${label(t.id)} to Ready (${t.name}).`, call: setStatus(t.pageId, "Ready") });
  }
  if (unblocked.length === 0) out.push("It unblocks no Backlog tickets.");

  if (ticket.projectId) {
    const project = services.project(ticket.projectId);
    const rest = tickets.filter((t) => t.projectId === ticket.projectId && t.id !== id);
    if (project.status !== "Done" && rest.every((t) => t.status === "Done")) {
      run({ summary: `Set the project ${project.name} to Done: every ticket is.`, call: setStatus(project.pageId, "Done") });
    }
  }

  // What's ready once this is merged: the tickets just unblocked count as Ready.
  const after = tickets.map((t) =>
    t.id === id ? { ...t, status: "Done" } : unblocked.some((u) => u.id === t.id) ? { ...t, status: "Ready" } : t,
  );
  const report = ready({ ...services, tickets: () => after, prStates: () => new Map([...prStates, [number, "MERGED"]]) });
  return [...out, "", ...report];
}

export type NewDecision = {
  name: string;
  needed: string;
  chosen: string;
  principles: string;
  alternatives: string;
  ticket: number;
};

/** The body that creates a Design decisions row, for `ntn.sh decisions create -d`. */
export function decisionBody(decision: NewDecision, ticketPageId: string) {
  return {
    properties: {
      Name: { title: [{ text: { content: decision.name } }] },
      Status: { status: { name: "Open" } },
      Needed: { rich_text: richText(decision.needed) },
      "Chosen and why": { rich_text: richText(decision.chosen) },
      Principles: { rich_text: richText(decision.principles) },
      Alternatives: { rich_text: richText(decision.alternatives) },
      Ticket: { relation: [{ id: ticketPageId }] },
    },
  };
}

/** The D-n Notion gave a new decision, from its create response. */
export function decisionNumber(response: string): { id: string; url: string } {
  const page = JSON.parse(response) as { url: string; properties: { ID: { unique_id: { number: number } } } };
  return { id: `D-${page.properties.ID.unique_id.number}`, url: page.url };
}

export function newDecision(services: Services, run: Run, decision: NewDecision): string[] {
  const { ticket } = find(services, decision.ticket);
  const response = run({
    summary: `Create the design decision "${decision.name}" for ${label(ticket.id)}.`,
    call: { tool: "ntn.sh", args: ["decisions", "create", "-d", json(decisionBody(decision, ticket.pageId))] },
  });
  if (response === undefined) return ["Notion gives it its D-n when it's created."];
  const { id, url } = decisionNumber(response);
  return [`${id}  ${url}`, `Mark the code with: provisional: ${id}`];
}
