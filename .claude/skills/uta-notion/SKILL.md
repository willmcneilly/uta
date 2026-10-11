---
name: uta-notion
description: How to read and write Uta's Notion (RFCs, Projects, Tickets, Scratchpad, Glossary, Design decisions, Process log) in Will's personal workspace, with the project's ntn.sh wrapper. Use for anything in Notion while working on Uta: reading or updating a ticket, RFC or project, changing a status, adding a PR link, reading or posting comments, or querying what work is ready. Not the global `notion` skill, which defaults to Stora.
---

# Uta's Notion via `ntn.sh`

## A ticket's steps: `scripts/ticket`

Claiming, ticking criteria, merging and unblocking a ticket, finding what's ready, and logging a design decision are each one command in `scripts/ticket` (`--help` for the details). Use it rather than building those calls by hand:

```bash
scripts/ticket show UTA-n                        # page ID, status, project, PR, and each dependency
scripts/ticket ready                             # what can start now, what's waiting, what's running
scripts/ticket claim UTA-n --branch <branch>     # In Progress, project Active, the Started comment
scripts/ticket tick UTA-n "<criterion>" …        # tick acceptance criteria by their text
scripts/ticket merge UTA-n                       # merge the PR, Done, unblock, project Done
scripts/ticket decision new --ticket UTA-n --name … --needed … --chosen … --principles … --alternatives …
```

Every command takes `--dry-run`, which prints the `ntn.sh`, `gh` and `git` calls it would make. A ticket's dependencies are the `UTA-n` IDs under its `## Depends on` heading. Everything else goes through the wrapper below.

## Always use the wrapper

```bash
.claude/skills/uta-notion/ntn.sh <args>
```

Below, `N` means `.claude/skills/uta-notion/ntn.sh`.

- **Workspace.** It pins `NOTION_WORKSPACE_ID` to Will's **personal** workspace ("Will"). Bare `ntn` defaults to the **Stora** work workspace: never use it for this project. `N whoami` should say "Will".
- **Stdin.** Run without a terminal, `ntn api` waits on an open stdin forever, so the wrapper closes stdin for you. Don't add `</dev/null`. To pipe a body in, put `--stdin` first: `N --stdin pages update <id> < body.md`, or `N --stdin api v1/pages < body.json` for a large JSON body.
- **Named data sources.** `N <source> query [args]`, `N <source> create -d '<json>'` (sets `parent` for you) and `N <source> id`. The sources are in the table below.
- **Output.** `query` prints TSV (page id, then the properties); add `--json` before piping it to `jq` or `json.load`. Sort with `--sort "Prop desc"` (or `-s`), not `--sorts`.

## Uta IDs

