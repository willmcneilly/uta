//! The 909 snare: two shell tones and the wires. See RFC-006, "How each
//! sound is made" (Snare, 909).
//!
//! Ported from Emilie Gillet's `SyntheticSnareDrum` in Plaits (MIT licence),
//! a model of the TR-909 snare: two oscillators at Tune and 1.47 times it,
//! shaped from triangles towards sines (the shell), and noise, low-passed
//! then high-passed (the wires). It keeps from the 909:
//! - **The wires' hold.** The noise holds at its level for 40 to 70 ms
//!   before it decays, which is a large part of the 909 snare's sound.
//! - **The bump.** The shell starts at twice its pitch and falls back in
//!   about 7 ms, with a burst of noise alongside it: the stick's crack.
//! - **The coupling.** On the 909 every oscillator is reset by one
//!   transistor, so each one's wrap jitters with the others' state: a little
//!   intermodulation that roughens the shell.
//! - **The shape.** The triangles are bent towards sines by a soft
//!   saturation, the 909's diode shaping: the "drive" this sound has.
//!
//! Where it differs from Plaits, and why:
//! - **A hit never restarts the oscillators.** Plaits resets their phases on
//!   every hit, as the 909 does. Uta's rule is that a hit adds energy to
//!   circuits that keep running (RFC-006, "What makes it sound good", point
//!   1), so they carry on from where they are, and each hit on a ringing
//!   snare comes out a little different. A snare that has died away starts
//!   its next hit from the same point, so renders don't depend on history.
//! - **A hit charges the envelopes; it doesn't set them.** Like a capacitor
//!   charged through a diode, a hit raises each envelope to its own level
//!   if it's higher, and leaves a louder ring alone, so a soft hit doesn't
//!   cut an accent short.
//! - **Every envelope edge is smoothed** over 0.1 ms, as the 808 cymbal's
//!   attack is (Werner), so even a hit's instant attack is a fast curve.
//! - **The controls are the 909's.** Plaits has one Decay for the shell and
//!   the wires together. On the 909, Tone is the wires' length alone, so the
//!   shell's decay is fixed at what Plaits gives at its Decay 0.3 and
//!   Harmonics 0.5, about 0.18 s to fall 40 dB, and Tone sets the wires'
//!   decay in seconds. The hold runs from 40 ms at the shortest Tone to 70
//!   ms at the longest, as Plaits' does across its Decay.
//! - **Velocity tilts the wires against the shell,** as the 909's accent
//!   changes "the balance between the noise and 'kick' sounds" (the SD909
//!   manual): a harder hit has relatively more wires. The crack's noise
//!   follows the hit's strength too; Plaits' is the same for every hit.
//! - **The noise is the kit's.** It comes from the noise source the clap
//!   shares, as on the 909.
//! - **Times are in seconds,** so it sounds the same at any sample rate.
//! - Plaits' "sustain" mode is left out: Uta's drums are one-shots.

use crate::drums::SnareSettings;
use crate::drums::filter::{OnePole, Svf, prewarp};
use crate::ramp::Ramp;

/// Plaits' FM amount at its Harmonics 0.5: 0.5².
const FM_AMOUNT: f64 = 0.25;
/// How much higher the shell starts: four times the FM amount, as in
/// Plaits, so it starts at twice Tune.
const BUMP_DEPTH: f64 = 4.0 * FM_AMOUNT;
/// How fast the bump and the crack fall away.
const BUMP_SECONDS: f64 = 0.007;
/// The shell's upper tone, over Tune.
const UPPER_RATIO: f64 = 1.47;
/// The shell's time constant at Snappy 0: Plaits' at its Decay 0.3 and
/// Harmonics 0.5. More Snappy shortens it, by up to 7 semitones' worth.
const SHELL_SECONDS: f64 = 0.0478;
const SNAPPY_SHORTENS_SEMITONES: f64 = 7.0;
/// Below this, the shell's tail dies away half as fast: Plaits' long tail.
const SHELL_TAIL_BELOW: f64 = 0.03;
/// The wires' hold, at the shortest and longest Tone.
const MIN_HOLD_SECONDS: f64 = 0.04;
const MAX_HOLD_SECONDS: f64 = 0.07;
/// How far a soft hit tilts the wires down against the shell: at full
/// accent they're Plaits' level, and at the softest, 40% under it.
const SOFT_WIRES: f64 = 0.4;
/// Smooths every envelope edge.
const EDGE_SECONDS: f64 = 0.1e-3;
/// The output level that puts a default snare at full accent at the kit's
/// reference peak (see [`super::REFERENCE_PEAK`]). Measured, not derived: see
/// the `the_snare_at_full_accent_peaks_at_the_reference` test.
const OUTPUT_GAIN: f32 = 0.6322;
/// Below this, summed over its envelopes, the snare has died away and stops
/// doing work: far under anything audible (its envelopes start at up to 1).
const SILENT: f64 = 1.0e-6;
/// How much a decay of 40 dB is in time constants: ln(100).
const LN_100: f64 = 4.605_170_185_988_091;

