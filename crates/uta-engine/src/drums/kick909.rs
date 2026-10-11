//! The 909 kick: an oscillator shaped towards a sine, with a fast downward
//! pitch sweep and a separate click. See RFC-006, "How each sound is made"
//! (Kick, 909).
//!
//! Ported from Emilie Gillet's `SyntheticBassDrum` in Plaits (MIT licence),
//! which she calls "inadvertently 909-ish", following the research's 909
//! recipe: a triangle oscillator through a soft sine shaper, a pitch
//! envelope (the 909's Tune, Uta's Sweep), an amplitude envelope (Decay),
//! and a click of a filtered step and filtered noise (Attack), through an
//! asymmetric transistor VCA.
//!
//! What makes it tighter and punchier than the 808:
//! - **The sweep.** The pitch starts up to 4.5 times Tune and falls to it in
//!   a few tens of milliseconds, so the attack is a fast "doom" rather than
//!   the 808's resonant thump.
//! - **The click** is its own part, with its own short envelope: a step
//!   through a resonant 5 kHz low-pass, plus a burst of low noise.
//! - **The VCA** is asymmetric, as the 909's transistor one is, so the body
//!   has some even harmonics, and a harder hit drives it further.
//! - **Velocity is the envelopes' height,** as the 909's accent is: a harder
//!   hit drives the VCA harder and has a louder click, so it's brighter, not
//!   just louder.
//!
//! Where it differs from Plaits, and why:
//! - Times are in seconds, so it sounds the same at any sample rate.
//! - **A hit adds energy, it doesn't restart anything.** Plaits sets its
//!   envelopes to the hit's height and holds the oscillator at a zero
//!   crossing on every hit. Here a hit adds to the envelopes, so a soft hit
//!   on a loud ring never dips, and the oscillator keeps running while the
//!   kick rings, so a fast repeat or a flam never jumps. It only goes back
//!   to its zero crossing when what's ringing is 40 dB under the new hit:
//!   then the jump is at most 1% of the hit, lost under it, and the hit
//!   starts cleanly, as a first hit does.
//! - Plaits' Tone is both the click's level and a low-pass. Uta's Attack is
//!   both too, but the low-pass starts at 4 times Tune rather than below it,
//!   so even with no click the sweep comes through.
//! - Decay is the seconds to die away by 40 dB, as on every Uta sound.
//! - The body is a fixed blend of the shaped triangle and a sine. Plaits
//!   blends them by Decay, and adds a little phase noise; Uta's drums vary
//!   from hit to hit through their running state instead.
//! - The output is AC-coupled, as the 909's is: the asymmetric VCA has an
//!   offset, which would otherwise sit under the kick as a slow swell.
//! - Plaits' "sustain" mode is left out: Uta's drums are one-shots.

use std::f64::consts::{LN_2, TAU};

use crate::drums::filter::{OnePole, Svf, prewarp};
use crate::drums::{Kick909Settings, log_tau, smoothing};
use crate::ramp::Ramp;

