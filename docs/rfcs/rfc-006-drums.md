# RFC-006: Drums

**Status:** Accepted (2026-10-10) · **Author:** Will · **Notion:** https://app.notion.com/p/Drums-3f53af969b6f81c2842fc37c520e89f1

## Summary

Add a second kind of track: a drum machine with eight synthesised sounds (kick, snare, clap, closed and open hi-hat, low and high tom, and cymbal), modelled on the Roland TR-808 and TR-909 and with the controls they had (Tune, Tone, Decay, Snappy, Level). The goal is that it sounds good, not just that it makes drum noises. So each sound is built the way the original circuits work: a hit sets a resonator ringing that never stops running, and velocity changes the tone of a hit, not just its loudness. A drum track's Notes tab shows the piano roll with eight labelled rows instead of the keyboard, and its Sound tab shows each sound as a strip of knobs with a Level fader. A new project starts with a synth track and a drum track.

## Motivation

- **Uta has one sound.** A song needs rhythm, and the synth can't make convincing drums. Drums are the cheapest way to make Uta feel like somewhere you'd make a track.
- **It's the first step in the sound-first stretch.** After this project come effects and a better synth. Until Uta sounds like something you want to make music in, it's a hobby project that could be dropped, so sound quality is the measure of this project, not feature count.
- **The model is ready for it.** RFC-003 outlined drums and checked them against the design: a track's source is already a list of kinds (with one kind today), each track has its own slot and voices, and the Notes and Sound tabs already switch with the selected track. This adds the second kind.
- **It's the first source that isn't a plain synth.** Getting a second source right (its commands, its own editor rows, its own panel, which tracks a clip can move to) is the groundwork for every source after it, including plugins.

**When:** after the Drafting table project (RFC-005) finishes, including its controls tickets (UTA-44 to UTA-46). The drum panel is built from the knob, fader and segmented choice they add, so it looks and behaves like the synth panel from the start.

## Proposal

### The kit

Eight sounds, each triggered by its General MIDI drum note, so a MIDI file or a MIDI keyboard plays the right sound later. Each sound follows the original whose version is the classic one. The 909's hi-hats and cymbals were recordings, not circuits, so those come from the 808.

| Sound | Note | Model | Controls |
|---|---|---|---|
| Kick | C2 (36) | 808, with a 909 model to choose (open question 1) | Tune · Tone · Decay · Level (909: Tune · Sweep · Attack · Decay · Level) |
| Snare | D2 (38) | 909 | Tune · Tone · Snappy · Level |
| Clap | E♭2 (39) | 808 | Tone · Decay · Level |
| Closed hat | F♯2 (42) | 808 | Tune · Tone · Decay · Level |
| Open hat | B♭2 (46) | 808 | Decay · Level (shares the closed hat's Tune and Tone) |
| Low tom | A2 (45) | 808 | Tune · Decay · Level |
| High tom | D3 (50) | 808 | Tune · Decay · Level |
| Cymbal | C♯3 (49) | 808 | Tune · Tone · Decay · Level |

The controls are the original's front panel, plus Tune where the original had none and a Decay on the toms (the 909's toms have one, the 808's don't). That's "the hardware, plus tuning", which is what you asked for (open question 4). What each one does:

- **Tune** sets the pitch: the kick's note, the snare's shell, the toms, or the whole metallic cluster of the hats and cymbal.
- **Decay** sets how long the sound rings.
- **Tone** sets brightness: on the kick it tames the click, on the snare it's the length of the rattle (as on the 909), on the hats, clap and cymbal it moves their filters.
- **Snappy** sets how much snare rattle there is against the drum's body.
- **Level** is in dB, and the default kit is balanced by measurement (below).
- On the 909 kick, **Sweep** is how far the pitch drops at the start (the 909 calls it Tune), and **Attack** is how much click there is.

The kit is fixed: one kit per drum track, these eight sounds, these rows. Choosing a different model per row, or loading other sounds, is later work (see Not in this project).

### How each sound is made

