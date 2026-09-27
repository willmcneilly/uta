---
name: notion
description: How to read and write the Uta project's Notion (RFCs, Projects, Tickets) with the ntn CLI. Use whenever you need to create or update an RFC, project or ticket, change a ticket's status, add a PR link, read or post comments, or query what work is ready.
---

# Notion via `ntn`

## Always use the wrapper

```bash
.claude/skills/notion/ntn.sh <args>
```

It pins `NOTION_WORKSPACE_ID` to Will's **personal** workspace ("Will"). Bare `ntn` defaults to the **Stora** work workspace. Never use it for this project. Check with `.claude/skills/notion/ntn.sh whoami` (the workspace should be "Will", not Stora).

Below, `N` means `.claude/skills/notion/ntn.sh`.

**Always redirect stdin: `N ... </dev/null`.** Run from a non-interactive shell, `ntn` waits on stdin and hangs forever, even with `-d`. For large bodies, build the JSON in the scratchpad with python and pass it as `-d "$(cat file.json)"`.

## Uta IDs

Uta page: `3e83af96-9b6f-8190-b519-dc8cf3aeab6f` (https://app.notion.com/p/Uta-3e83af969b6f8190b519dc8cf3aeab6f)

| Database | Database ID | Data source ID (use for rows and queries) |
|---|---|---|
| RFCs | `5c6fe131-a98d-4ea9-8119-44e8e10af9c6` | `ab1b763c-b9f2-48c5-ade6-a1ba218330f0` |
| Projects | `f11524a3-50c2-4951-895d-ac946a5dbcd2` | `e0dc7875-8865-4bbe-bcff-785efb0cda42` |
| Tickets | `ad66cdaf-cd4a-4914-b236-2cbc2f66291b` | `444117ac-b9f1-4686-b828-1919e9ba098f` |

Will's Notion user ID (for `people` properties such as RFC Author): `eafd9c84-49ac-412c-91f2-650a1edfa069`

**Properties** (names are exact):
- **RFCs:** `Name` (title), `Number` (auto, RFC-n), `Status` (Draft · In Discussion · Accepted · Rejected · Superseded), `Author` (people), `Date accepted` (date), `Projects` (relation)
- **Projects:** `Name`, `Status` (Planned · Active · Done), `RFCs` (relation), `Tickets` (relation), `Progress` (rollup, read-only)
- **Tickets:** `Name`, `ID` (auto, UTA-n), `Status` (Backlog · Ready · In Progress · Blocked · In Review · Done), `Type` (Feature · Bug · Chore · Spike), `Project` (relation), `PR` (url)

The Uta page body contains `<database …>` tags. If you ever update that page, keep them exactly as they are, or the databases get removed from the page.

## Page templates

**RFC body:** `## Summary` · `## Motivation` · `## Proposal` · `## What it looks like to you` (the result the user sees or hears) · `## Alternatives considered` · `## Risks & unknowns` · `## How we'll verify it` (required) · `## Open questions`

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
- `N pages update <id> --content '...'` (or via stdin) **replaces the whole body**. Read it first, edit, write back. Never blind-overwrite an RFC someone has edited.
- `N pages trash <id> --yes` (`--yes` is required when non-interactive).
- `N api <path> [-X METHOD] -d '<json>'` is the raw API, needed for properties, databases and comments. `N api ls` lists endpoints; `N api <path> -X POST --spec` shows the schema.

## Recipes

**Create a row with properties and body in one call.** `markdown` is a top-level field:
```bash
N api v1/pages -d '{"parent":{"data_source_id":"<DS>"},
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
N datasources query <DS> --filter '{"property":"Status","status":{"equals":"Ready"}}'
N datasources query <DS> --filter '{"property":"ID","unique_id":{"equals":12}}'   # find UTA-12
```
`N datasources resolve <database-id>` turns a database ID into its data source ID.

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
The response's `data_sources[0].id` is the data source ID. Record it above.

## Rules

- Moving an RFC to **Accepted** is Will's call only. Never do it on your own judgement.
- Show Will a breakdown before creating projects or tickets in bulk.
- Don't write temp files to `/tmp`. Use the session scratchpad.
