---
name: scratchpad
description: Capture and review Will's quick notes about things in Uta worth changing later. Use when Will says /note, "note this", "jot that down", "park that", "add to the scratchpad", or makes an aside about something to change that is outside the current work. Also use for the scratchpad review that starts the rfc and plan-project skills, or when Will says "review the scratchpad" or "what's in the scratchpad".
---

# Scratchpad

Will notices things to change while other work is going on. The scratchpad lets them note an idea in a line and keep going. Ideas wait there until there's a gap between projects, when they're reviewed.

It isn't a backlog. Notes never become Backlog tickets directly. They get promoted into an RFC or a project, folded into work that's about to start, or dropped. A PR's own loose ends ("this PR left X undone") stay in the PR's follow-ups, not here.

For Notion mechanics, use the `notion` skill. Below, `N` means `.claude/skills/notion/ntn.sh`, and every call needs `</dev/null`.

## The database

Scratchpad: database `057ed2bc-7159-4f9d-ba6d-b034fed1eacc`, data source `847d5e5a-0916-4048-b2a0-38722bb92e22` (https://app.notion.com/p/057ed2bc71594f9dba6db034fed1eacc)

Properties (names are exact):
- `Name` (title): the note, in one line
- `Area` (select): UI · Engine · Workflow · Tooling · Other
- `Status` (status): Raw · Promoted · Folded in · Dropped
- `Noticed during` (text): the ticket or context, e.g. `UTA-21`, or `using the app`
- `Date` (created time, read-only)
- `Projects` and `RFCs` (relations): what the note became. The back-link on both is `Scratchpad notes`.

## Capture

Make it cheap and don't break the flow of the current work.

1. Write `Name` as one plain line that sums up the observation, so the list is easy to scan at review. Don't expand it into a proposal.
2. Unless `Name` is exactly what Will wrote, put Will's note **verbatim** in the page body (the top-level `markdown` field), as a quote: `> …`. Don't tidy it. The original wording often carries the hunch the summary loses. If several notes came in one message, quote only the part for each row.
3. Pick `Area` yourself. Don't ask.
4. Fill in `Noticed during` from the current branch (`uta-21-…` → `UTA-21`), or from what's going on if it isn't a ticket branch.
5. Always set `Status` to `Raw`.

```bash
N api v1/pages -d '{"parent":{"data_source_id":"847d5e5a-0916-4048-b2a0-38722bb92e22"},
  "properties":{"Name":{"title":[{"text":{"content":"Mute button on track headers is too small to hit"}}]},
                "Area":{"select":{"name":"UI"}},
                "Status":{"status":{"name":"Raw"}},
                "Noticed during":{"rich_text":[{"text":{"content":"UTA-21"}}]}},
  "markdown":"> the mute buttons on the track headers are tiny, I keep missing them and hitting solo"}' </dev/null
```

Confirm in one line ("Noted: …") and go back to what you were doing. Don't discuss the idea, design it, or act on it, unless Will asks. If one message has several notes, make a row for each.

## Review

This runs between projects: at the start of the `rfc` and `plan-project` skills, or when Will asks for it.

1. Query the Raw notes:
   ```bash
   N datasources query 847d5e5a-0916-4048-b2a0-38722bb92e22 --filter '{"property":"Status","status":{"equals":"Raw"}}' </dev/null
   ```
   If there are none, say so in a line and carry on.
2. Group them by theme, not just by `Area`. Notes about the same thing go together, and duplicates are merged into one.
3. Propose an outcome for each group, with a one-line reason:
   - **Promote:** big enough for its own RFC or project. Say which.
   - **Fold in:** belongs in the RFC or project about to start (or one in progress). Say where.
   - **Drop:** already done, out of date, or not worth it.
   - **Keep:** leave it Raw for a later review.
   When the review was started by `rfc` or `plan-project`, start with the notes that touch the work being scoped.
4. Wait for Will's decisions. They are Will's call; don't change any statuses before then.
5. Apply them: set `Status`, and add the `Projects` or `RFCs` relation for Promoted and Folded-in notes once that page exists. Notes folded into an RFC or plan you're drafting get their relation when it's pushed to Notion. Until then, list them in the draft's open questions, or in the plan's notes, so they aren't lost.
   ```bash
   N api v1/pages/<id> -X PATCH -d '{"properties":{"Status":{"status":{"name":"Folded in"}},"Projects":{"relation":[{"id":"<project page id>"}]}}}' </dev/null
   ```