/// How long the oscillator waits at its zero crossing on a hit from
/// silence, while the envelopes rise, so it starts from nothing.
const PHASE_HOLD_SECONDS: f64 = 1.3e-3;
/// How long the envelopes hold at the top before they decay.
const BODY_HOLD_SECONDS: f64 = 1.0e-3;
/// How fast the envelopes follow their targets, so even an instant attack is
/// a curve over a few samples: Plaits' one-pole of 0.1 at 48 kHz.
const ENVELOPE_SMOOTHING_SECONDS: f64 = 0.2e-3;
/// How fast the sweep falls at its quickest, and how much slower the second
/// half of Sweep makes it: from 8 ms up to 40 ms.
const SWEEP_SECONDS: f64 = 8.0e-3;
const SWEEP_SLOWING: f64 = 4.0;
/// How far above Tune the sweep starts at its deepest: 1 + 3.5 times.
const SWEEP_DEPTH: f64 = 3.5;
/// How fast the click dies away.
const TRANSIENT_SECONDS: f64 = 5.0e-3;
/// How fast the click's step rises and falls, per second: Plaits' slope
/// limit of 0.5 up and 0.1 down a sample at 48 kHz.
const CLICK_RISE_PER_SECOND: f64 = 24_000.0;
const CLICK_FALL_PER_SECOND: f64 = 4_800.0;
/// The high-pass that turns the click's step into a pulse: Plaits' one-pole
/// of 0.04 at 48 kHz.
const CLICK_HIGH_PASS_SECONDS: f64 = 0.51e-3;
/// The resonant low-pass the click goes through.
const CLICK_LOW_PASS_HZ: f64 = 5000.0;
const CLICK_Q: f64 = 2.0;
/// The band the click's noise is filtered to: Plaits' one-poles of 0.05 and
/// 0.005 at 48 kHz, about 40 to 390 Hz, a low thud rather than a hiss.
const NOISE_LOW_PASS_SECONDS: f64 = 0.406e-3;
const NOISE_HIGH_PASS_SECONDS: f64 = 4.16e-3;
/// The kit's noise is even over -1 to 1. Plaits' is even over 0 to 1, which
/// has half the spread.
const NOISE_SCALE: f64 = 0.5;
/// How much of the body is the shaped triangle; the rest is a sine. Plaits
/// blends 15% to 40% of it, by Decay.
const TRIANGLE_SHARE: f64 = 0.3;
/// Where the low-pass after the VCA sits with no Attack, as a multiple of
/// Tune, and how many octaves Attack raises it.
const TONE_TUNE_MULTIPLE: f64 = 4.0;
const TONE_OCTAVES: f64 = 7.0;
/// The coupling high-pass at the output, which takes away the VCA's offset.
/// Well under the lowest Tune, 45 Hz, which it lowers by under 0.5 dB.
const COUPLING_HZ: f64 = 10.0;
/// What's ringing has to be this far under the new hit, as a share of it,
/// for the oscillator to go back to its zero crossing: 40 dB.
const RESTART_SHARE: f64 = 0.01;
/// The output level that puts a default kick at full accent at the kit's
/// reference peak (see [`super::REFERENCE_PEAK`]). Measured, not derived: see
/// the `the_909_kick_at_full_accent_peaks_at_the_reference` test.
const OUTPUT_GAIN: f32 = 0.5128;
/// Below this, summed over its state, the kick has died away and stops
/// doing work. Far under anything audible.
const SILENT: f64 = 1.0e-7;

/// The kick's controls, as they glide.
#[derive(Debug, Clone, Copy)]
struct Controls {
    /// Tune, in octaves (log2 of Hz), so it glides on a log scale.
    tune_octaves: Ramp,
    sweep: Ramp,
    attack: Ramp,
    /// The body's time constant, as its log, so it glides evenly.
    log_tau: Ramp,
    level: Ramp,
}

impl Controls {
    fn new(settings: Kick909Settings, smoothing: u32) -> Self {
        Self {
            tune_octaves: Ramp::new(settings.tune_hz.log2(), smoothing),
            sweep: Ramp::new(settings.sweep, smoothing),
            attack: Ramp::new(settings.attack, smoothing),
            log_tau: Ramp::new(log_tau(settings.decay_seconds), smoothing),
            level: Ramp::new(super::db_to_gain(settings.level_db), smoothing),
        }
    }
}

/// Per-sample constants for a sample rate.
#[derive(Debug, Clone, Copy)]
struct Rates {
    sample_rate: f64,
    phase_hold_samples: u32,
    body_hold_samples: u32,
    envelope_smoothing: f64,
    transient_decay: f64,
    click_rise: f64,
    click_fall: f64,
    click_high_pass: f64,
    click_low_pass: f64,
    noise_low_pass: f64,
    noise_high_pass: f64,
    coupling: f64,
}

