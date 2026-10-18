# Voicing the synth

How to turn an idea for a synth sound into Uta's synth settings: which settings give which qualities, and what making the factory sounds taught about them. It's for whoever makes sounds next, a person or an agent (RFC-008, "Factory sounds, and turning an idea into settings", and its open question 2).

The settings themselves, with their ranges and a sentence on how each sounds, are described in Rust, in `crates/uta-core/src/synth.rs` (`SynthParam::describe`). The factory sounds, each with its brief, audition and targets, are in `crates/uta-core/src/factory.rs`.

## The loop

Every factory sound is made the same way:

1. **Brief.** A sentence or two of what it should sound like, with a reference ("a round, short Juno pluck that sits under a chord"), and what it's for (the register, the part it plays).
2. **Targets.** Four measurable numbers, each a range, taken on the sound's **audition** (one note, or one chord for a pad, held for a set time at velocity 100):
   - **brightness**: the spectral centroid of the whole render, in Hz;
   - **attack**: the rise from 10% to 90% of the peak, in ms;
   - **fade**: from the first sound until it has faded for good to 40 dB under its loudest, in seconds;
   - **loudness**: the K-weighted RMS while it's held, against the default sound (a new track's saw) playing middle C for a second, in dB. Every factory sound aims for the default's loudness, within 1.5 dB, so all of them are within 3 dB of each other.
3. **Settings.** A first guess, from the table below.
4. **Render and measure.** `cargo test --release -p uta-engine --test factory -- --nocapture` prints every sound's numbers against its targets, and fails on any miss. `UTA_AUDITIONS=<folder>` also writes each audition as a WAV, for `uta analyse` or for listening.
5. **Listen.** `examples/listen/factory/` has a 2-bar phrase for each sound in the part it's for (see `examples/listen/README.md`).
6. **Adjust,** and go round again. When a sound changes, rewrite its listening render with `UTA_GOLDEN=1 cargo test -p uta-engine --test factory`, then the golden WAVs of the three that have one with `UTA_GOLDEN=1 cargo test -p uta-engine --test sound`, in that order, since the goldens are rendered from the listening renders. A person approves every golden change.

## Which settings give which qualities

