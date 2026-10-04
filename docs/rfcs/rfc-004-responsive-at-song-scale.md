# RFC-004: Responsive at song scale

**Status:** Accepted (2026-10-03) · **Author:** Will · **Notion:** https://app.notion.com/p/Responsive-at-song-scale-3ee3af969b6f81cbaf20e26d14134eee

## Summary

Make a song's manual check 7 failed: with a song full of stress notes, any slider drag froze the interface for over a second a step. This RFC measured the cause before proposing anything. After every change, Rust sends the UI the whole project twice. One copy travels the fast way. The other is a Tauri event that the web view has to run as a 57 MB program, and that copy is about 90% of the time. The proposal has four parts, in order:
1. Send each change once, the fast way.
2. Send a clip's notes only when they change, so the cost of a change stops growing with the number of notes.
3. Let a slider follow the mouse during a drag, sending only its latest value.
4. Prove it, and the dropout, with a live audio timing readout and the long playback checks.

The rule that Rust owns the project and the UI only draws what it's sent stays as it is.

## Motivation

- **Check 7 failed.** It used 7 tracks × 28 one-bar clips, each holding 3,000 stress notes (588,000 notes), at a 128-sample buffer. Idle frames were smooth (17 / 18 ms at p50 / p99). Any slider drag froze the window: the slider and the volume both lagged, and the readout showed 112 ms at p99. Deleting one note in the piano roll froze it for about 1.5 s. One dropout was heard, possibly on the first Play, and it didn't happen again.
- **RFC-003 saw this coming and deferred it.** Its shared model point 6 said: keep sending the whole project after every change until check 7 shows that's too slow, then send notes only when they change. Check 7 has now shown it.
- **The cause wasn't measured, and the fix touches RFC-001's contract.** RFC-001 says the UI sends a command and gets the new state back. Changing what comes back is an architectural decision, and we didn't want a quick fix that bends the architecture for the wrong reason. So this RFC measured first.
- **Songs only get bigger from here.** Drums, audio clips and automation will all add to what the UI is sent. Fixing how changes travel now means each later feature doesn't rediscover the problem.

## What we measured

All the numbers here come from a throwaway branch (`spike/song-scale`, never merged) on Will's Mac: an M4 with 4 performance cores and 6 efficiency cores, 24 GB of memory, macOS 15.0. They're release builds running the real app and web view. The branch has:
- an automated benchmark inside the app, which builds each test song, then times slider steps, note deletes and a 30-step drag;
- Rust tests that time each step on the Rust side;
- a live readout of the audio thread's slowest block.

Each result is a median of several runs. The web view's clock is only accurate to 1 ms.

**How big is a real song?** Gathered from published sources (see the Research page):
- a typical pop or rock MIDI file has about 5,000 notes;
- the largest of 176,000 files in the Lakh MIDI collection has about 41,000;
- the solo part of Rachmaninoff's 3rd piano concerto has about 29,000;
- orchestral templates have hundreds of tracks, but any one cue uses few of them.

