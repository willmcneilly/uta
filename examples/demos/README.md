# Demo songs

Songs written in musical terms, one TOML file each. `uta song` compiles a song file to the command list that builds it, through the real commands, so a song always builds with today's command format. See RFC-009, "The demo song".

```bash
cargo run -p uta-cli -- song examples/demos/beat.toml                       # print the command list
cargo run -p uta-cli -- song examples/demos/beat.toml --out beat.json       # or write it
cargo run -p uta-cli -- render beat.wav --song examples/demos/beat.toml     # render the song
cargo run -p uta-cli -- play --song examples/demos/loop.toml                # play it round its loop
```

Every `cargo test` compiles and builds every `.toml` file here (`every_demo_song_compiles_and_builds` in `crates/uta-song/tests/demos.rs`). CI also renders each, from the top to the end with the loop off, and checks it doesn't clip and that each track played on its own isn't silent (`every_demo_song_plays_without_clipping_and_has_no_silent_track`). That takes minutes in a debug build, so it's ignored by `cargo test`, and runs in release, in CI's "Timing reports and demo renders" job and in `scripts/check` when a song or the audio path changed. To run it yourself:

```bash
cargo test -p uta-song --release --test demos -- --ignored
```

Not a golden WAV: the songs are meant to change.

**In the app.** Every song here is built into the app and loads from Develop → Load Demo Song, which replaces the project with it as one undo step and plays it from the start of its loop. A new song needs its file here and one line in `SONGS` in `app/src-tauri/src/demos.rs`, with its file's name and what the menu calls it: `demo!("lo-fi", "Lo-fi"),`. A test fails if a file here isn't listed (`every_song_in_examples_demos_is_in_the_menu_once`).