/// The snare's controls, as they glide.
#[derive(Debug, Clone, Copy)]
struct Controls {
    /// Tune, in octaves (log2 of Hz), so it glides on a log scale.
    tune_octaves: Ramp,
    /// Tone, as its log, so it glides evenly.
    log_tone: Ramp,
    snappy: Ramp,
    level: Ramp,
}

impl Controls {
    fn new(settings: SnareSettings, smoothing: u32) -> Self {
        Self {
            tune_octaves: Ramp::new(settings.tune_hz.log2(), smoothing),
            log_tone: Ramp::new(settings.tone_seconds.ln(), smoothing),
            snappy: Ramp::new(settings.snappy, smoothing),
            level: Ramp::new(super::db_to_gain(settings.level_db), smoothing),
        }
    }
}

/// What the controls' values work out to, kept until they move.
#[derive(Debug, Clone, Copy)]
struct Derived {
    /// The control values these are for.
    tune_octaves: f32,
    log_tone: f32,
    snappy: f32,
    /// Tune, in cycles a sample.
    f0: f64,
    /// How much the oscillators' wraps jitter (Plaits' `reset_noise_amount`).
    coupling: f64,
    shell_lp: f64,
    wires_hp: f64,
    wires_lp: f64,
    wires_q: f64,
    shell_level: f64,
    wires_level: f64,
    shell_decay: f64,
    shell_tail_decay: f64,
    wires_decay: f64,
    hold_samples: u32,
}

impl Derived {
    /// None yet: the first update works them out.
    fn new() -> Self {
        Self {
            tune_octaves: f32::NAN,
            log_tone: f32::NAN,
            snappy: f32::NAN,
            f0: 0.0,
            coupling: 0.0,
            shell_lp: 0.0,
            wires_hp: 0.0,
            wires_lp: 0.0,
            wires_q: 0.0,
            shell_level: 0.0,
            wires_level: 0.0,
            shell_decay: 0.0,
            shell_tail_decay: 0.0,
            wires_decay: 0.0,
            hold_samples: 0,
        }
    }

    /// These values for the controls at this sample.
    #[inline]
    fn update(&mut self, tune_octaves: f32, log_tone: f32, snappy: f32, rate: f64) -> &Self {
        if tune_octaves != self.tune_octaves {
            self.tune_octaves = tune_octaves;
            let tune_hz = f64::from(tune_octaves).exp2();
            self.f0 = tune_hz / rate;
            // Plaits' coupling, from Tune as a share of its own 48 kHz rate,
            // so it's the same at any rate.
            let amount = ((0.125 - tune_hz / 48_000.0) * 8.0).clamp(0.0, 1.0);
            self.coupling = amount * amount * FM_AMOUNT;
            self.shell_lp = prewarp(3.0 * tune_hz, rate);
            self.wires_hp = prewarp(10.0 * tune_hz, rate);
            self.wires_lp = prewarp(35.0 * tune_hz, rate);
        }
        if snappy != self.snappy {
            self.snappy = snappy;
            let snappy = f64::from(snappy);
            let tau = SHELL_SECONDS * (-SNAPPY_SHORTENS_SEMITONES * snappy / 12.0).exp2();
            self.shell_decay = decay_per_sample(tau, rate);
            self.shell_tail_decay = decay_per_sample(2.0 * tau, rate);
            // Plaits stretches Snappy a little, so both ends are all of one.
            let snappy = (snappy * 1.1 - 0.05).clamp(0.0, 1.0);
            self.shell_level = (1.0 - snappy).sqrt();
            self.wires_level = snappy.sqrt();
            self.wires_q = 0.5 + 2.0 * snappy;
        }
        if log_tone != self.log_tone {
            self.log_tone = log_tone;
            let tone_seconds = f64::from(log_tone).exp();
            self.wires_decay = decay_per_sample(tone_seconds / LN_100, rate);
            let (min, max) = (
                f64::from(*SnareSettings::TONE_SECONDS.start()),
                f64::from(*SnareSettings::TONE_SECONDS.end()),
            );
            let position = ((tone_seconds / min).ln() / (max / min).ln()).clamp(0.0, 1.0);
            let hold = MIN_HOLD_SECONDS + (MAX_HOLD_SECONDS - MIN_HOLD_SECONDS) * position;
            self.hold_samples = (hold * rate).round() as u32;
        }
        self
    }
}

