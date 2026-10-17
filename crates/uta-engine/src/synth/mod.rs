//! The built-in synth: 16 voices, each two oscillators, a sub-oscillator and
//! noise, mixed into a low-pass filter and an envelope. Oscillator 1's pulse
//! has a width, and oscillator 2 can sync to oscillator 1. See RFC-002, "The
//! synth is ours, small and fixed-size", and RFC-008, "In the engine".
//!
//! Everything here runs on the audio thread. The voices are a fixed array
//! created with the processor, and a voice remembers its note only by a
//! [`NoteKey`], a plain number, so starting, stopping and taking over notes
//! never allocates or frees.

mod envelope;
mod filter;
pub(crate) mod oscillator;
mod settings;
#[cfg(test)]
mod sync_ab;

pub use settings::{SynthSettings, Waveform};

use envelope::{Envelope, Stage, Times};
use filter::{Coefficients, Filter};
use oscillator::{Decimator, Oscillator, OversampledSync, SQUARE};

use crate::noise::Noise;
use crate::ramp::Ramp;

/// How many notes can sound at once.
pub const VOICES: usize = 16;
/// How long a voice that's taken over by a new note fades out before the new
/// note starts in it.
pub const TAKE_OVER_SECONDS: f64 = 0.005;
/// How long a change to the synth's settings takes to glide to its new value.
/// A new wave or sub octave, or switching Sync, crossfades from the old one
/// over the same time.
/// An octave changes at once: the oscillators carry on from where they are
/// in their cycle, so the wave doesn't jump.
pub const SYNTH_SMOOTHING_SECONDS: f64 = 0.02;
/// Each voice's peak level at full velocity, before the master volume: -12 dB,
/// so four full-velocity notes together reach full scale.
pub const VOICE_LEVEL: f32 = 0.25;

/// Identifies a sounding note, so it can be stopped later. It's a plain
/// number: project notes use their permanent ID's UUID
/// (`Uuid::as_u128`), and live notes use any number the caller chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteKey(pub u128);

/// A note's frequency in Hz, from its MIDI note number (A4, 69, is 440 Hz).
pub fn pitch_to_hz(pitch: u8) -> f64 {
    440.0 * 2f64.powf((f64::from(pitch) - 69.0) / 12.0)
}

/// A velocity's level, as a linear gain: `(velocity / 127)²`, which is
/// 40 × log10(velocity / 127) in dB. Velocity 64 is about -12 dB, and 127 is
/// full level. It's the curve the MIDI DLS standard uses.
pub fn velocity_to_gain(velocity: u8) -> f32 {
    let v = f32::from(velocity) / 127.0;
    v * v
}

/// A note to start. `Copy`, so it can wait in a voice without allocating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NoteOn {
    pub key: NoteKey,
    pub pitch: u8,
    pub velocity: u8,
    /// Whether it's one of the project's notes, played by the sequencer,
    /// rather than a live note. Only the sequencer's are released when the
    /// notes change under them.
    pub sequenced: bool,
}

#[derive(Debug, Clone, Copy)]
struct Voice {
    /// The note this voice is playing, or last played.
    key: Option<NoteKey>,
    /// Its pitch, and whether the sequencer started it.
    pitch: u8,
    sequenced: bool,
    /// A note waiting to start here once the take-over fade ends.
    pending: Option<NoteOn>,
    /// Samples left in the take-over fade. Zero when not being taken over.
    take_over_remaining: u32,
    /// The note's frequency over the sample rate: how far an oscillator at
    /// the note's pitch moves in a sample. Each oscillator moves this times
    /// its octave and pitch.
    increment: f64,
    osc1: Oscillator,
    osc2: Oscillator,
    /// Oscillator 2 under sync, following oscillator 1 four steps a
    /// sample. While Sync crossfades, both it and `osc2` run.
    osc2_synced: OversampledSync,
    /// Whether `osc2_synced` is following oscillator 1. It stops whenever
    /// it isn't heard, and starts again from oscillator 1's phase.
    osc2_following: bool,
    /// Oscillator 1's wraps, counted round from 0 to 3, which the sub flips
    /// on.
    osc1_cycle: u32,
    /// Its own white noise, from its own seed.
    noise: Noise,
    filter: Filter,
    envelope: Envelope,
    velocity_gain: f32,
    /// When the note started, in the order notes started.
    started: u64,
    /// When the release started, in the same order.
    released: u64,
}

