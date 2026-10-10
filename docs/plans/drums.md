# Project: Drums

**Status:** Created · **RFC:** [RFC-006](../rfcs/rfc-006-drums.md) · **Notion:** https://app.notion.com/p/Drums-3f53af969b6f81e1ae4edc50119a07d3

## Goal

Build RFC-006. Uta has a second kind of track: a drum machine with eight synthesised sounds modelled on the TR-808 and TR-909, which you judge by ear to sound good, not just to make drum noises. A new project starts with Synth 1 and Drums 1. A drum track's Notes tab is the piano roll with eight labelled rows, and its Sound tab is a strip of knobs and a Level fader per sound. The kit is balanced by measurement and then by ear, and every sound is pinned down by measured tests, golden WAVs and the real-time checks.

## Where the code starts

Checked against `main` at `290e6f7`, after UTA-45. RFC-005's project is Active: UTA-43 (extract shared components, no visual change) is In Progress, and every other Drafting table ticket is Done.

- **One kind of source.** `Source` in [track.rs:128](../../crates/uta-core/src/track.rs) has one variant, `Synth(SynthSettings)`. Outside tests it's taken apart with an irrefutable `let Source::Synth(..)` in five places: [snapshot.rs:143](../../crates/uta-engine/src/snapshot.rs), [command.rs:448 and 633](../../crates/uta-core/src/command.rs), [project.rs:291 and 584](../../crates/uta-core/src/project.rs) and the app's [uta.rs:1049](../../app/src-tauri/src/uta.rs). Each becomes a `match`.
- **Commands** are at `COMMAND_FORMAT = 3` ([command.rs:15](../../crates/uta-core/src/command.rs)), and formats 1 to 3 load. `AddTracks` carries a whole track, source included, so a command list can already add a track of any kind once the kind exists. `SetSynthParam` is the model for a new `SetDrumParam`.
- **The default project** is made in `Project::with_id` ([project.rs:97](../../crates/uta-core/src/project.rs)): one track, "Synth 1", with one 4-bar clip, both with IDs derived from the project's ID. `examples/demo-loop.json` and `demo-song.json` refer to those IDs.
- **The engine** has 32 slots (`TRACK_SLOTS`, the project's `MAX_TRACKS`), each with a 16-voice `Synth` built at stream setup ([processor.rs:100](../../crates/uta-engine/src/processor.rs)). A slot chases notes on Play (`Slot::chase`). The synth's oscillator already uses PolyBLEP.
- **Tests** in `crates/uta-engine/tests`: `common/mod.rs` already has `measure_frequency`, `spectrum`, `rms`, `peak`, `max_jump` (the click check) and `level_at`. Golden WAVs exist for the demo loop and the demo song. `realtime.rs` drives the process path under `assert_no_alloc`. `timing.rs` has the check-7 and many-tracks loads.
- **The app.** The outline gives each track a `synth` and the project `synthLimits` and `synthDefaults` ([backend.ts](../../app/src/backend.ts)). `auditionNote(track, pitch, velocity)` already plays a note straight through the engine. "+ Add track" adds a synth track. The Notes and Sound tabs switch with the selected track ([App.tsx](../../app/src/App.tsx)).
- **Controls.** The fader, knob, drag field and segmented choice are in `app/src/design/` (UTA-44 to UTA-46). UTA-45 left a smaller knob size to this project, if the panel needs one.
- **Research.** Notion's Research database, related to RFC-006: "Drum synthesis: an 808/909 kit that sounds good" (recipes, ranges, techniques, reference code and licences), "Drum machine and metronome" (the first research, and the measurement methods), and "Faders, knobs and drag fields".
- **Glossary.** RFC-006's eight terms are already in Notion.

## Tickets

11 tickets: 10 PRs and one design spike. Six build the sounds, four put drums in the window, and one balances and tunes the kit. That's above the usual 4–8 because each sound gets its own listening round, which the RFC asks for, not because the tickets are small.

**Every sound ticket (1–5 and 10) also has these criteria,** written out in full on each Notion ticket:
- [ ] **Built the way the RFC says:** a hit adds energy to circuits that keep running, never restarts them; velocity sets the strength of the excitation, so the tone follows; envelopes are exponential and every edge is smoothed; drive only where the original has it. Ported code is credited in comments (Emilie Gillet's Plaits, MIT, without the "Mutable Instruments" name). GPL code is read only, and Sonic Pi's SC-808 is for ideas and numbers only.
- [ ] **Measured:** the sound's tests from "How we'll verify it" (listed on each ticket), with thresholds calibrated once against the reference behaviour and the reasons written in the test.
- [ ] **No clicks:** a hit, a fast repeat of the same sound, a flam, and turning each of the sound's controls while it plays all stay under the click limit.
- [ ] **Deterministic:** rendering twice gives identical audio, and blocks of 32, 128 and 1024 give the same audio.
- [ ] **Real-time safe:** the sound's hits, flams and control turns run in `realtime.rs` under `assert_no_alloc`, and its process path under RealtimeSanitizer.
- [ ] **Golden:** a per-sound golden WAV, which Will approves in the PR.
- [ ] **Renders for Will's ears:** committed command lists in `examples/listen/<sound>/`: the sound at its defaults, a sweep of each control, a velocity ramp, fast repeats and a flam. One command renders them all to a folder. The PR says what to listen for and how it should compare with the original. Will's verdict is part of the go-ahead to merge: changes he asks for are made in the PR, and anything that needs the whole kit to judge goes on ticket 11's tuning list.

### 1. [UTA-47] Add a drum track that plays an 808 kick (Feature)

**Goal:** A track's source can be Drums: a fixed kit of eight rows, each with its name and General MIDI note. A command list can add a drum track and put notes on it, and `uta render` plays the kick as an 808: a pulse that sets a resonator ringing, with its attack shift and pitch sigh. This ticket builds the drum machine the other sounds plug into.

**Acceptance criteria** (plus the shared ones above)
- [ ] Core: `Source::Drums` holds the kit's settings. The kit's eight rows (name and note, from kick at 36 to cymbal at 49) are defined once, in Rust. `SetDrumParam` sets one setting on one sound, with ranges checked, and a knob drag is one undo step, as with `SetSynthParam`. Commands go to format 4, and a format 3 command list still loads and renders the same.
- [ ] Core: a note on a drum track that isn't on one of the kit's notes is refused, by `AddNotes`, `SetNotes` and `AddTracks` alike.
- [ ] Engine: every slot can play a kit as well as a synth, preallocated at stream setup. One circuit per sound, no voice allocation. Drum notes are one-shots (the end of a note does nothing), drum tracks don't chase notes on Play, and starting playback restarts the kit's free-running state from the same point. Settings changes glide, and a sound that has died away stops doing work.
- [ ] The kick: Tune (40–80 Hz, default 49), Tone, Decay (50–800 ms, default 300) and Level in dB, from the research's 808 recipe. Velocity 100 sounds like an unaccented hit and 127 like a full accent, through one curve every sound will use.
- [ ] Tests: the kick ends within tolerance of its Tune and starts higher than it ends; raising Tune raises its pitch; doubling Decay roughly doubles the measured decay; its spectral centroid is well under 200 Hz and Tone moves it the right way; velocity 127 is brighter than 64, not just louder. The measurement helpers (pitch over time, decay time, spectral centroid) go in `tests/common` for the other sounds. Core: the command undoes exactly, round-trips through serialisation, and a note off the kit is refused.
- [ ] The timing report gets a case with drum tracks playing every kit note at once, which grows as the sounds arrive.

**Out of scope:** The other seven sounds (2–5) and the 909 kick (10); rows without a sound yet are silent. Anything in the app: the outline, a new project's Drums 1, "+ Add track" and moving clips (7).

**Depends on:** Nothing.

**Context:** RFC-006, Proposal: "The kit", "How each sound is made" (Kick, 808), "What makes it sound good", "In the engine", "In the project" (the first two bullets and the third's first sentence); "How we'll verify it" (Pitch, Decay, Brightness, Velocity, No clicks, Determinism, Real-time safety, Timing, Project core). Research: "Drum synthesis", sections 4.2, 4.4, 5 and "Kick".

### 2. [UTA-48] Add the snare and the clap (Feature)

**Goal:** The kit has a 909 snare (two shell tones and the wires, with the noise held before it decays) and an 808 clap (three quick bursts, a fourth, and a smooth tail). Both take their noise from one noise source per kit, as on the 909.

**Acceptance criteria** (plus the shared ones above)
- [ ] Snare: Tune (140–260 Hz), Tone (the wires' length, 40–400 ms), Snappy (how much rattle against the shell) and Level, from the research's 909 recipe. Velocity also tilts the wires against the shell.
- [ ] Clap: Tone (band-pass centre, 700 Hz–2 kHz), Decay (the tail, 50–400 ms) and Level, from the 808 recipe.
- [ ] One free-running noise source per kit, restarted on Play, shared by the snare and the clap.
- [ ] Tests: raising Tune raises the snare's measured shell pitch; Snappy raises the wires' share; Tone lengthens the noise; the clap has three or four bursts about 10 ms apart in its first 40 ms; Tone moves each sound's spectral centroid the right way; velocity 127 is brighter than 64.

**Out of scope:** The 808 snare as an alternative model. Clap spread (RFC open question 4).

**Depends on:** 1, and Will's verdict on its renders.

**Context:** RFC-006, Proposal: "How each sound is made" (Snare, Clap); "How we'll verify it" (Brightness, Clap, Velocity). Research: "Drum synthesis", sections 4.4, 4.7, "Snare" and "Clap".

### 3. [UTA-49] Add the hi-hats, with the choke and the oversampling test (Feature)

**Goal:** The kit has 808 closed and open hats: six free-running square waves at the 808's own frequencies, filtered down to their top end. They're one circuit, so a closed hit cuts off a ringing open hat. Will settles 2× oversampling by a blind A/B.

**Acceptance criteria** (plus the shared ones above)
- [ ] A metal bank of six PolyBLEP rectangular oscillators at 205.3, 304.4, 369.6, 522.7, 540 and 800 Hz with a 47.98% duty, free-running and restarted on Play, built so the cymbal (5) can share it.
- [ ] The closed hat has Tune, Tone, Decay (20–150 ms, default 50) and Level. The open hat has Decay (90–600 ms) and Level, and shares the closed hat's Tune and Tone. The filters above about 5 kHz are prewarped, and the hats' amplifier is the 808's asymmetric, clipping one.
- [ ] Choke: a closed hit cuts a ringing open hat, without a click.
- [ ] The A/B: two renders of the same beat, with and without 2× oversampling, labelled only A and B, with the key held back until Will answers. Keep oversampling if he can hear it. The PR reports its cost in the timing report.
- [ ] Tests: the hats' spectral centroid is in the kHz range and Tone moves it; raising Tune raises it; an open hat followed by a closed hat is far quieter 60 ms after the closed hit than one left ringing; the alias level stays at or below the figure measured here; velocity 127 is brighter than 64.

**Out of scope:** The cymbal (5).

**Depends on:** 2, and Will's verdict on its renders.

**Context:** RFC-006, Proposal: "How each sound is made" (Hats), "What makes it sound good" points 4 and 6, resolved open question 2; "How we'll verify it" (Choke, Aliasing, manual check 2). Research: "Drum synthesis", sections 3, 4.1, 4.5 and "Closed and open hats".

### 4. [UTA-50] Add the low and high toms (Feature)

**Goal:** The kit has 808 toms: struck resonators like the kick, at higher pitches, with a small pitch drop at the start and a little low noise for the skin.

**Acceptance criteria** (plus the shared ones above)
- [ ] Low tom: Tune (80–100 Hz, default 90), Decay (about 100–600 ms, default 200) and Level. High tom: Tune (165–220 Hz, default 185), Decay (default 100) and Level. Both reuse the kick's resonator.
- [ ] A slight pitch drop on the attack, "less like a boing, and more like a tonk", and quiet low-passed noise with a decay a little longer than the body.
- [ ] Tests: each tom ends within tolerance of its Tune; raising Tune raises it; doubling Decay roughly doubles the measured decay; velocity 127 is brighter than 64.

**Out of scope:** A mid tom. A Bend control (RFC open question 4).

**Depends on:** 3, and Will's verdict on its renders.

**Context:** RFC-006, Proposal: "How each sound is made" (Toms); "How we'll verify it" (Pitch, Decay). Research: "Drum synthesis", section 2 and "Low and high toms".

### 5. [UTA-51] Add the cymbal (Feature)

**Goal:** The kit has an 808 cymbal: the hats' metal bank split into three bands, each with its own envelope, so the low band rings longest and the high band is a short sizzle. This is the hardest sound, and it may take the most listening rounds.

**Acceptance criteria** (plus the shared ones above)
- [ ] Tune (the metal bank's pitch, shared with the hats as on the 808), Tone (mainly the high band's level), Decay (350–1200 ms) and Level.
- [ ] Three bands from the shared metal bank, around 3.44 kHz, 7.1 kHz and a slightly resonant high-pass near 10.5 kHz, each with the 808's clipping amplifier and an attack smoothed over about 0.1 ms. The filters are prewarped, and oversampled if 3 kept oversampling.
- [ ] Tests: the low band outlasts the high band; doubling Decay roughly doubles the measured decay; Tone lowers the spectral centroid; the alias level stays at or below 3's figure; velocity 127 is brighter than 64.

**Out of scope:** The 909's sampled crash and ride.

**Depends on:** 4, and Will's verdict on its renders.

**Context:** RFC-006, Proposal: "How each sound is made" (Cymbal); Risks ("The cymbal is the hardest sound"); "How we'll verify it" (Brightness, Aliasing). Research: "Drum synthesis", section 3 and "Cymbal".

### 6. [UTA-52] Duplicate notes with ⌥-drag, and resize the whole selection (Feature)

**Goal:** Two piano-roll comforts from the scratchpad, for every kind of track: ⌥-dragging selected notes leaves the originals and moves copies, and resizing one selected note resizes them all together.

**Acceptance criteria**
- [ ] ⌥-dragging a selected note moves a copy of the whole selection and leaves the originals. Letting go of ⌥ mid-drag goes back to a plain move. The copy is one command and one undo step, and the copies are selected afterwards.
- [ ] Resizing one note of a selection changes every selected note's length by the same amount, with each note kept at least one grid step long. One command, one undo step.
- [ ] UI tests against the mocked back end: ⌥-drag sends one `AddNotes` with the copies in the right place; a plain drag still moves; resizing a selection sends one `SetNotes` with every note's new length.

**Out of scope:** The same comforts for clips in the timeline. Drum lanes (7), which check that both work on a drum track.

**Depends on:** Nothing.

**Context:** RFC-006, Proposal: "In the window" (the third bullet); "How we'll verify it" (UI). Scratchpad notes "Option-drag duplicates notes (the whole selection) in the piano roll" and "Resize all selected notes together in the piano roll".

### 7. [UTA-53] Put drum tracks in the window, with drum lanes in the piano roll (Feature)

**Goal:** A new project starts with Synth 1 and Drums 1. "+ Add track" offers Synth or Drums. A drum track's Notes tab is the piano roll with eight labelled rows from the outline, and clicking a row's label plays its sound.

**Acceptance criteria**
- [ ] Core: a new project has Synth 1 and Drums 1, each with an empty 4-bar clip, with derived IDs, so the example command lists keep working. Drum tracks are named "Drums 1", "Drums 2" and so on. Moving a clip to a track of the other kind is refused. The examples and tests that count tracks are updated, and the existing golden WAVs don't change (if one does, Will re-approves it).
- [ ] The outline carries a drum track's kit: each row's name and note, and each sound's settings with their limits and defaults. The UI never hard-codes the kit.
- [ ] Notes tab on a drum track: eight labelled rows, kick at the bottom to cymbal at the top, all showing without scrolling. Drawing, moving, resizing, selecting, copy, paste, duplicate, velocity, ⌥-drag and group resize work as on a synth track. Dragging a note up or down moves it a row, and the arrows that move notes a semitone move them a row. Clicking a label plays its sound through `auditionNote`.
- [ ] Pasting notes from a synth clip into a drum clip keeps only the notes on the kit's rows, and says how many were left out. "+ Add track" offers Synth or Drums. Dragging a clip in the timeline skips over tracks of the other kind.
- [ ] Until 9 lands, a drum track's Sound tab says the drum panel is coming, in the empty-panel style (D-17).
- [ ] UI tests against the mocked back end: the rows come from the outline; clicking a label auditions its note; notes drawn and moved between rows send the expected commands; paste drops off-kit notes and says so; the clip drag skips the other kind. Rows and labels use tokens only, and anything `DESIGN.md` doesn't cover is logged as a provisional decision.

**Out of scope:** The drum panel (9). A step-sequencer view. Swing.

**Depends on:** 1, 6 and UTA-43.

**Context:** RFC-006, Proposal: "In the project" (the last three bullets), "In the window" (the first bullet); "How we'll verify it" (Project core: clip moves; UI). The RFC's "What it looks like to you", first two paragraphs.

### 8. [UTA-54] Sketch the drum panel's two layouts (Spike, one session)

**Goal:** Will chooses the drum panel's layout before 9 starts, from both layouts drawn with the kit's real controls in the private design sandbox (`uta-design/live/`): a strip per sound (knobs over a short Level fader), and a grid of value bars.

**Acceptance criteria**
- [ ] Both layouts in the sandbox, with all eight sounds and their real controls, the kick's Model choice included, at 1024 px wide and in both themes.
- [ ] Each tried at the knob's existing sizes and a smaller one, so 9 knows whether it needs a new size.
- [ ] Will's choice, its knob size and screenshots recorded on the ticket, as findings for 9.

**Out of scope:** Code in the `uta` repo. Building controls (RFC-005 part 7's).

**Depends on:** Nothing. Best done while the sound tickets run.

**Context:** RFC-006, "The drum panel's controls", resolved open question 6; Risks ("The panel is the knob's densest use"). The design process (`DESIGN.md`, Design decisions log).

### 9. [UTA-55] Lay out the drum panel on the Sound tab (Feature)

**Goal:** A drum track's Sound tab shows the eight sounds in the layout Will chose in 8: each sound's name, its knobs, and a Level fader. You can turn every knob while a beat plays, and clicking a sound's name plays it.

**Acceptance criteria**
- [ ] The layout from 8, built from the existing knob and fader with their shared behaviour (Shift-drag for fine adjustment, no jump on press, reset by double-click, ⌥-click or Delete). If 8 found the knob needs a smaller size, it's added as a size of the same knob, in `DESIGN.md`.
- [ ] Each sound's controls come from the outline (name, value, limits, default, unit), so a sound that arrives after this ticket needs no UI change.
- [ ] A knob drag or fader drag is one undo step, sending `SetDrumParam`. Clicking a sound's name plays it through `auditionNote`.
- [ ] UI tests against the mocked back end: the strips come from the outline; a drag sends the expected commands; double-click resets to the outline's default; clicking a name auditions its note.
- [ ] Tokens only; anything `DESIGN.md` doesn't cover is logged as a provisional decision. The PR has screenshots in both themes and is checked in the real app.

**Out of scope:** The kick's Model choice (10). Panning or effects per sound. Kit presets.

**Depends on:** 7, 8 and UTA-43.

**Context:** RFC-006, Proposal: "In the window" (the second bullet), "The drum panel's controls", resolved open question 6; "How we'll verify it" (UI: clicking a sound's name, manual check 4).

### 10. [UTA-56] Add the 909 kick as a model choice (Feature)

**Goal:** The kick has a Model choice, 808 or 909. The 909 kick is a triangle oscillator shaped towards a sine, with a fast downward pitch sweep and a separate click, so it's tighter and punchier than the 808's.

**Acceptance criteria** (plus the shared ones above)
- [ ] Model is a kick setting, 808 by default, set with `SetDrumParam`. Each model keeps its own settings, so switching back and forth loses nothing, and switching while a kick rings doesn't click.
- [ ] The 909 kick: Tune (base pitch), Sweep (how far the pitch drops at the start), Attack (how much click), Decay (about 100 ms–1.5 s) and Level, from the research's 909 recipe. Its phase starts at a zero crossing.
- [ ] The drum panel shows Model as a segmented choice, and only the chosen model's knobs.
- [ ] Tests: the 909 kick ends within tolerance of its Tune and starts higher; Sweep widens the drop; Attack raises the click's share of the energy in the first 5 ms; velocity 127 is brighter than 64. A UI test: choosing 909 sends the command and shows the 909's knobs.

**Out of scope:** A model choice for any other sound.

**Depends on:** 5 and 9, and Will's verdict on 5's renders.

**Context:** RFC-006, Proposal: "How each sound is made" (Kick, 909), resolved open question 1. Research: "Drum synthesis", section 4.3 and "Kick".

### 11. [UTA-57] Balance the kit and tune it by ear (Feature)

**Goal:** The default kit is balanced by measured loudness, then tuned by Will's ear, and a demo beat pins the whole kit down. It feels like a drum machine, not a test.

**Acceptance criteria**
- [ ] Every sound's default Level comes from measured loudness: kick 0 dB, snare and clap 2 to 4 dB below it, toms 6 dB below, hats and cymbal 10 to 12 dB below, and everything hitting at once peaks under −1 dBFS. A test checks the targets.
- [ ] Every item on the tuning list (collected on this ticket during 1–10) is done, or answered with a reason.
- [ ] `examples/demo-beat.json` (kick, snare or clap, closed hats, an open hat choked by a closed one, a tom fill, a cymbal) and its golden WAV. Per-sound golden WAVs whose levels changed are regenerated. Will approves every golden file in the PR.
- [ ] A real-time test plays every sound at once, chokes, flams, and turns every control while playing, under `assert_no_alloc` and RealtimeSanitizer.
- [ ] The timing report's drum case, every sound at once, is reported at p50 and p99 in the PR.
- [ ] "For Will to try": manual checks 3 to 5 in the app.

**Out of scope:** New sounds or controls. A kit-wide Accent (RFC open question 5).

**Depends on:** 9 and 10, so every sound and the panel.

**Context:** RFC-006, "What makes it sound good" point 7, Risks ("Sounds good is a judgement", "Some figures come from secondary sources", "CPU"); "How we'll verify it" (the golden demo beat, Real-time safety, Timing, manual checks 3–5). Research: "Drum synthesis", section 8.

## Order and checkpoints

Two lines of work, after 1. The sounds go one at a time, because each waits for Will's ears. The window work runs beside them and needs no listening.

```
1 Kick 808 ──┬─► 2 Snare, clap ─► 3 Hats ─► 4 Toms ─► 5 Cymbal ──┐
             │                                                   ├─► 10 Kick 909 ─► 11 Balance
6 ⌥-drag ────┴─► 7 Drum tracks and lanes ───────────► 9 Panel ───┘
(Ready now)      (also UTA-43)                ┌──────►
8 Layout sketch ──────────────────────────────┘
(any time)
```

- **1 comes first** on the sound line, and 6 and 8 can start straight away beside it.
- **The sound line is a chain** (1 → 2 → 3 → 4 → 5 → 10), so no sound starts before Will has heard the one before. The code only needs 1 for most of them (the cymbal needs 3's metal bank), so if the cymbal needs more than two listening rounds, Will can let 10 start beside it.
- **The window line:** 6, then 7 (after 1 and UTA-43), then 9 (after 7 and 8). Rows for sounds that aren't built yet are silent until their ticket lands.
- **11 is last.** Things heard during 1–10 that need the whole kit go on its tuning list.

**Checkpoints for Will:**
- **After each sound ticket (1, 2, 3, 4, 5, 10):** manual check 1, by ear, from the PR's renders, before the next sound starts.
- **During 3:** manual check 2, the oversampling A/B.
- **After 7:** open a new project: Synth 1 and Drums 1. Click the labels, draw a beat on the sounds that exist, and hear it loop. ⌥-drag the hats along the bar.
- **After 8:** choose the panel's layout.
- **After 9:** manual check 4: turn the knobs while a beat plays. Nothing clicks or jumps.
- **After 11:** manual checks 3 and 5: make a beat in the app, tune the kick to a bassline, and judge the kit's balance at its defaults.

## Coverage

| RFC-006 verification item | Ticket |
|---|---|
| Pitch: kick and toms end at their Tune; kick starts higher; Tune raises every tuned sound | 1, 2, 3, 4, 5, 10 |
| Decay follows the control, doubling roughly doubles it | 1, 2, 3, 4, 5 |
| Brightness: kick under 200 Hz, hats in kHz, Tone moves it the right way | 1, 2, 3, 5 |
| Clap: three or four bursts about 10 ms apart | 2 |
| Choke: open hat quieter 60 ms after a closed hit | 3 |
| Velocity changes tone | 1, 2, 3, 4, 5, 10 |
| No clicks: hit, fast repeat, flam, knob turn | every sound ticket |
| Aliasing stays at or below the agreed figure | 3 (figure set), 5 |
| Determinism: identical renders, block sizes 32, 128, 1024 | every sound ticket |
| Golden WAV of `examples/demo-beat.json` | 11 |
| Per-sound golden renders | every sound ticket; re-approved in 11 |
| Real-time safety under `assert_no_alloc` and RealtimeSanitizer | every sound ticket; the whole kit in 11 |
| Timing report: every sound at once, p50 and p99 | 1 (case added), 11 (reported) |
| Core: new commands undo exactly and round-trip; a drag is one undo step | 1, 10 |
| Core: notes off the kit refused | 1 |
| Core: clip moves to the other kind refused | 7 |
| Core: format 3 command lists still load and render the same | 1 |
| UI: rows from the outline; label and name play the sound | 7 (labels), 9 (names) |
| UI: notes drawn and moved between rows send the expected commands | 7 |
| UI: ⌥-drag duplicates; resize resizes the selection | 6 (and on drum lanes in 7) |
| Manual 1: listen to each sound | every sound ticket |
| Manual 2: the oversampling A/B | 3 |
| Manual 3: make a beat in the app | 11 |
| Manual 4: turn knobs while it plays | 9 |
| Manual 5: the kit's balance | 11 |
| Proposal: the kit and its controls | 1, 2, 3, 4, 5, 10 |
| Proposal: what makes it sound good, points 1–6 | the shared sound criteria; 3 (point 4) |
| Proposal: point 7, a balanced default kit | 11 |
| Proposal: in the engine | 1 |
| Proposal: in the project | 1 (kind, command, format, notes), 7 (outline, clip moves, new tracks, paste) |
| Proposal: in the window | 6, 7, 9 |
| Proposal: the drum panel's controls and layout | 8, 9, 10 (Model) |
| Resolved open questions 1–6 | 10 (1), 3 (2), 1 (3), the controls in 1–5 (4), 11's out of scope (5), 8 and 9 (6) |
| The RFC's new terms | in the Glossary since the RFC was accepted |

## Not in this project

- **Everything RFC-006 leaves out:** swing; more sounds (mid tom, rimshot, cowbell), a model per row, or samples; the 808 snare; a kit-wide Accent knob; separate outputs, panning or effects per sound; a step-sequencer view; kit presets; building or changing controls; recording and the metronome.
- **⌥-drag and group resize for clips in the timeline.** The scratchpad notes are about notes, and the RFC folds in only the piano roll.
- **Scratchpad notes added since the 2026-10-10 review:** three, none about drums, so I'd keep them Raw for the review at the end of the Drafting table project:
  - move the output, buffer, dropouts, slowest block and frame readouts out of the way (UTA-42);
  - canvases flash while the window is resized (UTA-42);
  - remove the dropped millimetre grid from `DESIGN.md` and RFC-005 (UTA-40).
- **The open Design decisions** (20 of them) are reviewed at the end of the Drafting table project, after UTA-43, as its plan says, not here.

The two scratchpad notes the RFC folded in (⌥-drag duplicates, resize the selection together) are Folded in and related to this project.

## Open questions

1. **Resolved: as recommended (Will, 2026-10-10).** **The order of the sounds.** Kick, then snare and clap, hats, toms, cymbal, and the 909 kick last. That gets the sounds of the RFC's first beat (kick, clap, hats) in front of you first, leaves the cymbal (riskiest) and the 909 kick (first to cut) at the end, and lets the toms reuse the kick's resonator while the hats' metal bank is fresh for the cymbal. Recommendation: this order.
2. **Resolved: as recommended (Will, 2026-10-10).** **Gate the sounds by dependencies?** Making each sound ticket depend on the one before is what stops two sounds being built before you've heard the first, since only `Ready` tickets with merged dependencies can start. The cost is a long chain, so a slow cymbal would hold up the 909 kick. Recommendation: the chain, with you free to let 10 start beside 5 if the cymbal needs more than two rounds.
3. **Resolved: as recommended (Will, 2026-10-10).** **The layout sketch as a Spike ticket?** It produces no PR, since it lives in the private design repo. As a ticket it has an ID that 9 can depend on, and a place for your choice and the screenshots. Recommendation: a Spike ticket, as 8.
4. **Resolved: as recommended (Will, 2026-10-10).** **Ticket 6 doesn't wait for UTA-43.** It changes how the piano roll edits, not how it looks, so it shouldn't collide much with UTA-43's extraction, and it can start now. 7 and 9 wait for UTA-43. Recommendation: as planned.