A Glossary-style word first. Most of the 808 works like a struck object: a short electrical pulse (the **excitation**) hits a **resonator**, a circuit that rings at its own pitch and dies away, like a bell or a drum head. That's different from Uta's synth, where an oscillator starts and an envelope shapes its volume.

- **Kick (808).** A pulse sets a resonator ringing at about 49 Hz. For the first 6 ms or so the resonator rings more than an octave higher, which you hear as punch, not as a sweep. Then the pitch sags slightly, from about 56 to 49 Hz over 300 ms. Tone is a low-pass after it that also lets some of the pulse's click through.
- **Kick (909).** An oscillator shaped towards a sine, with a fast downward pitch sweep, its own volume envelope, and a separate click of filtered noise.
- **Snare (909).** Two tones at about 180 Hz and 1.47 times that (the shell), plus filtered noise (the wires). The noise holds for 40 to 70 ms before it decays, which is a large part of the 909 snare's sound.
- **Clap (808).** Noise through a band-pass at about 1 kHz, with an envelope of three quick bursts 10 ms apart and then a fourth (several people clapping), plus a smooth 100 ms tail (the "reverb").
- **Hats (808).** Six square-wave oscillators at the 808's own odd frequencies (205.3, 304.4, 369.6, 522.7, 540 and 800 Hz) make a clangy metallic cluster, which is filtered down to its top end and shaped by a short envelope. The open and closed hat are one sound circuit, as on the 808, so a closed hit cuts off a ringing open hat (**choke**).
- **Toms (808).** Like the kick, a struck resonator, at higher pitches, with a small pitch drop at the start and a little low noise for the skin.
- **Cymbal (808).** The same six oscillators split into three bands, each with its own envelope: the low band rings longest and the high band is a short sizzle. Tone mainly turns the sizzle down.

Where these come from: Kurt Werner's circuit analyses of the 808 kick and cymbal, Roland's TR-08 and TR-09 manuals, the Sound On Sound "Practical synthesis" series, and the open-source drum code in Mutable Instruments' Plaits (MIT licence), which we read and credit, and port where it fits. There's no usable Rust library. The research is in Notion's Research database ("Drum machine and metronome", extended for this RFC).

### What makes it sound good

The research's clearest finding: digital drums sound thin and "machine-like" for a handful of known reasons, each with a known fix. These rules apply to every sound:

1. **Never restart a sound from scratch on a hit.** The resonators and oscillators keep running, and a new hit adds energy to what's already there. A drum that restarts its waveform clicks, and on fast repeats every hit is identical, which is the "machine-gun" sound of a cheap drum machine. A resonator that keeps running doesn't click, and two quick hits sum naturally, each one slightly different, as on the hardware.
2. **Velocity changes the tone, not just the volume.** On the 808 and 909, an accented hit has a harder pulse, so it's brighter and punchier, not only louder. Velocity sets the strength of the excitation, and the tone follows. Velocity 100 sounds like an unaccented hit on the original, and 127 like a full accent.
3. **Curves shaped like the real circuits.** Envelopes die away exponentially, as a capacitor discharges, never in a straight line. Even an instant attack is smoothed over a few samples, so it doesn't click.
4. **No aliasing in the hats and cymbal.** Digital audio stores 48,000 samples a second, and that can only hold tones up to 24 kHz. A square wave has harmonics far above that, and the ones that don't fit don't disappear: they fold back down as false tones at the wrong pitch, called **aliasing**. It's the audio version of a wagon wheel in a film seeming to spin backwards because the camera can't keep up. The hats and cymbal are built from six square waves and then filtered down to their top end, which is exactly where the false tones land, so they suffer most. Two fixes, which stack:
   - **PolyBLEP** smooths each corner of the square wave so it makes fewer harmonics that don't fit. The synth already uses it.
   - **Oversampling** makes the sound at twice the rate (96,000 samples a second), where those harmonics fit, filters them out, then converts back down to 48,000. It's the same idea as rendering an image at 2× and scaling it down so the edges don't look jagged.

   Measured in the research, the false tones sit about 9 dB below the hats' own sound with neither fix (clearly there), 18 dB below with PolyBLEP, and 30 dB below with both. The overall character barely changes either way: it's a faint, gritty layer, not a different sound. PolyBLEP is used regardless. Whether oversampling is worth it too is decided by listening (open question 2).