impl Rates {
    fn new(sample_rate: f64) -> Self {
        let samples = |seconds: f64| (seconds * sample_rate).round().max(1.0) as u32;
        Self {
            sample_rate,
            phase_hold_samples: samples(PHASE_HOLD_SECONDS),
            body_hold_samples: samples(BODY_HOLD_SECONDS),
            envelope_smoothing: smoothing(ENVELOPE_SMOOTHING_SECONDS, sample_rate),
            transient_decay: super::decay_per_sample(TRANSIENT_SECONDS, sample_rate),
            click_rise: CLICK_RISE_PER_SECOND / sample_rate,
            click_fall: CLICK_FALL_PER_SECOND / sample_rate,
            click_high_pass: smoothing(CLICK_HIGH_PASS_SECONDS, sample_rate),
            click_low_pass: prewarp(CLICK_LOW_PASS_HZ, sample_rate),
            noise_low_pass: smoothing(NOISE_LOW_PASS_SECONDS, sample_rate),
            noise_high_pass: smoothing(NOISE_HIGH_PASS_SECONDS, sample_rate),
            coupling: prewarp(COUPLING_HZ, sample_rate),
        }
    }
}

/// What's ringing: all zero when it's silent.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    /// Where the oscillator is in its cycle, from 0 to 1. A quarter is the
    /// shaped triangle's zero crossing on the way up.
    phase: f64,
    /// Samples left of the oscillator's wait, and of the envelopes' hold.
    phase_hold_remaining: u32,
    body_hold_remaining: u32,
    /// The envelopes, and the same smoothed: the body's, the click's and the
    /// sweep's.
    body: f64,
    body_lp: f64,
    transient: f64,
    transient_lp: f64,
    sweep: f64,
    sweep_lp: f64,
    /// The click's step, slope-limited, and its high-pass.
    click_step: f64,
    click_hp: f64,
    click_filter: Svf,
    /// The click's noise, and its high-pass.
    noise_lp: f64,
    noise_hp: f64,
    tone: OnePole,
    coupling: OnePole,
    /// Whether it's ringing, or about to.
    active: bool,
}

/// What the controls' values work out to, kept until they move, so a kick
/// whose knobs are still costs no `exp` a sample.
#[derive(Debug, Clone, Copy)]
struct Derived {
    /// The control values these are for.
    tune_octaves: f32,
    log_tau: f32,
    sweep: f32,
    attack: f32,
    tune_hz: f64,
    body_decay: f64,
    sweep_depth: f64,
    sweep_decay: f64,
    tone: f64,
}

impl Derived {
    /// None yet: the first update works them out.
    fn new() -> Self {
        Self {
            tune_octaves: f32::NAN,
            log_tau: f32::NAN,
            sweep: f32::NAN,
            attack: f32::NAN,
            tune_hz: 0.0,
            body_decay: 0.0,
            sweep_depth: 0.0,
            sweep_decay: 0.0,
            tone: 0.0,
        }
    }

    /// These values for the controls at this sample.
    #[inline]
    fn update(
        &mut self,
        tune_octaves: f32,
        log_tau: f32,
        sweep: f32,
        attack: f32,
        sample_rate: f64,
    ) -> Self {
        let tune_moved = tune_octaves != self.tune_octaves;
        if tune_moved {
            self.tune_octaves = tune_octaves;
            self.tune_hz = f64::from(tune_octaves).exp2();
        }
        if log_tau != self.log_tau {
            self.log_tau = log_tau;
            self.body_decay = super::decay_per_sample(f64::from(log_tau).exp(), sample_rate);
        }
        if sweep != self.sweep {
            self.sweep = sweep;
            // Plaits: the first half of the knob deepens the sweep, the
            // second half slows it.
            let sweep = f64::from(sweep);
            self.sweep_depth = SWEEP_DEPTH * (2.0 * sweep).min(1.0);
            let slowing = (2.0 * sweep - 1.0).max(0.0);
            let seconds = SWEEP_SECONDS * (1.0 + SWEEP_SLOWING * slowing * slowing);
            self.sweep_decay = super::decay_per_sample(seconds, sample_rate);
        }
        if tune_moved || attack != self.attack {
            self.attack = attack;
            let tone_hz =
                self.tune_hz * TONE_TUNE_MULTIPLE * (TONE_OCTAVES * f64::from(attack) * LN_2).exp();
            self.tone = prewarp(tone_hz, sample_rate);
        }
        *self
    }
}

