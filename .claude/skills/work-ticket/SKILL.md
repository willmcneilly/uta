---
name: work-ticket
description: Pick up a Uta ticket and take it through to a merged PR. Use when Will says /work-ticket, "work on UTA-n", "pick up the next ticket", says to merge a ticket's PR, or reports something he found trying it. Covers claiming the ticket, branching, implementing, verifying against the acceptance criteria, recording verification notes, opening the PR, the review loop with an independent reviewer, handing over to Will with what needs him, and merging on his go-ahead.
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
- `DESIGN.md`, before any UI work: its principles (in Overview), tokens and Do's and Don'ts;
- the code you'll touch.

**If something in the ticket is unclear, or doesn't match reality,** don't quietly reinterpret it. That covers ambiguous acceptance criteria, a criterion that can't be met as written, and a ticket much bigger than it looked. Ask Will, suggest a fix, and wait. A ticket that has to be split or re-scoped follows the plan-change rules in the `plan-project` skill.

## 4. Implement

- Follow `CLAUDE.md` and the RFC's rules. The audio-thread rules especially aren't negotiable.
- Write the tests in the same change as the code. The acceptance criteria are the minimum.
- **Stay in scope.** If you spot something worth doing that isn't in the ticket (a bug, a refactor, a missing piece), don't do it here. List it under follow-ups in the PR, and mention it to Will.
- Commit in logical steps, with clear messages ending in the attribution line from the system reminder.
- **UI work uses design tokens only.** A colour written directly in CSS or TypeScript fails `npm run lint`. Use the CSS variables or `src/design/tokens.ts`, and don't edit `DESIGN.md` or the generated token files to get a value you need.
- **If you're stuck on something outside your control,** such as a broken tool, a missing decision or an upstream bug: set the ticket to `Blocked`, comment why, and tell Will.

### Provisional design decisions

When the UI work needs something `DESIGN.md` doesn't cover (a new component, a layout pattern, a new token, or bending one of its rules), don't block the ticket and don't invent a rule quietly. Build the best version you can from the principles, then:

1. **Log it** in the Design decisions database (see the `notion` skill), with `Status: Open`: what was needed, what you chose and why, the principles you relied on, the alternatives, a screenshot, and the ticket. Its ID is `D-n`.
2. **Mark the code** with a comment where the decision lives: `provisional: D-n`.
3. **List it in the PR** under "Design decisions", with its link.

Will decides each one between projects: adopt it into `DESIGN.md`, revise it in the design sandbox, or reject it. Until then it stays provisional, so don't add it to `DESIGN.md` yourself.

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
  - **Design decisions**, for UI work: each provisional decision's `D-n` and link, or "None";
  - follow-ups;
  - new Glossary terms, if any;
  - the PR attribution line.
- **After opening:** use the `ccd_pr` tools. Call `get_status`, bind the PR if it isn't bound, and read its CI. Don't poll CI yourself. If CI fails, fix it on the branch.
- **Update the ticket:** set the `PR` URL, and set Status to `In Review`.

Then go straight into the review loop. Don't stop to tell Will about the PR yet.

## 8. Review loop

You and an independent reviewer go back and forth until you agree. Will isn't involved until then.

**Start each round by running the `review-round` workflow** (the Workflow tool with `name: "review-round"`), with `args` set to `{"pr": <n>}` on the first round and `{"pr": <n>, "round": <r>}` after that. Those are the only arguments. The workflow writes the reviewer's prompt itself, so you can't add anything about your implementation, and you shouldn't try another way (a subagent, a message, a note in the PR) to brief the reviewer. Each round is a fresh reviewer with no memory of earlier rounds. It reads the earlier reviews from GitHub.

**After each round:**
- **Ready to merge:** the loop is done. Go to step 9.
- **Changes needed:** read the review on GitHub (its link is in the result), not only the summary. For each finding:
  - fix it on the branch; or
  - if you think it's wrong, reply on the PR explaining why, with evidence. The next reviewer decides. **Never dismiss a blocking finding yourself.**

  Fix should-fix findings too, unless they're clearly bigger than the ticket; then reply that they belong in follow-ups. Re-run the step 5 checks, reply to each comment with what you changed, push, update the verification notes if the evidence changed, and start the next round.

**Stop after 3 rounds** if you still don't agree, and take the disagreement to Will in step 9: each open finding, the reviewer's position and yours, both in a sentence or two, with links.

If the workflow fails or the reviewer doesn't finish, run the round again once. If it fails again, tell Will, and set the ticket to `Blocked` only if nothing else can move it.

## 9. Hand over to Will

Send one message. It's the first Will hears of this ticket since you started, so make it complete and short:

1. **The PR link,** and a two-line summary of what changed.
2. **The review:** the final verdict, how many rounds it took, and a link to the last review. If a finding was resolved by accepting your disagreement, say so in a line.
3. **What needs you.** Take this from the last reviewer's `for_will` list and its `golden_changed` flag, not from your own view, and keep its ratings:
   - **High** checks: the steps, and what he should hear or see.
   - **Low** checks: one line each, with why they're low, marked "skip unless curious".
   - **Decisions:** golden WAV changes to approve, new provisional design decisions (`D-n`, decided between projects, so just list them), should-fix items the reviewer was happy to leave as follow-ups, and any disagreement left after 3 rounds.
   - **If nothing is High and there are no decisions,** say exactly that: "Nothing here needs you. Say merge when you're ready."
4. **Checkpoint:** if this ticket is one of the plan's checkpoints, say what Will can now try.

Then stop and wait for Will.

## 10. When Will replies

**"Merge":** check that the last review is Ready to merge and CI is green (`gh pr checks <n>`). If they aren't, tell Will what's wrong instead of merging. Then:
1. `gh pr merge <n> --squash --delete-branch`, so there's one commit per ticket on `main`, titled `UTA-<n>: …`.
2. Set the ticket to `Done`.
3. **Unblock:** for each `Backlog` ticket in the project whose **Depends on** tickets are now all `Done`, set it to `Ready`. If every ticket in the project is `Done`, set the project to `Done`.
4. Remove your worktree if you used one.
5. Tell Will what merged, which tickets are now Ready, and what he can try if this was a checkpoint.

**Something he found by hand:** fix it on the branch, re-run the step 5 checks and push. Then run one more review round, so the reviewer sees the change before Will does, and hand over again as in step 9.

The ticket stays `In Review` until it's merged.
