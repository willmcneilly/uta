# Project: Responsive at song scale

**Status:** Created · **RFC:** [RFC-004](../rfcs/rfc-004-responsive-at-song-scale.md) · **Notion:** https://app.notion.com/p/Responsive-at-song-scale-3ee3af969b6f81768c27d188dd2b197d

## Goal

Build RFC-004. A slider step or a note edit costs a few milliseconds however many notes the song holds, so the check-7 song (588,000 notes) never freezes and the 50,000-note target songs stay at 60 frames a second. Each change travels once, the fast way, and carries only the notes that changed. Sliders follow the mouse and a drag never builds a queue. A benchmark and a live "slowest block" readout prove it, and say where a dropout came from if one happens.

## Where the code starts

Checked against `main` at `1b20af4`, after UTA-23 and UTA-24:

- **Sending changes.** Every command goes through `edit` ([commands.rs:51](../../app/src-tauri/src/commands.rs)), which returns the whole `ProjectView` as the reply and also emits it as `project-changed`. The menu's Undo and Redo (and so ⌘Z and ⇧⌘Z) call the same `undo`/`redo` commands from `lib.rs`, so the event is their only way to reach the UI. The Track, Edit and Develop menus already send a small event with the item's ID, and the UI answers with an ordinary command.
- **The UI's copy.** `App` holds the project in one `useState` ([App.tsx:124](../../app/src/App.tsx)). Every reply and every `project-changed` calls `setProject`, so each change is drawn twice. `ClipView` carries every note of every clip.
- **Core.** Each clip owns a plain `BTreeMap<NoteId, Note>` ([track.rs:201](../../crates/uta-core/src/track.rs)). Nothing in the core is behind an `Arc`.
- **Engine.** `Snapshot::from` copies each clip's notes into its own `Arc<ClipNotes>` and finds unchanged clips by comparing every note ([snapshot.rs:103](../../crates/uta-engine/src/snapshot.rs)). `Status` has no timing. The timing report has the loop, voices, song and 32-tracks loads.
- **Sliders and drags.** Four components use the web view's `<input type="range">`: `Volume`, `Transport`, `SynthPanel` and `TrackHeaders`. Each shows the project's value, so it only moves when Rust replies. `useGesture` numbers a drag for undo. Every pointer move sends a step without waiting for the last reply, on the sliders and on the timeline's and piano roll's canvas drags.
- **Measuring.** The frame readout (`FrameTime`) shows frame and drawing times at the bottom of the window, and the Output panel shows the dropout count. The spike branch `spike/song-scale` (never merged) has the benchmark in `app/src/spike.ts`, a `spike_fill` command that builds test songs, and a slowest-block measurement in `live/callbacks.rs`. They're starting points to port, not code to merge.

## Tickets

### 1. [UTA-26] Run a benchmark from the Develop menu (Feature)

**Goal:** Develop → Run Benchmark builds each test song, works the controls by itself, and shows how long each step took against the RFC's targets. Every later ticket's PR shows its before and after with it.