impl Voice {
    /// The voice at `index` among the synth's voices, silent.
    fn new(index: usize) -> Self {
        Self {
            key: None,
            pitch: 0,
            sequenced: false,
            pending: None,
            take_over_remaining: 0,
            increment: 0.0,
            osc1: Oscillator::default(),
            osc2: Oscillator::default(),
            osc2_synced: OversampledSync::default(),
            osc2_following: false,
            osc1_cycle: 0,
            noise: Noise::seeded(noise_seed(index)),
            filter: Filter::default(),
            envelope: Envelope::default(),
            velocity_gain: 0.0,
            started: 0,
            released: 0,
        }
    }

    fn is_free(&self) -> bool {
        self.envelope.stage() == Stage::Idle && self.take_over_remaining == 0
    }

    fn is_taken_over(&self) -> bool {
        self.take_over_remaining > 0
    }

    /// Whether its note has started and not been released.
    fn is_held(&self) -> bool {
        !matches!(self.envelope.stage(), Stage::Idle | Stage::Release)
    }

    fn is_releasing(&self) -> bool {
        self.envelope.stage() == Stage::Release && !self.is_taken_over()
    }
}

/// Where voice `index`'s noise starts: a different number for each voice, so
/// two notes' noise never adds up as one, and never zero.
fn noise_seed(index: usize) -> u32 {
    // An odd number times 1 to 16 is never zero, and never the same twice.
    (index as u32 + 1).wrapping_mul(0x85EB_CA6B)
}

/// A stepped setting, such as a wave, changing while notes sound: it
/// crossfades from the old value to the new, so the sound never jumps.
#[derive(Debug, Clone, Copy)]
struct Crossfade<T> {
    from: T,
    to: T,
    /// From 0, all `from`, to 1, all `to`.
    mix: Ramp,
}

impl<T: Copy + PartialEq> Crossfade<T> {
    /// Resting at `value`, crossfading to each new value over `samples`.
    fn new(value: T, samples: u32) -> Self {
        Self {
            from: value,
            to: value,
            mix: Ramp::new(0.0, samples),
        }
    }

    /// Heads for `target`. The next crossfade starts only once the last one
    /// has ended, so there are never more than two values to blend.
    #[inline]
    fn follow(&mut self, target: T) {
        if !self.mix.is_moving() {
            if self.mix.value() == 1.0 {
                self.from = self.to;
                self.mix.jump_to(0.0);
            }
            if target != self.to {
                self.to = target;
                self.mix.set_target(1.0);
            }
        }
    }

    /// This sample's blend: the two values and how far it is from the
    /// first to the second. Moves on by a sample.
    #[inline]
    fn next(&mut self) -> Blend<T> {
        Blend {
            from: self.from,
            to: self.to,
            mix: self.mix.next_value(),
        }
    }
}

/// One sample of a [`Crossfade`].
#[derive(Clone, Copy)]
struct Blend<T> {
    from: T,
    to: T,
    mix: f32,
}

impl<T: Copy> Blend<T> {
    /// What `sound` makes of the blend: of `from` alone while there's no
    /// crossfade, and of both, mixed, during one.
    #[inline]
    fn of(self, sound: impl Fn(T) -> f32) -> f32 {
        if self.mix == 0.0 {
            sound(self.from)
        } else {
            let a = sound(self.from);
            let b = sound(self.to);
            a + (b - a) * self.mix
        }
    }
}

/// How many times lower than oscillator 1 the sub is, for its octave.
fn sub_divide(octave: i8) -> u32 {
    if octave <= -2 { 4 } else { 2 }
}

/// The gain for the mixer's levels: none while they add up to no more than
/// one oscillator at full level, in power, and otherwise the gain that
/// brings them back to it. Two detuned oscillators at full level then sound
/// about as loud as one, and the default, oscillator 1 alone at full level,
/// is untouched. See RFC-008, "Risks & unknowns" (Levels).
fn mix_gain(levels: [f32; 4]) -> f32 {
    let power: f32 = levels.iter().map(|level| level * level).sum();
    1.0 / power.max(1.0).sqrt()
}

/// The synth. Owned by the processor.
pub(crate) struct Synth {
    voices: [Voice; VOICES],
    sample_rate: f64,
    settings: SynthSettings,

    /// The cutoff in octaves (log2 of Hz), so it glides on a log scale.
    cutoff_octaves: Ramp,
    resonance: Ramp,
    sustain: Ramp,
    /// The filter coefficients for the current cutoff and resonance.
    coefficients: Coefficients,
    times: Times,

