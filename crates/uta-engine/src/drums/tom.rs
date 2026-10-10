//! The 808 toms: struck resonators like the kick, at higher pitches, with a
//! small pitch drop at the start and a little low noise for the skin. See
//! RFC-006, "How each sound is made" (Toms, 808).
//!
//! On the 808 the toms are the kick's bridged-T circuit at higher pitches
//! (Werner, Abel and Smith, DAFx-14, footnote 15), so they ring on the
//! kick's resonator and are struck by the kick's trigger pulse and pulse
//! shaper, ported from Emilie Gillet's `AnalogBassDrum` in Plaits (MIT
//! licence). What the tom adds, from the research's 808 tom recipe:
//! - **The bend.** A hit pushes the pitch up to about a quarter above Tune,
//!   and it falls back over about 100 ms: "less like a boing, and more like
//!   a tonk" (Baratatronix's 808 tom notes; the starting ratio, 1.25, is
//!   from the SC-808's tom, used for its numbers only).
//! - **The skin.** Quiet noise, low-passed, dying away a little slower than
//!   the body: the "room" the 808's toms have and its congas don't.
//!
//! What it keeps from the kick, and why:
//! - **A hit adds energy; it never restarts anything.** The resonator keeps
//!   ringing between hits, so a fast repeat sums with the last one and
//!   each hit comes out a little different, without a click.
//! - **Velocity is the pulse's height,** and a softer hit is like a softer
//!   stick: its pulse's edges are rounder, so it's duller as well as
//!   quieter. A harder hit also bends further, and hits the noise harder
//!   than the body, as the 909's accent "hits" its toms' noise VCA too
//!   ("slightly more attack and noise", the TOMS909 manual).
//!
//! Where it differs from the kick: no attack shift and no pitch sigh (the
//! bend stands in for both), and a fixed low-pass after the resonator
//! instead of a Tone control, as the 808's toms have none.

use std::f64::consts::PI;

use crate::drums::filter::{OnePole, prewarp};
use crate::drums::kick::{diode, one_pole};
use crate::drums::resonator::Resonator;
use crate::drums::{EDGE_SECONDS, decay_per_sample, log_tau, smoothing};
use crate::ramp::Ramp;

/// How long the trigger pulse lasts, as the kick's.
const TRIGGER_PULSE_SECONDS: f64 = 1.0e-3;
/// How fast the pulse's tail dies away once it ends, as the kick's.
const PULSE_DECAY_SECONDS: f64 = 0.2e-3;
/// The pulse shaper's high-pass, as the kick's.
const PULSE_FILTER_SECONDS: f64 = 0.1e-3;
/// Smooths the trigger pulse's edges at full accent, as the kick's. Softer
/// hits are smoothed more (see [`Tom::hit`]).
const PULSE_EDGE_SECONDS: f64 = 0.05e-3;
/// The pulse's height at full accent, as the kick's.
const MAX_PULSE_HEIGHT: f64 = 10.0;
/// The resonator's input scale, over Tune, as the kick's: it keeps the level
/// the same at every pitch.
const INPUT_SCALE_HZ: f64 = 48.0;
/// How far above Tune a full accent starts: a quarter, the SC-808's 1.25.
const BEND_DEPTH: f64 = 0.25;
/// How much of the bend the softest hit gets: a soft hit bends half as far
/// as a full accent, and an unaccented one about two thirds as far.
const SOFT_BEND: f64 = 0.5;
/// The bend's time constant: about 95% of it is gone by 100 ms.
const BEND_SECONDS: f64 = 0.033;
/// The low-pass after the resonator, over Tune, and how much of the pulse
/// leaks through it: a soft stick sound on top of the ring, as the kick's
/// Tone low-pass lets through, without a Tone control to move it.
const BODY_LOW_PASS_RATIO: f64 = 10.0;
const PULSE_LEAK: f64 = 0.03;
/// The skin's noise: two one-pole low-passes at this over Tune, so it's
/// low and sits with the body, not above it like the snare's wires.
const NOISE_LOW_PASS_RATIO: f64 = 6.0;
/// The skin's level against the body: quiet. Its decay is this much longer
/// than the body's.
const NOISE_LEVEL: f64 = 0.35;
const NOISE_DECAY_RATIO: f64 = 1.3;
/// How much of the skin the softest hit gets, against a full accent, on top
/// of its being softer: a harder hit has relatively more noise.
const SOFT_NOISE: f64 = 0.5;
/// Below this, summed over its state, the tom has died away and stops doing
/// work: far under anything audible, as the kick's.
const SILENT: f64 = 1.0e-7;

