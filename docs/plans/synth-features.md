# Project: Synth features

**Status:** Created · **RFC:** [RFC-008](../rfcs/rfc-008-synth-features.md) · **Notion:** https://app.notion.com/p/Synth-features-3f63af969b6f815f964dd809f44eef34

Planned together with [Effect chain and Delay](effect-chain-delay.md) (RFC-007), which runs alongside it. "Shared with Effect chain and Delay" below names the one owner of each piece the two projects share.

## Goal

Build RFC-008. The synth becomes a compact, Drift-sized instrument: two oscillators, a sub and noise, pulse width and hard sync, a filter with three modes, its own envelope and key tracking, one LFO, three mod slots, Mono with legato and glide, and about eight factory sounds made from written briefs. The default sound and every existing golden are unchanged. The panel is redesigned from `DESIGN.md`'s principles and built from Rust's description of each setting, and a voicing guide records how briefs became settings.

## Where the code starts

Checked against `main` at `2560c2d`. Drums is Done, and no ticket is open.

- **Seven settings.** `SynthSettings` ([synth.rs:25](../../crates/uta-core/src/synth.rs)) has a waveform (sine, triangle, saw, square), cutoff, resonance and one ADSR. It's `Copy` and `deny_unknown_fields`, and `AddTracks` carries it whole, so older command lists need converting when it grows. `SetSynthParam` sets one; a drag is one undo step.
- **Commands** are at `COMMAND_FORMAT = 4` ([command.rs:18](../../crates/uta-core/src/command.rs)).
- **The engine's synth** ([synth/mod.rs](../../crates/uta-engine/src/synth/mod.rs)) has 16 voices of one PolyBLEP oscillator and one envelope, and one set of filter settings shared by every voice, gliding through `Ramp`s. `filter.rs` is a state-variable filter that already works out low-, band- and high-pass. Each slot chases notes on Play (`Slot::chase`, [processor.rs:219](../../crates/uta-engine/src/processor.rs)).
- **Settings reach the UI two ways.** The drums send each setting with its label, limits, default and unit (`DrumSettingView`, [uta.rs:244](../../app/src-tauri/src/uta.rs)), with the labels and units worked out in the app layer. The synth sends a fixed `SynthView` and `SynthLimits` ([uta.rs:404](../../app/src-tauri/src/uta.rs)), and `SynthPanel.tsx` names each field by hand.
- **The panel** is D-22 (knobs in a row under each drawing) and D-10 (framed drawings), with the filter curve and envelope shape in `app/src/synth/`. The knob, fader, menu and segmented choice are in `app/src/design/`.
- **Tests.** `tests/synth.rs` already measures aliasing (`loudest_alias_db`). `tests/common` has pitch measurement, `spectral_centroid`, `rms`, `peak` and `max_jump`. The drums' seeded noise is the model for the synth's.
- **Research and glossary.** Notion's Research page "Synth features: DSP, layouts and modulation" is related to RFC-008. RFC-008's 24 terms are already in the Glossary.

## Tickets

11 tickets: 9 PRs, a design spike and a tuning pass. One shared chore, four build the voice, two add movement, two make the panel, and two make and tune the factory sounds. The listening checkpoint comes after ticket 5.

**Every engine ticket (2 to 7) also has these criteria,** written out in full on each Notion ticket:
- [ ] **The default is unchanged:** a new synth track plays today's sound, and the existing golden WAVs pass without being regenerated.
- [ ] **No clicks:** turning each new control while a note plays stays under the click limit.
- [ ] **Deterministic:** rendering twice gives identical audio, and blocks of 32, 128 and 1024 give the same audio.
- [ ] **Real-time safe:** the new features, turned while playing, run in `realtime.rs` under `assert_no_alloc`, and the process path under RealtimeSanitizer.
- [ ] **Timing:** the timing report's full-voice case (from 2) includes the new feature, and the baseline is re-recorded.
- [ ] **Renders for Will's ears:** committed command lists in `examples/listen/synth/` for the feature, with what to listen for in the PR.
- [ ] **Reachable before the panel:** each new setting is described from Rust, set by `SetSynthParam` and usable from a command list. The current panel keeps working; the new panel is ticket 9.

### 1. [UTA-67] Describe every setting from Rust in one shape (Chore)