    osc1_wave: Crossfade<Waveform>,
    /// Oscillator 1's pulse width.
    osc1_width: Ramp,
    osc2_wave: Crossfade<Waveform>,
    osc2_sync: Crossfade<bool>,
    /// Brings oscillator 2 under sync back down from four steps a sample.
    decimator: Decimator,
    sub_octave: Crossfade<i8>,
    /// Oscillator 2's Pitch, in semitones.
    osc2_pitch: Ramp,
    /// Each oscillator's frequency against the note's, from the octaves and
    /// oscillator 2's Pitch in `ratios_for`, which they're worked out again
    /// only when those change.
    osc1_ratio: f64,
    osc2_ratio: f64,
    ratios_for: (i8, i8, f32),
    /// Osc 1, Osc 2, Sub and Noise.
    levels: [Ramp; 4],
    /// [`mix_gain`] for the levels in `gain_for`.
    gain: f32,
    gain_for: [f32; 4],

    take_over_samples: u32,
    /// Counts note starts and releases, to find the oldest.
    clock: u64,
    /// Samples since Play, so a note's oscillators can start where they'd
    /// be had they been running all along, when Osc retrigger is off.
    running: u64,
}

impl Synth {
    pub(crate) fn new(settings: SynthSettings, sample_rate: f64) -> Self {
        let settings = settings.clamped();
        let mut synth = Self {
            voices: std::array::from_fn(Voice::new),
            sample_rate,
            settings,
            cutoff_octaves: Ramp::new(0.0, 1),
            resonance: Ramp::new(0.0, 1),
            sustain: Ramp::new(0.0, 1),
            coefficients: Coefficients::new(1.0, 0.0, sample_rate),
            times: Times::new(1.0, 1.0, 1.0, sample_rate),
            osc1_wave: Crossfade::new(settings.osc1_wave, 1),
            osc1_width: Ramp::new(0.0, 1),
            osc2_wave: Crossfade::new(settings.osc2_wave, 1),
            osc2_sync: Crossfade::new(settings.osc2_sync, 1),
            decimator: Decimator::new(),
            sub_octave: Crossfade::new(settings.sub_octave, 1),
            osc2_pitch: Ramp::new(0.0, 1),
            osc1_ratio: 1.0,
            osc2_ratio: 1.0,
            ratios_for: (0, 0, 0.0),
            levels: [Ramp::new(0.0, 1); 4],
            gain: 1.0,
            gain_for: [0.0; 4],
            take_over_samples: 1,
            clock: 0,
            running: 0,
        };
        synth.prepare(sample_rate);
        synth
    }

