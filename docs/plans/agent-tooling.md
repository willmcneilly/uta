# Project: Agent tooling

**Status:** Created · **RFC:** none (from the session audit of 8–11 Oct, below) · **Notion:** [Agent tooling](https://app.notion.com/p/Agent-tooling-3f63af969b6f81ad90b2eab313851dcc)

## Goal

Agents stop working out the same mechanical steps by hand on every ticket. The steps that repeat (starting the app, running the checks, setting up a review, the ticket's Notion bookkeeping) become scripts in the repo that the skills call. Fewer hand-built steps means fewer turns, fewer tokens, fewer usage-limit stalls, and fewer review rounds spent on stale text.

## Where this comes from

An audit of every agent session from 8 to 11 October: 50 main sessions (about 30 tickets, plus planning and RFC work) and 48 `pr-reviewer` runs, about 5,400 shell commands in all.

Most of a session's time goes on the agent's own turns, not on commands. Each step it works out by hand is another round trip that re-reads its whole context, about 150k tokens, so a ticket session reads 90–150M tokens. UTA-50 and UTA-55 each sat about 50 minutes on the usage limit. The tickets below take the steps that came up again and again and make them deterministic.

The 9–10 Oct process audit already fixed the review relay, the handover order and the effort levels, and wrote down workarounds for `gh pr edit`, `--delete-branch` and the app's coloured log. Those workarounds are prose, so agents still type them out each time. Several tickets here turn them into scripts.

## Conventions for every ticket in this project

- **Where scripts live:** `scripts/` at the repository root, each with a `--help` that says what it does. Logic that needs tests is TypeScript run by Node, like `app/scripts/`. Thin wrappers can be shell.
- **The skills call the scripts.** Each ticket updates the skills and agent docs that describe the steps it replaces, so agents stop hand-building them. A script nobody's told about doesn't help.
- **Process log:** each ticket adds an entry (Status `Trying`) with its "Why" as the evidence, as CLAUDE.md asks when the way we work changes.
- **Skill changes reach new sessions only.** Sessions already running keep the old text. That's fine, but don't merge mid-ticket in a way that leaves a running session's instructions pointing at something removed.

## Tickets

### 1. Stop the tool quirks that trip every session (Chore) · UTA-58

**Goal:** the small, known failures that cost nearly every session a few wasted turns stop happening, because the docs and the Notion wrapper prevent them.

**Why.** Each one is small, but they repeat in almost every session:
- **The wrong Notion skill.** The project's skill is called `notion`, the same name as Will's global skill (Stora by default). Ticket agents loaded the global one in about 20 sessions (9 of 14 in the first batch of tickets, all 7 drum-sound tickets, 4 planning sessions), then `cat`-ed the project's `SKILL.md` anyway. The project skill was read by hand 42 times in planning sessions and 54 times by reviewers, mostly to find a data source ID.
- **`gh pr view <n>` fails on this repo** because of the retired classic Projects field, and so does `--comments`. `pr-reviewer.md` tells reviewers to run both, so 46 of 48 reviewer runs opened with an error and retried with `--json`. 52 failures in all.
- **zsh quirks:**
  - an unquoted `--include=*.rs` fails with "no matches found" (about 15 failed calls across 9 sessions);
  - `echo ====` fails, because zsh expands a leading `=` (about 17 failures);
  - `$c:refs/heads/…` is read as the `:r` modifier, so pushes failed in 5 sessions (UTA-37, 38, 42, 45, 46).
- **`sleep N; cat …` is blocked** by the harness: at least 11 times. Agents then load Monitor or rewrite the wait.
- **Prettier:** there's no Prettier in the repo, but 5 sessions (UTA-38 to 42) ran `npx prettier --check` and investigated the output.
- **Python:** there's no numpy or PIL, and the stdlib `wave` module can't read Uta's float WAVs. That was rediscovered in UTA-25, 37, 39, 41, 47, 48, 51 and 56.
- **`ntn.sh` misuse:**
  - `json.load` on the default TSV output (4 times);
  - `--sorts` for `--sort` (twice, the same mistake in two planning sessions);
  - forgetting `</dev/null`, which makes `ntn` hang. The skill says it must be on every call, and agents copy it by hand.

**What to build:**
- Rename the project skill to `uta-notion` (`.claude/skills/uta-notion/`). Update every reference: CLAUDE.md, the other skills, `pr-reviewer.md`, and the `.claude/settings.json` allow rule for `ntn.sh`.
- In `ntn.sh`:
  - redirect stdin from `/dev/null` by default, with an opt-in for the commands that read a body from stdin;
  - add named data sources, so `ntn.sh tickets query …` works without looking up an ID (tickets, projects, rfcs, research, scratchpad, glossary, decisions, process-log).
- `pr-reviewer.md`, and anywhere else that says `gh pr view <n>` or `--comments`: use the `--json` and `gh api` forms that work. (UTA-61 replaces the reviewer's gathering with a script; this is the one-line fix until then.)
- A short **Shell and tools** section in CLAUDE.md:
  - the shell is zsh, so quote globs or use `rg`, don't start an `echo` with `=`, and brace variables before a colon (`${c}:refs/…`);
  - never `sleep` to wait: run it in the background and wait for the notification, or use Monitor;
  - there's no Prettier; `npm run lint` is the only formatting check for the frontend;
  - Python has only the standard library, and `wave` can't read float WAVs.

**Acceptance criteria:**
- [ ] The skill is `uta-notion`, and `rg -n "skills/notion"` finds no stale references outside git history.
- [ ] `ntn.sh tickets query --filter …` works, as does each named data source, and a query with no stdin redirect doesn't hang.
- [ ] No skill or agent doc tells an agent to run plain `gh pr view <n>` or `gh pr view <n> --comments`.
- [ ] CLAUDE.md has the Shell and tools section, under about 10 lines.
- [ ] Every skill recipe that used a raw data source ID or `</dev/null` is updated to the new wrapper form, and still works (run each changed recipe once, read-only).
- [ ] Process log entry added.

**Out of scope:** the reviewer's gathering script (UTA-61). Ticket scripts built on the wrapper (UTA-62). A WAV measuring tool (not in this project).

**Depends on:** Nothing.

**Context:** the audit above. `.claude/skills/notion/`, `.claude/agents/pr-reviewer.md`, CLAUDE.md.

### 2. One check script, with warm builds in every worktree (Chore) · UTA-59

**Goal:** an agent runs every check it needs with one command that only does the work the change needs, never re-runs checks on code that has already passed, and starts from a warm build in a new worktree.

**Why:**
- **Repeated full suites.** Implementers ran the full suite 2–4 times per ticket. UTA-47 ran it at 12:28, 12:31 after a rebase, 12:45 and 12:54. UTA-52 ran `tauri build` five times. About 25 minutes of implementers' foreground time went on `tauri build` alone.
- **Rust checks on frontend-only changes.** UTA-37 noted "Rust too, though no Rust changed".
- **No gate on push.** UTA-37 pushed a failing test, because nothing gated `git push` on the result.
- **Main moving mid-check.** In UTA-47, UTA-43 merged during the checks, so the agent killed its `tauri build`, rebased and ran everything again. In UTA-55, a merge was only noticed from an odd-looking diff.
- **Cold builds.** Every worktree builds its own Rust `target/` from cold (about 5.5–6 GB) and runs `npm ci`. UTA-35: "Fresh worktree, so Rust is compiling from scratch." The disk sat at 96–98% full, and `rm -rf target` failed during tidy in UTA-48 and UTA-49.
- **No timing baseline.** UTA-50 and UTA-51 built `main` just to get a baseline for the timing report. In UTA-49 the report was disturbed by other sessions' load and run again (186 s, then 104 s).

**What to build:**
- **`scripts/check`.** It runs CLAUDE.md's checks as jobs, in parallel where they don't fight over the same build.
  - **What it runs:**
    - Rust (fmt, clippy, test) when anything under `crates/`, `app/src-tauri/` or `Cargo.*` changed against `origin/main`.
    - Frontend (tokens, designmd, typecheck, lint, test) when anything else under `app/`, or `DESIGN.md`, changed.
    - RTSan when the engine changed.
    - `--all` forces everything.
  - **A stamp for each pass,** keyed on the tree's hash, so re-running on code that already passed is instant and says so.
  - **A warning when `origin/main` has moved** since the branch point, with the commits.
  - **A summary at the end:** one line per job, its time, and the log path of any failure.
- **The release build** runs locally only when `app/src-tauri/`, the Tauri config or `app/package.json` changed. CI builds it on every PR. Update CLAUDE.md's "run all of the above" rule to say so (decided by Will, 11 Oct).
- **A pre-push hook** (`core.hooksPath` set to `.githooks/`). It allows the push if the commit's tree has a passing stamp, and otherwise runs `scripts/check`.
- **Warm builds in a new worktree.** Measure, then pick one:
  - (a) `sccache` as the compiler wrapper;
  - (b) clone `main`'s `target/` into the new worktree with APFS copy-on-write (`cp -c -R`), which takes no extra disk until files change.

  One `CARGO_TARGET_DIR` shared by all worktrees is out: cargo locks it, so parallel sessions would queue behind each other. Do the same for `app/node_modules` if it's cheap.
- **A committed timing baseline**, so the timing report compares against it without building `main`.
- **The skills:** `work-ticket` step 5 and `pr-reviewer.md` call `scripts/check` instead of listing the commands.

**Acceptance criteria:**
- [ ] `scripts/check` on a frontend-only change runs no Rust jobs, and on a Rust-only change runs no frontend jobs. Show both with a real diff.
- [ ] A second `scripts/check` on the same tree finishes in under 2 seconds and says the tree already passed.
- [ ] It warns when `origin/main` has moved, and lists the commits.
- [ ] The pre-push hook blocks a push whose tree hasn't passed, and allows one that has.
- [ ] A new worktree's first `cargo test --workspace` is measurably faster than a cold one. Record both times, and which option was chosen and why.
- [ ] The timing report compares against the committed baseline without building `main`.
- [ ] CLAUDE.md, `work-ticket` and `pr-reviewer.md` point at `scripts/check`, and CI still runs the full set unchanged.
- [ ] `scripts/check` skips `tauri build` unless `app/src-tauri/`, the Tauri config or `app/package.json` changed, and CLAUDE.md says so.
- [ ] Process log entry added.

**Out of scope:** changing what CI runs. Reviewers skipping local checks (UTA-61). Caching anything in CI (it already uses `rust-cache`).

**Depends on:** Nothing.

**Context:** the audit above. CLAUDE.md "Commands", `work-ticket` step 5, `.github/workflows/ci.yml`, the `tidy` skill.

### 3. Start the app for Will with one command (Chore) · UTA-60

**Goal:** when Will needs the app, the agent runs one command that opens the right branch's app on a free port, and either reports it's open or says why it failed. Will never has to ask for it to be opened or restarted.

**Why:**
- **The wait loop is rewritten every session.** The skill describes how to wait for `tauri dev` (strip the colour codes, grep for `Running .*uta-app`), and every session writes that loop by hand.
- **Waits run to the cap.** The loop ran to its 10-minute cap in UTA-35 (twice), UTA-40 and UTA-45, and to 4–7 minutes in UTA-41, 42, 44 and 46. That's about 59 minutes blocked across 9 sessions.
- **Will has to ask.** He asked for the app to be opened, booted or restarted 6 times (UTA-35 twice, 38, 39, 41, 42).
- **Port 1420 is contested:**
  - the deadwax project held it in UTA-52 and UTA-56;
  - UTA-41 stopped UTA-37's app to take it;
  - UTA-44 moved to 1430 by hand, and UTA-56 used `--config devUrl 1430`;
  - UTA-45 didn't launch at all.
- **The dev server dies:**
  - in UTA-44 the background shell exited and took the dev server with it;
  - in UTA-56 the server died while idle and had to be restarted;
  - in UTA-52 the build ran from the repo root, exited 254, and the old app was shown.

**What to build:**
- **`scripts/dev-app start`** works from any directory, against the checkout the script lives in. It:
  - restarts this checkout's app if it's running, and leaves other checkouts' apps running, each window titled with its branch (changed during UTA-60, decision 3);
  - picks a free port from 1420 upward, plus the HMR port above it, and passes it to Vite and Tauri. That needs `vite.config.ts` to read the port from an environment variable, with 1420 as the default;
  - starts `tauri dev` fully detached, so it outlives the shell that started it, writing its log and pid to a known place in the checkout;
  - waits for the ready line with the colour stripped, printing progress, with a cap long enough for a cold build;
  - on success, prints the branch, short commit, port and pid. On failure, it exits non-zero with the last 30 lines of the log.
- **`scripts/dev-app stop`** and **`scripts/dev-app status`** (is it running, which checkout, which commit, has the log got errors since it started).
- **The skills:** `work-ticket` step 9 (have the app open) and step 10 (stop what you started) call the script, and the hand-written wait goes.

**Acceptance criteria:**
- [ ] `scripts/dev-app start` opens the app from a worktree, prints branch, commit and port, and returns. The app stays up after the calling shell exits.
- [ ] With port 1420 held by an unrelated process, it picks the next free port and the app works on it. The unrelated process is untouched.
- [ ] With another worktree's Uta app running, it leaves that one running, and the two windows can be told apart (changed during UTA-60, decision 3).
- [ ] A failed build (simulate one) returns non-zero within a few seconds of the failure, with the log tail, not after the cap.
- [ ] `status` and `stop` work, and `stop` leaves no Vite or `uta-app` processes behind.
- [ ] `work-ticket` uses the script in steps 9 and 10.
- [ ] Process log entry added.

**Out of scope:** screenshots and the screenshot harness (parked by Will on 10 Oct; the audit found it rebuilt in 8 sessions, which is worth a ticket of its own later). Release-build benchmarks.

**Depends on:** Nothing.

**Context:** the audit above. `work-ticket` steps 9 and 10, `app/vite.config.ts`, `app/src-tauri/tauri.conf.json`.

### 4. Let the reviewer trust CI and set up in one command (Chore) · UTA-61

**Goal:** a review round spends its time reviewing. One command gathers the PR, its history and its CI. The reviewer checks that CI is green on the exact commit instead of re-running the whole suite, and only runs locally what CI can't prove: the tests behind each claim and the break-it-on-purpose checks.

**Why.** Across the 48 reviewer runs (381 minutes, 1,570 tool calls; round 1 averages 10 minutes and 40 calls, later rounds 6 minutes and 29 calls):
- **Re-running the whole suite.** `pr-reviewer.md` says to run everything CI runs, locally. So every round:
  - makes a fresh worktree (47 of 48 runs removed theirs at the end), which means a cold cargo build and `npm ci` every round;
  - runs the full Rust and frontend suites, often RTSan, the timing reports and `tauri build`;
  - and then waits for CI anyway.

  That's about 90 minutes of suites plus about 38 minutes of hand-written `until`, `sleep` and `--watch` loops.
- **It gained nothing.** The local result matched CI in 46 of 47 runs, and no case was found where a local run caught something CI missed.
- **Small fixes, full runs.** 14 of 27 re-reviews were for fixes of 5 files or fewer:
  - PR 41 round 3 was a one-line CSS fix and ran the full Rust suite plus `npm ci` (109 s);
  - PR 57 round 2 spent 6 minutes of suites on a 71-line fix.
- **Waiting on CI.** Reviewers wrote their own wait loops. `gh pr checks` exits 8 while CI is pending, which reads as an error. Two waits hit the 10-minute cap (PR 52 round 1, on the timing report; PR 48, on a screenshot script).
- **Gathering is rebuilt every round.**
  - About 11 calls per run go on fetching the PR, reviews, comments and issue comments with ad-hoc `jq`, finding the ticket in the PR body, and making the worktree. That's 23% of all reviewer calls, and the jq failed in 4 runs.
  - 24 runs `cd`-ed into a review worktree path that doesn't exist, because `../uta-review-<n>` resolves differently depending on where the reviewer started.
- **Break-it-on-purpose by hand.** 41 of 48 runs did these checks by hand, 111 calls in all, and 18 reviewers wrote their own `mutate.sh` or `run()` helper.
  - The same mutations repeat: an allocation injected into the process path to prove the real-time guard fires (PRs 49, 51, 52).
  - The same mutation is re-run in later rounds: PR 49 rounds 1–3 re-broke `fade_out` each time.

**What to build:**
- **`scripts/review-setup <pr> [--round r | --commit sha]`** prints, in one go:
  - the PR's title, branch, head commit and body;
  - every earlier review with the commit it reviewed, and inline comments with their replies threaded;
  - the CI table with each job's commit;
  - the ticket's Notion page ID, from the PR body;
  - the review worktree, created or reset to the head commit, at a fixed absolute path. Reuse the same worktree across rounds so its build stays warm, and let `tidy` remove it after the merge;
  - the diff stat against the merge base and, on a re-review, against the last reviewed commit;
  - which areas changed (Rust, app, docs), and any golden WAVs that changed.

  With `--commit`, it leaves out everything recorded after that commit, as `pr-reviewer.md` already requires.
- **`scripts/ci-wait <pr>`** waits until every check has finished for the PR's current head commit, prints the table, and exits:
  - 0 when all are green;
  - 1 when any is red;
  - 2 when the checks are for a different commit;
  - 3 on timeout.
- **`scripts/mutate <file> <find> <replace> -- <test command>`** applies one exact replacement (it fails if the text doesn't match exactly once), runs the command, and always restores the file, even on Ctrl-C. It prints CAUGHT when the tests failed and MISSED when they passed.
  - Ship a named set of standard mutations: at least `rt-alloc`, an allocation at the top of the process path.
  - Reviewers then run `scripts/mutate --named rt-alloc` instead of inventing it.
- **`pr-reviewer.md` changes:**
  - **Gather** is `scripts/review-setup`.
  - **Checks:**
    - run `scripts/ci-wait` and require green on the head commit;
    - run locally only the tests that prove each criterion, the mutations, and anything CI can't do (listening renders, looking at golden diffs);
    - if CI is red, missing or for another commit, run the checks locally as now;
    - never run the timing reports locally (CI runs them, and they aren't gated).
  - **Re-reviews:** rerun only the mutations behind the findings being resolved, plus any for new code.
  - **The worktree:** keep it at the end of a round instead of removing it.

**Acceptance criteria:**
- [ ] `scripts/review-setup 57` and `scripts/review-setup 57 --round 2` print everything above for a real merged PR, in one call, with no errors. `--commit` leaves out later reviews.
- [ ] `scripts/ci-wait` returns the right exit code for a green PR, a red one, and a PR whose newest push has no checks yet.
- [ ] `scripts/mutate --named rt-alloc -- cargo test -p uta-engine --test realtime` prints CAUGHT, and the file is unchanged afterwards. The same is true when the run is interrupted.
- [ ] `pr-reviewer.md` uses all three, and keeps the "if CI isn't green on this commit, run locally" rule.
- [ ] A dry-run review round (`review-round` with `dryRun`) on a recent PR completes using the new steps. Record its time and tool calls against the baseline above (round 1: 10.2 min, 40 calls).
- [ ] Process log entry added.

**Out of scope:** rendering the GitHub review body from the structured result. Changing the workflow's arguments or the independence rule: the reviewer is still briefed with nothing but the PR number. The implementer's checks (UTA-59).

**Depends on:** UTA-58 (both change the gathering section of `pr-reviewer.md`).

**Context:** the audit above. `.claude/agents/pr-reviewer.md`, `.claude/workflows/review-round.js`, `.github/workflows/ci.yml`.

### 5. Script the ticket's Notion steps (Chore) · UTA-62

**Goal:** claiming, finishing, merging and unblocking a ticket, and logging a design decision, are each one command. The agent stops building Notion calls and inline Python by hand.

**Why:**
- **The same sequence, rebuilt every ticket.** Every ticket session builds the same Notion sequence by hand: 7–17 `ntn.sh` calls and 7–23 inline Python snippets per ticket. The sequence:
  - find the page ID from `UTA-n`;
  - check each dependency is Done and its PR merged;
  - set In Progress, and the project to Active if it was Planned;
  - post the Started comment;
  - later, list the page's blocks and PATCH each checkbox one call at a time;
  - append the notes, set the PR link and status;
  - on merge, set Done, query Backlog tickets, check each one's dependencies, set Ready, and set the project Done.
- **Hard-coded and broken JSON.** UTA-34 hard-coded block IDs. JSON quoting broke in UTA-51 and UTA-52.
- **Merging fails every time.** `gh pr merge --delete-branch` merged and then errored in 10 sessions, before the skill was changed to say so. Agents now hand-type the right sequence instead.
- **Design decision numbers collide.** Parallel sessions took the same numbers, because agents wrote `provisional: D-n` into the code before creating the Notion row: UTA-42 renumbered D-13 to D-17, UTA-45 renumbered D-18 to D-22, and UTA-46 created D-18 early to claim it.
- **Will has to ask what's Ready.** He asked which tickets could run in parallel 3 times (UTA-52, and twice in the parallel UTA-37 session). Each time the agent queried Notion and checked dependencies by hand.

**What to build:** `scripts/ticket`, built on UTA-58's wrapper:
- **`show UTA-n`:** page ID, status, type, project, and each dependency with its status and whether its PR is merged.
- **`ready`:** every Ready ticket whose dependencies are Done and merged, and which of them could run alongside each other (no dependency between them).
- **`claim UTA-n --branch <name>`:** refuses unless the ticket is Ready and its dependencies are merged. Then it sets In Progress (and the project to Active), and posts the `🤖 Claude:` Started comment.
- **`tick UTA-n <criterion>…`:** ticks checkboxes by matching their text, and fails on no match or an ambiguous one.
- **`merge UTA-n`:**
  - refuses unless the newest review says Ready to merge and CI is green on the head commit;
  - squash merges and deletes the remote branch without `--delete-branch`;
  - sets Done, moves newly unblocked Backlog tickets to Ready, and sets the project Done if everything is;
  - prints what's Ready now.
- **`decision new --name … --needed … --chosen … --principles … --alternatives … --ticket UTA-n`:** creates the Design decisions row first and prints its `D-n`, so the number is never guessed.
- Every command has `--dry-run`, which prints the Notion and GitHub calls it would make.
- **The skills:** `work-ticket` steps 1, 2, 6 and 10 and the `uta-notion` skill call these, and the hand-written recipes they replace go.

**Acceptance criteria:**
- [ ] `show` and `ready` give the right answers for the live Drums project (check against Notion by hand once).
- [ ] `claim --dry-run` refuses a Backlog ticket and a ticket with an unmerged dependency, and lists the calls it would make for a Ready one.
- [ ] `tick` ticks by text, and fails clearly on no match or an ambiguous one (try both against a Done ticket in `--dry-run`).
- [ ] `merge --dry-run` against a recently merged PR lists the right status changes and unblocked tickets, and refuses when the newest review isn't Ready to merge.
- [ ] `decision new` creates the row and prints the `D-n` that Notion gave it. The `work-ticket` skill says to run it before writing `provisional: D-n` anywhere.
- [ ] Unit tests cover dependency checking, unblocking and the text matching for ticks, with Notion and GitHub mocked.
- [ ] Process log entry added.

**Out of scope:** the verification notes and the PR body (UTA-63). Planning-side scripts: creating a project's tickets from a plan, pushing RFCs, the Glossary (worth a later ticket; six planning sessions each wrote their own ticket-creation script). Starting tickets automatically.

**Depends on:** UTA-58.

**Context:** the audit above. `work-ticket` steps 1, 2, 6 and 10, the `uta-notion` skill's ticket and design-decision sections.

### 6. Keep a ticket's evidence in one place (Chore) · UTA-63

**Goal:** a ticket's evidence is written once, in the PR body, and everything else is made from it. Renamed tests and moved files are caught before a reviewer sees them, so no review round is spent on stale text.

**Why:**
- **Three copies, edited by hand.** The evidence lives in three places: the PR body, the ticket's Verification notes and ticks, and the PR replies. After every review round the agent edits all three by hand. UTA-49 redid them at 14:51, 15:15, 15:58 and 16:05.
- **The copies drift**, and the reviewer, rightly, treats an overstated or outdated claim as blocking:
  - a test count (508 vs 509);
  - a test renamed in a fix but not in the notes;
  - a sentence written before a rebase.
- **Whole rounds spent on it:**
  - stale text was the only finding of a round in UTA-37, 39, 40, 42, 44 and 45;
  - it was flagged again in UTA-47, 48, 49, 50, 53 and 56 (UTA-56: "outdated test name in Verification notes");
  - each such round costs 4–10 minutes of review plus the fix.

**What to build:**
- **The PR body is the source.** The implementer keeps it as a file in the session's scratchpad and edits only that file.
- **`scripts/ticket evidence-check <file>`:** every test name, file path and command in backticks must exist in the branch:
  - a test name is a `fn` in the code;
  - a path is a file;
  - a script is in `package.json` or `scripts/`.

  It also flags totals ("509 tests") and points at the rule: name the tests, don't count them.
- **`scripts/ticket sync UTA-n --pr <n> <file>`:**
  - runs the check;
  - updates the PR body (with `gh api … -X PATCH`, since `gh pr edit` fails on this repo);
  - ticks the ticket's checkboxes to match the PR's `- [x]` criteria;
  - replaces the Verification notes with the evidence and the "For Will to try" list;
  - sets the PR link and the In Review status.

  Running it again after a fix updates everything in one go.
- **The skills:**
  - `work-ticket` steps 6 and 7, and the review loop's "update the verification notes", use `sync`;
  - the skill says not to quote test totals;
  - `pr-reviewer.md` runs `evidence-check` as part of its honesty check, so a stale reference becomes a quick, exact finding rather than a hunt.

**Acceptance criteria:**
- [ ] `evidence-check` passes on a recent merged PR's body against its merge commit, and fails, naming the reference, after a test in it is renamed.
- [ ] `sync --dry-run` on a recent ticket shows the right ticks, notes and PR body, and a real `sync` on a scratch Notion page (not a live ticket) does it in one call.
- [ ] Running `sync` twice makes no further changes the second time.
- [ ] `work-ticket` and `pr-reviewer.md` use it, and say not to quote test totals.
- [ ] Process log entry added.

**Out of scope:** what the evidence says, or the PR template's sections. Notion page formats other than tickets.

**Depends on:** UTA-62.

**Context:** the audit above. `work-ticket` steps 6, 7 and 8, `pr-reviewer.md` check 9 ("Honesty of the record").

## Order and checkpoints

- **First, in parallel:** UTA-58, UTA-59 and UTA-60. None depends on another, and they touch different parts of the skills.
- **Then:** UTA-61 and UTA-62, in parallel, once UTA-58 is merged.
- **Then:** UTA-63, once UTA-62 is merged.

Checkpoints for Will:
- **After UTA-60:** the next ticket's handover should have the app open without asking, on whatever port is free.
- **After UTA-61:** compare the next ticket's review rounds with the baseline (round 1: 10 minutes and 40 calls; later rounds: 6 minutes and 29 calls).
- **After UTA-63:** the next ticket should have no review round spent on stale text, and roughly half the Notion calls (baseline: 7–17 per ticket).

## How we'll know it helped

Measure the first two tickets after this project against the audit's baselines, and record the results in the Process log entries:
- tool calls and tokens per ticket session (now 100–250 calls, 50–150M cached tokens);
- reviewer minutes per round;
- rounds whose only finding is stale text;
- how often Will asks for the app.

## Not in this project

From the same audit, deliberately left for later:
- **A sound-work kit:** `uta stats` for WAVs, committed listening-render generators, a shared drum test macro, fixed-length goldens, and handovers that render and open the folder. It's worth doing before the effects and synth projects.
- **The screenshot harness,** parked by Will on 10 Oct.
- **Planning-side Notion scripts:** creating tickets from a plan, pushing RFCs, the Glossary, and a "state of the world" briefing.
- **Reusing earlier research and checking Artifacts before publishing.**

## Decisions

1. **The release build runs locally only when it could break (UTA-59).** That means when `app/src-tauri/`, the Tauri config or `app/package.json` changed. CI builds it on every PR and covers the rest. UTA-59 updates CLAUDE.md's "run all of the above" rule to match. (Will, 11 Oct.)
2. **A new project, "Agent tooling", with no RFC.** It's process work with its evidence above, not a product decision. (Will, 11 Oct.)
3. **Dev apps run side by side (UTA-60).** `scripts/dev-app start` was planned to stop every other Uta dev app. With tickets running in parallel, each handover then closed the previous one's app (it closed UTA-57's). So `start` restarts only its own checkout's app, each window's title names its branch, and a lock stops two starts choosing the same port. (Will, 11 Oct.)