/// One 909 kick. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Kick909 {
    settings: Kick909Settings,
    controls: Controls,
    rates: Rates,
    derived: Derived,
    state: State,
}

impl Kick909 {
    pub(crate) fn new(settings: Kick909Settings, sample_rate: f64) -> Self {
        let settings = settings.clamped();
        Self {
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
        *self = Self::new(self.settings, sample_rate);
    }

    /// Takes on new settings straight away, for a kick that isn't sounding.
    pub(crate) fn load(&mut self, settings: Kick909Settings) {
        self.settings = settings.clamped();
        self.controls = Controls::new(
            self.settings,
            super::smoothing_samples(self.rates.sample_rate),
        );
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: Kick909Settings) {
        let settings = settings.clamped();
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        let controls = &mut self.controls;
        controls.tune_octaves.set_target(settings.tune_hz.log2());
        controls.sweep.set_target(settings.sweep);
        controls.attack.set_target(settings.attack);
        controls.log_tau.set_target(log_tau(settings.decay_seconds));
        controls
            .level
            .set_target(super::db_to_gain(settings.level_db));
    }

    /// A hit with this strength (see [`super::velocity_to_strength`]): it
    /// adds to the envelopes and starts the sweep again. The oscillator
    /// carries on, unless what's ringing is far under the hit (see the
    /// module's notes).
    pub(crate) fn hit(&mut self, strength: f32) {
        let height = f64::from(strength);
        let state = &mut self.state;
        if state.body_lp.max(state.body) < RESTART_SHARE * height {
            state.phase = 0.25;
            state.phase_hold_remaining = self.rates.phase_hold_samples;
        }
        // Each envelope gains the hit's height, never past 1, so a soft hit
        // on a loud ring adds a little and never dips it.
        state.body += height * (1.0 - state.body);
        state.transient += height * (1.0 - state.transient);
        state.sweep = 1.0;
        state.body_hold_remaining = self.rates.body_hold_samples;
        state.active = true;
    }

    /// Whether it's ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.state.active
    }