5. **A little drive in the right places.** The originals distort slightly in specific places: the kick's pulse, the hats' and cymbal's amplifiers, the snare's shell. These are modelled. A distortion over the whole kit is not.
6. **Natural variation without randomness.** The hats' oscillators and the noise run freely, so each hit catches them at a different moment and sounds a little different, as on the 808. But it's deterministic: the same song always renders to the same audio. That's needed for the golden WAVs and for "what you hear is what you render". The free-running state starts from the same point each time playback starts (open question 3).
7. **A balanced default kit.** Every sound's default Level is set by measuring loudness, not by guessing. The targets: kick 0 dB, snare and clap 2 to 4 dB below it, toms 6 dB below, hats and cymbal 10 to 12 dB below, and everything hitting at once peaks under −1 dBFS.

### In the engine

- **One circuit per sound, not a pool of voices.** The synth has 16 voices and assigns notes to them. The 808 had one circuit per sound, and a new hit retriggers it. Uta does the same: seven circuits per drum track (the two hats share one). There's no voice allocation and no voice stealing, and the work per block is fixed, which suits the audio thread rules.
- **Drums are one-shots.** A drum sound plays out in full however long the note is, and the end of a note does nothing. Notes on a drum track keep a length, because notes always have one and you might change a track's sound later, but it doesn't affect the sound.
- **Drum tracks don't chase notes.** Starting playback in the middle of a drum note doesn't play the hit half-way through, because that sounds wrong (as RFC-003 decided).
- **Same audio thread rules as everything else.** Nothing allocates or waits. Every sound's state is plain numbers, and a sound that has died away stops doing work (also avoiding the slow "denormal" numbers a decaying sound produces near silence).
- **Settings changes glide,** as the synth's do, so turning a knob while a beat plays doesn't click.

### In the project

- **A track's source gets a second kind, Drums,** holding the kit's settings. A new command sets one drum setting on one sound, and like `SetSynthParam`, a knob drag is one undo step. Commands go to format 4, and format 3 command lists still load.
- **The kit's rows come from Rust.** The outline tells the UI each row's name and note, so the UI never hard-codes the kit.
- **Notes on a drum track must be on one of the kit's notes.** The core refuses a note on any other pitch, so a note can never sit hidden and silent on a drum track. Pasting notes from a synth clip into a drum clip keeps only the notes that land on the kit's rows, and says so if any were left out.
- **Clips move only between tracks of the same kind,** as RFC-003 decided. The core refuses a move to the other kind, and the timeline skips over tracks of the other kind as you drag.
- **New tracks.** "+ Add track" offers Synth or Drums. Drum tracks are named "Drums 1", "Drums 2" and so on. A new project starts with Synth 1 and Drums 1, each with an empty 4-bar clip.

### In the window

- **Notes tab on a drum track:** the same piano roll, with eight labelled rows in place of the keyboard, from kick at the bottom to cymbal at the top, tall enough that all eight show without scrolling. Clicking a row's label plays its sound. Drawing, moving, resizing, selecting, copy, paste, duplicate and velocity all work as on a synth track. Dragging a note up or down moves it to another row, and the arrow keys that move notes a semitone move them a row.
- **Sound tab on a drum track:** the eight sounds side by side, each a strip with its name, its knobs and a Level fader (the layout is open question 6). Clicking a sound's name plays it.
- **Two piano-roll comforts from the scratchpad, for both kinds of track:** ⌥-drag duplicates the selected notes, and resizing one selected note resizes all of them together.

### The drum panel's controls

The drum panel follows the control rule in RFC-005 part 7: each drum's Level is a **fader**, because levels are worth comparing side by side, and its Tune, Tone, Decay and Snappy are **knobs**, because each stands on its own and 27 sliders wouldn't fit. The kick's Model is a **segmented choice**. All three come from the Drafting table project (UTA-44 to UTA-46), with their shared behaviour: Shift-drag for fine adjustment, no jump on press, and reset by double-click, ⌥-click or Delete. This project only lays them out.