/// Which tom: they're the same circuit, with their own output level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TomKind {
    Low,
    High,
}

impl TomKind {
    /// The output level that puts this tom at its defaults, at full accent,
    /// at the kit's reference peak (see [`super::REFERENCE_PEAK`]). Measured,
    /// not derived: see the `each_tom_at_full_accent_peaks_at_the_reference`
    /// test.
    fn output_gain(self) -> f32 {
        match self {
            Self::Low => 1.2495,
            Self::High => 1.0907,
        }
    }
}

/// A tom's settings, inside their ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TomSettings {
    pub(crate) tune_hz: f32,
    pub(crate) decay_seconds: f32,
    pub(crate) level_db: f32,
}

/// The tom's controls, as they glide.
#[derive(Debug, Clone, Copy)]
struct Controls {
    /// Tune, in octaves (log2 of Hz), so it glides on a log scale.
    tune_octaves: Ramp,
    /// The resonator's time constant, as its log, so it glides evenly.
    log_tau: Ramp,
    level: Ramp,
}

impl Controls {
    fn new(settings: TomSettings, smoothing: u32) -> Self {
        Self {
            tune_octaves: Ramp::new(settings.tune_hz.log2(), smoothing),
            log_tau: Ramp::new(log_tau(settings.decay_seconds), smoothing),
            level: Ramp::new(super::db_to_gain(settings.level_db), smoothing),
        }
    }
}

/// Per-sample constants for a sample rate.
#[derive(Debug, Clone, Copy)]
struct Rates {
    sample_rate: f64,
    trigger_samples: u32,
    pulse_decay: f64,
    pulse_filter: f64,
    bend_decay: f64,
    /// Smooths the bend's and the skin's edges.
    edge: f64,
}

impl Rates {
    fn new(sample_rate: f64) -> Self {
        let samples = |seconds: f64| (seconds * sample_rate).round().max(1.0);
        Self {
            sample_rate,
            trigger_samples: samples(TRIGGER_PULSE_SECONDS) as u32,
            pulse_decay: 1.0 - 1.0 / samples(PULSE_DECAY_SECONDS),
            pulse_filter: 1.0 / samples(PULSE_FILTER_SECONDS),
            bend_decay: decay_per_sample(BEND_SECONDS, sample_rate),
            edge: smoothing(EDGE_SECONDS, sample_rate),
        }
    }
}

/// What the controls' values work out to, kept until they move, so a tom
/// whose knobs are still costs no `exp` or `tan` a sample.
#[derive(Debug, Clone, Copy)]
struct Derived {
    /// The control values these are for.
    tune_octaves: f32,
    log_tau: f32,
    tune_hz: f64,
    tau: f64,
    body_low_pass: f64,
    noise_low_pass: f64,
    noise_decay: f64,
}

impl Derived {
    /// None yet: the first update works them out.
    fn new() -> Self {
        Self {
            tune_octaves: f32::NAN,
            log_tau: f32::NAN,
            tune_hz: 0.0,
            tau: 0.0,
            body_low_pass: 0.0,
            noise_low_pass: 0.0,
            noise_decay: 0.0,
        }
    }

    /// These values for the controls at this sample.
    #[inline]
    fn update(&mut self, tune_octaves: f32, log_tau: f32, sample_rate: f64) -> Self {
        if tune_octaves != self.tune_octaves {
            self.tune_octaves = tune_octaves;
            self.tune_hz = f64::from(tune_octaves).exp2();
            self.body_low_pass = one_pole(BODY_LOW_PASS_RATIO * self.tune_hz, sample_rate);
            self.noise_low_pass = prewarp(NOISE_LOW_PASS_RATIO * self.tune_hz, sample_rate);
        }
        if log_tau != self.log_tau {
            self.log_tau = log_tau;
            self.tau = f64::from(log_tau).exp();
            self.noise_decay = decay_per_sample(self.tau * NOISE_DECAY_RATIO, sample_rate);
        }
        *self
    }
}