    /// The next sample, with `noise` the kit's. A kick that has died away
    /// returns silence without doing the work.
    #[inline]
    pub(crate) fn next_sample(&mut self, noise: f32) -> f32 {
        // The controls glide whether or not it's ringing.
        let controls = &mut self.controls;
        let tune_octaves = controls.tune_octaves.next_value();
        let sweep = controls.sweep.next_value();
        let attack = controls.attack.next_value();
        let log_tau = controls.log_tau.next_value();
        let level = controls.level.next_value();
        let rates = &self.rates;
        let state = &mut self.state;
        if !state.active {
            return 0.0;
        }
        let derived = self
            .derived
            .update(tune_octaves, log_tau, sweep, attack, rates.sample_rate);

        // The oscillator, waiting at its zero crossing on a fresh hit, then
        // swept down to Tune.
        if state.phase_hold_remaining > 0 {
            state.phase_hold_remaining -= 1;
            state.phase = 0.25;
        } else {
            state.sweep *= derived.sweep_decay;
            let frequency = derived.tune_hz * (1.0 + derived.sweep_depth * state.sweep_lp);
            state.phase += (frequency / rates.sample_rate).min(0.5);
            if state.phase >= 1.0 {
                state.phase -= 1.0;
            }
        }

        // The envelopes hold, then decay; all three are smoothed.
        if state.body_hold_remaining > 0 {
            state.body_hold_remaining -= 1;
        } else {
            state.body *= derived.body_decay;
            state.transient *= rates.transient_decay;
        }
        let smooth = rates.envelope_smoothing;
        state.body_lp += smooth * (state.body - state.body_lp);
        state.transient_lp += smooth * (state.transient - state.transient_lp);
        state.sweep_lp += smooth * (state.sweep - state.sweep_lp);

        let body = shaped_sine(state.phase);

        // The click: a slope-limited step as the hold ends, high-passed into
        // a pulse and rung through a resonant low-pass, plus low noise.
        let step = if state.body_hold_remaining > 0 {
            0.0
        } else {
            1.0
        };
        let error = step - state.click_step;
        state.click_step += error.clamp(-rates.click_fall, rates.click_rise);
        state.click_hp += rates.click_high_pass * (state.click_step - state.click_hp);
        let click = state
            .click_filter
            .process(
                state.click_step - state.click_hp,
                rates.click_low_pass,
                CLICK_Q,
            )
            .low_pass;
        let noise = f64::from(noise) * NOISE_SCALE;
        state.noise_lp += rates.noise_low_pass * (noise - state.noise_lp);
        state.noise_hp += rates.noise_high_pass * (state.noise_lp - state.noise_hp);
        let transient = click + state.noise_lp - state.noise_hp;

        let mix = -transistor_vca(body, state.body_lp)
            - transient * state.transient_lp * f64::from(attack);
        let toned = state.tone.low_pass(mix, derived.tone);
        let out = state.coupling.high_pass(toned, rates.coupling);

        let out_f32 = out as f32 * level * OUTPUT_GAIN;
        if state.phase_hold_remaining == 0
            && state.body_hold_remaining == 0
            && state.body_lp
                + state.transient_lp
                + state.click_filter.state()
                + (state.click_step - state.click_hp).abs()
                + state.tone.state().abs()
                + state.coupling.state().abs()
                < SILENT
        {
            // Died away: stop, and stop doing work until the next hit.
            *state = State::default();
        }
        out_f32
    }
}

/// The body: a triangle shaped towards a sine, blended with a sine, both
/// crossing zero on the way up at a quarter of the cycle.
#[inline]
fn shaped_sine(phase: f64) -> f64 {
    let triangle = (if phase < 0.5 { phase } else { 1.0 - phase }) * 4.0 - 1.0;
    let shaped = 2.0 * triangle / (1.0 + triangle.abs());
    let sine = (TAU * (phase + 0.75)).sin();
    sine + TRIANGLE_SHARE * (shaped - sine)
}

