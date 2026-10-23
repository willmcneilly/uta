# Project: Reverb

**Status:** Created · **RFC:** [RFC-010](../rfcs/rfc-010-reverb.md) · **Notion:** https://app.notion.com/p/Reverb-4023af969b6f81d9943dff67d85669e9

## Goal

Build RFC-010. Any track's chain or the master can hold a Reverb with an early burst and a late tail you balance, in three modes (Room, Chamber, Space), each tuned by ear until it's dense, smooth and never metallic. `uta analyse` measures a reverb's impulse response (decay per band, echo density, colouration, stereo spread, onset) and draws it, so Claude tunes with numbers before asking Will's ear. The Reverb has a core and an advanced panel and eleven factory sounds made from measured briefs. The ambient song is finished around it, and the other three songs each get the reverb their notes asked for.

## Where the code starts

Checked against `main` at `a8c22be`. Hearing tools is Done; Synth features is Active, with only UTA-83 (tune its factory sounds by ear) left, Ready. Nothing in this plan waits for it.

- **The chain is ready for a second kind.** `EffectKind` has one member, `Delay`, and `EffectKind::ALL` drives the **+** menu ([effect.rs:143](../../crates/uta-core/src/effect.rs)). `DelaySettings`, its `DelayParam` (tagged by name, each with `since_format`, `range()` and `describe()`) and `SetDelayParam` ([command.rs:128](../../crates/uta-core/src/command.rs)) are the models for the Reverb's. Settings are described in the shared `Description` shape ([setting.rs:20](../../crates/uta-core/src/setting.rs)).
- **Commands** are at `COMMAND_FORMAT = 14` ([command.rs:49](../../crates/uta-core/src/command.rs)). `SetSynthSettings` (format 14) is the model for "set every Reverb setting".
- **The engine's effect pattern is settled.** The `Effect` trait (prepare, process wet, glide to settings, is ringing, clear, and a default `restart()`) is in [effects/mod.rs:35](../../crates/uta-engine/src/effects/mod.rs); states are made on the control side (`EffectState::new`) and come back through `used_states`. The Delay's Catmull-Rom `cubic()` read is in [delay.rs](../../crates/uta-engine/src/effects/delay.rs). Its Wobble is deterministic sines, not random: the only seeded random source is `noise.rs`'s xorshift, with a `restart()`, which the Reverb's modulation can use.
- **The test kit** is `tests/effects.rs`: `kit_effect(kind)` is an exhaustive match, and its tests run over `EffectKind::ALL`, so adding the Reverb there gives it the kit. `realtime.rs` checks every state comes back; `timing.rs` has a `Delays` case (one on 32 tracks and the master) to copy; `sound.rs` has the golden WAVs.
- **Rendering** rings out up to `MAX_TAIL_SECONDS = 30.0` ([offline.rs:136](../../crates/uta-engine/src/offline.rs)) and the CLI says when it stopped a tail.
- **Measurements.** `uta-analysis` has `decay_seconds` (time to fall N dB from the peak on a 5 ms envelope), `stereo_width`, `correlation`, `first_sound` and `spectrogram`, but no backward-integrated decay, per-band decay, echo density, energy decay relief or colouration. `--png` draws five images in [images.rs](../../crates/uta-cli/src/images.rs), each with its name and numbers on it, none with what to look for.
- **Listening.** `scripts/listen` runs `app/scripts/listen.ts`. Each candidate is a command list; the loop comes from `loopRegion()` (the song's loop, or the shortest render), with nothing that trims a tail's silence, and there's no canvas.
- **Factory sounds** are data in [factory.rs](../../crates/uta-core/src/factory.rs) (`FactorySound` with its brief, audition and `Targets`: brightness, attack, fade, loudness, and a 3 dB loudness spread), checked in `tests/factory.rs`. A reverb's targets are different numbers (decay per band, onset, density), so they get their own type. `docs/sound-design/synth.md` is the voicing guide to sit beside.
- **The window.** `DelayPanel.tsx` builds its controls from Rust's descriptions through `SettingControl.tsx` (provisional D-31); the **+** menu is in `ChainRow.tsx`, which picks a panel per kind. The synth's Sound control is the `Menu` select (D-32). Sketches live in the `uta-design` sandbox (`live/chain.html`, `live/synth.html`).
- **Songs.** A song file's `effects` take `delay = { … }` by setting name; [uta-song's lib.rs](../../crates/uta-song/src/lib.rs) looks the kind up in `EffectKind::ALL` and needs a match arm for a new kind. The ambient song has Pad, Drone, Pulse, Strings and Bell, with Delays on Pad, Pulse and Bell. The README lists Reverb as missing from all four songs.
- **Research and glossary.** RFC-010 has three Research pages related to it, and its eleven new terms are already in the Glossary. "Factory sound" still has the synth's meaning.