/// What's ringing: all zero when it's silent.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    /// The two oscillators' phases, in cycles.
    phases: [f64; 2],
    /// The shell's and the wires' envelopes, and the bump's, before and
    /// after their edges are smoothed.
    shell: f64,
    wires: f64,
    bump: f64,
    /// The crack's noise: the bump's envelope, at the hit's strength.
    crack: f64,
    shell_smoothed: f64,
    wires_smoothed: f64,
    crack_smoothed: f64,
    /// How much of the wires' hold is left, in samples.
    hold_remaining: u32,
    shell_lp: OnePole,
    wires_lp: Svf,
    wires_hp: OnePole,
    /// Whether it's ringing, or about to.
    active: bool,
}

/// One 909 snare circuit. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Snare {
    settings: SnareSettings,
    controls: Controls,
    sample_rate: f64,
    bump_decay: f64,
    edge: f64,
    derived: Derived,
    state: State,
}

impl Snare {
    pub(crate) fn new(settings: SnareSettings, sample_rate: f64) -> Self {
        let settings = settings.clamped();
        Self {
            settings,
            controls: Controls::new(settings, super::smoothing_samples(sample_rate)),
            sample_rate,
            bump_decay: decay_per_sample(BUMP_SECONDS, sample_rate),
            edge: 1.0 - (-1.0 / (EDGE_SECONDS * sample_rate)).exp(),
            derived: Derived::new(),
            state: State::default(),
        }
    }

    /// Moves to a new sample rate, silent: the stream it was playing on has
    /// already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        *self = Self::new(self.settings, sample_rate);
    }

    /// Takes on new settings straight away, for a snare that isn't sounding.
    pub(crate) fn load(&mut self, settings: SnareSettings) {
        self.settings = settings.clamped();
        self.controls = Controls::new(self.settings, super::smoothing_samples(self.sample_rate));
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: SnareSettings) {
        let settings = settings.clamped();
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        let controls = &mut self.controls;
        controls.tune_octaves.set_target(settings.tune_hz.log2());
        controls.log_tone.set_target(settings.tone_seconds.ln());
        controls.snappy.set_target(settings.snappy);
        controls
            .level
            .set_target(super::db_to_gain(settings.level_db));
    }

    /// A hit with this strength (see [`super::velocity_to_strength`]): it
    /// charges the envelopes and starts the bump and the wires' hold. The
    /// oscillators and filters carry on from where they are.
    pub(crate) fn hit(&mut self, strength: f32) {
        let strength = f64::from(strength);
        let hold = self.derived_hold();
        let state = &mut self.state;
        // Plaits' envelope peak, 0.3 + 0.7 × accent, is the strength: 0.3 for
        // an unaccented hit and 1 for a full accent. The wires are tilted
        // against the shell: the softer the hit, the less of them.
        let tilt = 1.0 - SOFT_WIRES * (1.0 - strength);
        state.shell = state.shell.max(strength);
        state.wires = state.wires.max(strength * tilt);
        state.bump = 1.0;
        state.crack = state.crack.max(strength);
        state.hold_remaining = hold;
        state.active = true;
    }

    /// The wires' hold, in samples, at the Tone it has now.
    fn derived_hold(&mut self) -> u32 {
        let controls = &self.controls;
        self.derived
            .update(
                controls.tune_octaves.value(),
                controls.log_tone.value(),
                controls.snappy.value(),
                self.sample_rate,
            )
            .hold_samples
    }

    /// Whether it's ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.state.active
    }

    /// The next sample, from this sample of the kit's `noise`. A snare that
    /// has died away returns silence without doing the work.
    #[inline]
    pub(crate) fn next_sample(&mut self, noise: f32) -> f32 {
        // The controls glide whether or not it's ringing.
        let controls = &mut self.controls;
        let tune_octaves = controls.tune_octaves.next_value();
        let log_tone = controls.log_tone.next_value();
        let snappy = controls.snappy.next_value();
        let level = controls.level.next_value();
        let state = &mut self.state;
        if !state.active {
            return 0.0;
        }
        let d = self
            .derived
            .update(tune_octaves, log_tone, snappy, self.sample_rate);

        // The envelopes: the shell's with its long tail, the wires' after
        // their hold, and the bump's.
        state.shell *= if state.shell > SHELL_TAIL_BELOW {
            d.shell_decay
        } else {
            d.shell_tail_decay
        };
        if state.hold_remaining > 0 {
            state.hold_remaining -= 1;
        } else {
            state.wires *= d.wires_decay;
        }
        state.bump *= self.bump_decay;
        state.crack *= self.bump_decay;
        state.shell_smoothed += self.edge * (state.shell - state.shell_smoothed);
        state.wires_smoothed += self.edge * (state.wires - state.wires_smoothed);
        state.crack_smoothed += self.edge * (state.crack - state.crack_smoothed);

        // The coupling: each oscillator's wrap moves with both oscillators'
        // state.
        let mut jitter = 0.0;
        for phase in state.phases {
            jitter += if phase > 0.5 { -1.0 } else { 1.0 };
        }
        jitter *= d.coupling * 0.025;
        let f = d.f0 * (1.0 + BUMP_DEPTH * state.bump);
        state.phases[0] += f;
        state.phases[1] += f * UPPER_RATIO;
        for phase in &mut state.phases {
            if d.coupling > 0.1 {
                if *phase >= 1.0 + jitter {
                    *phase = 1.0 - *phase;
                }
            } else if *phase >= 1.0 {
                *phase -= 1.0;
            }
        }

        // The shell.
        let shell =
            -0.1 + distorted_sine(state.phases[0]) * 0.6 + distorted_sine(state.phases[1]) * 0.25;
        let shell = shell * state.shell_smoothed * d.shell_level;
        let shell = state.shell_lp.low_pass(shell, d.shell_lp);

        // The wires: Plaits' noise is 0 to 1.
        let noise = 0.5 * (f64::from(noise) + 1.0);
        let wires = state
            .wires_lp
            .process(noise, d.wires_lp, d.wires_q)
            .low_pass;
        let wires = state.wires_hp.high_pass(wires, d.wires_hp);
        let wires = (wires + 0.1) * (state.wires_smoothed + state.crack_smoothed) * d.wires_level;

        let out = (shell + wires) as f32 * level * OUTPUT_GAIN;
        if state.hold_remaining == 0
            && state.shell
                + state.wires
                + state.crack
                + state.shell_smoothed
                + state.wires_smoothed
                + state.crack_smoothed
                + state.shell_lp.state().abs()
                < SILENT
        {
            // Died away: stop, and stop doing work until the next hit. The
            // oscillators start the next hit from the same point.
            *state = State::default();
        }
        out
    }
}