    /// Moves to a new sample rate. Every voice falls silent: the stream it
    /// was playing on has already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.take_over_samples = samples(TAKE_OVER_SECONDS, sample_rate);
        self.jump_to_settings();
        for (index, voice) in self.voices.iter_mut().enumerate() {
            *voice = Voice::new(index);
        }
        self.running = 0;
    }

    /// Starts the free-running parts again from the same point: each voice's
    /// noise from its seed, and the oscillators' place in their cycle when
    /// Osc retrigger is off. On Play, so what you hear from a place is what
    /// a render plays from there. See RFC-008, "In the engine".
    pub(crate) fn restart(&mut self) {
        self.running = 0;
        for voice in self.voices.iter_mut() {
            voice.noise.restart();
        }
    }

    /// Takes on new settings straight away, with no glide: for a synth that
    /// isn't sounding, so the first note plays with them from its start.
    /// Real-time safe.
    pub(crate) fn load(&mut self, settings: SynthSettings) {
        self.settings = settings.clamped();
        self.jump_to_settings();
    }

    fn jump_to_settings(&mut self) {
        let smoothing = samples(SYNTH_SMOOTHING_SECONDS, self.sample_rate);
        let settings = self.settings;
        self.cutoff_octaves = Ramp::new(settings.cutoff_hz.log2(), smoothing);
        self.resonance = Ramp::new(settings.resonance, smoothing);
        self.sustain = Ramp::new(settings.sustain, smoothing);
        self.osc1_wave = Crossfade::new(settings.osc1_wave, smoothing);
        self.osc1_width = Ramp::new(settings.osc1_width, smoothing);
        self.osc2_wave = Crossfade::new(settings.osc2_wave, smoothing);
        self.osc2_sync = Crossfade::new(settings.osc2_sync, smoothing);
        self.sub_octave = Crossfade::new(settings.sub_octave, smoothing);
        self.osc2_pitch = Ramp::new(settings.osc2_pitch_semitones, smoothing);
        self.levels = [
            settings.osc1_level,
            settings.osc2_level,
            settings.sub_level,
            settings.noise_level,
        ]
        .map(|level| Ramp::new(level, smoothing));
        self.update_ratios(settings.osc2_pitch_semitones);
        self.update_gain(self.levels.map(|level| level.value()));
        self.update_coefficients();
        self.update_times();
    }

    /// Whether any voice is making sound, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.voices.iter().any(|voice| !voice.is_free())
    }

    /// Glides to new settings. Real-time safe.
    pub(crate) fn set_settings(&mut self, settings: SynthSettings) {
        let settings = settings.clamped();
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        self.cutoff_octaves.set_target(settings.cutoff_hz.log2());
        self.resonance.set_target(settings.resonance);
        self.sustain.set_target(settings.sustain);
        self.osc1_width.set_target(settings.osc1_width);
        self.osc2_pitch.set_target(settings.osc2_pitch_semitones);
        let levels = [
            settings.osc1_level,
            settings.osc2_level,
            settings.sub_level,
            settings.noise_level,
        ];
        for (ramp, level) in self.levels.iter_mut().zip(levels) {
            ramp.set_target(level);
        }
        self.update_times();
    }

    /// Starts a note in a free voice, or takes one over: the oldest releasing
    /// voice, then the oldest held one. A taken-over voice fades out over
    /// [`TAKE_OVER_SECONDS`] before the note starts. A note already sounding
    /// with the same key is released first.
    pub(crate) fn note_on(&mut self, note: NoteOn) {
        self.note_off(note.key);
        self.clock += 1;
        let index = self.choose_voice();
        if self.voices[index].is_free() {
            self.start_voice(index, note);
        } else {
            let voice = &mut self.voices[index];
            if !voice.is_taken_over() {
                voice.take_over_remaining = self.take_over_samples;
            }
            voice.pending = Some(note);
            voice.started = self.clock;
        }
    }

    /// Releases the note with this key. A note still waiting for its voice
    /// never starts.
    pub(crate) fn note_off(&mut self, key: NoteKey) {
        for voice in self.voices.iter_mut() {
            if voice.is_taken_over() {
                if voice.pending.is_some_and(|pending| pending.key == key) {
                    voice.pending = None;
                }
            } else if voice.key == Some(key) && !matches!(voice.envelope.stage(), Stage::Idle) {
                if voice.envelope.stage() != Stage::Release {
                    self.clock += 1;
                    voice.released = self.clock;
                }
                voice.envelope.release(&self.times);
            }
        }
    }

    /// Releases every note the sequencer started that `keep` doesn't accept,
    /// given its key and pitch, and cancels any such note still waiting for
    /// a voice. Live notes are left alone. Bounded by the number of voices.
    pub(crate) fn release_sequenced_unless(&mut self, keep: impl Fn(NoteKey, u8) -> bool) {
        for voice in self.voices.iter_mut() {
            if voice.is_taken_over() {
                if voice
                    .pending
                    .is_some_and(|pending| pending.sequenced && !keep(pending.key, pending.pitch))
                {
                    voice.pending = None;
                }
            } else if voice.sequenced
                && !matches!(voice.envelope.stage(), Stage::Idle | Stage::Release)
                && voice.key.is_some_and(|key| !keep(key, voice.pitch))
            {
                self.clock += 1;
                voice.released = self.clock;
                voice.envelope.release(&self.times);
            }
        }
    }

    /// Whether the note with this key is held: started and not released, or
    /// waiting for a voice. Bounded by the number of voices.
    pub(crate) fn is_holding(&self, key: NoteKey) -> bool {
        self.voices.iter().any(|voice| {
            if voice.is_taken_over() {
                voice.pending.is_some_and(|pending| pending.key == key)
            } else {
                voice.key == Some(key) && voice.is_held()
            }
        })
    }

    /// How many voices a new note can't have without taking over a held
    /// note: those holding one, and those being taken over, whether or not
    /// a note still waits there. The rest are free or releasing, which
    /// [`Self::note_on`] takes first. At most [`VOICES`].
    pub(crate) fn busy_voices(&self) -> usize {
        self.voices
            .iter()
            .filter(|voice| voice.is_taken_over() || voice.is_held())
            .count()
    }

    /// The key of each note started and not released, and when it started,
    /// in voice order.
    #[cfg(test)]
    pub(crate) fn held_notes(&self) -> Vec<(NoteKey, u64)> {
        self.voices
            .iter()
            .filter(|voice| !voice.is_taken_over() && voice.is_held())
            .filter_map(|voice| voice.key.map(|key| (key, voice.started)))
            .collect()
    }

    /// Releases every note, and cancels any waiting for a voice.
    pub(crate) fn release_all(&mut self) {
        for voice in self.voices.iter_mut() {
            if voice.is_taken_over() {
                voice.pending = None;
            } else if !matches!(voice.envelope.stage(), Stage::Idle | Stage::Release) {
                self.clock += 1;
                voice.released = self.clock;
                voice.envelope.release(&self.times);
            }
        }
    }

    /// Fades every voice out over [`TAKE_OVER_SECONDS`], as when a voice is
    /// taken over, and cancels any note waiting for a voice. For a slot handed
    /// to another track while its last track's notes still sound.
    pub(crate) fn fade_out(&mut self) {
        let fade = self.take_over_samples;
        for voice in self.voices.iter_mut() {
            voice.pending = None;
            if !voice.is_free() && !voice.is_taken_over() {
                voice.take_over_remaining = fade;
            }
        }
    }

    /// The next sample, the sum of every sounding voice.
    ///
    /// Each voice mixes its two oscillators, the sub and its noise at the
    /// mixer's levels, and filters the mix. A source whose level is zero
    /// isn't worked out, and oscillator 2 and the noise wait where they are
    /// until it's turned up: they cost nothing while they're off. So does
    /// whichever of oscillator 2, free or synced, isn't heard.
    #[inline]
    pub(crate) fn next_sample(&mut self) -> f32 {
        self.advance_settings();
        let osc1_wave = self.osc1_wave.next();
        let osc1_width = f64::from(self.osc1_width.next_value());
        let osc2_wave = self.osc2_wave.next();
        let osc2_sync = self.osc2_sync.next();
        // Whether each of oscillator 2 free and synced is heard.
        let crossfading = osc2_sync.mix != 0.0;
        let free = !osc2_sync.from || (crossfading && !osc2_sync.to);
        let synced = osc2_sync.from || (crossfading && osc2_sync.to);
        let sub_octave = self.sub_octave.next();
        let sustain = f64::from(self.sustain.next_value());
        let semitones = self.osc2_pitch.next_value();
        if (
            self.settings.osc1_octave,
            self.settings.osc2_octave,
            semitones,
        ) != self.ratios_for
        {
            self.update_ratios(semitones);
        }
        let (osc1_ratio, osc2_ratio) = (self.osc1_ratio, self.osc2_ratio);
        let levels = self.levels.each_mut().map(|level| level.next_value());
        if levels != self.gain_for {
            self.update_gain(levels);
        }
        let mix_gain = self.gain;
        let [osc1_level, osc2_level, sub_level, noise_level] = levels;
        self.running += 1;

        let mut sum = 0.0f32;
        for index in 0..VOICES {
            let voice = &mut self.voices[index];
            if voice.is_free() {
                continue;
            }
            voice.osc1.set_increment(voice.increment * osc1_ratio);
            let mut mix = 0.0;
            if osc1_level != 0.0 {
                mix = osc1_level * osc1_wave.of(|wave| voice.osc1.value(wave, osc1_width));
            }
            if osc2_level != 0.0 {
                let increment = voice.increment * osc2_ratio;
                let mut free_value = 0.0;
                if free {
                    voice.osc2.set_increment(increment);
                    free_value = osc2_wave.of(|wave| voice.osc2.value(wave, SQUARE));
                    voice.osc2.advance();
                }
                let mut synced_value = 0.0;
                if synced {
                    let osc = &mut voice.osc2_synced;
                    if voice.osc2_following {
                        osc.set_increment(increment);
                    } else {
                        osc.start_with(&voice.osc1, increment);
                        voice.osc2_following = true;
                    }
                    synced_value = osc.next(
                        &voice.osc1,
                        osc2_wave.from,
                        osc2_wave.to,
                        osc2_wave.mix,
                        &self.decimator,
                    );
                }
                voice.osc2_following &= synced;
                mix += osc2_level * osc2_sync.of(|on| if on { synced_value } else { free_value });
            } else {
                voice.osc2_following = false;
            }
            if sub_level != 0.0 {
                let cycle = voice.osc1_cycle;
                mix += sub_level
                    * sub_octave.of(|octave| voice.osc1.sub_value(cycle, sub_divide(octave)));
            }
            if noise_level != 0.0 {
                mix += noise_level * voice.noise.next_sample();
            }
            // Oscillator 1 always runs, as the sub and its place in the
            // cycle come from it.
            if voice.osc1.advance() {
                voice.osc1_cycle = (voice.osc1_cycle + 1) % 4;
            }
            let filtered = voice.filter.process(mix * mix_gain, &self.coefficients);
            let mut gain = voice.envelope.next_level(sustain, &self.times) as f32;
            let mut faded_out = false;
            if voice.take_over_remaining > 0 {
                gain *= voice.take_over_remaining as f32 / self.take_over_samples as f32;
                voice.take_over_remaining -= 1;
                faded_out = voice.take_over_remaining == 0;
            }
            sum += filtered * gain * voice.velocity_gain;

            if faded_out {
                voice.envelope.reset();
                if let Some(note) = voice.pending.take() {
                    self.start_voice(index, note);
                }
            }
        }
        sum * VOICE_LEVEL
    }

    /// The voice for a new note: free, then oldest releasing, then oldest
    /// held, then (if every voice is already being taken over) the one whose
    /// waiting note arrived first.
    fn choose_voice(&self) -> usize {
        let oldest = |filter: fn(&Voice) -> bool, order: fn(&Voice) -> u64| {
            (0..VOICES)
                .filter(|&i| filter(&self.voices[i]))
                .min_by_key(|&i| order(&self.voices[i]))
        };
        oldest(Voice::is_free, |_| 0)
            .or_else(|| oldest(Voice::is_releasing, |v| v.released))
            .or_else(|| oldest(|v| !v.is_taken_over(), |v| v.started))
            .or_else(|| oldest(|_| true, |v| v.started))
            .expect("there is always a voice")
    }

    /// Starts `note` in voice `index`. With Osc retrigger on, the
    /// oscillators start at the beginning of their cycle; off, where they'd
    /// be had they been running at this pitch since Play, so each note
    /// catches them somewhere different, yet the same in every render.
    fn start_voice(&mut self, index: usize, note: NoteOn) {
        let voice = &mut self.voices[index];
        voice.key = Some(note.key);
        voice.pitch = note.pitch;
        voice.sequenced = note.sequenced;
        voice.pending = None;
        voice.take_over_remaining = 0;
        voice.increment = pitch_to_hz(note.pitch) / self.sample_rate;
        if self.settings.osc_retrigger {
            voice.osc1.start_at(0.0);
            voice.osc2.start_at(0.0);
            voice.osc1_cycle = 0;
        } else {
            let running = self.running as f64;
            let cycles = running * voice.increment * self.osc1_ratio;
            voice.osc1.start_at(cycles.fract());
            voice.osc1_cycle = (cycles.floor() % 4.0) as u32;
            let cycles = running * voice.increment * self.osc2_ratio;
            voice.osc2.start_at(cycles.fract());
        }
        // Under sync, oscillator 2 starts from oscillator 1's phase.
        voice.osc2_following = false;
        voice.filter.reset();
        voice.envelope.start(&self.times);
        voice.velocity_gain = velocity_to_gain(note.velocity);
        voice.started = self.clock;
    }

    /// Moves the gliding settings on by one sample.
    #[inline]
    fn advance_settings(&mut self) {
        if self.cutoff_octaves.is_moving() || self.resonance.is_moving() {
            self.cutoff_octaves.next_value();
            self.resonance.next_value();
            self.update_coefficients();
        }
        self.osc1_wave.follow(self.settings.osc1_wave);
        self.osc2_wave.follow(self.settings.osc2_wave);
        self.osc2_sync.follow(self.settings.osc2_sync);
        self.sub_octave.follow(self.settings.sub_octave);
    }

    /// Works out each oscillator's frequency against the note's, from the
    /// octaves and oscillator 2's Pitch at `semitones`.
    fn update_ratios(&mut self, semitones: f32) {
        let (osc1, osc2) = (self.settings.osc1_octave, self.settings.osc2_octave);
        self.ratios_for = (osc1, osc2, semitones);
        self.osc1_ratio = f64::from(osc1).exp2();
        self.osc2_ratio = (f64::from(osc2) + f64::from(semitones) / 12.0).exp2();
    }

    fn update_gain(&mut self, levels: [f32; 4]) {
        self.gain_for = levels;
        self.gain = mix_gain(levels);
    }

    fn update_coefficients(&mut self) {
        self.coefficients = Coefficients::new(
            f64::from(self.cutoff_octaves.value().exp2()),
            f64::from(self.resonance.value()),
            self.sample_rate,
        );
    }

    fn update_times(&mut self) {
        let s = self.settings;
        self.times = Times::new(
            f64::from(s.attack_seconds),
            f64::from(s.decay_seconds),
            f64::from(s.release_seconds),
            self.sample_rate,
        );
    }

    #[cfg(test)]
    fn sounding_keys(&self) -> Vec<Option<NoteKey>> {
        self.voices
            .iter()
            .map(|v| (!v.is_free()).then_some(v.key).flatten())
            .collect()
    }
}