**Goal:** The synth's and the drums' settings reach the UI the same way: each with its label, unit, range, scale, default, choices (for a wave or a mode) and a sentence on how it sounds, defined in `uta-core` next to the ranges. The panels draw from that list. Nothing looks different, but the synth's 40 settings and every effect's can now be added without the UI naming a field.

**Acceptance criteria**
- [ ] One description of a setting in `uta-core`, used by the synth's seven settings and every drum setting. The labels and units move out of the app layer.
- [ ] Each description has a "how it sounds" sentence. The synth's seven are written here; the drums' are filled in too.
- [ ] The outline sends the synth's settings as a list of descriptions, as it already does for the drums, and `synthLimits` and `synthDefaults` go.
- [ ] `SynthPanel` and `DrumPanel` draw from the descriptions, with no range, default or label in TypeScript. They look and behave as before.
- [ ] Tests: every setting has a description and a unit (the outline test fails if one is missing); the UI tests for both panels pass against the new outline.

**Out of scope:** Any new setting (ticket 2 onwards). Showing the "how it sounds" sentence in the panel (RFC-008 open question 3: not in this project).

**Depends on:** Nothing.

**Context:** RFC-008, Proposal: "In the project" (the last bullet), "Factory sounds" (point 1). RFC-007, "Patterns for every effect after this one", point 2. Shared with Effect chain 2, 6 and 7.

### 2. [UTA-71] Add oscillator 2, the sub, noise and a mixer (Feature)

**Goal:** A synth track can layer a second oscillator (its own wave, octave and pitch offset), a square sub an octave or two down, and white noise, each with its own level, and its oscillators can run free between notes. Detuning oscillator 2 a few cents thickens the saw.