/// What's ringing: all zero when it's silent.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    /// Samples left of the trigger pulse.
    pulse_remaining: u32,
    pulse_height: f64,
    /// How fast the pulse's edges rise and fall: a one-pole coefficient.
    pulse_edge: f64,
    /// The trigger pulse before and after its edges are smoothed.
    pulse_raw: f64,
    pulse: f64,
    pulse_lp: f64,
    /// The bend, from 0 to 1, before and after its edge is smoothed.
    bend: f64,
    bend_smoothed: f64,
    /// The skin's envelope, before and after its edge is smoothed.
    skin: f64,
    skin_smoothed: f64,
    resonator: Resonator,
    body_lp: f64,
    noise_lp: [OnePole; 2],
    /// Whether it's ringing, or about to.
    active: bool,
}

/// One 808 tom circuit. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tom {
    kind: TomKind,
    settings: TomSettings,
    controls: Controls,
    rates: Rates,
    derived: Derived,
    state: State,
}

impl Tom {
    pub(crate) fn new(kind: TomKind, settings: TomSettings, sample_rate: f64) -> Self {
        Self {
            kind,
            settings,
            controls: Controls::new(settings, super::smoothing_samples(sample_rate)),
            rates: Rates::new(sample_rate),
            derived: Derived::new(),
            state: State::default(),
        }
    }

