//! The 808 clap: noise through a band-pass, in quick bursts and a tail. See
//! RFC-006, "How each sound is made" (Clap, 808).
//!
//! There's no clap in Plaits, so this is built from the research's 808
//! recipe (Baratatronix's analysis of the circuit), with Sonic Pi's SC-808
//! clap read for ideas only:
//! - **Noise through a band-pass** at about 1 kHz: Tone moves it, from 700
//!   Hz to 2 kHz.
//! - **The bursts.** Several people clapping, not quite together: the
//!   808's sawtooth envelope sends three quick bursts 10 ms apart, each an
//!   instant rise and a fast fall, then a fourth that lasts about 20 ms.
//! - **The tail.** A second envelope, which swells and dies away smoothly
//!   under the bursts: the room's "reverb". Decay sets how long, from 50 to
//!   400 ms to fall 40 dB.
//! - **A gentle high-pass** on the sum, as the 808's output has.
//!
//! As every sound does, it keeps to RFC-006's rules:
//! - **A hit never restarts anything that rings.** The filters carry on
//!   from where they are, and a hit charges the envelopes, like a capacitor
//!   through a diode: it raises each to its level if that's higher, and
//!   leaves a louder ring alone. A new hit starts its own bursts.
//! - **Velocity is the bursts' strength, and the tone follows.** A harder
//!   clap is brighter, not only louder: the band-pass sits higher for a
//!   harder hit, about a quarter of an octave over Tone at full accent, and a
//!   little under it for a soft one. That's a judgement, not a measurement
//!   of the 808: no source we found says how its accent changes the clap.
//! - **Every edge is smoothed,** over 0.1 ms, as the 808 cymbal's attack is
//!   (Werner), so a burst's instant rise is a fast curve.
//! - **The noise is the kit's,** shared with the snare, as on the 909.
//! - **Times are in seconds,** so it sounds the same at any sample rate.

use crate::drums::filter::{OnePole, Svf, prewarp};
use crate::drums::{ClapSettings, EDGE_SECONDS, decay_per_sample, log_tau};
use crate::ramp::Ramp;

/// When each burst starts, after the hit, in seconds: three 10 ms apart,
/// then the fourth.
const BURST_STARTS: [f64; 4] = [0.0, 0.010, 0.020, 0.030];
/// How fast each burst falls: the first three are gone in their 10 ms (a
/// time constant of 2.5 ms is 35 dB down at the next one), the fourth lasts
/// about 20 ms.
const BURST_SECONDS: [f64; 4] = [0.0025, 0.0025, 0.0025, 0.006];
/// The tail's level against the bursts', and how fast it swells.
const TAIL_LEVEL: f64 = 0.35;
const TAIL_SWELL_SECONDS: f64 = 0.008;
/// The band-pass's quality: wide enough to sound like hands, not a whistle.
const BAND_Q: f64 = 2.0;
/// The high-pass on the sum.
const HIGH_PASS_HZ: f64 = 300.0;
/// How far the band-pass moves with the hit's strength, in octaves a unit
/// of strength, from where it is at an unaccented hit (strength 0.3).
const BRIGHTER_OCTAVES: f64 = 0.4;
const UNACCENTED: f64 = 0.3;
/// How fast the band-pass follows a new hit's strength, so it never jumps.
const BRIGHTNESS_SECONDS: f64 = 0.002;
/// The output level that puts a default clap at full accent at the kit's
/// reference peak (see [`super::REFERENCE_PEAK`]). Measured, not derived: see
/// the `the_clap_at_full_accent_peaks_at_the_reference` test.
const OUTPUT_GAIN: f32 = 1.6034;
/// Below this, summed over its envelopes, the clap has died away and stops
/// doing work: far under anything audible.
const SILENT: f64 = 1.0e-6;

/// The clap's controls, as they glide.
#[derive(Debug, Clone, Copy)]
struct Controls {
    /// Tone, in octaves (log2 of Hz), so it glides on a log scale.
    tone_octaves: Ramp,
    /// The tail's time constant, as its log, so it glides evenly.
    log_tau: Ramp,
    level: Ramp,
}