## Tickets

12 tickets: two build the tools the Reverb is judged with, two build its sound, three make and tune the modes by ear, two put it in the window, one makes the factory sounds, and two put it in the songs. That's two over the skill's rough ceiling of ten, and it's on purpose: RFC-010 resolved that each mode is its own ticket, tuned by ear in its own sessions (open question 1), and RFC-009's pattern of songs in their own tickets keeps the long ambient rework from holding up the other three. Without the modes and the songs as separate tickets it would be eight.

**Every ticket that changes the Reverb's sound (3 to 7, and 10) also has this criterion,** written out in full on each Notion ticket:
- [ ] **Measured before it's heard:** the PR has `uta analyse` of the impulse responses it changed, before and after, and says what Claude changed because of the numbers before asking Will.

### 1. [UTA-97] Measure a reverb's impulse response with `uta analyse` (Feature)

**Goal:** `uta-analysis` has the five reverb measurements RFC-009 left to this RFC, each checked against a synthetic signal with a known answer, and `uta analyse` reports them and draws three new pictures. Every picture, old and new, says what to look for. It comes first because every Reverb ticket tests against these numbers, and it needs no reverb to build: the synthetic signals stand in.

**Acceptance criteria**
- [ ] Decay time per octave band: the energy summed backwards from the end (Schroeder), and the fall from −5 to −35 dB fitted and reported as the time to fall 60 dB. Tested on exponentially decaying noise of known decay times, including a different decay per band.
- [ ] Echo density over time (Abel and Huang's normalised measure), tested on white noise (near 1) and a sparse click train (near 0); and the time it first reaches near 1, as one number.
- [ ] Onset (when the first sound arrives, to the sample), stereo spread early and late (how alike left and right are), and colouration (how bumpy the tail's smoothed spectrum is), each tested on a signal with a known answer. Colouration is reported but has no limit yet: its limit is set against Will's verdicts in 5.
- [ ] `uta analyse` reports them for a WAV marked as an impulse response, in the text report, `--json` and `--against`.
- [ ] `--png` draws the impulse response (waveform and spectrogram), the decay by frequency (energy decay relief) and echo density over time, each with its numbers printed on and fixed scales.
- [ ] Every image, including the ones from UTA-88, has a line under its title saying what to look for (a Bass Mult that works shows the lows lasting longer; a metallic ring is a line outliving the rest), and the images' tests check the line is there.

**Out of scope:** The Reverb itself (3). The command lists that render a reverb's clicks (3). Setting colouration's limit (5). Heldmann and Schlecht's validated colouration measure, which needs the network's maths, not just its sound.

**Depends on:** Nothing.

**Context:** RFC-010, Proposal: "Measurements, as RFC-009 promised"; Risks ("The colouration measurement is our own proxy"); How we'll verify it ("The measurements themselves are tested against synthetic signals with known answers"). RFC-009, "Not in this project" (reverb's measurements). Scratchpad: "Explain what to look for in each analysis image". Research: RFC-010's.

### 2. [UTA-98] Give listening sessions a fixed canvas, and end the loop when the tails do (Feature)

**Goal:** A listening session can be built from a fixed canvas of parts (a snare hit, a short piano-like chord, a vocal-like swell and a pad) with each candidate's sound or effect added on top, so every verdict is on the same material and Will doesn't write test parts by hand. A session's loop ends when every candidate's tail has fallen below −60 dB, not after the silence a reverb leaves. Nothing here is reverb-specific: the synth's tuning can use it too.

**Acceptance criteria**
- [ ] The canvas: committed command lists (or a song file) for the four parts, each short, in `examples/listen/canvas/`, with what each is for.
- [ ] A session can name the canvas and give each candidate as commands applied on top of it (an effect on the canvas's tracks, or a sound), and `scripts/listen` renders each candidate through the same canvas.
- [ ] The loop ends when the last candidate's tail is below −60 dB, measured from the renders, so the candidates share one loop length. Tested against renders with a known tail.
- [ ] The listening page's vitest suite covers the canvas session and the loop length.
- [ ] `scripts/listen --help` and the `work-ticket` skill's listening steps say how to build a canvas session.

**Out of scope:** Any reverb candidate (5 onwards). Rendering an outside reference such as Ableton's Reverb: a session already takes a recording as a candidate.

**Depends on:** Nothing.

**Context:** RFC-010, Proposal: "Listening to it". Scratchpad: "Listening loop includes the ring-out silence, leaving little time to compare", "A consistent canvas of parts to tune sounds against".

**Will can try it:** a canvas session with two synth sounds, to check the canvas is the material he'd want and that the loop is no longer padded with silence.

### 3. [UTA-99] Add a Reverb to a track from a command list (Feature)

**Goal:** The thin slice through every layer. A command list can add a Reverb to any chain, and `uta render` plays its tail to the end. It has the predelay, the diffuser, the 16-line feedback delay network with every resonance fading at the same rate, and Depth and Early Send joining them, in the Room mode's arrangement. Modes are data from the start, so Chamber and Space are a new arrangement, not a new shape. The settings Room doesn't use yet (the tone, the crosses, modulation) are in the core and the command format already, so the format goes up once.

**Acceptance criteria**
- [ ] Core: `EffectKind::Reverb` with every setting in RFC-010's table (ranges, units and defaults as it gives them: Room, 2 s Decay, Depth 50%, High Mult 0.5, Mix 25%), described in the shared settings shape; a "set a Reverb setting" command that undoes exactly, a knob drag one undo step; it round-trips through saving, the format goes up by one, and older command lists still load and render the same. The song file takes `reverb = { … }` by setting name, as it does `delay`.
- [ ] Engine: the Reverb's state (predelay, diffuser and network delay lines, sized for the largest sizes at the sample rate) is made on the control side and handed over with the snapshot, as the Delay's is. Delay lengths are worked out in seconds, then samples. A mode is a description (line count and lengths, diffuser steps, where the outputs are taken, how dark it's built), and Room is the first.
- [ ] The diffuser takes the early output from the last step's lanes before their mixing (RFC-010's note), and Depth 0% leaves no late tail after the early burst while 100% leaves nothing from the early section on its own.
- [ ] Tests on impulse responses: the mid-band decay is within 10% of Decay at 0.3, 2 and 10 s; the first sound arrives at the Predelay time to the sample; decay per band and echo density match within tolerance at 44.1, 48, 96 and 192 kHz; at 30 s Decay and every setting at its extreme the output never grows and never passes full scale.
- [ ] The effect test kit runs on the Reverb by adding it to the list (off and Mix 0, clicks on add/remove/move/switch, block sizes, `assert_no_alloc` and RealtimeSanitizer, tails and sleeping at −120 dB), and `uta render` runs until it sleeps, up to 30 s, saying so when it stops a tail still ringing.
- [ ] `examples/listen/reverb/` has the command lists that render a click through it at Mix 100%, at a few Decays, for `uta analyse`; the PR has their reports.

**Out of scope:** Bass and High Mult and the Xovers, High Cut and Low Cut, Early and Late Cross, modulation, and gliding the sizes (4). Tuning Room by ear and its golden WAV (5). Chamber and Space, and switching mode (6, 7). The window (8, 9).

**Depends on:** 1.

**Context:** RFC-010, Proposal: "The building blocks" 1 to 4 (without the tone, cross and modulation bullets), "Modes" (the machinery), "Fitting the chain", "The controls"; open question 2 (Decay up to 30 s); Risks ("Sample-rate independence", "Long tails and the render limit", "Memory"); How we'll verify it (the kit, Decay is right, sample rates, Predelay, Depth, Bounded, Project core). RFC-007, "Patterns for every effect after this one".

### 4. [UTA-100] Shape the tail: tone, width, movement and gliding sizes (Feature)

**Goal:** Every control RFC-010 lists does what it says in Room: the lows and highs fade at their own rates, High Cut and Low Cut shape the whole reverb, the crosses set how wide it is, slow random modulation smears the resonances, and every setting, the sizes included, glides while the tail rings without a click.

**Acceptance criteria**
- [ ] Bass Mult and High Mult, at Bass Xover and High Xover, set the low and high octaves' decay as a multiple of the mid's, per line in the loop, so every resonance in a band still fades at the same rate. Tests: Bass Mult 2 gives low octaves about twice the mid's decay, High Mult 0.5 high octaves about half, within 15%. High Cut (−12 dB per octave) and Low Cut (a low shelf, off at its lowest) are tested on the impulse response's spectrum.
- [ ] Early and Late Cross. Test: with Late Cross at 0 a sound only on the left stays mostly on the left; at full the sides are about equal (stereo spread from 1).
- [ ] Early and Late modulation: slow, random (from a seeded source like `noise.rs`'s, since the Delay's Wobble is sines), each modulated line at its own rate, read with the Delay's cubic interpolation, and restarted from the same point on every Play. Tests: two renders are identical, and the pitch moves no more than the depth allows.
- [ ] Every setting glides. Sweeping Late Size and Early Size while a tail rings, over about a quarter of a second, stays under the click limit, as do the other knobs. Changing Decay doesn't change the balance Depth sets.
- [ ] The test kit and `realtime.rs` still pass with every new setting moving; the timing report gets a case with a Reverb on every track and the master, modulation on, and its baseline is re-recorded.

**Out of scope:** The defaults and the mod rates by ear (5). Mode switching (6). A crossfade for size changes: the fallback in RFC-010's Risks, only if Will hears a swoop in 5 or 9.

**Depends on:** 3.

**Context:** RFC-010, Proposal: "The building blocks" (the tone, cross and modulation bullets of 2 and 3; High Cut and Low Cut in 4), "Turning knobs while it rings"; Risks ("Modulation can sound like chorus", "Size changes while ringing are a guess", "Long tails cost CPU"); How we'll verify it (Mults, Cross, Modulation is repeatable, Gliding sizes don't click, Timing).

### 5. [UTA-101] Tune Room by ear, and set colouration's limit (Feature)

**Goal:** Room goes from correct to good. Will hears it on the fixed canvas, Claude tunes what he asks for (line lengths that ring, the modulation, the defaults), and he hears it again before and after. His verdicts set colouration's limit, so from here a test fails if a mode turns metallic. The voicing guide for reverb starts.

**Acceptance criteria**
- [ ] A listening session on the canvas: Room at its defaults against a variant Claude predicts is better, with "which would you use, and what's wrong with the other?" first. Claude's prediction is written first; the verdict is in the PR.
- [ ] A tuning pass on what Will asks for, each change with its reason in the code, then a before-and-after session.
- [ ] Colouration's limit set from the verdicts (the variants Will heard as metallic and the ones he didn't), with the evidence in the PR, and a test that Room's impulse responses stay under it.
- [ ] Echo density's limit for Room set, with a test (it reaches near 1 within it).
- [ ] A golden WAV of a short phrase through Room, from a committed command list, which Will approves.
- [ ] `docs/sound-design/reverb.md` begins: which settings give which qualities, and what needed an ear rather than a measurement.

