---
name: review-pr
description: Review a Uta ticket's PR as an independent reviewer, then merge it on Will's go-ahead. Use when Will says /review-pr, "review UTA-n", "review PR #n", or asks to merge a reviewed PR. Covers checking scope, independently verifying each acceptance criterion, the real-time audio rules, test quality, posting the review on GitHub, and merging and updating Notion.
---

# Reviewing a PR

The reviewer is how Will trusts work they won't read line by line. Your job isn't to confirm the implementer's story. It's to check it independently. Assume nothing in the PR description is true until you've seen the evidence yourself.

Use the `notion` skill for Notion mechanics.

## 0. Be independent

The review must not come from the agent that wrote the code.
- **In a fresh session:** carry on with this skill.
- **In the session that implemented the ticket:** don't review it yourself. Hand the whole review (steps 1–4) to a `general-purpose` subagent. Give it the PR number and tell it to follow this skill, but give it none of your implementation reasoning. Relay its verdict to Will.

## 1. Gather

- **Find the PR:** from the number, or from the ticket's `PR` property (`gh pr view <n>`).
- **Read:**
  - the ticket (`N pages get`): the goal, acceptance criteria, out of scope and verification notes;
  - the RFC sections in its Context;
  - `CLAUDE.md`;
  - the full diff (`gh pr diff <n>`).
- **Check out the branch in a separate worktree** so you don't disturb anyone's working copy: `git worktree add ../uta-review-<n> <branch>`. Remove it when you're done.
- For a large diff, delegate reading parts of it to subagents and keep only their findings.

## 2. Check

Work through these in order. Each finding gets a severity:
- **Blocking:** wrong, unsafe, breaks a rule, or a criterion isn't actually met.
- **Should fix:** a real problem, but it could reasonably be a follow-up.
- **Nit:** style or taste. Keep these few.

1. **Scope.**
   - Does the diff do what the ticket says, and only that?
   - Is anything missing?
   - Did out-of-scope work slip in? That's blocking unless it's trivial.
2. **Acceptance criteria, verified independently.** For each criterion:
   - find the evidence yourself;
   - run the tests and commands;
   - confirm the test actually tests the claim. A pitch test must fail on the wrong pitch, and a no-click test must fail on a click.

   **For the key claims, break the code on purpose:** change the frequency, remove a fade, add an allocation in the process path. Confirm a test or check fails, then revert. A criterion whose test can't fail isn't met.
3. **Checks.** Run everything CI runs, locally (the commands are in `CLAUDE.md`). Check the PR's CI status as well, with `gh pr checks <n>`.
4. **Real-time rules**, on any code the audio thread can reach, including callbacks cpal calls. Look for:
   - allocating or freeing, including hidden cases: `Vec` growth, `String`, `Box`/`Arc` drops, `format!`, collecting iterators;
   - locks and blocking channels;
   - I/O and logging;
   - loops with no fixed bound;
   - anything that could panic.

   Check that the real-time tests actually drive the code path. Any violation is **blocking**.
5. **Fit with the RFC and the architecture.** For example:
   - project state is changed only through commands in the core;
   - the UI holds no project state;
   - snapshots are swapped, not mutated in place.

   A departure is blocking unless the PR explains it and Will has agreed. Then it becomes an RFC amendment.
6. **Design**, on any UI change. Read `DESIGN.md` first.
   - It uses tokens only. `npm run lint` catches colours; check sizes, spacing and type by eye, since nothing checks them yet.
   - Anything `DESIGN.md` doesn't cover is a provisional decision: marked `provisional: D-n` in the code, logged in the Design decisions database (see the `notion` skill) with every field filled in, and listed in the PR. Something new that isn't logged is **should fix**.
   - An escape comment that lets a colour through (`eslint-disable` or `stylelint-disable`) needs a reason that holds up. One without is **blocking**.
7. **Test quality.**
   - Do tests check behaviour rather than implementation details?
   - Would they catch a regression?
   - Are they deterministic, with no timing flakiness?
8. **Code quality.**
   - Readable to the next agent?
   - Consistent with the surrounding code?
   - No dead code, no debug leftovers, and no clear simplification missed?
9. **Honesty of the record.**
   - Do the verification notes and the PR's evidence match what you found?
   - Are the manual checks for Will specific enough to act on?

   An overstated claim is blocking until it's corrected.

## 3. Post the review on GitHub

The `gh` account is Will's, and GitHub won't let an author approve or request changes on their own PR. So post a **comment review**, and put the verdict in the body. Start every comment with `🤖 Claude (review):`.

- Put findings on lines in one review:
  ```bash
  gh api repos/willmcneilly/uta/pulls/<n>/reviews -X POST --input review.json
  ```
  In `review.json`: `{"event":"COMMENT","body":"...","comments":[{"path":"...","line":N,"side":"RIGHT","body":"..."}]}`. Write it in the scratchpad, not `/tmp`.
- **The body contains:**
  - the verdict: **Ready to merge** or **Changes needed**;
  - the blocking findings, then should-fix, then nits;
  - for each acceptance criterion, whether you verified it and how;
  - what's still left for Will to check manually.
- **Don't fix the code yourself.** The implementer addresses findings (see the `work-ticket` skill). That keeps the roles separate.

## 4. Report

- Comment on the Notion ticket: `🤖 Claude (review): <verdict>, <n> blocking. <PR link>`. The status stays `In Review`.
- Tell Will:
  - the verdict;
  - the blocking findings in plain language;
  - what they should try by hand before merging, if anything.

**Re-reviews:** after the implementer pushes fixes, check each earlier finding is resolved, review the new diff, and post a new review with the updated verdict.

## 5. Merge (only on Will's go-ahead)

Merge only when your verdict is **Ready to merge**, CI is green, and Will has said to merge. If the PR has manual checks for Will, ask whether they've done them first.

1. `gh pr merge <n> --squash --delete-branch`, so there's one commit per ticket on `main`, titled `UTA-<n>: …`.
2. Set the ticket to `Done`.
3. **Unblock:**
   - For each `Backlog` ticket in the project whose **Depends on** tickets are now all `Done`, set it to `Ready`.
   - If every ticket in the project is `Done`, set the project to `Done`.
4. Remove your review worktree.
5. Tell Will:
   - what merged;
   - which tickets are now Ready;
   - whether this was one of the plan's checkpoints, and what they can try.