    /// Moves to a new sample rate, silent: the stream it was playing on has
    /// already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        *self = Self::new(self.kind, self.settings, sample_rate);
    }

    /// Takes on new settings straight away, for a tom that isn't sounding.
    pub(crate) fn load(&mut self, settings: TomSettings) {
        self.settings = settings;
        self.controls = Controls::new(settings, super::smoothing_samples(self.rates.sample_rate));
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: TomSettings) {
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        let controls = &mut self.controls;
        controls.tune_octaves.set_target(settings.tune_hz.log2());
        controls.log_tau.set_target(log_tau(settings.decay_seconds));
        controls
            .level
            .set_target(super::db_to_gain(settings.level_db));
    }

    /// A hit with this strength (see [`super::velocity_to_strength`]): a new
    /// trigger pulse, and it charges the bend and the skin. The resonator
    /// carries on from where it is.
    pub(crate) fn hit(&mut self, strength: f32) {
        let strength = f64::from(strength);
        let state = &mut self.state;
        state.pulse_remaining = self.rates.trigger_samples;
        state.pulse_height = MAX_PULSE_HEIGHT * strength;
        // A softer hit is like a softer stick: it's in contact longer, so its
        // edges are rounder and it's duller as well as quieter.
        let edge_seconds = PULSE_EDGE_SECONDS / strength.max(0.01).sqrt();
        state.pulse_edge = one_pole(1.0 / (2.0 * PI * edge_seconds), self.rates.sample_rate);
        // Like a capacitor charged through a diode, a hit raises the bend
        // and the skin to its own level if that's higher, and leaves a
        // bigger one alone, so a soft hit doesn't cut an accent short.
        let bend = 1.0 - SOFT_BEND * (1.0 - strength);
        state.bend = state.bend.max(bend);
        let skin = strength * (1.0 - SOFT_NOISE * (1.0 - strength));
        state.skin = state.skin.max(skin);
        state.active = true;
    }

    /// Whether it's ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.state.active
    }

    /// The next sample, from this sample of the kit's `noise`. A tom that
    /// has died away returns silence without doing the work.
    #[inline]
    pub(crate) fn next_sample(&mut self, noise: f32) -> f32 {
        // The controls glide whether or not it's ringing.
        let controls = &mut self.controls;
        let tune_octaves = controls.tune_octaves.next_value();
        let log_tau = controls.log_tau.next_value();
        let level = controls.level.next_value();
        let rates = &self.rates;
        let state = &mut self.state;
        if !state.active {
            return 0.0;
        }
        let d = self
            .derived
            .update(tune_octaves, log_tau, rates.sample_rate);

        // The trigger pulse, its edges smoothed, as the kick's.
        let raw = if state.pulse_remaining > 0 {
            state.pulse_remaining -= 1;
            state.pulse_height
        } else {
            state.pulse_raw * rates.pulse_decay
        };
        state.pulse_raw = raw;
        state.pulse += state.pulse_edge * (raw - state.pulse);
        let pulse = state.pulse;

        // The pulse shaper, as the kick's: a high-pass picks out its edges,
        // and the diode clips the falling one.
        state.pulse_lp += rates.pulse_filter * (pulse - state.pulse_lp);
        let pulse = diode((pulse - state.pulse_lp) + pulse * 0.044);

        // The bend: the pitch starts high and falls back to Tune.
        state.bend *= rates.bend_decay;
        state.bend_smoothed += rates.edge * (state.bend - state.bend_smoothed);
        let frequency = d.tune_hz * (1.0 + BEND_DEPTH * state.bend_smoothed);

        // The body: the kick's resonator, dying away with the time constant
        // `tau`, through a fixed low-pass that lets a little of the pulse
        // through.
        let input = pulse * INPUT_SCALE_HZ / d.tune_hz;
        let ring = state
            .resonator
            .process(input, frequency, d.tau, rates.sample_rate);
        state.body_lp += d.body_low_pass * (pulse * PULSE_LEAK + ring.band_pass - state.body_lp);

        // The skin: low-passed noise, dying away a little slower than the
        // body.
        state.skin *= d.noise_decay;
        state.skin_smoothed += rates.edge * (state.skin - state.skin_smoothed);
        let noise = state.noise_lp[0].low_pass(f64::from(noise), d.noise_low_pass);
        let noise = state.noise_lp[1].low_pass(noise, d.noise_low_pass);
        let skin = noise * state.skin_smoothed * NOISE_LEVEL;

        let out = (state.body_lp + skin) as f32 * level * self.kind.output_gain();
        if state.pulse_remaining == 0
            && state.resonator.state()
                + state.body_lp.abs()
                + state.pulse_raw.abs()
                + state.pulse.abs()
                + state.skin
                + state.skin_smoothed
                < SILENT
        {
            // Died away: stop, and stop doing work until the next hit.
            *state = State::default();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::noise::Noise;
    use crate::drums::{HighTomSettings, LowTomSettings, REFERENCE_PEAK};

    const RATE: f64 = 48_000.0;

    fn low() -> Tom {
        Tom::new(TomKind::Low, LowTomSettings::default().tom(), RATE)
    }

    fn high() -> Tom {
        Tom::new(TomKind::High, HighTomSettings::default().tom(), RATE)
    }

    /// Runs `tom` for `seconds` on the kit's noise, or on silence for the
    /// body alone.
    fn run(tom: &mut Tom, noise: Option<&mut Noise>, seconds: f64) -> Vec<f32> {
        let samples = (seconds * RATE) as usize;
        match noise {
            Some(noise) => (0..samples)
                .map(|_| tom.next_sample(noise.next_sample()))
                .collect(),
            None => (0..samples).map(|_| tom.next_sample(0.0)).collect(),
        }
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }

    fn db(ratio: f64) -> f64 {
        20.0 * ratio.log10()
    }

    #[test]
    fn each_tom_at_full_accent_peaks_at_the_reference() {
        for mut tom in [low(), high()] {
            tom.hit(1.0);
            let peak = peak(&run(&mut tom, Some(&mut Noise::new()), 0.5));
            assert!((peak - REFERENCE_PEAK).abs() < 0.005, "peak {peak}");
        }
    }

    /// The skin, heard apart from the body: the same hit with and without
    /// the kit's noise. It's quiet, about 18 to 22 dB under the body
    /// (measured), so it colours the tom rather than sounding as noise of
    /// its own. And it dies away a little slower: its level falls about 1.3
    /// times slower than the body's, so it's relatively louder by the
    /// tail, the recipe's "room".
    #[test]
    fn the_skin_is_quiet_and_outlasts_the_body() {
        for (make, what) in [(low as fn() -> Tom, "low"), (high, "high")] {
            for strength in [0.1, 0.3, 1.0] {
                let mut with = make();
                with.hit(strength);
                let with = run(&mut with, Some(&mut Noise::new()), 1.0);
                let mut body = make();
                body.hit(strength);
                let body = run(&mut body, None, 1.0);
                let skin: Vec<f32> = with.iter().zip(&body).map(|(a, b)| a - b).collect();
                let below = db(rms(&body) / rms(&skin));
                assert!(
                    (15.0..25.0).contains(&below),
                    "{what} at {strength}: the skin is {below:.1} dB under"
                );
                // Over the first 100 ms and the next 100 ms, the skin falls
                // less than the body does.
                let fall = |samples: &[f32]| db(rms(&samples[..4800]) / rms(&samples[4800..9600]));
                let (skin_fall, body_fall) = (fall(&skin), fall(&body));
                assert!(
                    skin_fall < body_fall * 0.9,
                    "{what} at {strength}: skin fell {skin_fall:.1} dB, body {body_fall:.1} dB"
                );
            }
        }
    }

    /// A harder hit has relatively more skin, as the 909's accent hits its
    /// toms' noise as well as their envelope.
    #[test]
    fn a_harder_hit_has_more_skin() {
        let share = |strength: f32| {
            let mut with = low();
            with.hit(strength);
            let with = run(&mut with, Some(&mut Noise::new()), 0.5);
            let mut body = low();
            body.hit(strength);
            let body = run(&mut body, None, 0.5);
            let skin: Vec<f32> = with.iter().zip(&body).map(|(a, b)| a - b).collect();
            rms(&skin) / rms(&body)
        };
        assert!(share(1.0) > share(0.1) * 1.2);
    }

    /// Once it has died away far below hearing it stops, and does no work
    /// until the next hit.
    #[test]
    fn a_tom_that_has_died_away_stops() {
        let mut tom = low();
        assert!(!tom.is_sounding());
        tom.hit(1.0);
        assert!(tom.is_sounding());
        run(&mut tom, Some(&mut Noise::new()), 3.0);
        assert!(!tom.is_sounding(), "still ringing after 3 s");
        tom.hit(1.0);
        assert!(peak(&run(&mut tom, Some(&mut Noise::new()), 0.1)) > 0.4);
    }

    /// A second hit adds to the ringing rather than restarting it: the
    /// sound carries on through the hit, and the second hit doesn't come
    /// out a copy of the first.
    #[test]
    fn a_hit_while_it_rings_adds_to_it() {
        let mut tom = high();
        tom.hit(0.3);
        let first = run(&mut tom, None, 0.03);
        let before = *first.last().unwrap();
        tom.hit(0.3);
        let second = run(&mut tom, None, 0.03);
        assert!(
            (second[0] - before).abs() < 0.01,
            "{before} then {}",
            second[0]
        );
        let difference = first
            .iter()
            .zip(&second)
            .fold(0.0f32, |max, (a, b)| max.max((a - b).abs()));
        assert!(difference > 0.02, "the second hit copied the first");
    }

    /// Every control glides to a new setting over the kit's smoothing time,
    /// rather than jumping. A jump in Tune or Decay doesn't click, since the
    /// resonator's state carries on through it, so the tests that listen
    /// can't tell; this watches the controls themselves. A tenth of the way
    /// into the glide, each has moved about a tenth of the way, and by the
    /// end it's there.
    #[test]
    fn every_control_glides_to_a_new_setting() {
        let mut tom = low();
        tom.hit(1.0);
        run(&mut tom, None, 0.01);
        let before = tom.controls;
        tom.set_settings(TomSettings {
            tune_hz: 100.0,
            decay_seconds: 0.6,
            level_db: 6.0,
        });
        let after = Controls::new(tom.settings, 1);
        let glide = super::super::smoothing_samples(RATE) as usize;
        run(&mut tom, None, (glide / 10) as f64 / RATE);
        let share = |from: &Ramp, now: &Ramp, to: &Ramp| {
            (now.value() - from.value()) / (to.value() - from.value())
        };
        let shares = |tom: &Tom| {
            [
                share(
                    &before.tune_octaves,
                    &tom.controls.tune_octaves,
                    &after.tune_octaves,
                ),
                share(&before.log_tau, &tom.controls.log_tau, &after.log_tau),
                share(&before.level, &tom.controls.level, &after.level),
            ]
        };
        for share in shares(&tom) {
            assert!((0.05..0.2).contains(&share), "{:?}", shares(&tom));
        }
        run(&mut tom, None, glide as f64 / RATE);
        for share in shares(&tom) {
            assert!((share - 1.0).abs() < 1e-3, "{:?}", shares(&tom));
        }
    }

    /// A soft hit on a ringing accent doesn't cut its bend or skin short:
    /// they only charge up.
    #[test]
    fn a_soft_hit_does_not_cut_an_accent_short() {
        let mut tom = low();
        tom.hit(1.0);
        run(&mut tom, None, 0.01);
        let (bend, skin) = (tom.state.bend, tom.state.skin);
        tom.hit(0.1);
        assert_eq!((tom.state.bend, tom.state.skin), (bend, skin));
    }
}