**Out of scope:** Chamber and Space (6, 7). The factory sounds (10).

**Depends on:** 2, 4.

**Context:** RFC-010, Proposal: "Listening to it"; Risks ("A reverb you love is a high bar", "Metallic ringing", "The colouration measurement is our own proxy"); How we'll verify it (Dense, not grainy; Not metallic; golden WAV; Manual 1). Open question 4 (our own candidates, no outside reference).

**Will can try it (the first listening checkpoint):** the Room sessions, then the reverb renders in `examples/listen/reverb/`.

### 6. [UTA-102] Add the Chamber mode, tuned by ear (Feature)

**Goal:** Chamber: dense from the very first moment, smooth and even, for snares and vocals. Switching mode works: the old tail fades out over about 50 ms and the new mode starts from silence.

**Acceptance criteria**
- [ ] Chamber's arrangement as a mode description, with RFC-010's character (no sparse onset; echo density near 1 within its own, fast limit).
- [ ] Its tests on impulse responses: the decay at 0.3, 2 and 10 s, density, colouration under the limit from 5, and the sample rates.
- [ ] Switching mode fades the old tail out over about 50 ms and the new one starts from silence, under the click limit, under `assert_no_alloc`. Whichever of 6 and 7 merges second adds its mode to this test.
- [ ] A listening session on the canvas (a snare through it, short Decay, then a long Early Size for a gated burst, as RFC-010's manual step 4), a tuning pass, and a before-and-after session.
- [ ] A golden WAV of the phrase through Chamber, which Will approves. The voicing guide gains what Chamber taught.

**Out of scope:** Space (7). The panel's Mode choice (9).

**Depends on:** 5.

**Context:** RFC-010, Proposal: "Modes" (Chamber), "Turning knobs while it rings" (changing mode); How we'll verify it (Decay per mode, Dense, Not metallic, golden; Manual 1 and 4). Open question 1.

### 7. [UTA-103] Add the Space mode, tuned by ear (Feature)

**Goal:** Space: huge and dark, with a slow, sparse onset and deep, slow modulation, nothing much above about 11 kHz. It's the ambient song's mode, and the reverb most likely to need a long tuning pass.

**Acceptance criteria**
- [ ] Space's arrangement as a mode description, with RFC-010's character (a slow onset; little energy above about 11 kHz; deep, slow modulation).
- [ ] Its tests on impulse responses: the decay at 0.3, 2 and 10 s and the 30 s top of the range, density within its own, slower limit, colouration under the limit from 5, the sample rates, and the pitch movement bounded at full Late Mod Depth.
- [ ] A listening session on the canvas (the pad and the swell, long Decays), a tuning pass, and a before-and-after session.
- [ ] A golden WAV of the phrase through Space, which Will approves. The voicing guide gains what Space taught.

**Out of scope:** Finishing the ambient song (11).

**Depends on:** 5. (It runs alongside 6.)

**Context:** RFC-010, Proposal: "Modes" (Space); Risks ("Modulation can sound like chorus or seasickness", "Long tails cost CPU"); How we'll verify it (Decay per mode, Dense, Not metallic, Modulation bounded, golden; Manual 1). Open question 1.

### 8. [UTA-104] Sketch the Reverb panel and the preset icon (Spike, one session)

**Goal:** Before the panel is built, two or three versions are sketched in the design sandbox: the core view, the advanced view (a wider panel or a second page), the switch between them, whether and how it draws the tail across low, mid and high, and the preset icon that opens a menu, which the synth's Sound menu will use too. Will picks one.

**Acceptance criteria**
- [ ] Two or three sketches, each with the core view (preset icon, Mode, Predelay, Decay, Size, Depth, High Cut, Low Cut, Mix) and the advanced view (Early and Late groups, as RFC-010 lists them), beside a Delay and the synth panel at a normal window width, in light and dark.
- [ ] Each follows RFC-005's control rule (Mode a segmented choice, the rest knobs) and "Draw the sound" if it draws the tail.
- [ ] The preset icon and its menu, shown on the Reverb and on the synth panel, with each factory sound's suggested Mix in the Reverb's.
- [ ] Will chooses one, and the reasons go in Design decisions entries (provisional, with `scripts/ticket decision new`).

**Out of scope:** Building any of it (9, 10).

**Depends on:** Nothing. Timebox: one session.

**Context:** RFC-010, Proposal: "In the window", "Factory sounds" (the panel bullet); Risks ("The panel is big"). `DESIGN.md`, "Draw the sound". Scratchpad: "Presets need a preset icon that opens a menu, not a select". Design decisions D-31 (the Delay panel) and D-32 (the synth panel's Sound menu).

**Will can try it:** choose the sketch.

### 9. [UTA-105] Build the Reverb panel (Feature)

**Goal:** You can add a Reverb from **+** and play it from the window: the panel Will chose in 8, with its core and advanced views, Mode as a segmented choice, knobs for the rest built from Rust's descriptions, and the tail drawing if the sketch kept one.

**Acceptance criteria**
- [ ] **+** lists Reverb after Delay, from Rust.
- [ ] The panel follows the chosen sketch, built from Rust's description of the Reverb's settings, with no range or default in the UI. It opens in core; whether it's core or advanced is remembered per panel, outside the project.
- [ ] If the sketch has a tail drawing, it's drawn from the settings Rust sent and follows a drag as it happens.
- [ ] UI tests against the mocked back end: **+** adds a Reverb; each control sends its command; the advanced switch shows the rest; the panel draws from the outline.
- [ ] New design decisions are marked `provisional: D-n` and listed in the PR; checked in the real app beside the Delay and synth panels, in light and dark.

**Out of scope:** The preset icon and the factory sounds (10).

**Depends on:** 4, 8. (Mode's choices grow as 6 and 7 land; the panel lists what Rust sends.)

**Context:** RFC-010, Proposal: "In the window"; How we'll verify it (UI); Manual 3 and 5.

**Will can try it:** add a Reverb to a track and the master while a loop plays, turn every knob while it rings (both sizes and the mod rates especially: nothing clicks, the sizes swell rather than swoop), then Stop mid-tail and switch it off mid-tail.

### 10. [UTA-106] Make the Reverb's factory sounds from briefs (Feature)

**Goal:** The eleven factory sounds RFC-010 lists, each from a written brief with measured targets, chosen from the preset icon's menu with one command and one undo. They leave Mix alone. The synth's Sound menu moves to the same icon.

**Acceptance criteria**
- [ ] Core: a "set every Reverb setting" command, like `SetSynthSettings`, that sets every setting but Mix, its mode included, and one undo puts them all back. The sounds live in Rust as data, each with its brief, its targets (a reverb's own: decay per band, onset, early and late balance, density, top and bottom of the spectrum, loudness) and its suggested Mix. The song file takes a Reverb's factory sound by name.
- [ ] The eleven: Drum room, Vocal plate, Echo chamber, Concert hall, Cathedral, Gated drums, Ambient wash, Dark room, Bloom, Endless and Distant. A test checks each against its brief's targets, and that at the same Mix their loudness is within an agreed range of each other.
- [ ] The preset icon from 8 opens the menu on the Reverb panel, with each sound's suggested Mix shown, and replaces the synth panel's Sound menu. UI tests against the mocked back end: the menu lists Rust's sounds and choosing one sends one command, on both panels.
- [ ] Choosing a sound mid-loop, including one that changes mode, stays under the click limit and runs under `assert_no_alloc`.
- [ ] A session on the canvas: each sound against its brief on the snare, keys and pad, and stepping through the menu while a loop plays. The changes Will asks for are made. If that runs long, the sounds the demo songs use land first and the rest follow in the same ticket's second PR.
- [ ] `docs/sound-design/reverb.md` gains what the briefs taught; the Glossary's "Factory sound" entry widens to an effect's settings.

**Out of scope:** Saving your own reverb presets; remembering which factory sound a reverb started from; any sound that needs a mode this project doesn't build (a plate, a spring, shimmer).

**Depends on:** 6, 7, 9.

**Context:** RFC-010, Proposal: "Factory sounds"; open questions 6 (the eleven) and 7 (Mix left alone); Risks ("Eleven factory sounds"); How we'll verify it (Factory sounds, Project core, UI; Manual 6). RFC-008, "Factory sounds, and turning an idea into settings".

**Will can try it:** step through the menu on the keys while a loop plays: the space changes, the amount of reverb doesn't.

### 11. [UTA-107] Finish the ambient song with Reverb (Feature)

**Goal:** The ambient song RFC-009 started thin, on purpose, is finished: a long Space reverb on the Pad and on the Strings, and one on the Bell after its Delay so the echoes bloom, then its sections reworked so the space has time to bloom and fade.

**Acceptance criteria**
- [ ] `examples/demos/ambient.toml` has a Reverb on the Pad, the Strings and the Bell (after its Delay), from the factory sounds or settings, each track its own until sends exist.
- [ ] The sections are reworked around the space, and the README's "unfinished" note and its Reverb gap are replaced by what it still lacks (sends, first).
- [ ] CI renders it: no clipping, no silent track. The PR has `uta analyse` of the whole render and of each reverb's track soloed, and says what Claude changed because of the numbers.
- [ ] A *listen and describe* session: does it bloom and hang the way Will wanted? The changes he asks for are made.
- [ ] The timing report gets a case for the finished song with its reverbs, at p50 and p99. A song's worth of reverbs anywhere near the block deadline is a bug fixed in this ticket, not a limit.
- [ ] The synth's share of that case is reported too, checking its headroom after the mod slots (the scratchpad note from UTA-79: with every slot empty a synth costs 1–8% more than before them). Winning that back is in scope only if the song comes near the deadline.

**Out of scope:** Sends, and moving the song to one shared reverb (the sends RFC).

**Depends on:** 10.

**Context:** RFC-010, Proposal: "The ambient song is finished in this project"; open question 5 (a reverb per track); How we'll verify it (Timing, Demo songs; Manual 2). RFC-009, "Ambient is started here, and grows with Reverb". Scratchpad: "Check the synth's performance after the mod slots".

**Will can try it:** load the ambient song from the Develop menu and play it.

### 12. [UTA-108] Give the lo-fi, house and techno songs their reverbs (Feature)

**Goal:** Each of the other three songs gets the reverb its notes asked for: the lo-fi keys and snare, the house claps and the stab's tail in the break, the techno dub chord and clap.

**Acceptance criteria**
- [ ] Each song file has its reverbs, from the factory sounds or settings, and each song's README entry drops Reverb from what it lacks (or says what's still missing about it).
- [ ] CI renders all three: no clipping, no silent track. The PR has `uta analyse` of each song, and of each reverb's track soloed.
- [ ] A *listen and describe* session per song, with Claude's prediction first. The changes Will asks for are made.

**Out of scope:** Anything else the songs are missing (swing, sidechain, saturation and the rest).

**Depends on:** 10. (It runs alongside 11.)

**Context:** RFC-010, Proposal: "The ambient song is finished in this project" (the last sentence); How we'll verify it (Demo songs).

**Will can try it:** load each song and play it.

## Order and checkpoints

```
1 measure ──► 3 Reverb slice ──► 4 tone, width, movement ─┬─► 5 tune Room ─┬─► 6 Chamber ─┐
2 canvas, loop end ──────────────────────────────────────┼───────────────┘   7 Space ───┤
8 panel sketch ───────────────────────────────────────────┴─► 9 panel ──────────────────┴─► 10 factory sounds ─┬─► 11 ambient
                                                                                                             └─► 12 the other three songs
```

- **Start in parallel:** 1 (measurements), 2 (canvas and loop end) and 8 (the sketch). 3 waits for 1.
- **After 4:** 5 (tune Room) and 9 (the panel) in parallel. 9 only needs the settings to exist; Mode's choices grow as the modes land.
- **After 5:** 6 and 7 in parallel. They wait for Room's tuning so colouration's limit exists and what Room taught reaches them.
- **After 6, 7 and 9:** 10. Then 11 and 12 in parallel.
- **Will's time is the critical path.** Tickets 5, 6, 7, 10, 11 and 12 each need sessions, so the engineering tickets (1, 3, 4) are kept free of them.
- **Checkpoints for Will:**
  - after 2: a canvas session on two synth sounds;
  - after 8: choose the panel sketch;
  - 5, 6 and 7: the mode sessions, before and after tuning (the first time he hears a Reverb);
  - after 9: the Reverb in the app, every knob while it rings, Stop and off mid-tail;
  - 10: the factory sounds against their briefs;
  - 11 and 12: each song.

**Shared numbers.** The command format goes up in 3 and again in 10 (the "set every setting" command). The timing baseline is re-recorded in 4 and 11; a clash in `timing-baseline.tsv` is only textual: re-record after rebasing.

## Coverage

| RFC-010 verification item | Ticket |
|---|---|
| The effect test kit on the Reverb (off, Mix 0, clicks, block sizes, `assert_no_alloc`, RTSan, tails and sleeping) | 3 (and 4 with every setting moving) |
| Decay is right: mid band within 10% at 0.3, 2 and 10 s, every mode | 3 (Room), 6, 7 |
| The Mults are right, within 15% | 4 |
| The same at every sample rate (44.1 to 192 kHz) | 3, 6, 7 |
| Dense, not grainy: density near 1 within a limit per mode | 5 (Room), 6, 7 |
| Not metallic: colouration under its limit, set against Will's verdicts | 5 (the limit), 6, 7 |
| Predelay is exact | 3 |
| Depth at 0% and 100% | 3 |
| Cross | 4 |
| Modulation is repeatable and bounded | 4, 7 (full depth in Space) |
| Bounded at the longest Decay and every extreme | 3 |
| Gliding sizes don't click | 4 |
| The measurements on synthetic signals | 1 |
| Golden WAV per mode, approved by Will | 5, 6, 7 |
| Timing: a Reverb on every track and the master; the finished ambient song | 4, 11 |
| Factory sounds meet their briefs; loudness within range | 10 |
| Project core: undo, round-trip, one-step drag, factory sound one step without Mix, old lists load | 3, 10 |
| UI: + adds a Reverb, controls send commands, the preset menu, draws from the outline | 9, 10 |
| Demo songs render, `uta analyse` in the PR | 11, 12 |
| Manual 1: listening sessions per mode, before and after | 5, 6, 7 |
| Manual 2: the ambient song blooms and hangs | 11 |
| Manual 3: every knob while it rings | 9 |
| Manual 4: snare through Chamber, then a long Early Size | 6 |
| Manual 5: Stop mid-tail, off mid-tail | 9 |
| Manual 6: each factory sound against its brief; the menu while a loop plays | 10 |

The Proposal's other parts: "What inspires it" shapes 3 to 7; the impulse-response command lists (`examples/listen/reverb`) are in 3; the fixed canvas and the loop's end are 2; the voicing guide is begun in 5 and grows in 6, 7 and 10.

## Not in this project

Everything in RFC-010's "Not in this project":
- **Sends and returns:** their own RFC. Until then, each track that needs a reverb has its own (open question 5).
- **Room's other nine modes,** and any mode beyond Room, Chamber and Space: each a later ticket, tuned by ear.
- **Space,** Room 2.0's early feedback control (open question 3).
- **Convolution reverb.**
- **Freeze, shimmer, a gated reverb with its own gate, ducking, tempo-synced Predelay.**
- **Saving your own reverb presets,** and remembering which factory sound a reverb started from.
- **A Plate mode, a spring,** and any factory sound needing a mode this project doesn't build.
- **Automation.**

## Scratchpad

Folded into RFC-010, and related to this project in Notion:
- "Explain what to look for in each analysis image" → 1 (every image, old and new).
- "Listening loop includes the ring-out silence, leaving little time to compare" → 2.
- "A consistent canvas of parts to tune sounds against" → 2.
- "Presets need a preset icon that opens a menu, not a select" → 8 (sketched), 10 (built, on both panels).

Raw notes that touch this project, and what Will decided (open question 2):
- "Shimmer reverb: a pitch shifter inside the reverb feedback loop, for ambient": **keep.** It needs a pitch shifter in the network's loop, which this design doesn't build; it's a later mode or effect.
- "Check the synth's performance after the mod slots": **folded into 11.** The finished ambient song's timing case is a whole song's cost, synth and reverbs together, so it's where the synth's headroom is checked. Winning back the empty-slot cost is in scope only if the song comes near the deadline.
- "An AI sound designer for each instrument and effect": **keep.** The reverb's briefs and measurements are more of its groundwork, as RFC-009 was.

The other Raw notes (UI, tooling) don't touch this project and stay Raw.

## Open questions

1. **Does Room's tuning hold up Chamber and Space?** As planned, 6 and 7 wait for 5, so colouration's limit is set once from Will's verdicts on Room and what Room's tuning taught reaches the other two. That puts 3 → 4 → 5 → 6/7 in a line. The alternative is 6 and 7 starting after 4, alongside 5, each setting a provisional colouration limit that 5 then replaces: quicker on paper, but three modes tuned at once means three sets of sessions at once, and Will's time is the bottleneck anyway. Recommendation: wait for 5. **Resolved: wait for 5 (Will, 2026-10-22).**
2. **The Raw scratchpad notes.** Recommendation as in "Scratchpad" above: keep the shimmer and the AI sound designer, fold the synth's performance check into 11. **Resolved: as recommended (Will, 2026-10-22).** Kept means out of this project, left Raw for a later review.
