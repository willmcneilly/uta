---
name: plan-project
description: Turn an accepted Uta RFC into a project and tickets. Use when Will says /plan-project, "let's plan RFC-n", "break this into tickets", or asks to add a batch of work to an existing project. Covers sizing and ordering tickets, drafting the plan locally, reviewing it in Crit, and creating the project and tickets in Notion on sign-off.
---

# Planning a project

This turns an accepted RFC into work agents can pick up. The plan's job is to let Will see, before anything starts, what will be delivered in what order and how each piece will be proven. It also stops the work arriving as a flood of tiny PRs.

Workflow: read the RFC → draft the plan locally → review in Crit → create in Notion on sign-off. For Notion mechanics, use the `notion` skill. For the Crit loop, use `crit:crit`.

**Only plan from an accepted RFC.** If the RFC isn't accepted yet, say so and stop. Small work that doesn't need an RFC (a bug, a tweak inside an existing project) can go straight in as a single ticket. Tell Will when you add one.

## 1. Read

- The accepted RFC in `docs/rfcs/`, especially **Proposal**, **How we'll verify it** and any resolved open questions or amendments.
- Existing projects and open tickets in Notion, so you don't duplicate work and can decide whether this is a new project or belongs in an existing one.
- The current code, so tickets start from what actually exists.
- The scratchpad. Run its review (the `scratchpad` skill), starting with the notes that touch this project. Notes Will folds in become ticket scope, or go in "Not in this project" with a reason.

## 2. Break it down

**One ticket = one PR** that a reviewing agent can check on its own and that leaves `main` working.

Sizing:
- **Big enough to matter.** Each ticket delivers something you can verify, not just "a layer" that can't be tested until three tickets later. Tests belong in the ticket that adds the code, never in a separate ticket.
- **Small enough to review.** If you can't describe its acceptance criteria in about six bullets, or it touches several unrelated areas, split it.
- **Rough range:** 4–8 tickets for a project of one RFC. More than about 10 means the tickets are too small or the RFC is too big. Say which.
- Prefer thin end-to-end slices (a crude version working through every layer, then improved) over building each layer in full one at a time. Pure setup work, such as a repo skeleton or CI, is fine as its own ticket.

For each ticket, write:
- **Name:** an imperative phrase ("Play a tone through cpal with device recovery").
- **Type:** Feature · Bug · Chore · Spike. Chores are work Will won't notice, such as CI or refactors. Spikes are time-boxed investigations whose output is findings, not merged product code; give them a timebox.
- **Goal:** one or two sentences in plain language: what exists after this that didn't before.
- **Acceptance criteria:** a checklist of observable results: tests that exist and pass, behaviour Will can try, checks that are green. Take them from the RFC's verification section wherever possible.
- **Out of scope:** what a reader might assume is included but isn't, especially things the next ticket does.
- **Depends on:** the tickets that must be merged first. Keep chains short, so work can run in parallel where it's safe.
- **Context:** the RFC and the sections it implements.

Then check:
- **Coverage.** Every item in the RFC's "How we'll verify it" maps to at least one ticket, and every part of the Proposal is covered or explicitly deferred. Anything deferred goes in the plan's "Not in this project" list, with a reason.
- **Order.** The first ticket has no dependencies. Each ticket, once merged, leaves the app building and every check passing.
- **Will's checkpoints.** Mark the tickets after which Will can try something themselves, and say what. Those are the moments the experiment depends on.

## 3. Draft the plan locally

Write `docs/plans/<project-slug>.md`:

```markdown
# Project: <Name>

**Status:** Draft · **RFC:** [RFC-n](../rfcs/rfc-nnn-....md) · **Notion:** not yet created

## Goal
Two or three sentences: what's true when the project is done.

## Tickets
### 1. <Name> (<Type>)
Goal · Acceptance criteria (checklist) · Out of scope · Depends on · Context
### 2. ...

## Order and checkpoints
The order to work in, what can run in parallel, and where Will gets to try something.

## Coverage
| RFC verification item | Ticket |
|---|---|

## Not in this project
Anything in the RFC we're deferring, and why.

## Open questions
Real decisions for Will only, each with a recommendation.
```

Give Will a short summary: how many tickets, the order, and where the checkpoints are. Then review it in Crit: run `crit docs/plans/<file>.md` in the background, give Will the URL, address every comment, reply without resolving, and repeat. A round with no comments approves the text. Creating the tickets still needs Will's explicit go-ahead.

## 4. Create in Notion (on Will's go-ahead)

1. **Project:** create a row in Projects with `Name`, `Status: Planned` and the `RFCs` relation set to the RFC's page. The relation is two-way, so the RFC gets the back-link automatically. The page body is the plan's Goal, Order and checkpoints, and Not in this project.
2. **Tickets:** create one row per ticket, in order, with `Name`, `Type`, `Project` relation and `Status`:
   - `Ready` if it has no unmet dependencies;
   - otherwise `Backlog`.

   The body uses the ticket template from the `notion` skill. Write **Depends on** using real IDs (`UTA-12`), so create tickets in dependency order.
3. **Write back** to the plan file: the Notion link, `**Status:** Created`, and each ticket's `UTA-n` next to its heading.
4. Give Will the project link and the list of `UTA-n` ticket IDs with their names.

**Don't start any ticket.** Starting work is a separate step that Will kicks off.

## Changing a plan mid-project

- **Adding one ticket** (a bug found, a missed piece): create it in Notion, add it to the plan file, and tell Will.
- **Adding several tickets, re-scoping, or dropping tickets:** update the plan file, show Will the change in Crit or chat first, and only then change Notion.
- **If the change contradicts the RFC:** it's an RFC amendment (see the `rfc` skill), not just a plan change.