impl Controls {
    fn new(settings: ClapSettings, smoothing: u32) -> Self {
        Self {
            tone_octaves: Ramp::new(settings.tone_hz.log2(), smoothing),
            log_tau: Ramp::new(log_tau(settings.decay_seconds), smoothing),
            level: Ramp::new(super::db_to_gain(settings.level_db), smoothing),
        }
    }
}

/// Per-sample constants for a sample rate.
#[derive(Debug, Clone, Copy)]
struct Rates {
    sample_rate: f64,
    burst_starts: [u32; 4],
    burst_decays: [f64; 4],
    tail_swell: f64,
    brightness: f64,
    edge: f64,
    high_pass: f64,
}

impl Rates {
    fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            burst_starts: BURST_STARTS.map(|seconds| (seconds * sample_rate).round() as u32),
            burst_decays: BURST_SECONDS.map(|seconds| decay_per_sample(seconds, sample_rate)),
            tail_swell: smoothing(TAIL_SWELL_SECONDS, sample_rate),
            brightness: smoothing(BRIGHTNESS_SECONDS, sample_rate),
            edge: smoothing(EDGE_SECONDS, sample_rate),
            high_pass: prewarp(HIGH_PASS_HZ, sample_rate),
        }
    }
}

/// What the controls' values work out to, kept until they move.
#[derive(Debug, Clone, Copy)]
struct Derived {
    log_tau: f32,
    tail_decay: f64,
}

/// What's ringing: all zero when it's silent.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    /// Samples since the last hit, while its bursts are still to come.
    since_hit: u32,
    /// The next burst, or 4 once they've all started.
    next_burst: usize,
    /// The burst playing now.
    burst: usize,
    /// The hit's strength, which each of its bursts charges to.
    strength: f64,
    /// The bursts' envelope, before and after its edges are smoothed.
    bursts: f64,
    bursts_smoothed: f64,
    /// The tail: what it was charged to, dying away, and the swell that
    /// follows it.
    tail_charge: f64,
    tail: f64,
    /// How far the band-pass sits from Tone, in octaves, and where it's
    /// heading.
    brightness: f64,
    brightness_target: f64,
    band: Svf,
    high_pass: OnePole,
    /// Whether it's ringing, or about to.
    active: bool,
}

/// One 808 clap circuit. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Clap {
    settings: ClapSettings,
    controls: Controls,
    rates: Rates,
    derived: Derived,
    /// The band-pass's last centre and its prewarped gain, kept until it
    /// moves.
    centre: (f64, f64),
    state: State,
}

impl Clap {
    pub(crate) fn new(settings: ClapSettings, sample_rate: f64) -> Self {
        let settings = settings.clamped();
        Self {
            settings,
            controls: Controls::new(settings, super::smoothing_samples(sample_rate)),
            rates: Rates::new(sample_rate),
            derived: Derived {
                log_tau: f32::NAN,
                tail_decay: 0.0,
            },
            centre: (f64::NAN, 0.0),
            state: State {
                next_burst: BURST_STARTS.len(),
                ..State::default()
            },
        }
    }