**Acceptance criteria**
- [ ] Develop → Run Benchmark asks first, because it replaces the current song. It then builds, in turn, the heavy song (12 tracks × 64 one-bar clips × 64 notes), the wide song (32 × 200 × 8) and the check-7 song (7 × 28 × 3,000 stress notes).
- [ ] Rust builds each test song from a recipe (tracks, bars, notes per clip) as ordinary commands, so it's undoable and replayable like any other change. The UI only says which recipe.
- [ ] At each load it times, from sending to the screen updating: slider steps (a track's volume), note deletes in the piano roll, and a 30-step drag that sends a step every frame, as a real drag does. It also records frame times during the drag.
- [ ] The report shows each load's note count, p50 / p99 / max of each step, and the drag's frame times, with the RFC's targets beside them: under 16.7 ms a step and under 20 ms p99 frames at heavy and wide, no frame over 50 ms at check 7. It's shown in the window, and can be copied as text for a PR.
- [ ] Tests: Rust tests that each recipe builds the expected tracks, clips and notes; unit tests for the report's percentiles and targets; a UI test against the mocked back end that the menu item asks, then sends the expected commands.
- [ ] The PR has a report from Will's Mac on `main` as it stands, matching the RFC's measurements roughly (about 80 ms a step at heavy, 1.2 s at check 7). That's the baseline for tickets 2, 5 and 6.

**Out of scope:** Running it in CI (it needs the real web view). Timing the audio thread during the benchmark (ticket 3's readout does that). The big song (262,144 notes) from the RFC's table: the three loads above are the ones the manual checks name.

**Depends on:** Nothing.

**Context:** RFC-004, "What we measured", Proposal part 4 ("The benchmark stays"), "How we'll verify it" manual check 1, and resolved open question 3. The spike's `app/src/spike.ts` and `spike_fill`.

### 2. [UTA-27] Send each change once, the fast way (Bug)

**Goal:** A change the UI asks for comes back only as the command's reply. Changes the UI didn't ask for (the menu bar's Undo and Redo) send a `project-changed` event with no payload, and the UI fetches the project the fast way. Every step gets much faster, though each reply still carries every note until ticket 5.

**Acceptance criteria**
- [ ] Commands the UI calls return the new project view and no longer emit `project-changed`. The UI draws each change once.
- [ ] Undo and Redo from the menu bar, including ⌘Z and ⇧⌘Z, emit `project-changed` with no payload. The UI answers with `get_project` and draws the result. The menu's enabled states still follow every change, wherever it came from.
- [ ] The rule is written down where the next person will see it: no large data in an event, in a comment by `edit` and in `CLAUDE.md`'s "How the pieces talk".
- [ ] Tests: Rust tests that a command's change emits no event and a menu change emits one without a payload; UI tests against the mocked back end that a `project-changed` with no payload calls `get_project` and draws it, and a command's reply is applied once.
- [ ] The benchmark report in the PR shows no `project-changed` events for the benchmark's changes, and every step at every load faster than ticket 1's baseline. (Re-scoped after UTA-27's run: it was "under one frame at heavy and wide". Each reply still carries every note, about 5 MB at heavy and wide, so those targets move to ticket 5.)

**Out of scope:** Sending only changed notes, and the sequence number that guards against out-of-order replies (ticket 5). The check-7 load: it still takes about 130 ms a step until ticket 5.

**Depends on:** 1, so the PR carries a before and after from the same tool.

**Context:** RFC-004, "What we measured" (the project is sent twice) and Proposal part 1.

### 3. [UTA-28] Show the audio thread's slowest block (Feature)

**Goal:** The window shows the slowest audio block since the last frame, as a share of its deadline, next to the dropout count. It's the number that tells an engine that ran late from a device that skipped. The timing report gains the check-7 load.

**Acceptance criteria**
- [ ] The processor times each block on the audio thread and reports the slowest since its last status, as a share of that block's deadline (the block's frames at the sample rate). It's a plain number, kept and sent following the audio thread rules.
- [ ] The frame carries it, and the window shows it as a percentage next to the dropout count. The readout holds the highest value since its last refresh, so one slow block between refreshes is still seen.
- [ ] Tests drive the real process path with the measurement in, under `assert_no_alloc` and RealtimeSanitizer, including a snapshot swap and a simulated device error. An offline render reports a slowest block above 0. A UI test checks the readout shows the frame's value.
- [ ] The timing report adds the check-7 load (7 tracks × 28 one-bar clips × 3,000 stress notes) and the first block after Play from bar 10.5, at p50 / p99. Reported, not gated.

**Out of scope:** Acting on what it shows (a dropout fix is only for after the end-of-project retest). Logging or graphing it over time.

**Depends on:** Nothing. It can run alongside 1 and 4.

**Context:** RFC-004, "What we measured" (the dropout), Proposal part 4 ("A live slowest block readout", "The timing report gains the check-7 load"), "Risks & unknowns" (timing the audio thread on the audio thread), and resolved open question 3. The spike's `live/callbacks.rs`.

### 4. [UTA-29] Share unchanged notes by pointer, in the core and the engine (Chore)

**Goal:** Each clip's notes sit behind an `Arc` in the core, and changing them copies only that clip's notes. "Has this clip's notes changed?" becomes a pointer comparison, and the engine's snapshot shares the core's notes instead of copying and comparing them. Nothing you can see or hear changes. It's what ticket 5 is built on.

**Acceptance criteria**
- [ ] A clip's notes are an `Arc` in the core, changed copy-on-write: a change to one clip's notes copies that clip's only. The core can say whether two clips share their notes. The project's meaning, equality and serialised commands are unchanged.
- [ ] Tests in the core: after every kind of command (including undo and redo), every clip whose notes it didn't change shares them with the previous project, checked by pointer. Moving or resizing a clip shares its notes. Every command still undoes to the exact previous state, and the property tests still replay random command series to the same project.
- [ ] The engine's snapshot holds the core's `Arc` for each clip's notes and finds unchanged clips by pointer, not by comparing notes. A test checks snapshots share the core's notes for unchanged clips, by pointer. The audio thread still holds no reference to snapshot data.
- [ ] The existing sound, golden and real-time tests pass unchanged. No golden WAV is regenerated.
- [ ] A release-mode test times editing one note in a 3,000-note clip (the copy-on-write cost), reported in the PR, not gated.

**Out of scope:** Revisions and anything sent to the UI (ticket 5). Pieces other than clip notes.

**Depends on:** Nothing. It can run alongside 1 and 3.

**Context:** RFC-004, "What we measured" (the correction to RFC-003), Proposal part 2 ("How changes are spotted", the core layer), "Risks & unknowns" (copy-on-write costs a copy), and "How we'll verify it" (copy-on-write in the core, the engine).

### 5. [UTA-30] Send a clip's notes only when they change (Feature)

**Goal:** Each update is the outline, always whole, plus the notes of any clip whose revision the UI doesn't have yet. The UI keeps the notes it was sent and draws from them. A slider step sends no notes and a note delete sends one clip's, so the check-7 song stops freezing.

**Acceptance criteria**
- [ ] **The update.** Every command reply, `get_project` and the menu's fetch return an update: a sequence number, the outline (the project view without notes, with each clip's revision), and a `notes` entry for each clip whose notes are newer than Rust last sent. `get_notes` returns one clip's notes at its current revision. It's shaped for more kinds of piece: each kind has its own key next to the outline.
- [ ] **Revisions in the app layer.** For each clip, `uta-app` keeps the `Arc` it last sent and the revision it gave it, keyed by kind and ID. A different pointer gets the next revision. Revisions never go back, even after undo. They aren't project data, so nothing in `uta-core` changes.
- [ ] **The UI's cache.** `App` keeps the outline and the notes in one reducer with one action ("an update arrived") and RFC-004's four-step merge: ignore an older update, take the outline whole, keep a held notes array (the same array) only if its revision matches, otherwise use what was sent or call `get_notes`, and drop clips the outline no longer has. The timeline and piano roll draw from it.
- [ ] `CLAUDE.md`'s "No project state in the UI" rule is reworded as in resolved open question 1, and "How the pieces talk" describes the outline and pieces.
- [ ] Rust tests, with the stress song built in a test:
  - sizes: a slider step's update holds no notes, and its size doesn't change when notes are added to the song; a note delete's holds exactly one clip's notes; adding a track holds only the new track's; the outline at the heavy and wide loads is under 1 MB;
  - revisions: editing, undoing and redoing a clip's notes each give it a new revision, and only it; moving a clip changes the outline, not the notes; a revision never goes back.
- [ ] UI tests against the mocked back end:
  - the reducer: an update with no notes keeps every held array by identity; sent notes replace only their clip; clips missing from the outline are dropped; an older update is ignored; a held list with the wrong revision is fetched again, including every clip after a reload;
  - the app draws clips from the cache, takes new notes when a revision changes, drops a clip's notes when it goes, and fetches notes for a revision it doesn't have.
- [ ] The benchmark report in the PR meets all of the RFC's targets: under one frame a step at heavy and wide, and no frame over 50 ms at check 7.

**Out of scope:** Any other kind of piece, such as automation (the update is only shaped for them). Loading notes only for clips on screen. A binary format.

**Depends on:** 2 and 4.

**Context:** RFC-004, Proposal part 2 (all of it, including "Where it lives in the UI", "Why revisions are needed" and "Growing later"), "What this changes in RFC-001's contract", "Risks & unknowns" (replies out of order, the outline grows), resolved open question 1, and "How we'll verify it" (size checks, revisions, UI tests).

### 6. [UTA-31] Send only the latest step of a drag (Feature)

**Goal:** A drag keeps at most one step on its way to Rust. Newer positions replace each other while it's in flight, and the newest goes when the reply comes back. A slow step can never build a queue, so the check-7 drag can't freeze for seconds. One helper serves every slider and canvas drag.

**Acceptance criteria**
- [ ] One helper for a drag's steps: while a step is in flight, newer steps replace each other, and only the newest is sent when the reply arrives. The final position is always sent. Anything the drag sends when it ends (trimming notes after a note drag) goes after the final step.
- [ ] Every drag uses it: the master volume, tempo, synth and track mixer sliders, moving and resizing clips on the timeline, moving and resizing notes in the piano roll, and the loop region on the ruler.
- [ ] A drag is still one undo step. Esc mid-drag drops any waiting step, and the cancel goes after any step in flight, so everything is put back. A failed step doesn't stall the drag.
- [ ] The benchmark's drag goes through the same helper, so it times what a real drag does.
- [ ] UI tests against the mocked back end, with replies held back: at most one step is in flight, the final value is always sent, a drag keeps one gesture number, Esc puts everything back, and a note drag's trim follows its final step.
- [ ] The benchmark report in the PR shows the check-7 drag finishing without a queue building: no multi-second freeze, though each step is still slow until ticket 5.

**Out of scope:** The slider showing its own value during a drag (ticket 7). Changing what a drag sends, only when.

**Depends on:** 1. It can run alongside 2, 4 and 5.

**Context:** RFC-004, Proposal part 3 ("One step in flight at a time, latest wins") and "How we'll verify it" (a drag sends at most one step at a time).

### 7. [UTA-25] Replace the built-in sliders with one that follows the mouse and takes focus on click (Feature)

UTA-25 already exists as a Ready chore outside any project. It joins this project (resolved open question 2), its criteria gain the gesture state, and it waits for tickets 5 and 6.

**Goal:** Every slider in the app is one component. It stays under your mouse during a drag, works from the keyboard as soon as you click it, and can be set finely or reset. The web view's built-in range inputs move only when Rust replies, and don't take focus on click.

**Acceptance criteria**
- [ ] One slider component, used by every slider in the app: the transport, the synth panel and the track headers.
- [ ] From press to release, it shows its own value, where the mouse is, and sends steps through ticket 6's helper. On release it shows the project's value again. That value is gesture state, gone when the drag ends.
- [ ] Clicking or dragging it gives it focus. Arrow keys step it, and Shift+arrow takes bigger steps. ⌥-drag moves it in fine steps. Double-click resets it to its default.
- [ ] The outline carries each control's default (master volume, tempo, synth settings and the track mixer), so the UI never hard-codes them.
- [ ] A drag is still one undo step. It's accessible as a slider (role, value and range for assistive tech).
- [ ] UI tests: it shows its own value during a drag and the project's after; click focuses; arrow and Shift+arrow step; ⌥-drag takes fine steps; double-click resets, each sending the expected commands.

**Out of scope:** Knobs, and any control that isn't a slider today. Coalescing itself (ticket 6).

**Depends on:** 5 and 6.

**Context:** RFC-004, Proposal part 3 ("During a drag, the slider shows where your mouse is", "This belongs in UTA-25's slider") and resolved open question 2. RFC-003, "Alternatives considered" (a custom slider).

## Order and checkpoints

```
1 Benchmark ──┬─► 2 Send once ──┐
              │                 ├─► 5 Notes only when changed ──┐
4 Share notes ┼─────────────────┘                               ├─► 7 Slider (UTA-25)
              └─► 6 Latest step of a drag ──────────────────────┘
3 Slowest block (any time)
```

- Tickets 1, 3 and 4 start together. 2 and 6 follow 1. 5 needs 2 and 4. 7 comes last, after 5 and 6. 3 can land any time before the end-of-project checks.
- **Checkpoint after 1:** Develop → Run Benchmark on today's `main`. The numbers should look like the RFC's table. That's the baseline.
- **Checkpoint after 2:** the benchmark again. Steps are about 2.5 times faster at heavy and wide and 5 times at check 7, and no change sends an event, but no target is met yet: each reply still carries every note (about 5 MB at heavy and wide, 57 MB at check 7). The targets wait for 5.
- **Checkpoint after 3:** play the check-7 song at a 64-sample buffer and watch the slowest-block readout while you work the controls.
- **Checkpoint after 6:** drag a volume on the check-7 song. Each step is still slow, but the drag never freezes for seconds.
- **Checkpoint after 5:** delete notes in the piano roll on the check-7 song. They go at once. The benchmark meets every target (manual check 1). Undo and redo from the keyboard and the menu bar show the right notes (manual check 3).
- **Checkpoint after 7:** RFC-004's "What it looks like to you" end to end, and manual checks 2, 4 and 5, including both 10-minute runs at a 64-sample buffer. That's the project done, and the dropout retest: if dropouts remain, the slowest-block readout says whether the engine or the device is to blame.

## Coverage

| RFC-004 verification item | Ticket |
|---|---|
| Size: a slider step's update holds no notes, and doesn't grow with notes | 5 |
| Size: a note delete holds one clip's notes; adding a track only the new track's | 5 |
| Size: the outline at heavy and wide stays under 1 MB | 5 |
| Revisions: edit, undo and redo bump only that clip; a move changes only the outline; never goes back | 5 |
| Copy-on-write: unchanged clips share notes by pointer | 4 |
| Copy-on-write: exact undo, random replay | 4 |
| Engine: snapshots share the core's notes by pointer; sound, golden and real-time tests unchanged | 4 |
| UI: reducer merge rule (identity, replace, drop, older ignored, refetch after reload) | 5 |
| UI: draws from the cache, takes new notes, drops gone clips, fetches a missing revision | 5 |
| UI: `project-changed` with no payload calls `get_project` | 2 |
| UI: a slider shows its own value during a drag and the project's after | 7 |
| UI: one step in flight, final value sent, one undo step | 6 (and 7 for the slider) |
| Real-time safety: slowest-block measurement under `assert_no_alloc` and RealtimeSanitizer | 3 |
| Timing report: check-7 load and first block after Play mid-song | 3 |
| Manual 1: benchmark targets at heavy, wide and check 7 | 1 (the tool), 5 (all three loads; 2 measured a part of the way there) |
| Manual 2: drag sliders at check 7, slider stays under the mouse | 7 |
| Manual 3: delete notes, drag clips and notes, undo and redo at check 7 | 5 and 6 |
| Manual 4: check 8, 10 minutes at buffer 64, slowest block under 50% | end of project (needs 3) |
| Manual 5: 10 minutes at buffer 64 while dragging | end of project (needs 3) |
| Proposal 1: no large data in an event | 2 |
| Open question 1: `CLAUDE.md` rule reworded | 5 |
| Open question 2: UTA-25 joins, after parts 1 and 2 | 7 |
| Open question 3: benchmark and readout kept in the app | 1, 3 |

## Not in this project

- **Everything RFC-004 leaves out:** automation and any piece besides clip notes, opening or loading big songs quickly, a binary format such as MessagePack, loading notes only for clips on screen, and pushing updates through a Tauri channel (the RFC's next thing to try, only if the benchmark shows the extra round trip).
- **A fix for the dropout.** The RFC asks for a retest, not a guess. If dropouts remain at the end-of-project checks, the slowest-block readout points at the engine or the system, and that becomes its own ticket or RFC.
- **The big song** (32 × 128 bars, 262,144 notes). It's in the RFC's measurements but no check names it. The benchmark can add it later as one more recipe.
- **Scratchpad notes.** None of the five raw notes touches this project, so all five stay Raw for a later review:
  - overlapping clips hide each other (RFC-004 already parks it);
  - the synth sounds basic and digital (a sound RFC of its own);
  - open the window full-screen and remember its size;
  - ⌥-drag duplicates the selected notes in the piano roll;
  - resize all selected notes together in the piano roll (the last two would make a small piano roll project; ticket 6 changes when a note drag sends, not what).

## Open questions

1. **Resolved: benchmark first.** Will agreed with the recommendation. The question was: benchmark first, or the quick fix first? Ticket 2 is small and is the biggest single win, and it could start straight away. Recommendation: land the benchmark (1) first and make 2 wait for it, so 2's PR shows a before and after measured by the tool everyone will keep using, not by the spike. Tickets 3 and 4 run alongside 1, so nothing else waits.
2. **Resolved: UTA-25 becomes a Feature, in Backlog.** Will agreed with the recommendation. The question was its type. It was a chore, but with the slider following the mouse it's something you'll notice. Recommendation: change it to Feature when it joins the project, and move it from Ready to Backlog until 5 and 6 are merged.
