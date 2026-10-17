# Project: Hearing tools

**Status:** Created · **RFC:** [RFC-009](../rfcs/rfc-009-hearing-tools.md) · **Notion:** https://app.notion.com/p/Hearing-tools-3fc3af969b6f81449c4adbca946cbedc

## Goal

Build RFC-009. Claude can measure any render with the same code the sound tests use (`uta analyse`), with images that have their numbers printed on. When it needs Will's ear, it asks one question at a time in a fair, blind listening page, with its prediction written down first and Will's verdict kept privately beside the numbers. And there are four demo songs (lo-fi, house, techno, and the start of an ambient one), written in a readable song file that compiles to commands, loaded from Develop → Load demo song and rendered in CI.

## Where the code starts

Checked against `main` at `a74c7a6`. Synth features is Active: UTA-81 (the synth panel) is In Progress, and UTA-82 (factory sounds from briefs) and UTA-83 (tuning them) are waiting on it.

- **The measurements** are in `crates/uta-engine/tests/common/mod.rs` (720 lines), mixed in with the projects the tests build. The measurement half is about twenty functions: `measure_frequency`, `pitch_over_time`, `rms`, `peak`, `max_jump`, `spectrum`, `spectral_centroid`, `share_above`, `strongest_frequency`, `envelope_db`, `rms_envelope_db`, `decay_seconds`, `burst_peaks`, `hit_loudness_db` with its K-weighting, `level_at`, `first_sound`, and a radix-2 `fft` that needs power-of-two lengths. Several assume 48 kHz. The other half (`demo_loop`, `drum_project`, `hit`, `with_every_slot_taken`, …) builds projects and stays in the tests.
- **Measurements kept in single tests:** `loudest_alias_db` in `tests/synth.rs` (aliasing between a known pitch's harmonics) and `correlation` in `tests/clap.rs`.
- **The `uta` command** (`crates/uta-cli/src/main.rs`) has `render`, `render-all` and `play`. WAVs are written and read by `offline::write_wav` and `read_wav` in `uta-engine` (32-bit float, through `hound`).
- **Scripts** follow one pattern: a `sh` wrapper in `scripts/` that runs `node app/scripts/<name>Cli.ts`, with node built-ins only and vitest tests beside it. `designServer.ts` is the nearest thing to a local page.
- **The Develop menu** (`app/src-tauri/src/menu.rs`) has Add Stress Notes and Run Benchmark…. `Uta::build_test_song` already replaces every track as one undo step, which is the pattern for loading a song.
- **Example songs** are command lists: `examples/demo-song.json` (format 3), `demo-loop.json` (format 2), `demo-beat.json`, `demo-delay.json`. The golden and CLI tests build from them.
- **The PR template** is the PR body's list of sections in the `work-ticket` skill (step 6); `scripts/ticket sync` reads it. There's no `.github` template.
- **`toml`** is already in `Cargo.lock` (through Tauri), so the song file and session file add no new parser to the build.

## Tickets

11 tickets. Five build the tools in a chain (move the measurements, the report, the new measurements, the images, the listening page), two build the song file and its menu, and four write the songs. That's one more than the skill's rough ceiling of ten, because RFC-009 asks for each song as its own ticket so they can run in parallel and a long one doesn't hold up the rest. Without the songs it's seven.

**Every song ticket (8 to 11) also has these criteria,** written out in full on each Notion ticket:
- [ ] **A song file** at `examples/demos/<genre>.toml`, written in musical terms, using only what the song file covers.
- [ ] **CI renders it:** it builds, doesn't clip, and has no silent track.
- [ ] **Measured:** the PR has `uta analyse` of the whole render and of each track soloed, and says what Claude changed because of the numbers before asking Will.
- [ ] **Listened to:** a *listen and describe* session for Will, with Claude's prediction written first. The changes Will asks for are made.
- [ ] **What it's missing:** the features the genre wanted and Uta lacks (swing, saturation, sidechain, automation, …) are listed under the song's heading in `examples/demos/README.md`.

### 1. [UTA-85] Move the measurements into `uta-analysis` (Chore)

**Goal:** The sound tests' measurements live in a new crate, `uta-analysis`, moved unchanged. The sound tests call it and pass exactly as before, so we know the move broke nothing. It's the first of the two steps RFC-009 asks for: move first, change later.

**Acceptance criteria**
- [ ] A new crate `crates/uta-analysis` that only reads samples: no `uta-core`, no `uta-engine`, no Tauri, no audio thread. `uta-engine` uses it as a dev-dependency only.
- [ ] Every measurement in `tests/common` moves there unchanged (same code, same FFT, same 48 kHz assumptions), as does `loudest_alias_db` from `tests/synth.rs` and `correlation` from `tests/clap.rs`. The project builders stay in `tests/common`.
- [ ] Every existing sound test passes with its current limits, and no golden WAV changes.
- [ ] `uta-analysis` has its own unit tests for what the sound tests didn't check directly, at least: a 440 Hz sine reads 440 Hz, a sine's RMS is its peak over √2, and an exponential decay of a known rate reads that decay time.

**Out of scope:** Changing any measurement or the FFT (3). The CLI (2).

**Depends on:** Nothing.

**Context:** RFC-009, Proposal part 1 ("Shared measurements, in one place"); Risks ("Moving the helpers could change test numbers slightly"); How we'll verify it ("The moved helpers keep every existing sound test passing").

### 2. [UTA-86] Measure any render with `uta analyse` (Feature)

**Goal:** `uta analyse <wav>` prints one line per measurement, with units, from the same code the tests use. `--against` puts two renders side by side with the differences, `--json` gives the report as data, and `--pitch` names the intended note. Attack time and crest factor join the report, since factory-sound briefs (UTA-82) target attack. This is the early path: it should merge before UTA-82 starts.

**Acceptance criteria**
- [ ] `uta analyse <wav>` reads a WAV and prints peak, RMS, crest factor, loudness (today's K-weighted measure, until 3), brightness and the share above a frequency, attack (10–90% rise), decay (time to fall 20 and 40 dB), pitch and pitch over time (start, end, largest drift), and clicks (the largest step and where).
- [ ] `--pitch C3` names the intended note: the report says how many cents off the measured pitch is.
- [ ] `--against <wav>` prints both columns and the difference, and `--json` prints the report (or both, with `--against`) as data whose fields are documented in the crate.
- [ ] Attack time and crest factor are in `uta-analysis`, each checked on a synthetic signal with a known answer (a linear ramp of known rise time; a sine's crest factor is √2).
- [ ] Tests on WAVs the test renders itself: the report has every field; `--json` has the documented shape; `--against` of a render with itself shows no difference, and against a quieter copy shows only the level differences.
- [ ] Adding a measurement is a function in `uta-analysis` and one entry in the report, and the crate's docs say so.

**Out of scope:** The FFT swap and the other new measurements (3). Images and `--png` (4). Stereo: a stereo WAV is measured as its mid (left plus right) until 3.

**Depends on:** 1.

**Context:** RFC-009, Proposal parts 1 (the table's attack and crest factor rows) and 2 ("`uta analyse`", without the images); How we'll verify it ("`uta analyse` is tested on WAVs it renders itself"). Motivation: UTA-82 and 83 can use it.

### 3. [UTA-87] Swap the FFT and add the new measurements (Feature)

**Goal:** `uta-analysis` uses `rustfft`/`realfft`, so windows can be any length, sized in milliseconds, and works at any sample rate. It gains noisiness, inharmonicity, stereo correlation and width, aliasing and BS.1770 loudness, each checked on a synthetic signal with a known answer, and each in the report.

**Acceptance criteria**
- [ ] The home-made FFT is replaced by `realfft` (on `rustfft`). Measurements take the sample rate rather than assuming 48 kHz. Any sound test whose measured value moves is looked at and named in the PR, not loosened, and no golden WAV changes.
- [ ] Loudness to BS.1770: momentary (400 ms), short-term (3 s) and gated integrated, on the existing K-weighting, checked against the EBU Tech 3341 test signals (a 1 kHz sine at a known level reads the loudness the standard gives).
- [ ] Spectral flatness (white noise reads near 1, a sine near 0) and inharmonicity (a square wave's partials read as harmonic, a sum of unrelated partials as inharmonic).
- [ ] Stereo correlation and width (L = R reads correlation 1 and no width; L = −R reads −1; unrelated noise reads near 0).
- [ ] Aliasing: the energy between a known pitch's harmonics, as the loudest alias in dB, generalised from `loudest_alias_db` (the synth tests use the new one and still catch a naive saw).
- [ ] Each joins `uta analyse`'s report and `--json` (aliasing and inharmonicity when `--pitch` is given or a pitch is measured), with the tests from 2 extended to them.

**Out of scope:** Images (4). Reverb's measurements (the Reverb RFC). Roughness.

**Depends on:** 2.

**Context:** RFC-009, Proposal part 1 (the FFT and the table); How we'll verify it ("Each measurement is checked on a synthetic signal with a known answer"); Risks ("Moving the helpers could change test numbers slightly"). Research: "Hearing tools", Measurements and Rust crates.

### 4. [UTA-88] Draw the images with `uta analyse --png` (Feature)

**Goal:** `uta analyse <wav> --png <dir>` draws the first set of images as SVG and turns them into PNG with `resvg`, each with its key numbers printed on, fixed scales and labelled axes, so two images compare at a glance and Claude can sanity-check what the numbers say.

**Acceptance criteria**
- [ ] Two spectrograms, short window (about 3 ms) and long (about 50 ms), on a log-frequency axis with note names and a fixed 0 to −90 dB scale.
- [ ] Level over time in dB, with the measured attack and decay marked; waveform close-ups with each sample as a dot, at each place a click was found; harmonics on a grid when the pitch is known, where aliasing shows as bars between the lines; and with `--against`, a difference spectrogram.
- [ ] Each image has its key numbers and the file's name printed on it as real text, and keeps both the `.svg` and the `.png`.
- [ ] `resvg` is a dependency of the CLI only, not of `uta-analysis`'s measurements, the engine or the app. The PR reports the CLI's build time and binary size before and after; if the cost is too high, it says so and proposes the plain-PNG fallback.
- [ ] Tests: each image is drawn for a rendered WAV, at the expected size, and parses as SVG and PNG. Not pixel for pixel.
- [ ] The images in the listening page's reveal: whichever of 4 and 5 merges second shows each candidate's images there.

**Out of scope:** Reverb's images (impulse response, decay per band, echo density). A vectorscope and sweep strips (scratchpad ideas, later).

**Depends on:** 3.

**Context:** RFC-009, Proposal part 2 ("The images are a sanity check, not the evidence", "The first set of images"); open question 2 (SVG and resvg); Risks ("resvg's size"); How we'll verify it ("The images are checked for size and that they render").

### 5. [UTA-89] Ask Will one question at a time with `scripts/listen` (Feature)

**Goal:** Claude prepares a listening session (the question, the candidates as command lists, the kind of question, and its prediction written first) in `~/.uta/listening/`. `scripts/listen <session>` opens a local page, like Crit, and Claude waits for Will to press Finish. The page is fair by default, writes `verdict.toml`, then shows which was which and what Claude predicted. Sound tickets hand listening over this way from now on.

**Acceptance criteria**
- [ ] The session format: `session.toml` (question; candidates, each a command list beside it; kind: *pick one*, *rate each* or *listen and describe*; Claude's prediction and the measurements it relies on; loudness matching on unless turned off) and `verdict.toml` (choice or ratings, note, the gain applied to each, time), documented in the script's `--help`. Sessions live in `~/.uta/listening/`, or `UTA_LISTENING_DIR`, never in the repo.
- [ ] `scripts/listen <session>` (a `sh` wrapper → `node app/scripts/listenCli.ts`, node built-ins only) renders any candidate not yet rendered with `uta render`, measures each with `uta analyse --json`, serves the page, and exits when Finish is pressed. Several sessions can be queued in one sitting.
- [ ] Fair by default: gains match each candidate's integrated loudness; the candidates are lined up in time; they're labelled A, B, C in a shuffled order that isn't in anything the page receives before Finish; they loop and switch instantly on the same beat.
- [ ] After Finish the page shows which was which, Claude's prediction, and each candidate's numbers.
- [ ] Vitest against its own server: the gains are the ones the measured loudness asks for; the order is shuffled and absent from every response before Finish; Finish writes `verdict.toml` with every field; a session that turns matching off applies no gain.
- [ ] The process: the `work-ticket` skill says a sound ticket analyses its renders before asking, writes a prediction first, and hands every listening step over as a session; the `pr-reviewer` checks that each listening step is a session and that any test made from verdicts carries its range and a one-line reason; a Process log entry records the change, status Trying.

**Out of scope:** The images in the reveal if 4 hasn't merged (see 4). The voicing guide and sound tests made from verdicts: they come from verdicts as they build up, in the tickets that make sounds. A listening view inside Uta.

**Depends on:** 3 (BS.1770 loudness for the matching).

**Context:** RFC-009, Proposal part 3 ("Listening sessions"); open question 3 (private, outside the repo); Risks ("Loudness matching can hide a real difference", "Private verdicts aren't backed up by git", "Your time"); How we'll verify it ("The listening page is tested in vitest against its own server"). Research: "Fair listening tests".

**Will can try it (the listening checkpoint):** a session with two identical candidates, to check they can't be told apart, and a real one from the next sound ticket (UTA-83 if it hasn't merged). Will says whether it's quicker than a folder of WAVs and whether anything was confusing.

### 6. [UTA-90] Write songs in a song file, compiled by `uta song` (Feature)

**Goal:** A song can be written as a TOML file in musical terms (tempo, tracks, drum patterns, note patterns, sections, sounds, effect settings) and `uta song <file>` compiles it to a command list through the real commands, so it always builds with today's format. The demo loop and beat are rewritten as the first song files, CI renders every song in `examples/demos/`, and each PR says whether it touched the demo songs.

**Acceptance criteria**
- [ ] The song file format, documented in `examples/demos/README.md`: tempo; tracks (synth or drums), each with a name, a sound (a factory sound by name from Rust's list, then settings by name), mixer settings and effects with their settings; patterns per section (drum steps, notes with lengths); sections with bar counts and an arrangement. Anything else is written as raw commands inside the file.
- [ ] The compiler lives where both the CLI and the app can use it (a `song` module in `uta-core`, or its own crate), and `uta song <file>` prints the command list or writes it with `--out`. `uta render` and `uta play` take a song file directly.
- [ ] Compile tests: a pattern becomes the right notes; a section repeats in the arrangement; a factory sound and settings by name become the right commands; an unknown sound, setting or note is an error with its line number; compiling twice gives the same IDs.
- [ ] `examples/demos/loop.toml` and `beat.toml`, rewritten from `demo-loop.json` and `demo-beat.json`, build the same tracks and notes.
- [ ] CI renders every song in `examples/demos/` and checks each builds, doesn't clip, and has no silent track (measured with `uta-analysis`), as a test named in the PR. Not a golden WAV.
- [ ] The PR body in the `work-ticket` skill gets a "Demo song: added / not yet / doesn't apply" line, and `scripts/ticket sync` and the `pr-reviewer` accept it.

**Out of scope:** Loading songs in the app (7). The genre songs (8 to 11). Retiring the JSON command lists, which the golden and CLI tests use. Saving.

**Depends on:** 1 (for CI's checks).

**Context:** RFC-009, Proposal part 4 ("The demo song": the song file, `uta song`, "They're kept up", "CI renders"); Risks ("The song file is a small language"); How we'll verify it ("The song file has tests for compiling"). Scratchpad: "A demo song to load from the menu", "Load the demo song, loop and beat from the Develop menu".

### 7. [UTA-91] Load a demo song from the Develop menu (Feature)

**Goal:** Develop → Load demo song lists every song in `examples/demos/`. Choosing one asks first, then replaces the current project with the compiled song: tracks, tempo, effects and the master's chain, clean every time.

**Acceptance criteria**
- [ ] The songs are built into the app from `examples/demos/` and compiled with 6's compiler, so a new song needs only its file and one line in a list. The menu has the loop and the beat now, and each genre as it lands.
- [ ] Choosing one shows a confirm ("This replaces the current project"). Cancel changes nothing.
- [ ] Loading replaces the whole project as one undo step, the way the benchmark's test songs do, and plays from the start of its loop.
- [ ] Tests: in Rust, loading a song gives the same project as compiling it with `uta song` and building it, and undo brings the old project back; in the UI, against the mocked back end, the menu item asks first and sends one call.
- [ ] Checked in the real app: load the beat, play it, load the loop, undo.

**Out of scope:** Opening a song file from disk, saving, a recent-files list.

**Depends on:** 6.

**Context:** RFC-009, Proposal part 4 ("They load from the app's menu"); What it looks like to you (the last paragraph). Scratchpad: "Load the demo song, loop and beat from the Develop menu".

**Will can try it:** Develop → Load demo song → Beat, play it and change things while it plays.

### 8. [UTA-92] Write the lo-fi demo song (Feature)

**Goal:** A lo-fi song of about a minute at around 80 BPM, in a few sections: a laid-back drum loop, mellow keys, a round bass and a Delay, using the factory sounds. It's the song every feature is tried in from now on.

**Acceptance criteria** (plus the shared ones above)
- [ ] About a minute at around 80 BPM, with at least three sections in an arrangement, so the timeline and arrangement are used.
- [ ] Drums, keys and bass, each from a factory sound or the kit, and at least one Delay, each doing something audible.
- [ ] In the menu once 7 has merged.

**Out of scope:** Swing, saturation, a bitcrusher, vinyl noise and reverb: they go in its "what it's missing" list.

**Depends on:** 5, 6, UTA-82 (the factory sounds).

**Context:** RFC-009, Proposal part 4 ("Lo-fi comes first"); open question 4; How we'll verify it ("Load the lo-fi song from the menu").

**Will can try it:** load it from the menu, play it, and change a few things while it plays. Does it feel like the genre, and what's missing?

### 9. [UTA-93] Write the house demo song (Feature)

**Goal:** A 32-bar house arrangement at around 124 BPM: four-on-the-floor kick, an off-beat hat, claps, a chord stab and a rolling mono bass with glide, with a Delay.

**Acceptance criteria** (plus the shared ones above)
- [ ] 32 bars at around 124 BPM, in sections that build (for example intro, groove, break, groove).
- [ ] The 909 kick, an off-beat open hat, a clap, a chord stab and a rolling mono bass with glide, and a Delay, each audible.
- [ ] In the menu once 7 has merged.

**Out of scope:** Swing and sidechain ducking: they go in its "what it's missing" list.

**Depends on:** 5, 6, UTA-82.

**Context:** RFC-009, Proposal part 4 ("House and techno come next").

### 10. [UTA-94] Write the techno demo song (Feature)

**Goal:** A 32-bar techno arrangement at around 130 BPM: a driving kick, a hypnotic synth line slowly moved by the LFO, and its filter opening over a section.

**Acceptance criteria** (plus the shared ones above)
- [ ] 32 bars at around 130 BPM, in sections that build.
- [ ] A driving kick and hats, a repeating synth line whose sound the LFO moves slowly, and the filter opening over a section, done with a slow synced LFO or a mod slot, since there's no automation. If that can't be done well, it says so in the "what it's missing" list.
- [ ] In the menu once 7 has merged.

**Out of scope:** Distortion and automation over long builds: they go in its "what it's missing" list.

**Depends on:** 5, 6, UTA-82.

**Context:** RFC-009, Proposal part 4 ("House and techno come next").

### 11. [UTA-95] Start the ambient demo song (Feature)

**Goal:** The start of an ambient song: long pads, slow LFO movement and a long Delay. It's thin without reverb, on purpose: the Reverb project fills in the space, and it's the song Reverb is tried in first.

**Acceptance criteria** (plus the shared ones above)
- [ ] Slow pads and long notes, with slow LFO movement and a long, low-feedback Delay, over at least two sections.
- [ ] Its heading in `examples/demos/README.md` says it's unfinished and what the Reverb project should add.
- [ ] In the menu once 7 has merged.

**Out of scope:** Reverb, and finishing the song: the Reverb project does both.

**Depends on:** 5, 6, UTA-82.

**Context:** RFC-009, Proposal part 4 ("Ambient is started here, and grows with Reverb"); open question 4.

## Order and checkpoints

```
1 move ──► 2 uta analyse ──► 3 FFT, new measurements ─┬─► 4 images
   │            (before UTA-82)                        └─► 5 scripts/listen ─┐
   └─► 6 song file ──► 7 Develop menu                                        ├─► 8 lo-fi
            │                                                                ├─► 9 house
            └────────────────────────────────────────────────────────────────┼─► 10 techno
                              UTA-82 (factory sounds) ───────────────────────┴─► 11 ambient
```

- **Start now:** 1, then 2 straight after. They're short, and 2 should merge before UTA-82 starts, which waits on UTA-81 (In Progress), so UTA-82 can measure its sounds against their briefs with `uta analyse`.
- **6 can start once 1 has merged,** alongside 2 and 3. It touches the CLI's argument list, as 2 does, so whichever merges second rebases.
- **After 3:** 4 and 5 in parallel.
- **After 5, 6 and UTA-82:** the four songs in parallel. Lo-fi first if they're done one at a time.
- **Checkpoints for Will:**
  - after 5, the identical-candidates session and a real one;
  - after 7, loading the beat from the menu;
  - each song's *listen and describe* session, and after 8, loading the lo-fi song and changing things while it plays.

## Coverage

| RFC-009 verification item | Ticket |
|---|---|
| Each measurement is checked on a synthetic signal with a known answer (440 Hz, BS.1770 level, decay rate, noise flat and sine tonal, square harmonic and unrelated partials inharmonic, L = R correlation 1) | 1, 2, 3 |
| The moved helpers keep every existing sound test passing, and the goldens don't change | 1 (moved), 3 (FFT swapped) |
| `uta analyse` on WAVs it renders itself: fields, `--json`'s shape, `--against` differs only where there is a difference | 2, 3 |
| The images are checked for size and that they render | 4 |
| The listening page in vitest: loudness gains, shuffled order hidden until Finish, `verdict.toml` with every field | 5 |
| The song file's compile tests (pattern to notes, sections repeat, unknown sound errors with its line) | 6 |
| CI renders the demo songs: no clipping, no silent tracks | 6, and every song ticket |
| Manual: one real session end to end, quicker than a folder of WAVs | 5 |
| Manual: two identical candidates can't be told apart | 5 |
| Manual: load the lo-fi song from the menu, play it, change things; does it feel like the genre, what's missing | 7, 8 |

## Not in this project

- **Reverb's measurements and images** (decay per band, echo density, coloration, the impulse response): the Reverb RFC, which adds them to `uta-analysis` (open question 5). Finishing the ambient song goes with them.
- **Reference recordings:** none to hand. `--against` and the session format take one as another candidate, later.
- **Learned similarity** (CLAP, ViSQOL, FAD), **roughness**, and the scratchpad's other image ideas (vectorscope, sweep strips, a roll's hits stacked): later, if the first set leaves gaps.
- **The AI sound designer:** this project is its groundwork. Its scratchpad note stays Raw.
- **A listening view inside Uta.**
- **Swing, saturation, sidechain, automation and the rest of what the songs are missing:** listed in `examples/demos/README.md` as input to the next RFCs, not built here.
- **More songs** beyond the four: the format allows `examples/demos/<genre>.toml` for more.
- **The voicing guide and sound tests made from verdicts:** they're written by the sound tickets as verdicts build up, not by this project.

## Scratchpad

Folded in, related to RFC-009 and this project in Notion:
- "Help Claude hear renders better: analyse command, spectrograms, reference sounds, logged verdicts" → 2 to 5 (reference sounds deferred, above).
- "A demo song to load from the menu, as a sandbox for trying features" → 6 to 11.
- "Load the demo song, loop and beat from the Develop menu" → 6 (loop and beat as song files) and 7 (the menu).

Left Raw: "Check the synth's performance after the mod slots" (Synth features, before UTA-82 leans on the slots), "An AI sound designer for each instrument and effect" (its own RFC later), and the UI and tooling notes, which don't touch this project.

## Open questions

1. **Do the songs wait for the factory sounds?** RFC-009 says the lo-fi song uses them once UTA-82 lands, and every song is better for them: the bass, pad and stab presets are exactly what the songs need. Recommendation: each song depends on UTA-82. By the time 5 and 6 have merged, UTA-82 should have too. If it hasn't, the songs can start with sounds written as settings and switch to the factory names after, but that's work done twice. **Resolved: yes, each song depends on UTA-82 (Will, 2026-10-17).**
2. **The loop and the beat in the menu as well as the genre songs.** The scratchpad note asked for the demo song, loop and beat in the Develop menu. Recommendation: 6 rewrites the loop and beat as the first song files (they prove the compiler on something small and known), and 7's menu lists them with the genres. `demo-song.json` isn't rewritten: the lo-fi song replaces it as the song to try things in, and the JSON stays for the tests that build from it. **Resolved: as recommended (Will, 2026-10-17).**
