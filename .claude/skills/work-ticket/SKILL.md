---
name: work-ticket
description: Pick up a Uta ticket and take it through to a PR. Use when Will says /work-ticket, "work on UTA-n", "pick up the next ticket", or asks for review feedback on a ticket's PR to be addressed. Covers claiming the ticket, branching, implementing, verifying against the acceptance criteria, recording verification notes, opening the PR and moving the ticket to In Review.
---

# Working a ticket

A ticket is done when a reviewer and Will can trust it without reading every line, because the evidence is in the ticket and the PR. The job isn't just writing the code: it's showing, criterion by criterion, that it works.

Use the `notion` skill for Notion mechanics. Keep your own context lean: send broad exploration, library research and large log reading to subagents, and keep the conclusions.

## 1. Pick the ticket

- **If Will named one** (`/work-ticket UTA-3`), use it.
- **If not,** query Tickets for `Status: Ready` and take the lowest ID. Tell Will which one you picked.
- **Check before starting:**
  - The ticket's status is `Ready`.
  - Every ticket in its **Depends on** is `Done`, and its PR is merged into `main`.
- **If a check fails, stop and tell Will.** Don't start a Backlog ticket, and don't build on an unmerged branch.

## 2. Claim it

1. Set the ticket to `In Progress`. If its project is `Planned`, set the project to `Active`.
2. Update `main` (`git fetch && git switch main && git pull --ff-only`).
3. Create a branch named `uta-<n>-<short-slug>`, e.g. `uta-2-engine-core`.
   - If another ticket is in progress in this checkout, or it has uncommitted work that isn't yours, use a git worktree instead of switching branches.
4. Post a ticket comment: `🤖 Claude: Started on branch uta-2-engine-core.`

## 3. Understand before building

Read:
- the ticket (`N pages get <id>`);
- the RFC sections in its **Context**;
- the project plan in `docs/plans/`;
- `CLAUDE.md`;
- the code you'll touch.

**If something in the ticket is unclear, or doesn't match reality,** don't quietly reinterpret it. That covers ambiguous acceptance criteria, a criterion that can't be met as written, and a ticket much bigger than it looked. Ask Will, suggest a fix, and wait. A ticket that has to be split or re-scoped follows the plan-change rules in the `plan-project` skill.

## 4. Implement

- Follow `CLAUDE.md` and the RFC's rules. The audio-thread rules especially aren't negotiable.
- Write the tests in the same change as the code. The acceptance criteria are the minimum.
- **Stay in scope.** If you spot something worth doing that isn't in the ticket (a bug, a refactor, a missing piece), don't do it here. List it under follow-ups in the PR, and mention it to Will.
- Commit in logical steps, with clear messages ending in the attribution line from the system reminder.
- **If you're stuck on something outside your control,** such as a broken tool, a missing decision or an upstream bug: set the ticket to `Blocked`, comment why, and tell Will.

## 5. Verify

Before opening the PR:

1. **Run every check CI runs, locally,** and fix everything they report: fmt, clippy with warnings as errors, all tests, the frontend checks and the app build. The exact commands are in `CLAUDE.md`. Never open a PR you know is red.
2. **Go through the acceptance criteria one by one.** For each, record the evidence: the test that proves it, the command you ran and what it showed, or a screenshot for UI.
3. **Manual checks you can't do yourself.** You can't hear audio or unplug an interface. Do what you can (run it, read the dropout count, check the logs), then list exactly what Will should try and what they should hear or see. **Never claim you verified something you didn't.**
4. **Self-review the diff:**
   - remove debug leftovers;
   - check nothing out of scope slipped in;
   - check that names and comments match the surrounding code.

5. **New concepts.** Did this work bring in a concept that isn't in the Glossary, something you had to reach for that the RFC didn't name? Usually the answer is no. If yes, add an entry (see the `notion` skill), with `Introduced in` set to the ticket's RFC, and list the new terms in the PR.

## 6. Record it on the ticket

- **Tick the checkboxes** of the criteria you met. PATCH each `to_do` block with `{"to_do":{"checked":true}}`, which doesn't touch the text or its comments. Leave unmet ones unticked, and explain why in the notes.
- **Fill in Verification notes.** It's the last section, so delete the placeholder paragraph and append bullets to the page (`PATCH v1/blocks/<page id>/children` with `children`). Give:
  - for each criterion, how it was verified;
  - what's left for Will to check manually;
  - any follow-ups.

## 7. Open the PR

- Push the branch, then `gh pr create`.
- **Title:** `UTA-<n>: <ticket name>`.
- **Body:**
  - one paragraph on what changed and why, plus the Notion ticket link;
  - the acceptance criteria as a checklist with evidence;
  - **For Will to try:** the manual checks, as steps with expected results;
  - follow-ups;
  - new Glossary terms, if any;
  - the PR attribution line.
- **After opening:** use the `ccd_pr` tools. Call `get_status`, bind the PR if it isn't bound, and read its CI. Don't poll CI yourself. If CI fails, fix it on the branch.
- **Update the ticket:** set the `PR` URL, and set Status to `In Review`.
- **Tell Will:**
  - the PR link;
  - a two-line summary;
  - what they can try now, especially if this ticket is one of the plan's checkpoints.

**Don't merge.** Merging happens after the review and Will's go-ahead.

## Addressing review feedback

When the reviewer or Will leaves comments on the PR:
- Fix them on the same branch, and re-run the step 5 checks.
- Reply to each comment with what you changed, or why you didn't.
- Update the verification notes if the evidence changed.

The ticket stays `In Review`.