    /// Moves to a new sample rate, silent: the stream it was playing on has
    /// already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        *self = Self::new(self.settings, sample_rate);
    }

    /// Takes on new settings straight away, for a clap that isn't sounding.
    pub(crate) fn load(&mut self, settings: ClapSettings) {
        self.settings = settings.clamped();
        self.controls = Controls::new(
            self.settings,
            super::smoothing_samples(self.rates.sample_rate),
        );
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: ClapSettings) {
        let settings = settings.clamped();
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        let controls = &mut self.controls;
        controls.tone_octaves.set_target(settings.tone_hz.log2());
        controls.log_tau.set_target(log_tau(settings.decay_seconds));
        controls
            .level
            .set_target(super::db_to_gain(settings.level_db));
    }

    /// A hit with this strength (see [`super::velocity_to_strength`]): its
    /// bursts start, and it charges the tail. The filters carry on from
    /// where they are.
    pub(crate) fn hit(&mut self, strength: f32) {
        let strength = f64::from(strength);
        let state = &mut self.state;
        state.since_hit = 0;
        state.next_burst = 0;
        state.strength = strength;
        state.tail_charge = state.tail_charge.max(strength * TAIL_LEVEL);
        state.brightness_target = BRIGHTER_OCTAVES * (strength - UNACCENTED);
        if !state.active {
            state.brightness = state.brightness_target;
        }
        state.active = true;
    }

    /// Whether it's ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.state.active
    }

    /// The next sample, from this sample of the kit's `noise`. A clap that
    /// has died away returns silence without doing the work.
    #[inline]
    pub(crate) fn next_sample(&mut self, noise: f32) -> f32 {
        // The controls glide whether or not it's ringing.
        let controls = &mut self.controls;
        let tone_octaves = controls.tone_octaves.next_value();
        let log_tau = controls.log_tau.next_value();
        let level = controls.level.next_value();
        let rates = &self.rates;
        let state = &mut self.state;
        if !state.active {
            return 0.0;
        }
        if log_tau != self.derived.log_tau {
            self.derived = Derived {
                log_tau,
                tail_decay: decay_per_sample(f64::from(log_tau).exp(), rates.sample_rate),
            };
        }

        // The bursts: each starts on time, charging to the hit's strength.
        if state.next_burst < BURST_STARTS.len() {
            if state.since_hit == rates.burst_starts[state.next_burst] {
                state.bursts = state.bursts.max(state.strength);
                state.burst = state.next_burst;
                state.next_burst += 1;
            }
            state.since_hit += 1;
        }
        state.bursts *= rates.burst_decays[state.burst];
        state.bursts_smoothed += rates.edge * (state.bursts - state.bursts_smoothed);

        // The tail swells towards its charge as the charge dies away.
        state.tail_charge *= self.derived.tail_decay;
        state.tail += rates.tail_swell * (state.tail_charge - state.tail);

        // The band-pass, moved by the hit's strength.
        state.brightness += rates.brightness * (state.brightness_target - state.brightness);
        let centre = f64::from(tone_octaves) + state.brightness;
        if centre != self.centre.0 {
            self.centre = (centre, prewarp(centre.exp2(), rates.sample_rate));
        }
        let band = state
            .band
            .process(f64::from(noise), self.centre.1, BAND_Q)
            .band_pass
            / BAND_Q;

        let clap = band * (state.bursts_smoothed + state.tail);
        let out = state.high_pass.high_pass(clap, rates.high_pass);

        let out = out as f32 * level * OUTPUT_GAIN;
        if state.next_burst == BURST_STARTS.len()
            && state.bursts
                + state.bursts_smoothed
                + state.tail_charge
                + state.tail
                + state.high_pass.state().abs()
                < SILENT
        {
            // Died away: stop, and stop doing work until the next hit.
            *state = State {
                next_burst: BURST_STARTS.len(),
                ..State::default()
            };
        }
        out
    }
}

/// The coefficient of a one-pole smoother with time constant `seconds`.
fn smoothing(seconds: f64, sample_rate: f64) -> f64 {
    1.0 - decay_per_sample(seconds, sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::REFERENCE_PEAK;
    use crate::drums::noise::Noise;

    const RATE: f64 = 48_000.0;

    /// Runs `clap` for `seconds`, on the kit's noise.
    fn run(clap: &mut Clap, noise: &mut Noise, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|_| clap.next_sample(noise.next_sample()))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn the_clap_at_full_accent_peaks_at_the_reference() {
        let mut clap = Clap::new(ClapSettings::default(), RATE);
        clap.hit(1.0);
        let peak = peak(&run(&mut clap, &mut Noise::new(), 0.5));
        assert!((peak - REFERENCE_PEAK).abs() < 0.005, "peak {peak}");
    }

    /// Once it has died away far below hearing it stops, and does no work
    /// until the next hit.
    #[test]
    fn a_clap_that_has_died_away_stops() {
        let settings = ClapSettings {
            decay_seconds: 0.4,
            ..ClapSettings::default()
        };
        let mut clap = Clap::new(settings, RATE);
        let mut noise = Noise::new();
        assert!(!clap.is_sounding());
        clap.hit(1.0);
        let samples = run(&mut clap, &mut noise, 2.0);
        assert!(!clap.is_sounding(), "still ringing after 2 s");
        let stopped = samples.iter().rposition(|&s| s != 0.0).unwrap();
        let last = peak(&samples[stopped.saturating_sub(4800)..=stopped]);
        assert!(last < 1e-5, "last 0.1 s peaked at {last}");
        clap.hit(1.0);
        assert!(peak(&run(&mut clap, &mut noise, 0.1)) > 0.2);
    }
}