Uta page: `3e83af96-9b6f-8190-b519-dc8cf3aeab6f` (https://app.notion.com/p/Uta-3e83af969b6f8190b519dc8cf3aeab6f)

| Database | Source name in `N` | Database ID |
|---|---|---|
| RFCs | `rfcs` | `5c6fe131-a98d-4ea9-8119-44e8e10af9c6` |
| Projects | `projects` | `f11524a3-50c2-4951-895d-ac946a5dbcd2` |
| Tickets | `tickets` | `ad66cdaf-cd4a-4914-b236-2cbc2f66291b` |
| Research | `research` | `aac52824-9d59-4441-bcb6-33e622295ba5` |
| Scratchpad | `scratchpad` | `057ed2bc-7159-4f9d-ba6d-b034fed1eacc` |
| Glossary | `glossary` | `8390e24d-2f94-4ad8-b596-7f8bb96f33f4` |
| Design decisions | `decisions` | `5ffe6cc3-e708-432a-9012-6365764d9c2f` |
| Process log | `process-log` | `1493a924-3535-4547-a39e-5be2c2f344b2` |

A new database gets a line in `ntn.sh`'s `source_id` and a row here.

Will's Notion user ID (for `people` properties such as RFC Author): `eafd9c84-49ac-412c-91f2-650a1edfa069`

**Properties** (names are exact):
- **RFCs:** `Name` (title), `Number` (auto, RFC-n), `Status` (Draft · In Discussion · Accepted · Rejected · Superseded), `Author` (people), `Date accepted` (date), `Projects` (relation)
- **Projects:** `Name`, `Status` (Planned · Active · Done), `RFCs` (relation), `Tickets` (relation), `Progress` (rollup, read-only)
- **Research:** `Name` (title), `Date` (date gathered), `RFCs` (relation; the back-link on RFCs is `Research`)
- **Tickets:** `Name`, `ID` (auto, UTA-n), `Status` (Backlog · Ready · In Progress · Blocked · In Review · Done), `Type` (Feature · Bug · Chore · Spike), `Project` (relation), `PR` (url)
- **Scratchpad:** see the `scratchpad` skill. RFCs and Projects have a `Scratchpad notes` back-link to it.
- **Design decisions:** the provisional design decisions log (see the `work-ticket` skill). Create a row with `scripts/ticket decision new`, which prints the `D-n` Notion gave it. `Name` (title, the decision in a few words), `ID` (auto, a plain number: Notion wants a prefix of at least two characters, so call number n `D-n` everywhere else), `Status` (Open · Revised · Adopted · Rejected), `Needed` (text: what the ticket needed that `DESIGN.md` doesn't cover), `Chosen and why` (text), `Principles` (text: the `DESIGN.md` principles relied on), `Alternatives` (text), `Screenshot` (files), `Ticket` (relation to Tickets; the back-link on Tickets is `Design decisions`)
- **Process log:** `Name` (title, the change in one line), `Date` (date of the change), `Status` (Proposed · Trying · Working · Replaced · Reverted), `Area` (select: Planning · Building · Review · Learning · Tooling · Design), `Lives in` (text: the skills, files or memory that hold it)
- **Glossary:** `Term` (title), `Area` (select: Digital audio · Real-time · Synthesis · Music and time · Mixing · Project model · Testing · Stack and Rust), `Definition` (text, one plain sentence), `Related` (relation to other terms, one-way, so set it on both terms), `Introduced in` (relation to RFCs; the back-link on RFCs is `Glossary`)

The Uta page body contains `<database …>` tags. If you ever update that page, keep them exactly as they are, or the databases get removed from the page.

## Page templates

**Research body:** one opening paragraph saying what it's for (which RFC or question) and when it was gathered, then a section per topic. Every claim carries a source link, and anything from a secondary source or a search snippet says so.

**RFC body:** `## Summary` · `## Motivation` · `## Proposal` · `## What it looks like to you` (the result the user sees or hears) · `## Alternatives considered` · `## Risks & unknowns` · `## How we'll verify it` (required) · `## Open questions` · `## New terms` (if any)

**Process log entry body:** `## What wasn't working` (with evidence: Will's words, PR numbers, timings) · `## What changed` · `## How it's going` (dated bullets, added at each check-in). A `Proposed` entry uses `## What isn't working` · `## The idea` · `## Status`.

**Glossary entry body:** it's about the concept, not the code, so no file or function names. A paragraph or two explaining it in plain words for someone who knows web development and DAWs but not audio programming or Rust. Then, where they fit:
- `**Think of it as:**` an analogy from web, React or Canvas work.
- `**Why it matters in Uta:**` the decision it drove, or what goes wrong without it.

Before adding a term, query the Glossary for it so you don't add a duplicate. If it's there but the new work changes its meaning in Uta, update that entry instead.

**Ticket body:**
- `## Goal`
- `## Acceptance criteria` (a checklist, `- [ ]`)
- `## Out of scope`
- `## Depends on` (UTA-n IDs, or "Nothing")
- `## Context` (RFC link and the sections this implements)
- `## Verification notes`: left empty at creation. The implementer fills it in before moving the ticket to In Review, saying what was checked and how.

Branches and PR titles use the ticket ID: branch `uta-12-short-name`, PR title `UTA-12: …`.

## Two tools: `pages` for content, `api` for properties

- `N pages get <id>` returns the page as Markdown with properties as YAML frontmatter. Best way to read a ticket/RFC.
- `N pages create --content '# Title\n\nBody'` creates a page. A leading `# H1` becomes the title and is removed from the body. With no `--parent` it lands at the workspace root.
- `N pages update <id> --content '...'` (or `N --stdin pages update <id> < body.md`) **replaces the whole body**. Read it first, edit, write back. Never blind-overwrite an RFC someone has edited.
- `N pages trash <id> --yes` (`--yes` is required when non-interactive).
- `N api <path> [-X METHOD] -d '<json>'` is the raw API, needed for properties, databases and comments. `N api ls` lists endpoints; `N api <path> -X POST --spec` shows the schema.

## Recipes

**Create a row with properties and body in one call.** `markdown` is a top-level field, and `create` adds the `parent`:
```bash
N tickets create -d '{
  "properties":{"Name":{"title":[{"text":{"content":"Ticket title"}}]},
                "Status":{"status":{"name":"Backlog"}},
                "Project":{"relation":[{"id":"<project page id>"}]}},
  "markdown":"## Goal\n..."}'
```

Markdown code fences with no language become `javascript` code blocks, and leading indentation is trimmed. To fix a diagram, find the block with `N api v1/blocks/<page>/children page_size==100` and PATCH it with `{"code":{"language":"plain text"}}`.

**Update properties** (status, PR link):
```bash
N api v1/pages/<id> -X PATCH -d '{"properties":{"Status":{"status":{"name":"In Review"}},"PR":{"url":"https://github.com/..."}}}'
```

**Query a data source.** Default output is TSV (id, ID, title, props…), and `--json` gives full objects:
```bash
N tickets query --filter '{"property":"Status","status":{"equals":"Ready"}}'
N tickets query --filter '{"property":"ID","unique_id":{"equals":12}}'   # find UTA-12
N tickets query --json --filter '{"property":"Status","status":{"equals":"Ready"}}' | jq -r '.results[].id'
```
`N datasources resolve <database-id>` turns any other database ID into its data source ID.

**Comments:**
```bash
N api v1/comments block_id==<page id>                     # list
N api v1/comments -d '{"parent":{"page_id":"<id>"},"rich_text":[{"text":{"content":"🤖 Claude: ..."}}]}'
```
Comments posted via the CLI show as authored by Will, so **always prefix yours with `🤖 Claude:`** to tell them apart.
- **Inline comments** live on individual blocks. Walk the whole page with `.claude/skills/rfc/comments.py`.
- **Replying** in a thread: `-d '{"discussion_id":"...","rich_text":[...]}'`.
- **Rewriting a block's text deletes the inline threads anchored to it.**

**Create a database** (under a page). Property types that were tested and work: `title`, `status` with custom options (each option's `group` is `To-do` | `In progress` | `Complete`), `unique_id` with a `prefix` (gives IDs like UTA-1), `relation` with `"type":"dual_property"` (creates the back-link on the other database), `url`, `select`, `date`, `people`.
```bash
N api v1/databases -d '{"parent":{"type":"page_id","page_id":"<page>"},
  "title":[{"text":{"content":"Tickets"}}],
  "initial_data_source":{"properties":{"Name":{"title":{}}, "ID":{"unique_id":{"prefix":"UTA"}}}}}'
```
The response's `data_sources[0].id` is the data source ID. Add it to `source_id` in `ntn.sh`, and the database to the table above.

## Rules

- Moving an RFC to **Accepted** is Will's call only. Never do it on your own judgement.
- Show Will a breakdown before creating projects or tickets in bulk.
- Don't write temp files to `/tmp`. Use the session scratchpad.
