# Demo songs

Songs written in musical terms, one TOML file each. `uta song` compiles a song file to the command list that builds it, through the real commands, so a song always builds with today's command format. See RFC-009, "The demo song".

```bash
cargo run -p uta-cli -- song examples/demos/beat.toml                       # print the command list
cargo run -p uta-cli -- song examples/demos/beat.toml --out beat.json       # or write it
cargo run -p uta-cli -- render beat.wav --song examples/demos/beat.toml     # render the song
cargo run -p uta-cli -- play --song examples/demos/loop.toml                # play it round its loop
```

CI renders every `.toml` file here, from the top to the end with the loop off, and checks it builds, doesn't clip, and that each track played on its own isn't silent (`every_demo_song_builds_plays_without_clipping_and_has_no_silent_track` in `crates/uta-song/tests/demos.rs`). Not a golden WAV: the songs are meant to change.

**In the app.** Every song here is built into the app and loads from Develop → Load Demo Song, which replaces the project with it as one undo step and plays it from the start of its loop. A new song needs its file here and one line in `SONGS` in `app/src-tauri/src/demos.rs`, with its file's name and what the menu calls it: `demo!("lo-fi", "Lo-fi"),`. A test fails if a file here isn't listed (`every_song_in_examples_demos_is_in_the_menu_once`).

**Keeping them up.** Each ticket's PR says `Demo song: added`, `not yet` or `doesn't apply` (the `work-ticket` skill). A feature that a song could use gets added to one, or says why not yet.

## The songs

### Loop

`loop.toml`: a bass line and chords on one synth, round a 2-bar loop at 112 BPM. Rewritten from `examples/demo-loop.json`, and builds the same track and notes.

### Beat

`beat.toml`: every sound of the kit in a 2-bar groove at 112 BPM, with a fill. Rewritten from `examples/demo-beat.json`, and builds the same track and notes.

### House

`house.toml`: 32 bars at 124 BPM, in sections that build: intro (kick and hats), build (the clap and the bass come in), groove (the chord stab joins), break (no kick: one stab a bar, its echoes, the bass's roots, a pad and a snare roll) and drop (everything, with the pad, a crash and a snare under the clap). The 909 kick on every beat, the open hat on every off-beat, a seventh-chord stab (Am7, Dm7, Fmaj7, Em7) on the Pluck through a ping-pong Delay at a dotted eighth (its mix kept low, at 0.2, so the echoes don't spread too wide), held chords on the Pad voiced from D4 to C5, over the top of the stab, and a rolling bass on the Glide bass whose notes are held a little past their step, so two side by side slide and a note after a rest starts fresh.

What house wanted that Uta doesn't have yet:
- **Swing.** House hats and bass are pushed off the grid; every step here is straight, so it sits stiffer than the genre.
- **Sidechain ducking.** The bass, the stab and the pad should dip under each kick. Without it, the kick is made to lead by the mixer alone.
- **Louder than the mixer allows.** The drums are at a track's limit (+6 dB), and the master's +6 dB is cut to 0 dB by the engine (`Snapshot::MAX_VOLUME_DB`), so the balance came from turning the synths down.
- **Automation.** A filter sweep on the stab or the bass into the drop, and the hats opening up through the build, are written today as separate sections.
- **A stab sound.** The Pluck is the nearest factory sound; house stabs are usually an organ, a piano or a sampled chord, brighter and with more body.
- **Reverb**, for the claps and the stab's tail in the break.
- **A shaker or ride.** The kit has a closed and an open hat and a crash; a groove usually has a ride or a shaker on top in the drop.

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