**The layout.** Two layouts fit the rule:
- **One strip per sound, like the 808's front panel:** each sound's name, its knobs, and a short fader for its Level underneath, side by side across the panel, so the Levels line up like a mixer.
- **A grid of value bars:** sounds as rows and controls as columns, each cell a drag field with a fill showing its position. It's the densest and flattest. But the columns don't really compare (a kick's Tune and a cymbal's Tune mean different things), which is a grid's main advantage.

Recommendation: the strip per sound, because it follows the rule (Level is the one setting worth comparing) and it's what the originals looked like. Following the design process, both are sketched in the design sandbox before the panel ticket starts, and the chosen one is checked in the real app (open question 6).

### Not in this project

- Swing. It will be a feature of the piano roll for every track.
- More sounds (mid tom, rimshot, cowbell), a choice of model per row, or samples.
- The 808 snare as an alternative model.
- A kit-wide Accent knob (open question 5).
- Separate outputs, panning or effects per drum sound. The drum track has one mixer channel, like any track.
- A step-sequencer view.
- Saving kits as presets.
- Building or changing controls. The knob, fader and segmented choice, and their shared behaviour, are RFC-005 part 7's.
- Recording and the metronome (Play it in).

## What it looks like to you

You open Uta. There are two tracks, Synth 1 and Drums 1, each with an empty 4-bar clip. You click the drum clip, and the piano roll below shows eight rows labelled Kick, Snare, Clap, Low tom, High tom, Closed hat, Open hat and Cymbal. You click "Kick" and hear a deep, round 808 kick.

You draw a kick on every beat, a clap on 2 and 4, and closed hats on every eighth note, and press Play. It loops. You turn some of the hats down in velocity and hear them get softer *and* duller, the way a drummer playing quieter sounds. You put an open hat on the off-beat, and the closed hat that follows cuts it short, as on an 808. You ⌥-drag the hats to copy them along the bar.

On the Sound tab you see the eight sounds side by side, each with a few small flat knobs and a Level fader, like the front of an 808. You drag the kick's Decay up while it plays and the kick turns into a long, booming 808 bass. You tune it down to match your bassline. You turn the snare's Snappy up for more rattle, and pull the cymbal's Tone down to take the sizzle off. Double-clicking any knob puts it back.

You put two snare hits very close together for a flam. Neither clicks, and the second one doesn't sound like a copy of the first. You play the same bar over and over, and the hats sound alive, not looped, but rendering the song twice gives the same file.

You switch the kick to the 909 model and it gets tighter and punchier, with an Attack knob for its click. You try to drag the drum clip onto the synth track, and it won't go. You add a second drum track for a separate percussion part.

## Alternatives considered

