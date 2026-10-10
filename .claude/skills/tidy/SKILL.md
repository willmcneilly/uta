---
name: tidy
description: Tidy Uta's worktrees, build output and local branches to free disk space, without touching anything still in use. Use when Will says /tidy, "tidy up", "clean up the worktrees", "free some space", or asks whether there are build artifacts taking up space. Also run by the work-ticket skill after a merge.
---

# Tidying worktrees, build output and branches

Every ticket runs in its own worktree, and each one builds its own Rust `target/` of about 5.5 GB. Sessions end, PRs merge with `--squash`, and the worktrees, build output and branches stay behind. This finds what's safe to remove, removes it, and lists anything it isn't sure of rather than guessing.

The rule throughout: **remove something only when its work is on `main` or in a merged PR, and nothing is using it.** If you can't show both, leave it and list it.

## 1. Take stock

From the main checkout (`/Users/willmcneilly/Sites/uta`), after `git fetch --prune`:
- `git worktree list --porcelain`, and `du -sh` each worktree, its `target/`, `app/node_modules` and `app/dist`.
- The sessions, with `list_sessions` (the `ccd_session_mgmt` tool), to match each worktree to its session by `cwd`, and whether that session is running.
- Free disk space (`df -h .`), to report before and after.

## 2. Worktrees

For each worktree in `.claude/worktrees/`:

| State | What to do |
|---|---|
| Its session is running | Leave it, including its build output. Report its size. |
| It's this session's own | Remove its `target/` and `app/dist`. The app removes the worktree when the session is archived. |
| Its session is idle but not archived | Remove its `target/` and `app/dist` if its work is on `main`. Don't remove the worktree; suggest Will archives the session, which does it properly. |
| No unarchived session uses it | Remove the worktree if its work is on `main` (below). Otherwise list it. |

**Its work is on `main`** when `git status` is clean, and one of these holds:
- its tip is an ancestor of `origin/main`;
- it's the head of a merged PR and its tip equals the PR's `headRefOid` (`gh pr list --state merged --json headRefName,headRefOid`);
- `git diff <tip> <squash commit>` is empty, for a detached HEAD left behind by a squash merge.

**Before removing a worktree, check its ignored files:** `git status --short --ignored`, leaving out `target/`, `app/node_modules/`, `app/dist/` and `app/src-tauri/gen/`. Anything else (a screenshot harness, notes, images) gets copied to the session scratchpad and mentioned to Will.

Remove with `git worktree remove --force <path>` (forced only because ignored build output is present), then `git worktree prune`.

## 3. Branches

Delete a local branch when it isn't checked out in any worktree and its work is on `main`, by the same tests as above. Squash merges mean `git branch -d` refuses most of them, so use `git branch -D` once a test has passed.

Always keep:
- `main`;
- `assets/*`, which hold the images PRs link to;
- `spike/*`, which are kept on purpose, unmerged.

List any other branch with commits you can't find on `main` or in a merged PR, with its last commit and age, and leave it for Will. Never delete a remote branch without asking.

## 4. Strays

Files sitting directly in `.claude/worktrees/` (backups, scratch files) aren't worktrees. Remove one if it's identical to a file on `main`; otherwise list it.

Leave the main checkout's `target/` alone unless Will asks: it's the build cache for running the app from `main`.

## 5. Permissions

Run each removal as its own simple command (`git worktree remove --force <path>`, `rm -rf <worktree>/target`, `git branch -D <branch>`), not chained in one script, so it can match an allow rule in `.claude/settings.json` (`git worktree remove *` is one). If auto mode still blocks one, don't look for another way to do it. Stop and give Will the remaining commands, each in its own `bash` block.

## 6. Report

In a few lines:
- the space freed (free before and after);
- what was removed: worktrees, build output, the number of branches;
- what was left, and why: running sessions with their sizes, idle sessions worth archiving (as `[title](#<sessionId>)` links), and branches or files you weren't sure about;
- anything copied to the scratchpad.

Only ask about the things you weren't sure of.