/// Plaits' bent triangle: a triangle wave pushed towards a sine by a soft
/// saturation, from a phase in cycles.
#[inline]
fn distorted_sine(phase: f64) -> f64 {
    let triangle = (if phase < 0.5 { phase } else { 1.0 - phase }) * 4.0 - 1.3;
    2.0 * triangle / (1.0 + triangle.abs())
}

/// The multiplier a sample for an exponential decay with time constant
/// `tau_seconds`.
fn decay_per_sample(tau_seconds: f64, sample_rate: f64) -> f64 {
    (-1.0 / (tau_seconds * sample_rate)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::REFERENCE_PEAK;
    use crate::drums::noise::Noise;

    const RATE: f64 = 48_000.0;

    /// Runs `snare` for `seconds`, on the kit's noise.
    fn run(snare: &mut Snare, noise: &mut Noise, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|_| snare.next_sample(noise.next_sample()))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn the_snare_at_full_accent_peaks_at_the_reference() {
        let mut snare = Snare::new(SnareSettings::default(), RATE);
        snare.hit(1.0);
        let peak = peak(&run(&mut snare, &mut Noise::new(), 0.5));
        assert!((peak - REFERENCE_PEAK).abs() < 0.005, "peak {peak}");
    }

    /// Once it has died away far below hearing it stops, and does no work
    /// until the next hit.
    #[test]
    fn a_snare_that_has_died_away_stops() {
        let settings = SnareSettings {
            tone_seconds: 0.4,
            snappy: 0.0,
            ..SnareSettings::default()
        };
        let mut snare = Snare::new(settings, RATE);
        let mut noise = Noise::new();
        assert!(!snare.is_sounding());
        snare.hit(1.0);
        let samples = run(&mut snare, &mut noise, 3.0);
        assert!(!snare.is_sounding(), "still ringing after 3 s");
        let stopped = samples.iter().rposition(|&s| s != 0.0).unwrap();
        let last = peak(&samples[stopped.saturating_sub(4800)..=stopped]);
        assert!(last < 1e-5, "last 0.1 s peaked at {last}");
        snare.hit(1.0);
        assert!(peak(&run(&mut snare, &mut noise, 0.1)) > 0.2);
    }

    /// A soft hit on a ringing accent doesn't cut it short: the envelopes
    /// only charge up.
    #[test]
    fn a_soft_hit_does_not_cut_an_accent_short() {
        let mut snare = Snare::new(SnareSettings::default(), RATE);
        let mut noise = Noise::new();
        snare.hit(1.0);
        run(&mut snare, &mut noise, 0.01);
        let before = snare.state.wires;
        snare.hit(0.1);
        assert_eq!(snare.state.wires, before);
    }
}
