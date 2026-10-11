# Project: Effect chain and Delay

**Status:** Created · **RFC:** [RFC-007](../rfcs/rfc-007-effect-chain-delay.md) · **Notion:** https://app.notion.com/p/Effect-chain-and-Delay-3f63af969b6f819bb641cd528700049e

Planned together with [Synth features](synth-features.md) (RFC-008), which runs alongside it. "Shared with Synth features" below names the one owner of each piece the two projects share.

## Goal

Build RFC-007. Every track and the master has a chain of up to eight insert effects, shown on the Sound tab after the synth or kit, and the first effect is a tape-style Delay that syncs to the tempo, ping-pongs, darkens, saturates and wobbles its echoes, and rings out after Stop. Every change can be undone. The patterns the next five effects will reuse (how an effect is stored, how its memory reaches the audio thread, how it rings out and sleeps, and how it's tested) are settled and written down, so a later effect is a new kind, not a change to the chain.

## Where the code starts

Checked against `main` at `2560c2d`. Drums is Done, and no ticket is open.

- **The model is ready for a chain.** `Track` already has `effects: Vec<Effect>` ([track.rs:21](../../crates/uta-core/src/track.rs)), always empty, and `Effect` is an empty enum ([track.rs:202](../../crates/uta-core/src/track.rs)). The master has only a volume.
- **Commands** are at `COMMAND_FORMAT = 4` ([command.rs:18](../../crates/uta-core/src/command.rs)). `SetSynthParam` and `SetDrumParam` are the models for `SetDelayParam`.
- **A track is mono until the pan stage.** Each `Slot` has one buffer, `[f32; TRACK_BUFFER_FRAMES]` ([processor.rs:105](../../crates/uta-engine/src/processor.rs)), and `MixerStrip::gains` turns pan into a left and right gain ([mixer.rs:34](../../crates/uta-engine/src/mixer.rs)). The mix is already stereo.
- **Tails already exist for sources.** `TrackSound::tail_seconds` ([snapshot.rs](../../crates/uta-engine/src/snapshot.rs)) keeps a slot from being handed on until a removed track's release or drum ring has died away. A chain's tail extends that.
- **Nothing is handed to the audio thread except snapshots.** Used snapshots come back through an `rtrb` queue. Effect state is the first thing besides a snapshot to cross.
- **Rendering stops at the song's end** (`default_seconds` in [main.rs:147](../../crates/uta-cli/src/main.rs)), so it would cut off echoes.
- **The tempo** reaches the audio thread in the snapshot's `Sequence` (a `TempoMap`). There's no note-division type yet.
- **The UI.** The Sound tab shows `SynthPanel` or `DrumPanel` ([App.tsx](../../app/src/App.tsx)). The knob, fader, menu and segmented choice are in `app/src/design/`. Drum settings reach the UI with labels, ranges, defaults and units (`DrumSettingView`, [uta.rs:244](../../app/src-tauri/src/uta.rs)); the synth's with a fixed `SynthView` and `SynthLimits`. Synth features ticket 1 turns both into one shape that the Delay uses too.
- **Tests.** `tests/common` has `max_jump` (the click check), `spectral_centroid`, `rms`, `peak` and pitch measurement. `realtime.rs` drives the process path under `assert_no_alloc`, and `timing.rs` has the timing report and its baseline.
- **Research and glossary.** Notion's Research page "Delays and effect chains" is related to RFC-007. RFC-007's 17 terms are already in the Glossary.

## Tickets

8 tickets: 6 PRs and 2 that are mostly listening and sketching. Two build the chain's engine, one the Delay's sound, three put the chain in the window, and one tunes it by ear in a real song.

### 1. [UTA-65] Make a track stereo from the chain onwards, with pan as a balance (Chore)

**Goal:** Each track's working buffer is left and right from where the chain will sit, and pan becomes a balance on that stereo sound. Nobody can hear the difference: this is the ground the chain and Synth features both build on, done once.

**Acceptance criteria**
- [ ] Each slot's buffer is stereo, preallocated at stream setup. The synth or kit's one channel goes into both sides.
- [ ] Pan is a balance: it turns one side down, using today's pan-law gains (centre unchanged; hard pan is one side 3 dB up and the other silent).
- [ ] Every existing golden WAV passes without being regenerated, bit for bit (a change is a bug, not a new golden).
- [ ] A test pans a stereo signal with different left and right content and checks the balance turns down only the far side.
- [ ] `realtime.rs` and RealtimeSanitizer still pass, and the timing report shows the cost of the second channel.

**Out of scope:** The chain itself (2). Anything the synth puts into the stereo buffer beyond its one channel (Synth features keeps the synth mono).

**Depends on:** Nothing.

**Context:** RFC-007, Proposal: "The chain becomes stereo"; "Shared with RFC-008", first bullet. Risks: "The stereo change touches every track's audio path".

### 2. [UTA-68] Add a Delay to a track from a command list (Feature)

**Goal:** The thin slice through every layer. A command list can add a Delay to a track's chain, and `uta render` plays its echoes, including after the song ends. The Delay is still plain: a time in ms, Feedback and Mix. What this ticket settles is the pattern every effect uses: how it's stored and undone, and how its memory is made off the audio thread, handed over and given back.

**Acceptance criteria**
- [ ] Core: an effect is its permanent ID, kind (Delay), settings, on/off and Mix. A chain sits on a track or the master and holds up to eight. Commands `AddEffect` (at a position), `RemoveEffects`, `SetEffectMix` and `SetDelayParam`, each undoing exactly; a knob drag is one undo step; a ninth effect is refused; removing a track removes its chain and undo restores both. The format goes up, and format 4 lists still load and render the same.
- [ ] Core: the Delay's settings are described with the shape from Synth features ticket 1 (label, unit, range, default, how it sounds), and the outline carries each chain.
- [ ] Engine: the snapshot carries each effect as plain numbers. When it brings an effect the audio thread hasn't seen, the control side makes its state first, sized for the sample rate, and sends it with the snapshot. The audio thread keeps states in a fixed table found by effect ID, and returns a removed effect's state through a fixed-size queue. Every effect implements the same five things (prepare, process a stereo block in place, glide to new settings, say if it's ringing, clear).
- [ ] The Delay: a delay line of 5 s per side (plus room for Wobble later), read with cubic interpolation, with feedback and an equal-power Mix. Tests: a click comes back at exactly the right sample at 44.1, 48 and 96 kHz; each echo is Feedback times the one before; Mix 0 matches no effect bit for bit.
- [ ] Tails: after Stop the track keeps sounding until the echoes die away, and `uta render` runs past the song's end until every chain is silent, up to 30 s, saying so when it hits the limit.
- [ ] Real-time safe: a render that adds and removes Delays and swaps snapshots runs under `assert_no_alloc` and RealtimeSanitizer, and every state handed over comes back. `CLAUDE.md`'s audio thread rules gain the one about effect state.

**Out of scope:** On/off, moving, the add/remove/move fades, sleeping and the master's chain (3). Sync, ping-pong, the cuts, Drive and Wobble (4). Anything in the window (6).

**Depends on:** 1, and Synth features ticket 1 (the settings shape).

**Context:** RFC-007, Proposal: "The chain", "The Delay" (tape loop, interpolation, feedback), "Stopping, switching off, and tails" (Stop, Rendering), "Patterns for every effect after this one" (1 to 4, 6, 7); "How we'll verify it" (On time, Feedback, Rendering includes the tail, Real-time safety, Project core). Research: "Delays and effect chains".

### 3. [UTA-69] Switch, move and fade effects, let them sleep, and give the master a chain (Feature)

**Goal:** The chain behaves the way RFC-007 describes everywhere an effect is added, removed, moved, switched off or left alone, on tracks and on the master. The effect test kit is finished here, written once against any effect, so the next effect gets it by adding itself to a list.

**Acceptance criteria**
- [ ] Core: `MoveEffect` (within its chain) and `SetEffectEnabled`, each undoing exactly. The master's chain takes every chain command, and "the master" is a chain address alongside "track n".
- [ ] Engine: switching off fades to the dry sound over 10 ms and then does no work, and switching on starts from silence; adding fades in over 10 ms, removing fades out over 10 ms before the state goes back, and moving fades out and back in while keeping its echoes. Removing a track fades its echoes out quickly.
- [ ] Sleeping: a Delay whose output has been below −120 dB for a little longer than its time stops working until sound arrives, and a track with no notes and a sleeping chain costs nothing. A test plays a long, quiet tail out before it sleeps.
- [ ] A change of sample rate rebuilds every state on the control side, in the step that already re-prepares the synth and kit.
- [ ] The effect test kit, run for every kind in a list: off and Mix 0 match no effect bit for bit; adding, removing, moving and switching while playing stay under the click limit; blocks of 32, 128 and 1024 give the same audio; tails ring out and then sleep. Denormals are flushed in the chain.
- [ ] Real-time safe: a render that adds, removes, moves and switches effects on tracks and the master, swaps snapshots and changes the sample rate runs under `assert_no_alloc` and RealtimeSanitizer.

**Out of scope:** The Delay's tape loop (4). The window (6). Moving an effect to another chain.

**Depends on:** 2.

**Context:** RFC-007, Proposal: "The chain", "Stopping, switching off, and tails", "Patterns for every effect after this one" (5 to 8); "How we'll verify it" (the effect test kit, Tails). Open questions 2, 3 and 5.

### 4. [UTA-70] Sync the Delay to the tempo and give it its tape loop (Feature)

**Goal:** The Delay sounds like RFC-007's tape echo: synced or free time, stereo or ping-pong, repitching when its time moves, with the cuts, saturation and wobble in its feedback loop. Uta has its first note-division type, which the synth's LFO will use too.

**Acceptance criteria**
- [ ] Core: a note-division type (1/32 to 1/1, each straight, dotted or triplet: eighteen choices) with its length at a tempo, defined once for any setting that syncs. The Delay's Sync, synced Time and free Time (1 ms to 5 s); a synced time longer than 5 s is held at 5 s.
- [ ] Time changes and tempo changes repitch: the read head glides, never jumps. Tests: changing the time while playing stays under the click limit, and the echo lands at the new time once the glide is over; a click comes back on time when synced, at two tempos.
- [ ] The loop: Low cut and High cut inside the feedback path, then a saturation that's always there with Drive setting how hard it's hit; Feedback up to 110%. Tests: each echo's spectral centroid falls with the High cut on, the Low cut thins each one, and at 110% with full Drive the output settles below full scale and stays there.
- [ ] Ping-pong sums the input into the left tape and crosses each echo to the other side. Test: a mono input's echoes alternate left and right.
- [ ] Wobble (wow and flutter), deterministic and restarted on Play; at 0 the Delay is exactly on time. Tests: two renders are identical, and Wobble moves the echo's timing by about the amount set.
- [ ] A golden WAV from `examples/demo-delay.json` (a Delay on a synth track and one on the master), which Will approves. Listening renders in `examples/listen/delay/`: a stab through each Mode, a High cut sweep, Drive and Wobble at a few settings, and Feedback at 100% and 110%, with what to listen for. The timing report gets a case with a Delay on every track and the master, and its baseline is re-recorded.

**Out of scope:** Freeze, hiss, separate left and right times, Fade or Jump modes, ducking, tap tempo. Tuning the defaults in a real song (8).

**Depends on:** 2. (It can run alongside 3.)

**Context:** RFC-007, Proposal: "The Delay" (the table and every bullet under "How each part works"); "Shared with RFC-008" (note divisions); "How we'll verify it" (the Delay's own tests, the golden WAV, Timing). Open questions 1 and 4. Research: "Delays and effect chains".

### 5. [UTA-66] Sketch the chain row, the effect panel frame and the Delay panel (Spike, one session)

**Goal:** Before any chain UI is built, two or three ways of drawing the Sound tab as a row are sketched in the design sandbox: the frame every effect panel shares (title bar, on/off key, drag handle), the **+**, sideways scrolling, the master's header row, and the Delay's panel, including whether it draws its echoes. Will picks one. Synth features' panel sketch (Synth features 8) is done beside the frame chosen here.

**Acceptance criteria**
- [ ] Two or three sketches in the design sandbox, each showing the synth panel, a Delay, a second effect and the **+** at a normal window width, in light and dark.
- [ ] Each follows RFC-005's control rule: Time a menu when synced and a knob in ms, Sync a toggle, Mode a segmented choice, the rest knobs.
- [ ] Will chooses one, and the reasons go in the frame's and the panel's Design decisions entries (provisional, logged with `scripts/ticket decision new`).

**Out of scope:** Building any of it (6, 7). The synth panel's own layout (Synth features 8).

**Depends on:** Nothing. Timebox: one session.

**Context:** RFC-007, Proposal: "In the window"; "Shared with RFC-008" (the Sound tab). `DESIGN.md`, "Draw the sound".

### 6. [UTA-75] Put the chain on the Sound tab, and the master below the tracks (Feature)

**Goal:** You can do everything ticket 3 made possible from the window: the Sound tab shows the source panel, then each effect in the chosen frame, then a **+**. You add, drag, switch off and delete effects, and ⌘Z undoes each. A Master row below the tracks opens the master's chain. The Delay's controls are built from Rust's description of its settings, so the panel works before ticket 7 lays it out.

**Acceptance criteria**
- [ ] The Sound tab is a row, scrolling sideways when wider than the window: the synth or kit panel, each effect panel in the frame from 5, then **+**.
- [ ] **+** lists the kinds of effect from Rust (just Delay), adds one at the end of the chain, and is greyed out at eight.
- [ ] Each panel's on/off key, drag handle (sends a move) and Mix send their commands; a selected panel is removed with Delete and comes back with ⌘Z.
- [ ] A Master header row below the tracks, with today's master volume and meter. Selecting it shows the master's chain with no source panel, and the Notes tab says there's nothing to show.
- [ ] UI tests against the mocked back end: the chain draws from the outline; **+** adds a Delay; dragging sends a move; Delete removes the selected panel; the on/off key sends its command; selecting Master shows the master's chain.
- [ ] New design decisions are marked `provisional: D-n` and listed in the PR, and the result is checked in the real app.

**Out of scope:** The Delay panel's own layout and drawing (7). Changing the synth panel (Synth features 9).

**Depends on:** 3, 5.

**Context:** RFC-007, Proposal: "In the window"; "How we'll verify it" (UI).

**Will can try it:** add, drag, switch and delete Delays on tracks and the master while a loop plays, and undo each.

### 7. [UTA-76] Lay out the Delay panel (Feature)

**Goal:** The Delay's panel is the one Will chose in 5: Time as a menu or a knob with Sync beside it, Mode as a segmented choice, knobs for the rest, and a drawing if the sketch kept one.

**Acceptance criteria**
- [ ] The panel follows the chosen sketch and RFC-005's control rule, built from Rust's description of the Delay's settings, with no range or default in the UI.
- [ ] Synced Time is a menu of the eighteen divisions from Rust; switching Sync swaps the menu and the ms knob.
- [ ] If the sketch has a drawing (the echoes as fading ticks, say), it follows a drag as it happens.
- [ ] UI tests: each control sends the right command; the menu lists Rust's divisions; the drawing follows the settings.
- [ ] Checked in the real app beside the synth and kit panels, in light and dark.

**Out of scope:** Tuning the defaults (8).

**Depends on:** 4, 6.

**Context:** RFC-007, Proposal: "In the window" (the Delay panel); `DESIGN.md`, "Draw the sound".

### 8. [UTA-77] Tune the Delay by ear in a real song (Feature)

**Goal:** Will makes a loop with a Delay on the synth and the hats, then one on the master, and it sounds like a record, not a demo. What he asks for is tuned here: the defaults, the Drive curve, the Wobble's rates and depths, and Feedback's top end.

**Acceptance criteria**
- [ ] A listening session from RFC-007's manual steps 1 to 6, with the Delay compared against Ableton's Echo on the same sounds. Will's notes are in the PR.
- [ ] The changes he asks for are made, each with its reason in the code; any golden that changes is re-approved.
- [ ] If 110% feedback is still too easy to make painful, the top of the range comes down (RFC-007 risk).
- [ ] The master's level: Will decides whether the −12 dB master default stays, given the master's chain sees the mix 12 dB hotter (the scratchpad note from UTA-57, folded in here).

**Out of scope:** New Delay features.

**Depends on:** 7.

**Context:** RFC-007, "Risks & unknowns" ("Sounds like tape" is your judgement, Runaway, Effects on the master); "How we'll verify it", Manual 1 to 6.

**Will can try it:** the whole project.

## Order and checkpoints

```
1 stereo ─────────┐
Synth features 1 ─┴─► 2 Delay slice ─┬─► 3 chain behaviour ─┐
                                     └─► 4 tape loop ───────┼──────────────┐
5 sketch ───────────────────────────────────────────────────┴─► 6 chain UI ┴─► 7 Delay panel ──► 8 tune

4 also unblocks Synth features 6 (the note division); 5 unblocks Synth features 8.
```

- **Start in parallel:** 1 (stereo), 5 (sketch), and Synth features 1 (settings shape). 2 waits for 1 and Synth 1.
- **After 2:** 3 and 4 in parallel.
- **Checkpoints for Will:**
  - after 4: the listening renders and the golden (from a command list, no UI yet);
  - after 5: choose the sketch;
  - after 6: use the chain in the app;
  - 8 is the listening session.

## Shared with Synth features

Each shared piece is built once, in the ticket named here. The [Synth features](synth-features.md) plan has the same table.

| Shared piece | Built in | Used by |
|---|---|---|
| A track's stereo working buffer, pan as balance | Effect chain 1 | The chain. Synth features keeps the synth mono and only fills the buffer as 1 defines. |
| Settings described from Rust in one shape (label, unit, range, scale, default, choices, how it sounds) | Synth features 1 | Effect chain 2 (Delay settings), 6 and 7 (panels built from it); Synth features 2 to 10 |
| Note-division type and its menu of eighteen | Effect chain 4 | Synth features 6 (synced LFO rate) |
| The Sound tab as a row of panels, and the effect frame | Effect chain 5 (sketch), 6 (built) | Synth features 8 (sketched beside it), 9 (checked beside a Delay if 6 has landed; not a hard dependency) |
| Command format number | Whichever merges first: Effect chain 2 or Synth features 2 | The second takes the next number and converts from the first |
| Timing baseline | Each engine ticket re-records it | A clash in `timing-baseline.tsv` is only textual: re-record after rebasing |

## Coverage

| RFC-007 verification item | Ticket |
|---|---|
| Kit: off and Mix 0 change nothing | 2 (Mix 0), 3 (off, in the kit) |
| Kit: no clicks from add, remove, move, on/off | 3 |
| Kit: same audio at 32, 128, 1024 | 3 |
| Kit: real-time safety, every state comes back | 2, 3 |
| Kit: tails ring out, then sleep | 2 (ring out), 3 (sleep) |
| On time (ms at three rates; synced at two tempos) | 2 (ms), 4 (synced) |
| Feedback | 2 |
| The loop's cuts | 4 |
| Ping-pong | 4 |
| Repitch | 4 |
| Bounded runaway | 4 |
| Wobble is repeatable | 4 |
| Rendering includes the tail, 30 s limit | 2 |
| Golden WAV `demo-delay.json` | 4 |
| Existing goldens unchanged | 1 (and every ticket after) |
| Timing report case | 4 (1 reports the stereo cost) |
| Project core: undo, round-trip, one-step drag, ninth refused, track removal, old lists | 2, 3 |
| UI against mocked back end | 6, 7 |
| Manual 1: listen to renders, compare with Echo | 4 (renders), 8 (session) |
| Manual 2: turn every knob, especially Time | 7, 8 |
| Manual 3: a loop with Delays on synth, hats, master | 8 |
| Manual 4: Stop mid-echo, off mid-echo | 6, 8 |
| Manual 5: add, reorder, delete while playing, undo | 6 |
| Manual 6: tempo change with a synced Delay | 8 |

## Not in this project

Everything in RFC-007's "Not in this project": sends, returns, buses and groups; the other five effects; more Delay features; presets and copying effects; automation; effects on individual drum sounds and a mixer view; third-party plugins; echoes of notes before a mid-song start.

## Open questions

1. **The scratchpad note "Start a new project's master at 0 dB instead of −12 dB"** (from UTA-57). RFC-007 warns that the master's chain sees the mix 12 dB hotter than you hear it, so the Delay's saturation bites sooner there. Recommendation: fold it into ticket 8 as a decision you make by ear with a Delay on the master, rather than its own project. If you'd rather it stay separate, keep it Raw. **Resolved: fold it into ticket 8 (Will, 2026-10-11).**
2. **One design session or two?** Ticket 5 (chain and Delay) and Synth features 8 (synth panel) could be one sandbox session, since the synth panel has to fit beside the Delay. Recommendation: two, with 5 first. The synth panel is the densest yet and deserves its own round, and it's easier to sketch against a frame that's already chosen. **Resolved: two sessions, 5 first (Will, 2026-10-11).**