fn samples(seconds: f64, sample_rate: f64) -> u32 {
    (seconds * sample_rate).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn note(key: u128) -> NoteOn {
        NoteOn {
            key: NoteKey(key),
            pitch: 60,
            velocity: 100,
            sequenced: false,
        }
    }

    fn sequenced(key: u128, pitch: u8) -> NoteOn {
        NoteOn {
            key: NoteKey(key),
            pitch,
            velocity: 100,
            sequenced: true,
        }
    }

    fn run(synth: &mut Synth, samples: usize) {
        for _ in 0..samples {
            synth.next_sample();
        }
    }

    fn voice_of(synth: &Synth, key: u128) -> Option<usize> {
        synth
            .voices
            .iter()
            .position(|v| v.key == Some(NoteKey(key)) && !v.is_free() && !v.is_taken_over())
    }

    fn full_synth() -> Synth {
        let mut synth = Synth::new(SynthSettings::default(), RATE);
        for key in 0..VOICES as u128 {
            synth.note_on(note(key));
            run(&mut synth, 10);
        }
        synth
    }

    #[test]
    fn notes_take_free_voices_first() {
        let synth = full_synth();
        assert!(synth.voices.iter().all(|v| !v.is_free()));
        let mut keys: Vec<u128> = synth.sounding_keys().iter().map(|k| k.unwrap().0).collect();
        keys.sort_unstable();
        assert_eq!(keys, (0..VOICES as u128).collect::<Vec<_>>());
    }

    #[test]
    fn busy_voices_hold_a_note_or_are_being_taken_over() {
        let mut synth = full_synth();
        assert_eq!(synth.busy_voices(), VOICES);
        synth.note_off(NoteKey(3));
        assert_eq!(synth.busy_voices(), VOICES - 1);
        assert!(!synth.is_holding(NoteKey(3)));
        assert!(synth.is_holding(NoteKey(4)));

        // It takes over the releasing voice: the note waiting there is
        // held, and the one fading out isn't.
        synth.note_on(note(100));
        assert_eq!(synth.busy_voices(), VOICES);
        assert!(synth.is_holding(NoteKey(100)));
        assert!(!synth.is_holding(NoteKey(3)));
    }

    /// A voice being taken over is busy even once the note waiting there is
    /// cancelled: a new note would take over a held voice, not that one.
    #[test]
    fn a_voice_being_taken_over_is_busy_without_a_waiting_note() {
        let mut synth = full_synth();
        synth.note_on(note(100));
        synth.note_off(NoteKey(100));
        assert!(!synth.is_holding(NoteKey(100)));
        assert_eq!(synth.busy_voices(), VOICES);
        synth.note_on(note(101));
        assert!(
            !synth.is_holding(NoteKey(1)),
            "the next note takes over a held voice"
        );
    }

    #[test]
    fn a_new_note_takes_over_the_oldest_held_voice() {
        let mut synth = full_synth();
        let oldest = voice_of(&synth, 0).unwrap();
        synth.note_on(note(100));
        assert!(synth.voices[oldest].is_taken_over());

        let fade = synth.take_over_samples as usize;
        run(&mut synth, fade);
        assert_eq!(voice_of(&synth, 100), Some(oldest));
        assert_eq!(voice_of(&synth, 0), None);
    }

    #[test]
    fn releasing_voices_are_taken_over_before_held_ones() {
        let mut synth = full_synth();
        synth.note_off(NoteKey(9));
        run(&mut synth, 10);
        synth.note_off(NoteKey(5));
        run(&mut synth, 10);
        let released_first = voice_of(&synth, 9).unwrap();
        synth.note_on(note(100));
        assert!(synth.voices[released_first].is_taken_over());
        // The next one takes the other releasing voice, not a held one.
        let released_second = voice_of(&synth, 5).unwrap();
        synth.note_on(note(101));
        assert!(synth.voices[released_second].is_taken_over());
    }

    #[test]
    fn a_taken_over_voice_fades_out_before_the_new_note() {
        let mut synth = Synth::new(
            SynthSettings {
                attack_seconds: 0.001,
                ..SynthSettings::default()
            },
            RATE,
        );
        for key in 0..VOICES as u128 {
            synth.note_on(note(key));
        }
        run(&mut synth, 4_800);
        let index = voice_of(&synth, 0).unwrap();
        synth.note_on(note(100));
        let fade = synth.take_over_samples;
        assert_eq!(fade, 240, "5 ms at 48 kHz");
        let mut gains = Vec::new();
        for _ in 0..fade {
            gains.push(synth.voices[index].take_over_remaining);
            synth.next_sample();
        }
        assert!(gains.windows(2).all(|pair| pair[1] < pair[0]));
        assert_eq!(voice_of(&synth, 100), Some(index));
    }

    #[test]
    fn note_off_releases_only_its_note() {
        let mut synth = Synth::new(SynthSettings::default(), RATE);
        synth.note_on(note(1));
        synth.note_on(note(2));
        run(&mut synth, 100);
        synth.note_off(NoteKey(1));
        let one = voice_of(&synth, 1).unwrap();
        let two = voice_of(&synth, 2).unwrap();
        assert_eq!(synth.voices[one].envelope.stage(), Stage::Release);
        assert_ne!(synth.voices[two].envelope.stage(), Stage::Release);
        run(&mut synth, RATE as usize);
        assert!(synth.voices[one].is_free());
        assert!(!synth.voices[two].is_free());
    }

    #[test]
    fn the_same_key_again_releases_the_first() {
        let mut synth = Synth::new(SynthSettings::default(), RATE);
        synth.note_on(note(1));
        run(&mut synth, 100);
        synth.note_on(note(1));
        let stages: Vec<Stage> = synth
            .voices
            .iter()
            .filter(|v| !v.is_free())
            .map(|v| v.envelope.stage())
            .collect();
        assert_eq!(stages, [Stage::Release, Stage::Attack]);
    }

    #[test]
    fn note_off_while_waiting_for_a_voice_cancels_the_note() {
        let mut synth = full_synth();
        let index = voice_of(&synth, 0).unwrap();
        synth.note_on(note(100));
        synth.note_off(NoteKey(100));
        let fade = synth.take_over_samples as usize;
        run(&mut synth, fade);
        assert!(synth.voices[index].is_free());
    }

    #[test]
    fn release_all_releases_every_note_and_cancels_waiting_ones() {
        let mut synth = full_synth();
        synth.note_on(note(100));
        synth.release_all();
        let taken_over = synth.voices.iter().filter(|v| v.is_taken_over()).count();
        assert_eq!(taken_over, 1);
        assert!(
            synth
                .voices
                .iter()
                .filter(|v| !v.is_taken_over())
                .all(|v| v.envelope.stage() == Stage::Release)
        );
        run(&mut synth, RATE as usize);
        assert!(synth.voices.iter().all(Voice::is_free));
    }

    #[test]
    fn only_sequenced_notes_that_are_not_kept_are_released() {
        let mut synth = Synth::new(SynthSettings::default(), RATE);
        synth.note_on(note(1));
        synth.note_on(sequenced(2, 62));
        synth.note_on(sequenced(3, 64));
        run(&mut synth, 100);
        // Keep note 3 at its pitch; note 2 isn't accepted, and the live note
        // 1 isn't the sequencer's to release.
        synth.release_sequenced_unless(|key, pitch| key == NoteKey(3) && pitch == 64);
        let stage =
            |synth: &Synth, key| synth.voices[voice_of(synth, key).unwrap()].envelope.stage();
        assert_ne!(stage(&synth, 1), Stage::Release);
        assert_eq!(stage(&synth, 2), Stage::Release);
        assert_ne!(stage(&synth, 3), Stage::Release);
        // The same key at a new pitch isn't kept.
        synth.release_sequenced_unless(|key, pitch| key == NoteKey(3) && pitch == 65);
        assert_eq!(stage(&synth, 3), Stage::Release);
    }

    #[test]
    fn a_sequenced_note_waiting_for_a_voice_is_cancelled_if_not_kept() {
        let mut synth = full_synth();
        let index = voice_of(&synth, 0).unwrap();
        synth.note_on(sequenced(100, 60));
        synth.release_sequenced_unless(|_, _| false);
        let fade = synth.take_over_samples as usize;
        run(&mut synth, fade);
        assert!(synth.voices[index].is_free());
        // The live notes still sound.
        assert!(synth.voices.iter().filter(|v| !v.is_free()).count() == VOICES - 1);
    }

    #[test]
    fn many_notes_at_once_never_run_out_of_voices() {
        let mut synth = full_synth();
        for key in 100..100 + 3 * VOICES as u128 {
            synth.note_on(note(key));
        }
        let fade = synth.take_over_samples as usize;
        run(&mut synth, fade);
        // The last 16 notes are the ones sounding.
        let mut keys: Vec<u128> = synth.sounding_keys().iter().map(|k| k.unwrap().0).collect();
        keys.sort_unstable();
        let expected: Vec<u128> = (100 + 2 * VOICES as u128..100 + 3 * VOICES as u128).collect();
        assert_eq!(keys, expected);
    }

    #[test]
    fn fade_out_silences_every_voice_in_the_take_over_time() {
        let mut synth = full_synth();
        synth.note_on(note(100));
        assert!(synth.is_sounding());
        synth.fade_out();
        let fade = synth.take_over_samples as usize;
        run(&mut synth, fade);
        assert!(!synth.is_sounding());
        assert_eq!(synth.next_sample(), 0.0);
    }

    #[test]
    fn pitches_and_velocities() {
        assert_eq!(pitch_to_hz(69), 440.0);
        assert!((pitch_to_hz(60) - 261.625_565).abs() < 1e-5);
        assert_eq!(velocity_to_gain(127), 1.0);
        let db = 20.0 * velocity_to_gain(64).log10();
        assert!((db + 11.9).abs() < 0.1, "{db}");
    }
}