**Acceptance criteria** (plus the shared ones above)
- [ ] Core: the settings grow by Oscillator 1's Octave; Oscillator 2's Wave, Octave and Pitch (±12 semitones, fine near zero); Sub Octave (−1 or −2); a level each for Osc 1, Osc 2, Sub and Noise; and Osc retrigger. Square becomes Pulse. The format goes up, and format 4 lists convert (missing settings take today's defaults, Square becomes Pulse) and render the same.
- [ ] Engine: oscillator 2 and the sub (flipping on oscillator 1's wraps, with its PolyBLEP correction) per voice; one seeded noise generator per voice, restarted on Play; free-running oscillators when Osc retrigger is off, restarted on Play. The mixer's levels are scaled so a typical mix is close to today's level.
- [ ] Tests: oscillator 2's and the sub's measured pitches match their Octave and Pitch; the sub is one or two octaves under oscillator 1; noise's spectrum is flat and its level follows the mixer; with Osc retrigger off two identical notes differ, yet two renders are identical.
- [ ] The timing report gets a 16-voice full-voice case and a 32-tracks-of-8 case, which grow with each ticket after.

**Out of scope:** Pulse width and sync (3). Unison, drift and stereo spread (the later character RFC).

**Depends on:** 1.

**Context:** RFC-008, Proposal: "The instrument" (Oscillator 1 and 2 without Width and Sync, Sub, Noise, Mixer, Voice's Osc retrigger), "In the engine" (Sub, Noise, Free-running), "In the project"; "How we'll verify it" (Oscillator 2 and the sub, Noise, Free-running, Determinism). Research: "Synth features".

### 3. [UTA-72] Add pulse width and hard sync (Feature)

**Goal:** Oscillator 1's pulse has a Width, from square to a thin buzz, and oscillator 2 can sync to oscillator 1, so sweeping its Pitch tears like a sync lead, with aliasing kept to the agreed figure.

**Acceptance criteria** (plus the shared ones above)
- [ ] Width (5% to 95%), built as one saw minus another shifted by the width, so it never drifts off centre.
- [ ] Sync: oscillator 2 restarts where oscillator 1 wraps, with a PolyBLEP correction sized by the jump, and PolyBLAMP on triangle and sine under sync.
- [ ] Tests: Width 50% matches today's square; at 25% every fourth harmonic is missing; no DC offset at any width; under sync the output's pitch follows oscillator 1 whatever oscillator 2's Pitch; the aliasing in a sync sweep stays at or below a figure measured here and written into the test.
- [ ] The sync A/B: a sync sweep made two ways (as built, and oversampled), labelled only A and B, with the key held back until Will answers. If he can hear a difference, the synced oscillator is oversampled.

**Out of scope:** Moving Width from an LFO (6) or a mod slot (7).

**Depends on:** 2.

**Context:** RFC-008, Proposal: "The instrument" (Width, Sync), "In the engine" (Width, Sync); "How we'll verify it" (Width, Sync; Manual 2). Risks: "Sync aliasing at extreme settings".

### 4. [UTA-73] Give the filter its modes, its own envelope and key tracking (Feature)

**Goal:** The filter can be low-, band- or high-pass, and has its own ADSR with a ± Env amount and Key tracking. A pluck is now possible: a short filter decay with Env amount up. Each voice works out its own filter settings when something moves them.

**Acceptance criteria** (plus the shared ones above)
- [ ] Core: Filter Mode, Env amount (±), Key, and the filter envelope's Attack, Decay, Sustain and Release.
- [ ] Engine: per-voice filter settings worked out about every 16 samples and blended in between, only when something moves the cutoff per voice. Otherwise the voices share one set as today, which keeps the default identical.
- [ ] Tests: each mode passes and cuts where it should; Env amount moves the measured brightness over a note's life in the right direction and zero does nothing; Key makes higher notes as bright as lower ones.
- [ ] The timing report shows the per-voice filter's cost. If it's well over RFC-008's guess, the PR says so and proposes the lookup table.

**Out of scope:** The filter's LFO amount (6). A second filter, a nonlinear or 24 dB filter.

**Depends on:** 2.

**Context:** RFC-008, Proposal: "The instrument" (Filter, Filter envelope), "In the engine" (the filter's settings become per voice); "How we'll verify it" (Filter). Risks: "CPU per voice goes up".

### 5. [UTA-74] Add Mono mode with legato and glide (Feature)

**Goal:** A synth track can play one note at a time: the latest note wins, letting go returns to one still held, Glide slides between overlapping notes, and with Legato the envelopes carry on. A note after silence always starts fresh, and no note can stick.

**Acceptance criteria** (plus the shared ones above)
- [ ] Core: Poly/Mono, Legato and Glide.
- [ ] Engine: a fixed array of up to 16 held notes as plain values (key, pitch, velocity); glide as a ramp on the voice's pitch. Notes from the sequencer still go through today's bookkeeping.
- [ ] Tests: one voice sounds; the latest note wins; letting go returns to a held note; with Legato the envelope doesn't restart; the glide reaches the new pitch in the set time; a note after silence doesn't glide.
- [ ] Stuck notes, by name: chasing on Play, editing a note while it sounds, deleting the held note, and going round the loop each leave no note sounding in Mono, with and without Legato.

**Out of scope:** Pitch bend and MIDI input.

**Depends on:** 2. (It can run alongside 3 and 4.)

**Context:** RFC-008, Proposal: "The instrument" (Voice), "In the engine" (Mono mode); "How we'll verify it" (Mono, legato, glide). Risks: "Mono mode meets the note bookkeeping".

**Will can try it (the listening checkpoint):** after 3, 4 and 5 have merged, a set of renders from `examples/listen/synth/`: two detuned oscillators, the sub, noise, filter-envelope plucks, a PWM tone, a sync sweep and a mono glide bassline. Will says whether the voice is worth building movement on before 6 starts.

### 6. [UTA-78] Add the LFO (Feature)

**Goal:** The synth has one LFO per voice: five shapes, a rate in Hz or synced to a note division, Retrigger, and fixed routings to vibrato, pulse width and the filter. A synced LFO lines up with the bars wherever playback starts.

**Acceptance criteria** (plus the shared ones above)
- [ ] Core: Shape (sine, triangle, saw, square, sample-and-hold), Rate (Hz), Sync with a synced rate in RFC-007's note-division type, Retrigger, Vibrato, Width and the filter's LFO amount.
- [ ] Engine: one LFO per voice. With Retrigger on, each note starts its own; off, the voices share one phase, restarted on Play. A synced LFO takes its phase from the song's position, which reaches the slot here. Square and sample-and-hold are smoothed over a few milliseconds; sample-and-hold uses the voice's seeded generator.
- [ ] Tests: vibrato's measured pitch wobble has the set rate and depth; a synced LFO's cycle is exactly the note length at the tempo and lines up with the bar from a mid-song start; square and sample-and-hold stay under the click limit; Width and the LFO amount move what they name.

**Out of scope:** A second LFO, an LFO fade-in.

**Depends on:** 3, 4, and Effect chain 4 (the note-division type).

**Context:** RFC-008, Proposal: "The instrument" (LFO), "In the engine" (Noise and sample-and-hold, the LFO's shared phase, Everything glides); "Where this meets RFC-007", point 5; "How we'll verify it" (LFO).

### 7. [UTA-79] Add three mod slots (Feature)

**Goal:** Three Source → Destination slots with a ± Amount cover what the fixed knobs don't: velocity to cutoff, the filter envelope to oscillator 2's pitch for the sync sweep, the LFO to volume for tremolo. With this every synth feature exists.

**Acceptance criteria** (plus the shared ones above)
- [ ] Core: three slots, each a Source (filter envelope, amp envelope, LFO, velocity, key), a Destination (oscillator 1 and 2 pitch, each mixer level, width, cutoff, resonance, LFO rate, volume) and an Amount, set with `SetSynthParam`. An empty slot does nothing.
- [ ] Engine: modulation applied per voice at control rate, on top of each setting's knob value and the fixed routings.
- [ ] Tests: each destination moves the thing it names in the right direction, with the amount's sign; amount zero does nothing.
- [ ] Real-time safe: a render with every feature on, mono and poly, every knob turned while playing, runs under `assert_no_alloc` and RealtimeSanitizer.

**Out of scope:** Drag-to-modulate, showing modulation on the knobs it moves, a full mod matrix.

**Depends on:** 5, 6.

**Context:** RFC-008, Proposal: "The instrument" (Mod); "How we'll verify it" (Mod slots, Real-time safety, Timing).

### 8. [UTA-80] Sketch the synth panel beside a Delay (Spike, one session)

**Goal:** Two or three layouts for the synth's 40 controls are sketched in the design sandbox, left to right like the signal: Sound menu, oscillators, mixer, filter, envelopes, LFO, voice, with the mod slots behind a Mod section. Each sits in the Sound tab's row beside the effect frame and Delay panel chosen in Effect chain 5. Will picks one.

**Acceptance criteria**
- [ ] Two or three sketches at a normal window width, in light and dark, each with a Delay panel in view beside it.
- [ ] Each draws the sound where a drawing shows what a control does: the filter's curve (and the envelope's reach), both envelopes, the LFO's wave and a cycle of each oscillator at its width.
- [ ] Controls by RFC-005 part 7's rule: faders for the mixer, segmented choices for the waves, mode, Poly/Mono and LFO shape, toggles for the switches, menus for the mod slots, knobs for the rest. Any new knob size goes into the shared knob.
- [ ] Will chooses one. What happens to D-22 and D-10 is decided with it, and new decisions are logged as provisional.

**Out of scope:** Building it (9).

**Depends on:** Effect chain 5 (the chosen frame). Timebox: one session.

**Context:** RFC-008, Proposal: "The panel"; "Where this meets RFC-007", point 4. Risks: "The panel is the densest yet".

### 9. [UTA-81] Build the synth panel from Rust's descriptions (Feature)

**Goal:** The Sound tab shows the panel Will chose: every synth control, built from the list Rust sends, with the drawings following a drag.

**Acceptance criteria**
- [ ] The panel follows the chosen sketch, with every control built from Rust's descriptions: no range, default, label or choice in TypeScript.
- [ ] The drawings (filter curve with the envelope's reach, both envelopes, the LFO, each oscillator's cycle) follow a drag as it happens.
- [ ] The Mod section opens and closes; an empty slot shows only its "+".
- [ ] UI tests against the mocked back end: the panel is built from the settings Rust describes; each control sends the right command; the drawings follow a drag.
- [ ] Checked in the real app at a normal window size, with a Delay beside it if Effect chain 6 has landed. New decisions are marked `provisional: D-n` and listed in the PR.

**Out of scope:** The Sound menu of factory sounds (10).

**Depends on:** 7, 8.

**Context:** RFC-008, Proposal: "The panel"; "How we'll verify it" (UI; Manual 6).

**Will can try it:** build a bass, a pad and a lead from the default, turning every control while the loop plays.

### 10. [UTA-82] Make the factory sounds from briefs (Feature)

**Goal:** A Sound menu at the start of the panel offers about eight factory sounds (a bass, a mono bass with glide, a pluck, a pad, a PWM string pad, a lead, a sync lead, and a bell or noise sound), each made from a written brief with measurable targets. Choosing one is one undo step. The voicing guide records what was learned.

**Acceptance criteria**
- [ ] Core: `SetSynthSettings` sets every setting at once, and its undo puts them all back. The factory sounds live in Rust as data, each with its brief and targets (brightness as spectral centroid, attack, fade time, loudness). The menu's list comes from Rust.
- [ ] Each sound is made with RFC-008's loop (brief → settings → render → measure → Will listens → adjust), and a test checks each against its brief's targets and their loudness within an agreed range of each other.
- [ ] Golden WAVs from committed command lists: a sync lead sweep, a PWM pad and a mono bass with glide. Will approves them.
- [ ] `docs/sound-design/synth.md`, the voicing guide: which settings give which qualities, learned from making these, including what needed an ear rather than a measurement.
- [ ] UI: the Sound menu lists Rust's sounds and sends one command. A test against the mocked back end checks it.
- [ ] Real-time safe: switching factory sounds mid-loop runs under `assert_no_alloc` and stays under the click limit.

**Out of scope:** Saving your own sounds, remembering which sound a track started from, the AI sound designer.

**Depends on:** 9.

**Context:** RFC-008, Proposal: "In the project" (`SetSynthSettings`), "Factory sounds, and turning an idea into settings"; "How we'll verify it" (Factory sounds, Golden WAVs, Project core, UI). Open question 2.

### 11. [UTA-83] Tune the factory sounds by ear (Feature)

**Goal:** Will plays each factory sound against its brief and in a loop, and what he asks for is tuned. The voicing guide gets what the tuning taught.

**Acceptance criteria**
- [ ] A listening session from RFC-008's manual steps 3 to 6. Will's verdict on each sound is in the PR.
- [ ] The changes he asks for are made; each sound still meets its targets (or its targets change, with the reason in its brief); any golden that changes is re-approved.
- [ ] The voicing guide is updated with what changed and why.

**Out of scope:** New synth features.

**Depends on:** 10.

**Context:** RFC-008, "Risks & unknowns" ("Meaningful" is judged by ear, Levels, The brief-and-measure loop); "How we'll verify it", Manual 3 to 6.

**Will can try it:** the whole project.

### 12. [UTA-84] Send each setting's description once, not on every track (Chore)

Added during 6 (UTA-78), whose settings took the Wide test song's outline past RFC-004's 1 MB early warning: each synth track carried 12.5 KB of descriptions that are the same on every track. UTA-78 trimmed its descriptions to get back under the line, at 987 KB.

**Goal:** Each setting's description reaches the UI once, not on every track, so the outline carries only each track's values, and the synth's remaining settings and every effect's fit under the 1 MB line.

**Acceptance criteria**
- [ ] The outline carries each synth, drum and effect setting as its name and value only, and the descriptions travel once: a table by kind of sound or effect, sent where the UI can keep it (in the outline once, or fetched with a call), never once per track.
- [ ] The UI keeps the descriptions as it was sent them and draws every panel from them as today: the synth, drum and Delay panels look and behave as before. No label, range or default in TypeScript.
- [ ] Tests: the outline test checks a track's settings carry no descriptions and the table has every setting described; `the_outline_stays_under_1_mb_at_the_target_loads` passes with a wide margin, and the PR reports the Wide song's outline size before and after; the UI tests for the three panels pass against the new outline.

**Out of scope:** Any new setting. Splitting anything else out of the outline.

**Depends on:** Nothing. It should land before 5 (UTA-74) or 7 (UTA-79) merges, as either would cross the line again.

**Context:** RFC-004, part 2, "Growing later". RFC-008, "In the project". Ticket 1 made the descriptions.

## Order and checkpoints

```
1 settings ──► 2 voice ─┬─► 3 width, sync ─┐
                        ├─► 4 filter ──────┴─► 6 LFO ─┐
                        └─► 5 mono ───────────────────┴─► 7 mod ─┐
                    Effect chain 4 (note division) ──► 6         ├─► 9 panel ──► 10 sounds ──► 11 tune
                    Effect chain 5 (frame sketch) ──► 8 sketch ──┘
```

- **Start in parallel:** 1 here, with Effect chain 1 and 5. Ticket 1 unblocks Effect chain 2 as well as 2 here, so it goes first.
- **After 2:** 3, 4 and 5 in parallel.
- **12 can start any time,** and lands before 5 or 7 merges.
- **The listening checkpoint** comes once 3, 4 and 5 have merged, before 6 starts (RFC-008 open question 4).
- **8 can be sketched any time** after Effect chain 5, so the panel design is ready when 7 merges.
- **Checkpoints for Will:**
  - the sync A/B in 3;
  - the listening checkpoint after 5;
  - choosing the sketch in 8;
  - building sounds himself after 9;
  - 11 is the listening session.

## Shared with Effect chain and Delay

Each shared piece is built once, in the ticket named here. The [Effect chain and Delay](effect-chain-delay.md) plan has the same table.

| Shared piece | Built in | Used by |
|---|---|---|
| A track's stereo working buffer, pan as balance | Effect chain 1 | The chain. Synth features keeps the synth mono and only fills the buffer as 1 defines. |
| Settings described from Rust in one shape (label, unit, range, scale, default, choices, how it sounds) | Synth features 1 | Effect chain 2 (Delay settings), 6 and 7 (panels built from it); Synth features 2 to 10 |
| Note-division type and its menu of eighteen | Effect chain 4 | Synth features 6 (synced LFO rate) |
| The Sound tab as a row of panels, and the effect frame | Effect chain 5 (sketch), 6 (built) | Synth features 8 (sketched beside it), 9 (checked beside a Delay if 6 has landed; not a hard dependency) |
| Command format number | Whichever merges first: Effect chain 2 or Synth features 2 | The second takes the next number and converts from the first |
| Timing baseline | Each engine ticket re-records it | A clash in `timing-baseline.tsv` is only textual: re-record after rebasing |

## Coverage

| RFC-008 verification item | Ticket |
|---|---|
| The default is unchanged, format 4 lists render the same | 2 (conversion), every engine ticket |
| Oscillator 2 and the sub | 2 |
| Width | 3 |
| Sync (pitch follows, aliasing figure) | 3 |
| Noise | 2 |
| Filter (modes, Env amount, Key) | 4 |
| LFO (vibrato, synced cycle and bar, smoothing) | 6 |
| Mod slots | 7 |
| Mono, legato, glide, no stuck notes | 5 |
| Free-running | 2 |
| Determinism (twice, three block sizes) | every engine ticket |
| Factory sounds meet their targets, loudness range | 10 |
| Golden WAVs: sync lead, PWM pad, mono bass | 10 |
| Real-time safety, everything on | every engine ticket; 7 (all at once), 10 (sound switching) |
| Timing report (16 full voices, 32 tracks of 8) | 2, grown by 3 to 7 |
| Project core: undo, round-trip, one-step drag and sound choice, refusals, format 4 | 2 to 7, 10 |
| UI against mocked back end | 1, 9, 10 |
| Manual 1: listen at the checkpoint | after 5 |
| Manual 2: the sync A/B | 3 |
| Manual 3: build three sounds from the default | 9, 11 |
| Manual 4: turn every control while it plays | 9, 11 |
| Manual 5: each factory sound against its brief | 10, 11 |
| Manual 6: the panel at a normal window size, beside a Delay | 9, 11 |

## Not in this project

Everything in RFC-008's "Not in this project": analog character (drift, analog curves, a nonlinear or 24 dB filter, drive, unison, stereo spread, chorus); a second filter or LFO, LFO fade-in, a cycling envelope; pitch bend, mod wheel and aftertouch; saving your own sounds; drag-to-modulate; wavetables and FM; the AI sound designer. Showing each setting's "how it sounds" sentence in the panel (open question 3).

## Open questions

1. **The synced LFO waits on the Delay (Effect chain 4).** It needs RFC-007's note-division type. Recommendation: keep the dependency. By the time 3 and 4 have merged and the checkpoint is done, Effect chain 4 should have landed. If it hasn't, the LFO ticket can define the type itself and Effect chain 4 picks it up. Whichever comes first builds it; it isn't built twice. **Resolved: keep the dependency (Will, 2026-10-11).**
2. **The scratchpad note "An AI sound designer for each instrument and effect".** Recommendation: keep it Raw. This project builds the groundwork (descriptions, briefs, the voicing guide), and the designer is its own RFC once both projects have shipped. **Resolved: keep it Raw (Will, 2026-10-11).**