**Keeping them up.** Each ticket's PR says `Demo song: added`, `not yet` or `doesn't apply` (the `work-ticket` skill). A feature that a song could use gets added to one, or says why not yet.

## The songs

### Loop

`loop.toml`: a bass line and chords on one synth, round a 2-bar loop at 112 BPM. Rewritten from `examples/demo-loop.json`, and builds the same track and notes.

### Beat

`beat.toml`: every sound of the kit in a 2-bar groove at 112 BPM, with a fill. Rewritten from `examples/demo-beat.json`, and builds the same track and notes.

### Lo-fi

`lofi.toml`: a minute at 80 BPM in F major, the song every feature is tried in. Soft keys play a ii-V-I-vi (Gm9, C13, Fmaj9, Dm9) through a dark, wobbly ping-pong tape delay, over a round bass and a swung boom-bap beat on the 808 kick. It runs intro, groove, groove, break, groove, outro: the break drops the kick for a turn round Bbmaj9, and the outro resolves to Fmaj9.

What the genre wanted and Uta lacks:

- **Swing**, as a setting. The song swings by hand: every part steps in 1/32 triplets, twelve to a beat, with each off-beat eighth one step late, a 58.3% swing. The finest step the song file has allows only that or 62.5% (on a 1/32 grid), and 62.5% felt unravelled; a swing setting would let it be dialled in, and leave the lines readable at a sixteenth a step instead of 48 steps a bar.
- **Saturation or a bitcrusher**, for the dusty, worn sound. The Delay's Drive warms only the echoes.
- **Vinyl noise**, crackle and hiss under everything.
- **Reverb**, so the keys and snare sit in a room. Only the Delay gives the keys any space, and the mix is nearly mono apart from its echoes.
- **An electric piano.** The keys are the Bell, played low with some sustain: closer to an FM tine than a warm Rhodes. A Rhodes wants a tine with a bark when played hard and a tremolo that pans.
- **A low-pass on the whole mix, or an EQ**, to roll off the top the way a sampled record does, and a filter sweep into the break, which wants **automation**.
- **A low bass that stays round.** The Bass sound adds a sub an octave under each note, so with the line down at F1 and G1 the sub falls to 22–25 Hz, and the low notes buzz. The line sits an octave up, from F2, instead.
- **Master gain past 0 dB, or a limiter.** The song peaks a few dB under full scale but is still quiet: the drums are at the mixer's +6 dB limit to lead the mix, and the engine holds the master's volume at 0 dB (so nothing passes full scale), though the project takes up to +6, so the song can't be made louder.

### House

`house.toml`: 32 bars at 124 BPM, in sections that build: intro (kick and hats), build (the clap and the bass come in), groove (the chord stab joins), break (no kick: one stab a bar, its echoes, the bass's roots and a snare roll) and drop (everything, with a crash and a snare under the clap). The 909 kick on every beat, the open hat on every off-beat, a seventh-chord stab (Am7, Dm7, Fmaj7, Em7) on the Pluck through a ping-pong Delay at a dotted eighth (its mix kept low, at 0.2, so the echoes don't spread too wide), and a rolling bass on the Glide bass whose notes are held a little past their step, so two side by side slide and a note after a rest starts fresh.

What house wanted that Uta doesn't have yet:
- **Swing.** House hats and bass are pushed off the grid; every step here is straight, so it sits stiffer than the genre.
- **Sidechain ducking.** The bass and the stab should dip under each kick. Without it, the kick is made to lead by the mixer alone.
- **Louder than the mixer allows.** The drums are at a track's limit (+6 dB), and the master's +6 dB is cut to 0 dB by the engine (`Snapshot::MAX_VOLUME_DB`), so the balance came from turning the synths down.
- **Automation.** A filter sweep on the stab or the bass into the drop, and the hats opening up through the build, are written today as separate sections.
- **A stab sound.** The Pluck is the nearest factory sound; house stabs are usually an organ, a piano or a sampled chord, brighter and with more body.
- **Reverb**, for the claps and the stab's tail in the break.
- **A shaker or ride.** The kit has a closed and an open hat and a crash; a groove usually has a ride or a shaker on top in the drop.

### Techno

`techno.toml`: 32 bars at 130 BPM in A minor, in five sections that build: intro (kick and hats), groove (the clap, a rolling off-beat bass and the line), break (no kick or bass: the line, the hats, a clap on beat 4 and a dub chord into a long delay), rise (everything back, the line's filter opening over eight bars) and peak (open hats, toms, the chord again, the filter open).

The line is the Pluck reworked into a resonant pulse. A triangle LFO synced to a dotted bar moves its cutoff and its pulse's width against the one-bar line all the way through. The filter opens over the rise with a mod slot: velocity → cutoff, with the rise's velocities written out to climb from 64 to 121, note by note. A second slot turns the level down with velocity, so it gets brighter more than louder.

What techno wanted that Uta doesn't have yet:

- **Automation.** The filter opening is velocities written out by hand, 72 notes of them. It moves only as each note starts, can't keep going across a section's edge, and velocity also sets the level, so a second mod slot has to undo some of that. A synced LFO can't do it: its slowest is a dotted bar. A free LFO can go slower (20 s at 0.05 Hz), but isn't tied to the bars, and starts wherever Play does.
- **Distortion or saturation,** on the line and the kick: the line stays clean and polite, where techno would drive it.
- **EQ,** to clear the low mids and give the kick room: the bottom end is muddy and lacks punch, with the kick, the bass and the line's sub all around 50–110 Hz.
- **Sidechain ducking,** so the bass ducks under the kick rather than blurring with it.
- **A bus compressor or limiter,** to make it loud and punchy. The engine plays the master at no more than 0 dB, though a project can hold up to +6, and the line is at its track's +6 dB limit already: the song peaks at -7 dBFS, -25.5 LUFS.
- **Reverb,** for the dub chord and the clap.
- **A crash at a section's start** needs a drum pattern as long as the section, since a pattern repeats; there's no fill or one-off hit across sections.
- **Swing** on the hats, less needed than in house.

The factory sounds, as they are (not tuned by ear yet, UTA-83): none is a techno line, so the Pluck needed most of its filter, envelope and LFO settings changed. Its detuned second oscillator measures as aliasing (-13.6 dB re harmonics on a held A2, -28 dB with it in tune), but it's the detune, not aliasing.

## The song file

A song is its tempo, its tracks, what each plays in each section, the sections, and the order they play in. It covers notes, drum patterns, sections, sounds and effect settings, and nothing else on purpose: anything else is written as [raw commands](#raw-commands) inside the file.

```toml
name = "Lo-fi"                      # the song's name; its IDs are made from it
tempo = 82                          # beats (quarter notes) a minute
arrangement = ["verse", "chorus", "verse"]   # optional: each section once, in order
loop = true                         # optional: the loop goes round the whole arrangement, on unless false

[master]                            # optional
volume_db = -3
effects = [{ delay = { sync = true, division = "1/4" }, mix = 0.15 }]

[[track]]
name = "Bass"
kind = "synth"                      # synth or drums
sound = "Bass"                      # a factory sound, by its key or name
settings = { cutoff_hz = 400, glide_seconds = 0.05 }   # then settings by name, on top of it
mixer = { volume_db = -4, pan = 0.0 }                  # what's left out keeps a new track's
effects = [{ delay = { time_seconds = 0.3, feedback = 0.35 }, mix = 0.2, enabled = true }]
pattern.verse = "F2 - - . . . C2 . | D#2 - - - . . . ."

[[track]]
name = "Drums"
kind = "drums"
settings.kick = { model = "909", tune_hz = 52 }
settings.closed_hat = { decay_seconds = 0.04 }
pattern.verse = { kick = "x... x... x... x...", snare = ".... x... .... x...", closed_hat = "x.x. x.x. x.x. xX.x" }

