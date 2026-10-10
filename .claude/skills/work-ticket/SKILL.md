---
name: work-ticket
description: Pick up a Uta ticket and take it through to a merged PR. Use when Will says /work-ticket, "work on UTA-n", "pick up the next ticket", says to merge a ticket's PR, or reports something he found trying it. Covers claiming the ticket, branching, implementing, verifying against the acceptance criteria, recording verification notes, opening the PR, the review loop with an independent reviewer, handing over to Will with what needs him, and merging on his go-ahead.
effort: medium
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

1. **Log it** in the Design decisions database (see the `notion` skill), with `Status: Open`: what was needed, what you chose and why, the principles you relied on, the alternatives, and the ticket. Its ID is `D-n`. Leave `Screenshot` empty: the PR body has the screenshots, and that's enough.
2. **Mark the code** with a comment where the decision lives: `provisional: D-n`.
3. **List it in the PR** under "Design decisions", with its link and a screenshot.

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
- **To change the PR's description later,** use `gh api repos/willmcneilly/uta/pulls/<n> -X PATCH -F body=@<file>`. `gh pr edit` fails on this repo, because it asks GitHub for the retired classic Projects field.
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

Send one message. It's the first Will hears of this ticket since you started. Lead with what needs him, and keep the rest short: the detail is in the PR, so link to it rather than repeating it.

1. **What needs you,** first. Take it from the last reviewer's `for_will` list and its `golden_changed` flag, not from your own view, and keep its ratings:
   - **High** checks: the steps, and what he should hear or see.
   - **Low** checks: one line each, with why they're low, marked "skip unless curious".
   - **Decisions:** golden WAV changes to approve, new provisional design decisions (`D-n`, decided between projects, so just list them with a line each), should-fix items the reviewer was happy to leave as follow-ups, and any disagreement left after 3 rounds.
   - **If nothing is High and there are no decisions,** say exactly that: "Nothing here needs you. Say merge when you're ready."
2. **The PR,** with a summary of what changed in two or three lines.
3. **The review,** in one line: the verdict, how many rounds, and a link to the last review. Add a line for any finding resolved by accepting your disagreement.
4. **Checkpoint:** if this ticket is one of the plan's checkpoints, say what Will can now try.

Leave out benchmark tables, implementation detail and housekeeping.

**Have everything ready before you send it.** Will should be able to start his checks the moment he reads the message:
- **If any check uses the app,** start it from the PR's branch: `npm run tauri dev` in `app/` of the worktree you built in, run in the background. Wait until it has built and the window is open, and check the log for errors. The log is coloured, so strip the colour codes before matching, or the wait never ends: `sed 's/\x1b\[[0-9;]*m//g' <log> | grep -m1 'Running .*uta-app'`. Give it a timeout of a few minutes, and if it times out, read the log rather than waiting longer. If another copy of the app is already running from a different checkout, stop it first, so Will doesn't test the wrong build. Say "The app is open, running this branch" at the top of "What needs you".
- **If a check starts from a particular state** (a song from the Develop menu, a buffer size), say exactly how to get there in its first step.
- **If a check needs a terminal command** (`uta play`, say), give it ready to run in its own `bash` block.

Leave it all running until Will merges or reports back.

Then stop and wait for Will.

## 10. When Will replies

**"Merge":** check that the last review is Ready to merge and CI is green (`gh pr checks <n>`). If they aren't, tell Will what's wrong instead of merging. Then:
1. `gh pr merge <n> --squash`, so there's one commit per ticket on `main`, titled `UTA-<n>: …`. Then delete the remote branch with `git push origin --delete <branch>`. Don't use `--delete-branch`: from a worktree it fails, because it tries to switch to `main`, which is checked out in Will's main checkout.
2. Set the ticket to `Done`.
3. **Unblock:** for each `Backlog` ticket in the project whose **Depends on** tickets are now all `Done`, set it to `Ready`. If every ticket in the project is `Done`, set the project to `Done`.
4. **Clean up after yourself, without asking:**
   - stop any dev servers and previews you started, and close the browser tabs you opened, in the built-in browser or elsewhere;
   - remove any worktree you created, and its build output. If this session itself runs in a worktree the app made, leave that one: the app removes it;
   - delete scratch branches you pushed, except a branch that holds images the PR links to, which stays so the PR keeps working;
   - leave anything you didn't create alone;
   - then run the `tidy` skill, which clears what earlier tickets left behind: worktrees and build output whose work is on `main`, and merged branches.
5. **Line up the next ticket.** Query the Tickets for `Ready` ones whose **Depends on** are all `Done` and merged. Pick the next one the plan's order says to do in the same project, or the lowest ID if the plan doesn't say. Then create a chip for it with the `spawn_task` tool (the `ccd_session` one): title `Work on UTA-<n>: <name>`, a one-line tldr of its goal, and the prompt `/work-ticket UTA-<n>`, so Will only has to click it. If that tool isn't available, give him the command to paste instead. If nothing is Ready, say what the next ticket is waiting on.
6. Tell Will, in a few lines: what merged, which tickets are now Ready, the next ticket you lined up (and any other Ready tickets that could run alongside it), what he can try if this was a checkpoint, and one line on what you cleaned up. Only ask about something you couldn't clean up safely.

**Something he found by hand:** fix it on the branch, re-run the step 5 checks and push. Will checks these fixes himself in the app, so there's usually no need for another review round. CI must still be green before merging. Run one more round only if the fix:
- touches the engine, the audio thread or `uta-core`; or
- isn't small, for example a new behaviour or a change across several files.

Then tell Will what you changed, briefly, keeping the app open on the new code. If you ran a review round, include its verdict.

The ticket stays `In Review` until it's merged.