Will agreed two loads from this:
- **The target** is about 50,000 notes across up to 32 tracks (Uta's limit). Two test songs cover it:
  - **heavy:** 12 tracks × 64 bars, 64 notes a bar (49,152 notes);
  - **wide:** 32 tracks × 200 bars, 8 notes a bar (51,200 notes and 6,400 clips).
- **The stress ceiling** is Will's check 7 song (588,000 notes). It must never freeze, even if it isn't perfectly smooth. A **big** song (32 × 128 bars, 262,144 notes) sits between them.

**What one slider step costs today, from Rust sending to the screen updating:**

| Song | Notes | Project as JSON | One step | The same step without the event |
|---|---|---|---|---|
| heavy | 49k | 4.9 MB | 80 ms | 13 ms |
| wide | 51k | 5.6 MB | 97 ms | 14 ms |
| big | 262k | 25.9 MB | 500 ms | 62 ms |
| check 7 | 588k | 57.4 MB | 1,230 ms | 130 ms |

A note delete costs the same as a slider step. The screen can't draw anything during a step, so a 1,230 ms step is a 1.2-second freeze. A real drag sends a step every frame without waiting for replies. At the check-7 load, 30 drag steps took 36 seconds to work through, with one 8.8-second freeze.

**Where the time goes in one step.** The parts that aren't slow:
- **Applying the command** in the project core: under 0.01 ms.
- **Rebuilding the engine's snapshot:** under 2 ms at every load.
- **Building the project view:** under 3.5 ms. The project view is the copy of the project Rust sends the UI to draw.
- **React working out what changed:** about 1 ms.
- **Drawing:** idle frames stay at 17 ms with every load.

Two things are slow, and both come from sending every note after every change:

1. **The project is sent twice, and the second copy is the slow one.** Every change is sent as the command's reply and also as a `project-changed` event ([commands.rs:51](../../app/src-tauri/src/commands.rs)).
   - **The reply** travels like a `fetch` returning JSON: about 13 ms at the heavy load, including the web view reading the JSON.
   - **The event** is how menu changes (Undo in the menu bar, say) reach the UI. Its payload, the data attached to it, is the same whole project view as the reply: every track, clip and note as JSON. Each note is about 100 bytes, so that's 57 MB at the check-7 load.
   - **Tauri delivers an event as a program.** The reply's JSON is read with `JSON.parse`, a parser built only for data. An event's JSON is pasted into a small JavaScript program instead (roughly `fn({event: 'project-changed', payload: {…57 MB…}})`), and the web view runs it with its full JavaScript parser. It's the difference between `JSON.parse(text)` and `eval(text)`: the same data, but about nine times slower to get through.
   - **The UI handles both copies,** and draws twice.
   - **Turning the event off** for changes the UI asked for (last column above) cuts each step by 85–90%. A 30-step drag at the heavy and wide loads then stays at 17 ms frames throughout.
2. **Even the fast copy grows with the notes.** Without the event, the check-7 load still takes 130 ms a step, because 57 MB of JSON has to be made and read every time.
   - Almost all of it is notes. The same project without notes, which this RFC calls the **outline**, is 19 KB at the check-7 load. The biggest outline measured is 560 KB, for the wide song with its 6,400 clips.
   - A slider step changes no notes, and deleting one note changes one clip's notes. That clip is 290 KB at the check-7 load.

**One correction to RFC-003.** RFC-003's point 6 said the core already keeps each clip's notes behind an `Arc`, Rust's shared pointer, so spotting a change would be a quick comparison. It doesn't: each clip owns a plain sorted map of its notes ([track.rs:201](../../crates/uta-core/src/track.rs)). The engine finds unchanged clips by comparing every note, which costs 1.6 ms at the check-7 load. That's fine on the control side, but the UI couldn't use the same trick.

**The dropout.** The engine isn't what's slow:
- **Offline timing at the check-7 load:** at a 64-sample buffer (a 1.33 ms deadline), the slowest of 15,000 blocks took 0.35 ms. That was the first block after Play from bar 10.5, where held notes are restarted. No block went over its deadline, and no note events were dropped.
- **Live, on the real device:** across three playback runs of about a minute each, nine Plays (first Plays, repeat Plays and Plays from mid-song) never took more than 0.5 ms a block. The first Play was clean every time.
- **The one dropout came during a drag.** In the first run, during a 30-step slider drag while playing, blocks that normally take 0.1 ms took 0.9 ms and then 1.7 ms, and 2 dropouts were counted.
  - The engine's work hadn't changed, so something else slowed the audio thread down.
  - At that moment the web view was working through gigabytes of event scripts. Its memory went from 1.2 GB to 5.8 GB, and the Mac still had over 58% of its memory free.
  - An identical second run had no dropouts. A third run with the event switched off had none either, and the web view stayed at 2.2 GB.
- **My best guess, unconfirmed:** the dropout comes from the CPU being swamped by the web view's work, not from anything the engine does. macOS's audio log recorded no reason for it.

## Proposal

### 1. Send each change once, the fast way

- **A command's reply carries the update.** Commands the UI sends get the new project in their reply, as now. They no longer also send a `project-changed` event.
- **Changes the UI didn't ask for send a small event.** That covers the menu bar's Undo and Redo (including ⌘Z and ⇧⌘Z, which are the menu items' shortcuts) and the Develop menu. Rust sends `project-changed` with no payload, and the UI asks for the update with an ordinary call (`get_project`), which travels the fast way.
- **Large data never goes in an event.** Rust can only push into the web view by asking it to run JavaScript, so an event is fine for a small message and slow for a big one. The playhead and meters already arrive 60 times a second this way, at a few hundred bytes each. Anything big is fetched by the UI instead.

This is a small change and the measurements say it's enough for the target load on its own. Doing it first gives most of the improvement early.

### 2. Send notes only when they change

Each update becomes two parts: the outline, always sent in full, and the **pieces** that changed. A piece is a part of the project that is big and changes on its own, so it's tracked and sent separately. This project builds one kind of piece, a clip's notes, and is designed so more kinds can follow (see "Growing later").

- **The outline** holds everything except notes: tracks, names, mixer settings, synth settings, clip positions and lengths, the loop and the transport. It's 19–560 KB across the test songs and takes under 0.4 ms to build. Every clip in it carries a **revision**: a number that goes up whenever that clip's notes change.
- **Notes** are sent for a clip only when its revision is newer than the one Rust last sent. A slider step sends no notes. Deleting a note sends that one clip's notes. Adding a track sends the new track's clips.
- **The UI keeps the notes it was sent, per clip and revision,** and draws from them. When the outline lists a clip at a revision the UI doesn't have, which shouldn't happen but could if replies arrived out of order, the UI asks for that clip's notes (`get_notes`). It never works out notes for itself. When a clip disappears from the outline, its notes go too.
  - *Web analogy:* it's an HTTP cache with ETags. The outline is the index page and the revisions are the ETags. The UI keeps what it fetched, and refetches only what the server says changed.
  - *React bonus:* an unchanged clip keeps the same notes array between updates, so views can skip redrawing what didn't change.
- **Undo, redo and the menu work the same way.** Undoing a note edit changes that clip's notes, so its revision goes up and its notes are sent. The project-changed path from part 1 returns the same kind of update.

How changes are spotted, in three layers:
- **Core (`uta-core`):** each clip's notes go behind an `Arc`, Rust's shared, reference-counted pointer, as RFC-003 assumed. Changing a clip's notes makes a new copy of that clip's notes only, and every other clip stays shared. This is **copy-on-write**. "Has this clip's notes changed?" becomes "is it still the same pointer?", which costs nothing. The engine's snapshot can share the core's notes directly too, instead of copying and comparing them. Nothing about the project's meaning changes, and replaying commands gives the same project.
- **App layer (`uta-app`):** remembers, for each clip, the notes it last sent and the revision it gave them, and holds on to that `Arc`. Building an update compares pointers: a different pointer gets a new revision and is sent. Holding the old `Arc` matters because it stops that memory being freed and reused for a different clip's notes, which could otherwise look like "unchanged". Revisions are bookkeeping for the UI only. They aren't project data, aren't saved, and aren't in the core.
- **UI:** keeps the notes cache, as above.

**Where it lives in the UI.** Today the project view is one piece of React state in `App` ([App.tsx:115](../../app/src/App.tsx)), and every update replaces it whole. After part 2 it's still one piece of state in `App`, holding the outline and the notes cache, and updates go through a reducer. It's a narrow reducer with exactly one action, "an update arrived from Rust", and a fixed merge rule:
1. Ignore the update if it's older than the last one applied. Every update carries a sequence number, so a late reply can never put back an older outline.
2. Take the new outline whole.
3. For each clip in it, use the notes Rust just sent if there are any. Otherwise keep the list already held, as the same array, so views can tell it hasn't changed, but only if its revision matches the outline's. If it doesn't, or there's none held (after the web view reloads, say), ask Rust for that clip's notes (`get_notes`).
4. Drop clips the outline no longer mentions.

**Why revisions are needed.** Most updates carry no notes for most clips, and that absence has to mean exactly "you already have the right version". Without a revision, "unchanged" and "changed, but you missed the update" look the same. The outline's revision names the current version, so the UI can check its copy:
- **After a reload,** the web view's cache is empty while Rust's app layer thinks it sent everything. The first outline's revisions don't match anything held, so the UI fetches them and recovers. The dev server reloads the web view on every frontend save, so this happens often.
- **Alternatives:**
  - **The `Arc` pointer itself** is a memory address inside Rust, meaningless to the UI, and can be reused.
  - **A hash of the notes** means reading every note on every change, the cost part 2 removes.
  - **One version number for the whole project** says something changed but not which clip, so the UI would refetch everything.

That's different from a typical Redux reducer, which contains the app's logic: a "delete note" action that works out the new state. That would be the patches alternative, with every command written again in TypeScript. This reducer never knows what a note delete or a clip move is, and adding a command to Uta never touches it. It's closer to a data-fetching cache such as Apollo's or RTK Query's, which merges server responses by ID and never computes the server's data itself.

Two kinds of state stay outside the reducer:
- **UI-only state**, such as the selection, the open tab and panel heights, stays in plain `useState` in `App`, as now.
- **A slider's own value during a drag** (part 3) is local state inside the slider, gone when you let go.

**Growing later.** The outline will grow as Uta does, so part 2 is designed for more kinds of piece, while building only clip notes:
- **The rule for splitting:** something becomes its own piece when it's large and changes independently of the rest of the outline. Small things stay in the outline, because sending it whole is simpler than tracking it.
- **Built generally:** each kind of piece gets its own key in the update, next to the outline: `notes` now, and `automation`, say, later. The outline holds each piece's ID and revision. The app layer's tracker is keyed by kind and ID, and the UI's reducer applies the same three-step merge to every kind. Adding a kind is then an addition, not a redesign. Every piece sits behind its own `Arc` in the core.
- **An early warning:** a CI test fails if the outline at the target loads grows past 1 MB (the widest is 560 KB today). When a feature pushes it past that, the failing test says it's time to split something out, before anyone feels a freeze.
- **What's likely to come:**

| Future data | Size | Where it would go |
|---|---|---|
| Drum clips (RFC-005) | notes, as now | already covered: drum hits are notes |
| Automation lanes | small if drawn, thousands of points if recorded | a piece per lane |
| Audio clips | small: start, length, gain | the outline |
| Audio waveforms, for drawing | large, but fixed once the file is imported | sent once per audio file as raw bytes (RFC-001), keyed by the file's ID, never tracked as a change |
| Effects and their settings | a few dozen numbers per effect | the outline, until a track can hold many |
| Plugin state | large and opaque | never sent to the UI: plugins draw their own windows |
| Tempo changes, markers, time signatures | small | the outline |
| Many more tracks than 32 | the outline grows per track | a piece per track's details |

This RFC builds none of these. It only avoids boxing them in.

**What this changes in RFC-001's contract.** The UI still sends commands and still gets the new state back, but now as "the outline, plus anything you don't have yet" rather than "everything". "No project state in the UI" (`CLAUDE.md`) still holds in spirit: the UI never changes or decides project data, it only keeps what Rust sent until Rust says it changed. The rule in `CLAUDE.md` is reworded to say so (open question 1, resolved).

### 3. Sliders follow the mouse, and send only their latest value

Today a slider's position comes from the project, so it can only move when Rust replies. That's why the slider itself lagged. Plugin formats solve this the same way everywhere: VST3's `beginEdit` / `performEdit` / `endEdit`, and JUCE's change gestures.

- **During a drag, the slider shows where your mouse is.** From press to release it draws its own value, and the volume you hear follows as fast as Rust can apply it. On release it shows the project's value again, which by then is the same. This is gesture state, like a text field showing what you've typed: it isn't project data, and it's gone when the gesture ends.
- **One step in flight at a time, latest wins.** While a step is on its way to Rust, newer positions replace each other, and only the newest is sent when the reply comes back. A slow step can then never build a queue: at most one step waits, and the final position is always sent.
  - This is called **coalescing**.
  - Undo is unchanged: one drag is still one undo step.
  - The same helper serves canvas drags: moving clips and notes, and resizing them.
- **This belongs in UTA-25's slider.** UTA-25 replaces every slider in the app with one component. That component is the natural place for gesture state, so UTA-25 joins this project, after parts 1 and 2, and its slider is built with gesture state and coalescing from the start (open question 2, resolved).

With parts 1 and 2 in, a step should take a few milliseconds at any load, so the slider's own value mostly matters at the stress ceiling. It's still worth having: it's what makes a control feel attached to the mouse.

### 4. Measure, and prove it

- **A live "slowest block" readout.** The audio thread already reports its status once a block. It adds the time its slowest block took since the last frame, as a plain number. The Develop menu's frame readout shows it next to the dropout count, as a share of the deadline. That's the one number that tells a CPU spike from a device problem, and it's what the dropout investigation was missing.
- **The benchmark stays.** The spike's in-app benchmark becomes a Develop menu item ("Run Benchmark"). It builds the heavy, wide and check-7 songs, times slider steps, note deletes and a drag, and writes a report.
  - It can't run in CI, because it needs the real web view.
  - Anyone can rerun it after a change, and its report goes in the PR.
- **The timing report gains the check-7 load.** It times a song full of stress notes, and the first block after Play from mid-song.
- **The dropout gets a retest, not a guess at a fix.** Once parts 1 and 2 are in, run check 8 (10 minutes at a 64-sample buffer) at the check-7 load, and a new check: 10 minutes at a 64-sample buffer while dragging sliders. If dropouts still happen, the slowest-block readout says whether the engine was late (an engine problem) or the device skipped (a system problem). Both are listed under "Risks & unknowns".

### Not in this project

Automation and any other kind of piece besides clip notes (part 2 is only shaped for them), overlapping clips (parked in the scratchpad), opening or loading big songs quickly (no saving yet), a different data format such as MessagePack (see the alternatives), loading notes only for the clips on screen, and UTA-25's own keyboard, fine-drag and reset behaviour (that's still UTA-25's).

## What it looks like to you

You build the check 7 song again: 7 tracks, 28 bars, every clip packed with stress notes. Idle, the frame readout says 17 / 18 ms, as before. You drag a track's volume. The slider stays under your mouse and the level changes as you drag. No freeze, and the readout stays close to idle. You select a clip, delete a note in the piano roll, and it disappears at once. You drag a clip along the timeline, and it moves smoothly.

In the Develop menu, the frame readout now also shows the audio thread's slowest block, as a percentage of its deadline. You set the buffer to 64, press Play, and drag sliders around for a while. The slowest block stays well under 100% and the dropout count stays at 0. Leave it playing for 10 minutes and it's still 0.

You choose Develop → Run Benchmark. It warns you that it replaces the current song. Then it builds each test song in turn, works the controls by itself for a minute or two, and shows a report of how long each step took.

## Alternatives considered

- **Only merge drag steps (coalescing alone).** This stops a queue of steps building up, so a drag can't freeze for 9 seconds. But each step at the check-7 load still costs 1.2 s, and each step at the heavy load still costs 80 ms, five frames. It's part 3 of the proposal, not a fix on its own.
- **Push updates through a Tauri channel instead of ping-and-fetch.** A channel is Tauri's ordered stream from Rust to the UI. According to Tauri's source, a channel message over 8 KB isn't run as a script: the page is told to fetch it. That would give one push path with no extra round trip. It isn't chosen because that behaviour comes from reading Tauri's code, not from a measurement, and ping-and-fetch uses only paths this RFC timed. If the extra round trip ever shows up in the benchmark, this is the next thing to try.
- **Only part 1 (send it once, the fast way).** It's measured to make the target load smooth, and it's the smallest change. But the check-7 load still takes 130 ms a step, and the cost of every change still grows with every note in the song. Audio clips, drums and automation would bring the problem back. Part 2 makes the cost of a change depend on what changed, not on the song's size.
- **Send exact changes, not whole pieces.** Both sides keep a copy, and Rust sends patches such as "note X was deleted from clip Y" for the UI to apply. Updates would be as small as possible. Part 2 already keeps a copy on both sides and sends only what changed, but in whole pieces: the outline, and a changed clip's complete note list.
  - **Why it loses on correctness:** patches mean the UI applies every change to its copy itself. That's a second description of every change, in TypeScript, kept in step with Rust's forever. One missed or misapplied patch shows the wrong song with nothing to notice it. With whole pieces, the UI only ever swaps in what Rust sent, and a revision it doesn't have fixes itself by fetching one clip.
  - **Why it loses on gain:** the pieces are already small. The outline is 19–560 KB and under 0.4 ms to build, and one clip at the check-7 load is 290 KB. That's a few milliseconds a step against a 16.7 ms frame.
  - **When to revisit:** if a measurement ever shows a single huge clip, or a huge outline, is too slow. The outline can then split per track using the same revisions.
  - UI-to-Rust already works this way: a command describes only the change.
- **A generic diff of the whole project, such as JSON Patch.** Rust would compare the old and new project view after each change and send the difference. But making the diff means walking the whole project every time, including every note. Nearly all the data is notes, and notes already group naturally into clips, so per-clip revisions get almost all the benefit for far less work. RFC-003 weighed this the same way.
- **A faster format: raw bytes, or a binary format such as MessagePack.** RFC-001 already says large binary data travels as raw bytes. A binary format would make reading the project perhaps two or three times faster (a guess; not measured). But it would still send every note on every change, so the cost would still grow with the song. Worth revisiting for big one-off transfers, such as opening a song, once saving exists.
- **Let the UI apply commands to its own copy of the project** ("optimistic updates", as Figma and Replicache do). Every change would show instantly. But it means writing every command twice, in Rust and TypeScript, and keeping them identical forever, and it breaks RFC-001's rule that the core alone owns the project. That's too high a price for a problem parts 1 to 3 solve.
- **Send notes only for clips on screen, and fetch the rest as you scroll.** It would also make the first load faster. But it adds a delay whenever you scroll to new clips, and the timeline already draws a preview of every visible clip's notes. It can be added on top of part 2 later if loading becomes the problem.
- **Do the work off the main thread.** Tauri runs Uta's commands on the app's main thread. But the Rust side of a step takes a few milliseconds once parts 1 and 2 are in. The time was in the web view, not in Rust.

## Risks & unknowns

- **The dropout's cause is a guess.** The evidence points at the CPU being swamped while the web view worked through the event scripts. It's never been seen without that load, but it was only reproduced once. If it's still there after parts 1 and 2:
  - the slowest-block readout tells us whether the engine's block ran long or the device skipped regardless;
  - the next things to check are how macOS schedules the audio thread across performance and efficiency cores (Apple's audio workgroups), and the memory the audio thread touches for the first time.
- **Replies arriving out of order.** Tauri runs Uta's commands one at a time, so replies should arrive in order, but Tauri doesn't promise it as far as I can find. If they didn't, two things could go wrong. An older outline could land after a newer one, and a slider would jump back. That's caught by the update's sequence number. A clip's notes could also be missed, which the revisions catch. UI tests cover both.
- **Copy-on-write costs a copy per edited clip.** Editing one note copies that clip's notes once: 3,000 notes at the stress load. That should be well under a millisecond, but it's a guess. The command tests will time it.
- **The outline grows as Uta does.** It's 560 KB at 6,400 clips today. More clips, more tracks, effects and automation will all add to it. The 1 MB budget test catches that early, and "Growing later" says how to split. The risk is that a split is put off once the test fails. The test makes that a visible decision rather than a slow drift.
- **Timing the audio thread on the audio thread.** Reading the clock each block must follow the audio thread rules. On macOS it doesn't make a system call, but that's from general knowledge, not measured here. RealtimeSanitizer and `assert_no_alloc` will confirm it.
- **One machine.** Every number comes from one M4 Mac. A slower Mac will have smaller margins, but the gains are 10× or more, so the conclusions should hold.
- **The rule about project state in the UI.** If the cache reads as bending "no project state in the UI", open question 1 settles the wording.

## How we'll verify it

**Automated, on every PR:**
- **Size checks in Rust,** with the stress song built in a test:
  - a slider step's update holds no notes;
  - a note delete's update holds exactly one clip's notes;
  - adding a track holds only the new track's notes;
  - the size of a slider step's update doesn't change when notes are added to the song;
  - the outline at the heavy and wide loads stays under 1 MB (the budget in "Growing later").
- **Revisions:**
  - Editing, undoing and redoing a clip's notes each gives it a new revision, and only that clip's.
  - Moving a clip changes the outline, not the notes.
  - A clip's revision never goes back, even after undo.
- **Copy-on-write in the core:**
  - Changing one clip's notes leaves every other clip sharing its notes with the previous project (checked by pointer).
  - Every command still undoes to the exact previous state, and random command series still replay to the same project.
- **The engine:** snapshots share the core's notes for unchanged clips (by pointer). The existing sound, golden and real-time tests pass unchanged.
- **UI tests against the mocked back end:**
  - The merge rule, as a unit test of the reducer:
    - an update with no notes keeps every held array by identity;
    - sent notes replace only their clip;
    - clips missing from the outline are dropped;
    - an update older than the last one applied is ignored;
    - a held list whose revision doesn't match the outline's is fetched again, including every clip after a reload.
  - The cache draws clips from the notes it holds.
  - It takes new notes when a revision changes and drops a clip's notes when the clip goes.
  - It fetches notes for a revision it doesn't have.
  - It handles a `project-changed` with no payload by calling `get_project`.
  - A slider shows its own value during a drag and the project's after.
  - A drag sends at most one step at a time, always sends the final value, and is still one undo step.
- **Real-time safety:** the slowest-block measurement runs under `assert_no_alloc` and RealtimeSanitizer.
- **Timing report:** adds the check-7 load and the first block after Play from mid-song, at p50 / p99, reported, not gated.

**Manual, by you, with the release build:**
1. Run Develop → Run Benchmark. At the heavy and wide loads, slider steps and note deletes take under one frame (16.7 ms), and the 30-step drag keeps frames under 20 ms at p99. At the check-7 load, the drag never freezes: no frame over 50 ms.
2. Load the check-7 song and drag a track's volume, a synth setting and the master volume. The slider stays under your mouse, the sound follows, and the frame readout stays near 17 / 18 ms.
3. At the same load, delete notes in the piano roll, drag clips and notes, and undo and redo from both the keyboard and the menu bar. There are no freezes, and undo shows the right notes.
4. Check 8 from RFC-003: leave the check-7 song playing for 10 minutes at a 64-sample buffer. The dropout count stays at 0, and the slowest block stays under 50% of the deadline.
5. The same for 10 minutes at a 64-sample buffer while you keep dragging sliders and clips. The dropout count stays at 0.

## Open questions

1. **Resolved: reword the rule in `CLAUDE.md`.** Will agreed with the recommendation. The rewording lands with part 2, when the cache does. The question was: reword "no project state in the UI"? The notes cache keeps what Rust sent until Rust says it changed. Recommendation: reword the rule in `CLAUDE.md` to "No project state in the UI: it renders what Rust sends. It may keep what Rust sent until Rust says it changed, but never changes or works out project data itself." That keeps the intent, which is that Rust decides everything, and stops the cache looking like an exception. The reducer in part 2 ("Where it lives in the UI") is what makes this hold: the only way project data changes in the UI is that merge rule, which can't make decisions.
2. **Resolved: bring UTA-25 into this project.** Will agreed with the recommendation. The question was: how does part 3 relate to UTA-25? UTA-25 is Ready, outside any project, and replaces every slider. Recommendation: bring UTA-25 into this project, ordered after parts 1 and 2, and build the gesture state and coalescing into its slider component. Otherwise either the gesture work gets built twice, or UTA-25 lands first and this project reworks it.
3. **Resolved: keep both in the Develop menu.** Will agreed with the recommendation. The question was: keep the benchmark and the slowest-block readout for good? Both cost some code to maintain. Recommendation: keep both in the Develop menu. The benchmark can't run in CI, but it's the only way to see web view timings, and "run the benchmark and paste the report" is a cheap PR habit for UI-heavy work. The readout is the first thing to look at when a dropout happens.

## New terms

- **Project view:** the copy of the project Rust sends the UI to draw. Made of the outline and the notes of each clip.
- **Outline:** the part of the project view without any notes: tracks, mixer and synth settings, clip positions and the transport, always sent in full.
- **Revision:** a number on each piece, such as a clip's notes, that goes up whenever it changes, so the UI can tell whether the copy it holds is current.
- **Copy-on-write:** sharing data between versions until one of them changes it, then copying only the part being changed.
- **Coalescing:** while one change is on its way, keeping only the newest of the changes that come after it, so a slow step never builds a queue.
- **Frame budget:** the 16.7 ms the UI has to make each frame at 60 frames a second. Work that runs longer freezes the screen until it's done.
- **Piece:** a part of the project view that's big and changes on its own, such as a clip's notes, so it's tracked with a revision and sent only when it changes.
- **Tauri event:** a message Rust broadcasts to the UI. Tauri delivers it as a small program for the web view to run, which is fine for small messages and slow for large ones.

## Amendments

- **2026-10-04, manual check 1's "under one frame":** a step now counts as under one frame if it's drawn on the next frame, measured as p99 of at most 20 ms (the same bar as the drag's frames). The benchmark sends a step just after a frame and times it to the frame that draws it, on a clock that counts whole milliseconds, so even a step that takes no time measures 17 ms, and "under 16.7 ms" could never be met. Found in UTA-30. Agreed with Will. Its runs met this at heavy and check 7. At wide, steps still land on the next frame, but that frame sometimes starts late (up to 25 ms), and that is now UTA-33.