pattern.chorus = { kick = "X...", closed_hat = "x.x.", velocity = 90 }

[[section]]
name = "verse"
bars = 8

[[section]]
name = "chorus"
bars = 4
```

Top-level keys (`name`, `tempo`, `arrangement`, `loop`) come before the first `[table]`, as TOML has it.

### Tracks

Each `[[track]]` has:

- **`name`**, different from every other track's.
- **`kind`**: `synth`, or `drums` for the kit.
- **`sound`** (synth only): a factory sound by its key or its name, either case, from Rust's list (`FactorySound::all` in `crates/uta-core/src/factory.rs`): `bass`, `glide_bass`, `pluck`, `pad`, `strings`, `lead`, `sync_lead`, `bell`. Left out, the track has a new track's sound.
- **`settings`**: settings by name, set after the sound. A synth's are the names `set_synth_param` carries (`cutoff_hz`, `osc1_wave = "pulse"`, `lfo_division = "1/8d"`, …). The drums' are a table for each sound (`kick`, `snare`, `clap`, `low_tom`, `high_tom`, `closed_hat`, `open_hat`, `cymbal`) of the names `set_drum_param` carries (`tune_hz`, `decay_seconds`, `level_db`, `model = "909"`, …); the kick's model is set before its other settings, since their limits are its.
- **`mixer`**: any of `volume_db`, `pan`, `mute` and `solo`.
- **`effects`**: the track's chain, in order. Each entry names its effect with a table of its settings by name, as `set_delay_param` carries them (`delay = { sync = true, division = "1/8d", feedback = 0.4, mode = "ping_pong" }`; `{}` for its defaults), and can have a `mix` and `enabled`.
- **`pattern.<section>`**: what it plays in that section (below). A section with no pattern is silent on that track.

A value out of range, or a setting a sound doesn't have, is an error on its line, worded as the command's.

### Patterns

A pattern is played from the start of its section, and repeated to fill it: a 1-bar drum pattern in an 8-bar section plays eight times. A pattern longer than its section is an error.

**A synth line** is a string of steps separated by spaces, each a sixteenth unless the line says otherwise:

| Step | Means |
|---|---|
| `C4` | a note, one step long. C4 is middle C (60), as in `uta analyse --pitch`; sharps and flats are `C#4`, `Db4` |
| `A3+C4+E4` | a chord |
| `C4@90` | a velocity, from 1 to 127 (100 if left out) |
| `-` | hold the note or chord before one more step |
| `.` | a rest |
| `\|` | a bar line, to read by: not a step |

A line can be a table instead, to set its step, its notes' length or its velocity:

```toml
pattern.verse = { notes = "A2 A2 A3 A2", step = "1/8", length = 400, velocity = 80 }
```

- **`step`**: a note length, as a command has it (`"1/8"`, `"1/8d"`, `"1/8t"`) or as a menu does (`"1/8 dotted"`), from 1/32 to 1/1.
- **`length`**: every note in the line this long, whatever its steps: a note length, or a whole number of ticks (960 to a quarter), for notes cut short of their step.
- **`velocity`**: the velocity of a note without its own `@`.

Several lines play together as a list: `pattern.verse = [{ notes = "…" }, { notes = "…", step = "1/4" }]`.

**A drum pattern** is a table of each drum's steps, one character a step, each a sixteenth unless `step` says otherwise:

| Step | Means |
|---|---|
| `x` | a hit (velocity 100, or the table's `velocity`) |
| `X` | an accent, at 127 |
| `x90` | a hit at that velocity |
| `.` | a rest |
| spaces and `\|` | to read by: not steps |

Each hit lasts its step, so a drum table has no `length`. It can also have `step` and `velocity`, as a synth line does.

### Sections and the arrangement

Each `[[section]]` has a `name` and its length in `bars` (4/4). `arrangement` plays them in order, as many times as it names them; each time a section comes round, each track that plays in it gets a clip there, with its own notes. The loop goes round the whole arrangement.

### Raw commands

Anything the song file doesn't cover is written as a command, as a command list carries it, in a `[[command]]` table after everything else. Its `format` is today's unless it gives one. A string can name what the song made, and is replaced by its ID:

| Name | Is |
|---|---|
| `$track:<track>` | a track |
| `$clip:<track>:<n>` | the track's nth clip, counting from 1 |
| `$effect:<track>:<n>` | the nth effect in the track's chain |
| `$master_effect:<n>` | the nth effect in the master's chain |

```toml
[[command]]
type = "set_loop"
start_bar = 4
bars = 4
```

### What it builds

The project's ID is made from the song's `name`, and every track's, clip's, note's and effect's from it, so compiling the same file always gives the same commands and IDs. The project has only the song's tracks: the empty tracks a new project starts with are removed. Every mistake is an error with its line.