/// Plaits' transistor VCA: asymmetric, with an offset, so it adds even
/// harmonics and a harder hit drives it further.
#[inline]
fn transistor_vca(s: f64, gain: f64) -> f64 {
    let s = (s - 0.6) * gain;
    3.0 * s / (2.0 + s.abs()) + gain * 0.3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::REFERENCE_PEAK;

    const RATE: f64 = 48_000.0;

    /// Runs `kick` for `seconds`, with no noise.
    fn run(kick: &mut Kick909, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|_| kick.next_sample(0.0))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn the_909_kick_at_full_accent_peaks_at_the_reference() {
        let mut kick = Kick909::new(Kick909Settings::default(), RATE);
        kick.hit(1.0);
        let peak = peak(&run(&mut kick, 0.5));
        assert!((peak - REFERENCE_PEAK).abs() < 0.005, "peak {peak}");
    }

    /// From silence, the oscillator starts at its zero crossing and waits
    /// there while the envelopes rise.
    #[test]
    fn its_phase_starts_at_a_zero_crossing() {
        assert!(shaped_sine(0.25).abs() < 1e-12);
        assert!(shaped_sine(0.26) > 0.0, "rising");
        let mut kick = Kick909::new(Kick909Settings::default(), RATE);
        kick.hit(1.0);
        for _ in 0..kick.rates.phase_hold_samples {
            kick.next_sample(0.0);
            assert_eq!(kick.state.phase, 0.25);
        }
        kick.next_sample(0.0);
        assert!(kick.state.phase > 0.25);
    }

    /// Every control glides to a new setting over the drum smoothing time,
    /// rather than jumping: a tenth of the way in, each has moved about a
    /// tenth of the way, and by the end it's there.
    #[test]
    fn every_control_glides() {
        let mut kick = Kick909::new(Kick909Settings::default(), RATE);
        let to = Kick909Settings {
            tune_hz: 70.0,
            sweep: 1.0,
            attack: 1.0,
            decay_seconds: 1.5,
            level_db: 6.0,
        };
        kick.set_settings(to);
        let glide = super::super::smoothing_samples(RATE) as usize;
        let shares = |kick: &Kick909| {
            let c = &kick.controls;
            let share = |now: f32, from: f32, to: f32| (now - from) / (to - from);
            let from = Kick909Settings::default();
            [
                share(
                    c.tune_octaves.value(),
                    from.tune_hz.log2(),
                    to.tune_hz.log2(),
                ),
                share(c.sweep.value(), from.sweep, to.sweep),
                share(c.attack.value(), from.attack, to.attack),
                share(
                    c.log_tau.value(),
                    log_tau(from.decay_seconds),
                    log_tau(to.decay_seconds),
                ),
                share(
                    c.level.value(),
                    super::super::db_to_gain(from.level_db),
                    super::super::db_to_gain(to.level_db),
                ),
            ]
        };
        for _ in 0..glide / 10 {
            kick.next_sample(0.0);
        }
        for share in shares(&kick) {
            assert!((0.05..0.15).contains(&share), "{:?}", shares(&kick));
        }
        for _ in glide / 10..glide {
            kick.next_sample(0.0);
        }
        for share in shares(&kick) {
            assert!((share - 1.0).abs() < 1e-3, "{:?}", shares(&kick));
        }
    }

    /// Once it has died away far below hearing it stops, and does no work
    /// until the next hit. At the longest Decay, 1.5 s to fall 40 dB, it's
    /// about 140 dB down after 5.3 s.
    #[test]
    fn a_kick_that_has_died_away_stops() {
        let settings = Kick909Settings {
            decay_seconds: 1.5,
            ..Kick909Settings::default()
        };
        let mut kick = Kick909::new(settings, RATE);
        assert!(!kick.is_sounding());
        kick.hit(1.0);
        assert!(kick.is_sounding());
        let samples = run(&mut kick, 7.0);
        assert!(!kick.is_sounding(), "still ringing after 7 s");
        let stopped = samples.iter().rposition(|&s| s != 0.0).unwrap();
        let seconds = stopped as f64 / RATE;
        assert!((3.0..7.0).contains(&seconds), "stopped after {seconds} s");
        let last = peak(&samples[stopped - 4800..=stopped]);
        assert!(last < 1e-6, "last 0.1 s peaked at {last}");
        kick.hit(1.0);
        assert!(peak(&run(&mut kick, 0.1)) > 0.3);
        assert_eq!(kick.state.phase_hold_remaining, 0);
    }

    /// A second hit while it rings adds to it: the oscillator carries on,
    /// and the body's envelope only grows.
    #[test]
    fn a_hit_while_it_rings_adds_to_it() {
        let mut kick = Kick909::new(Kick909Settings::default(), RATE);
        kick.hit(1.0);
        run(&mut kick, 0.05);
        let (phase, body) = (kick.state.phase, kick.state.body);
        kick.hit(0.1);
        assert_eq!(kick.state.phase, phase, "the oscillator carries on");
        assert_eq!(kick.state.phase_hold_remaining, 0);
        assert!(kick.state.body > body, "{} after {body}", kick.state.body);
        // Once it's 40 dB under a new hit, the oscillator goes back to its
        // zero crossing for it.
        run(&mut kick, 0.9);
        assert!(kick.is_sounding());
        assert!(kick.state.body_lp < 0.01);
        kick.hit(1.0);
        assert_eq!(kick.state.phase, 0.25);
    }
}