- **Samples instead of synthesis.** Samples sound real immediately, and the 909's own hats and cymbals were samples. But they bring audio files, loading from disk, licensing, and a dependency on saving, and you can't tune or reshape them the way the original knobs do. Synthesis is what the 808 was, it's tunable, and it fits a project with no file handling yet. Samples can be a later kind of source.
- **A plain "oscillator plus envelope" for every sound.** It's simpler, and it's how many basic drum synths work. But it's exactly what makes drums sound thin: the waveform restarts on every hit, so it clicks and machine-guns, and velocity can only change the volume. The research found the struck-resonator structure matters more than any fine detail of the circuits.
- **Circuit-level modelling (like Roland's ACB).** Modelling every component would get closest to the hardware. But it's a large research job per sound, and Werner's own conclusion is that the 808's structure matters more than subtle component behaviour. Modelling the structure and a few key nonlinearities, as Plaits does, gets most of the way for a fraction of the work.
- **Use the synth's voice pool for drums.** It would reuse existing code. But drums don't want stealing or polyphony per sound, and one circuit per sound is what makes a closed hat choke an open one and what makes fast repeats sum naturally.
- **A step sequencer instead of the piano roll.** It's faster for a basic beat and very 808. But you preferred the piano roll's consistency, it already does velocity, selection and copying, and it allows hits off the grid. A step view could be added later.
- **Random variation on each hit (slight detune or level changes).** It's a common trick for liveliness. But it makes renders unrepeatable, and the free-running oscillators and noise already give natural variation the way the hardware did.
- **Oversample the whole kit.** It's the bluntest fix for aliasing. But only the hats and cymbal have high enough content to need it, and PolyBLEP handles most of it there. The kick, snare, toms and clap don't benefit.
- **Allow notes on any pitch on a drum track and ignore the extra ones.** It's simpler in the core. But those notes would be invisible in the eight-row piano roll and silent, which is confusing and wasteful.
- **Other controls for the drum settings** (sliders, or drag fields as in Ableton's drum synths). Settled for the whole app by RFC-005 part 7's rule.

## Risks & unknowns

- **"Sounds good" is a judgement, and only you can make it.** Tests can show a kick has the right pitch and decay, not that it's satisfying. So the tickets are split by sound, and each one ends with renders for you to listen to, before the next sound starts. The plan should allow a tuning pass at the end.
- **The cymbal is the hardest sound.** Metallic sounds are notoriously hard to synthesise, and no open-source cymbal model is usable as code (Sonic Pi's has an informal licence, so it's for ideas and numbers only). My guess is that it takes the most listening rounds. If it isn't good enough, it could ship later than the rest rather than hold them up.
- **Some figures come from secondary sources.** There are no published figures for the 909's toms, and the relative levels in the original machines aren't documented anywhere we found. The level targets are a starting point, calibrated by measurement and then by ear.
- **CPU.** Seven circuits per track, with the hats and cymbal possibly at 2×, should be cheap next to 16 synth voices. That's a guess until the timing report measures a kit playing every sound at once.
- **The new-project default changes.** Adding Drums 1 to every new project changes the starting point the example command lists build on. The examples and tests that count tracks need updating. The existing golden WAVs shouldn't change, because the drum track is empty, and if they do, you re-approve them.
- **The panel is the knob's densest use.** UTA-45 builds the knob for pan and the synth panel, and leaves a smaller size to this project. Eight strips of small knobs may need a smaller size than either. If it does, the panel ticket adds a size to the knob rather than a second kind of knob.

## How we'll verify it

**Automated, on every PR:**
- **Each sound, measured** (no standard exists for testing drum synths, so these borrow from audio analysis, and the thresholds are calibrated once against the reference behaviour):
  - **Pitch:** the kick and toms end within tolerance of their Tune setting, and the kick starts higher than it ends. Raising Tune raises the measured pitch of every tuned sound.
  - **Decay:** the measured decay time follows the Decay control, and doubling Decay roughly doubles it.
  - **Brightness:** measured as the spectral centroid (the average frequency, weighted by level). The kick is well under 200 Hz, the hats are in the kHz range, and Tone moves brightness the right way on every sound with a Tone control.
  - **Clap:** three or four bursts about 10 ms apart in the first 40 ms.
  - **Choke:** an open hat followed by a closed hat is far quieter 60 ms after the closed hit than an open hat left ringing.
  - **Velocity changes tone:** a hit at velocity 127 is brighter than one at 64, not just louder.
  - **No clicks:** a hit, a fast repeat of the same sound, a flam, and a knob turn during playback all stay under the click limit.
  - **Aliasing:** the metal sound's alias level stays at or below the agreed figure, so it can't quietly regress.
- **Determinism:** rendering a beat twice gives identical files, and blocks of 32, 128 and 1024 give the same audio.
- **A golden WAV** of a demo beat from a committed command list, `examples/demo-beat.json`, and per-sound renders. You approve every golden file in the PR.
- **Real-time safety:** a render that plays every sound, chokes, flams, and turns every control while playing runs under `assert_no_alloc` and RealtimeSanitizer.
- **Timing report:** adds a case with drum tracks playing every sound at once, at p50 and p99. Reported, not gated.
- **Project core:** the new commands undo exactly and round-trip through serialisation, a knob drag is one undo step, notes off the kit's rows and clip moves to the other kind are refused, and format 3 command lists still load and render the same.
- **UI:** against the mocked back end: the eight rows come from the outline, clicking a row label or a sound's name plays it, notes drawn and moved between rows send the expected commands, ⌥-drag duplicates the selection, and resizing resizes the whole selection.

**Manual, by you:**
1. **Listen to each sound as its ticket finishes.** Each PR has short renders: the sound at its defaults, sweeps of each control, a velocity ramp, and fast repeats. Judge each against how you remember the original.
2. **The 2× oversampling A/B** for the hats and cymbal (open question 2): two renders, told apart only by letter. Say which you prefer, or that you can't tell.
3. **Make a beat** in the app: kick, clap, hats, an open hat choked by a closed one, and a tom fill. Tune the kick to a bassline. It should feel like a drum machine, not a test.
4. **Turn knobs while it plays.** Nothing clicks or jumps.
5. **Check the kit's balance** at its defaults: nothing jumps out or disappears.

## Open questions

1. **One kick or two models?** The kick matters most, and the 808 and 909 kicks are the two most famous, built in different ways. Plaits has a reference for both. Recommendation: one Kick row with a Model choice, 808 by default, so you can have either. The panel shows the chosen model's knobs. If the project runs long, the 909 model is the first thing to move out. **Resolved: one Kick row with an 808/909 Model choice (Will, 2026-10-10).**
2. **Oversampling for the hats and cymbal?** That is, make the hats and cymbal at twice the sample rate and convert back down, to remove more of the false tones (aliasing) square waves create. See "What makes it sound good", point 4. PolyBLEP is used either way. Recommendation: decide by a blind listening test in the hats ticket: two renders of the same beat, one with and one without, labelled only A and B. Its cost is small, so if you can hear a difference, keep it. **Resolved: PolyBLEP always, and 2× oversampling settled by the A/B listening test in the hats ticket (Will, 2026-10-10).**
3. **Restart the free-running oscillators and noise when playback starts?** If they restart, playing from bar 1 always sounds exactly like the render. If they never restart, it's slightly more like the hardware, but what you hear each time differs a little from the render. Recommendation: restart them on every Play, so what you hear is what you render. **Resolved: restart on every Play (Will, 2026-10-10).**
4. **Controls beyond the original's?** Recommendation: the original panels plus Tune wherever it's useful and Decay on the toms, as in the table. Leave out extras such as clap spread or tom bend for now, since they can be added later without changing anything else. **Resolved: the original panels plus Tune, and Decay on the toms (Will, 2026-10-10).**
5. **A kit-wide Accent knob?** The 808 and 909 had a global Accent that set how much accented steps stood out. With velocity on every note, that's a fixed curve for now. Recommendation: no knob now. Revisit when the synth project looks at velocity across Uta. **Resolved: no Accent knob now (Will, 2026-10-10).**
6. **The drum panel's layout: a strip per sound, or a grid of value bars?** Recommendation: a strip per sound (knobs plus a Level fader), sketched against the grid in the design sandbox before the panel ticket, and checked in the real app. **Resolved: a strip per sound, sketched against the grid first (Will, 2026-10-10).**
7. **Is the control rule right?** **Resolved, then moved: the rule, the shared behaviour and the components became RFC-005 part 7, accepted 2026-10-10 (Will).**

## New terms

- **Excitation:** the short burst of energy, such as a pulse or a puff of noise, that sets a resonator ringing, like the stick hitting a drum.
- **Resonator:** a filter that rings at its own pitch when struck and dies away on its own, the way most of the 808's drums make their tone.
- **Free-running:** an oscillator or noise source that keeps running between notes instead of restarting on each one, which gives each hit a slightly different start.
- **One-shot:** a sound that plays out in full when triggered, whatever the length of the note, as drum sounds do.
- **Choke:** one sound cutting another off, as a closed hi-hat stops an open one ringing.
- **Accent:** a hit played harder, which on the 808 and 909 makes it brighter and punchier as well as louder. In Uta it comes from note velocity.
- **Drum lane:** one labelled row in a drum track's piano roll, for one sound of the kit.
- **Spectral centroid:** the average frequency of a sound, weighted by level, used in tests as a measure of brightness.