| Quality | Turn | Notes |
|---|---|---|
| **Bright ↔ dark** | Cutoff first, then Env amount (how far it opens), then Resonance | Key at 0.5 to 0.7 keeps high notes from going dull and low ones from going buzzy, so one cutoff works across a part. |
| **Plucked ↔ swelling** | Filter Decay with Filter Sustain at 0 (pluck); Amp Attack and Filter Attack (swell) | A pluck is mostly the filter closing, not the volume dying: give the amp a longer Decay than the filter and let the filter do the plucking. |
| **Short ↔ long** | Amp Decay with Sustain at 0 (it dies while held); Amp Release (it rings on after) | A bell or pluck dies while held; a pad holds and fades on release. |
| **Thin ↔ fat** | Osc 2 a few cents off Osc 1 (0.05 to 0.12 semitones); the Sub an octave down | Detune thickens notes from about C3 up. Under that, use the Sub or Osc 2 an octave down instead (see "Detuning low notes"). |
| **Nasal, hollow** | Pulse, Width away from 0.5 | Moving the Width with the LFO (LFO Width) is the string-machine chorus. |
| **Moving ↔ still** | LFO to the cutoff (slow, small: 0.2 to 0.4 octaves), LFO Width, Vibrato (0.08 to 0.15 semitones at 5 to 6 Hz) | Small amounts read as life; large ones read as an effect. |
| **Vocal "yow"** | Osc 2 Sync on, a mod slot from the Filter env to Osc 2 pitch | The sweep is in the timbre, not the pitch: the note stays at Osc 1's. |
| **Glassy, metallic** | Two sines, Osc 2 at an inharmonic ratio (Octave +1, Pitch +5.57 is 2.76 times, a struck bar's second mode), its level from the Filter env through a mod slot | Inharmonic partials say "struck"; letting them fade fast leaves a clean tone. |
| **Connected line** | Mono, Legato, Glide 0.03 to 0.08 s | Notes have to overlap to slide; a note after a gap starts fresh. |

## What making the factory sounds taught

Each of these came from a measurement that disagreed with the first guess.

- **An envelope's time is how long it takes to fall 60 dB.** Uta's decay and release curves land on their end level exactly when their time is up (`synth/envelope.rs`), and they're exponential, so they're 40 dB down at about two thirds of the time, and most of what you hear is in the first third. The Bell first faded in 1.0 s with a 2 s Decay and a 1.5 s Release; it needed 3 s and 2.5 s to ring for a couple of seconds. The Pluck's filter, with a 0.15 s Decay, was effectively shut after 50 ms, a blip rather than a pluck; 0.4 s gave the brief's "closing over about 150 ms".
- **The whole-render brightness can't tell a blip from a pluck.** The Pluck read about 1,000 Hz either way. The sweep only showed in the brightness measured over time, in 85 ms windows: from about 470 Hz to 280 Hz within 50 ms before, and from about 640 Hz over about 100 ms after. A brief about how a sound moves needs a measure over time, not only over the whole render.
- **Detuning low notes makes the level swell, not the tone thicker.** The Glide bass first had its pulse 0.08 semitones under its saw. At 65 Hz the two drift in and out of phase about once every three seconds, so the level climbed for the whole half-second note, and its "attack" read 368 ms, because its loudest moment was the end of the note. Osc 2 an octave down instead gives the weight without the drift.
- **A saw and a pulse at the same pitch partly cancel.** With the oscillators restarting on each note, the Lead's saw and pulse at the same pitch measured about 8 dB quieter than with a few cents between them, and with a few cents the level crept up over the first 70 ms as they drifted out of cancelling. Two saws, which start in phase, add instead. Whether two waves add or cancel depends on how their edges line up, so check the level when you change a wave.
- **The attack measure follows the peak sample.** A slow creep in level after the real attack (beating, a resonant filter settling) counts as attack. When the attack reads far longer than the Amp Attack, look at the level over time before changing the envelope.
- **A low note can't start faster than about one cycle.** At C2 a cycle is 15 ms, so the basses' attack reads 13 to 15 ms with a 2 to 3 ms Amp Attack. Their briefs allow up to 25 ms.
- **Short sounds read quiet on an average.** The loudness while held is an average over the held time, so a pluck dying inside its 250 ms reads quieter than it sounds next to a pad, and its peak is the highest of all the sounds. The Pluck got a longer amp Decay (1.2 s, with the filter still doing the plucking) and the Sub for body, which also made it rounder. A fairer loudness for short sounds is BS.1770's momentary loudness (the loudest 400 ms), which `uta-analysis` now has (`loudness_over_time` with a 400 ms window, from UTA-87); the targets haven't moved to it yet.
- **Chords add up.** The pads are auditioned on a 3-note chord, as they're played, so their levels are set for chords: about 0.45 each for two saws, against 0.8 for one saw and a sub on a single bass note. In the listening renders the pads play 4-note chords and come out 2 to 3 dB louder than the single-note sounds.
- **"Largest step" isn't a click detector for bright sounds.** A saw's reset is its steepest step by nature, so with the filter open a note's start reads a step near the peak (-5 to -8 dB re peak in the Pluck, Lead and Sync lead renders). The switching test (`tests/factory.rs`) holds a switch to the sounds' own steepest steps instead.

## What needed an ear

The measurements can say a sound is as bright, as fast, as long and as loud as its brief asks. They can't say it's the sound the brief means. These are the questions left for listening, with what the numbers led us to expect:

- **Pluck:** round under a chord, or still a little "blippy"? The filter's sweep was judged from the brightness over time, not by ear.
- **Pad:** is the slow filter drift (0.3 octaves at 0.25 Hz) heard as movement, or not at all? Does the breath of noise (0.03) come through?
- **PWM strings:** does the Width moving read as an ensemble, and is it clearly thinner and brighter than the Pad?
- **Sync lead:** does each fresh note tear in, and does the steady tone after it sound nasal rather than harsh? The sweep measured from about 2,500 to 1,700 Hz over 0.3 s.
- **Glide bass:** do the slides feel rubbery at 80 ms, and does the resonance squelch without whistling?
- **Bell:** glassy rather than metallic? The inharmonic partial is gone within about 150 ms.
- **Balance:** do they sound equally loud when switched in a loop, given the pads' chords?

**Will's verdict (2026-10-18, UTA-82): "The sounds themselves are good."** He asked for no changes to any sound, so the settings above are what the brief-and-measure loop landed on, unadjusted by ear. The questions above stay open for the tuning pass (UTA-83).

What listening taught about the loop itself: judging a sound needs **a consistent canvas** of parts to play it in. Writing chords, basslines and lead lines to test each sound takes Will a lot of work, and which ones he picks can mask what needs fixing. The listening renders (`examples/listen/factory/`) give each sound one phrase in its role, but they're files to play, not parts to try sounds against in the app. A fixed set of parts, one per role, to switch sounds under is on the scratchpad, and RFC-009's demo songs (UTA-91, UTA-92) are the nearest planned thing.
