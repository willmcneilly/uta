//! The 808 kick: a pulse that sets a resonator ringing. See RFC-006, "How
//! each sound is made" (Kick, 808).
//!
//! Ported from Emilie Gillet's `AnalogBassDrum` in Plaits (MIT licence),
//! itself a model of the TR-808 bass drum circuit that Kurt Werner, Jonathan
//! Abel and Julius Smith analysed ("A Physically-Informed, Circuit-Bendable,
//! Digital Model of the Roland TR-808 Bass Drum Circuit", DAFx-14). The
//! comments name the circuit parts each step stands for, as Plaits does.
//!
//! What it keeps from the circuit, and why it sounds like an 808 rather than
//! a sine with an envelope:
//! - **A hit adds energy; it never restarts anything.** The resonator keeps
//!   ringing between hits, so a fast repeat sums with the last one and each
//!   hit comes out a little different, without a click.
//! - **The attack shift.** For the first 6 ms the resonator rings about 2.7
//!   times higher, with a higher Q: less than one cycle, heard as punch
//!   rather than as a sweep. A small negative retrigger pulse when it drops
//!   back fills in the energy.
//! - **The pitch sigh.** Leakage makes the pitch follow the level, so it
//!   sags towards Tune as the kick dies away: from about 15% sharp on a
//!   hard, long hit (Werner's 56 Hz to 49).
//! - **Velocity is the pulse's height.** A harder hit rings more, sighs
//!   further and punches higher, so it's brighter, not just louder.
//!
//! Where it differs from Plaits, and why:
//! - Times are in seconds, so it sounds the same at any sample rate.
//! - The attack shift lasts 6 ms at the 808's own 49 Hz, and the same share
//!   of a cycle at other Tunes, so its level doesn't dip at some pitches.
//! - Plaits' softest hit is an unaccented 808 hit, and two parts of its
//!   pulse have a fixed size. Uta's velocities go softer, so below an
//!   unaccented hit those parts shrink with it, and soft hits get softer all
//!   the way down.
//! - The resonator's Q follows its frequency so it dies away by 40 dB in
//!   the Decay time, at any Tune (Plaits' Q also grows with the
//!   frequency, so its decay doesn't depend on pitch either).
//! - Tone is a low-pass from 200 Hz to 8 kHz, whatever the Tune, from the
//!   research's 808 recipe.
//! - The pitch sigh is measured from the resonator at rest, so the kick
//!   settles exactly on Tune.
//! - The trigger pulse's edges are smoothed over about 0.05 ms, so even the
//!   click is a fast curve rather than a one-sample step, and over longer
//!   for softer hits, like a softer stick, so they're duller as well as
//!   quieter.
//! - Plaits' "sustain" mode, which swaps the resonator for an oscillator, is
//!   left out: Uta's drums are one-shots.

use std::f64::consts::PI;

use crate::drums::{KickSettings, log_tau};
use crate::ramp::Ramp;

/// How long the trigger pulse lasts (Q39 / Q40).
const TRIGGER_PULSE_SECONDS: f64 = 1.0e-3;
/// How long the attack shift lasts at the 808's own pitch (Q41 / Q42). At
/// other Tunes it lasts the same share of a cycle, so the shift ends at the
/// same point in the waveform and the level doesn't dip at some pitches.
const FM_PULSE_SECONDS: f64 = 6.0e-3;
/// The 808 kick's own pitch, which [`FM_PULSE_SECONDS`] is for.
const NATIVE_TUNE_HZ: f64 = 49.0;
/// How fast the pulse's tail dies away once it ends.
const PULSE_DECAY_SECONDS: f64 = 0.2e-3;
/// The pulse shaper's high-pass (C40 / R163), and the smoothing of the
/// attack shift's edges.
const PULSE_FILTER_SECONDS: f64 = 0.1e-3;
/// How fast the retrigger pulse dies away (C39 / R161). At the shortest
/// Decays the ring itself is quicker than this, and the pulse dies away with
/// it instead, or its tail would hold the kick on past its Decay.
const RETRIG_PULSE_SECONDS: f64 = 0.05;
/// Smooths the trigger pulse's edges at full accent, so the click has no
/// one-sample step. Softer hits are smoothed more (see [`Kick::hit`]).
const PULSE_EDGE_SECONDS: f64 = 0.05e-3;
/// How much higher the resonator rings during the attack shift: 1 + 1.7.
const ATTACK_FM: f64 = 1.7;
/// How far the pitch follows the level (Q43 and R170 leakage).
const SELF_FM: f64 = 0.08;
/// The leakage's output with the resonator at rest, which the sigh is
/// measured from, so the kick settles on Tune: 0.7 + diode(-1).
const PUNCH_AT_REST: f64 = 0.7 - 0.7 * 2.0 / 3.0;
/// The pulse's height at full accent, as in Plaits (an analogue of the
/// 808's 14 V trigger).
const MAX_PULSE_HEIGHT: f64 = 10.0;
/// An unaccented hit's pulse, Plaits' softest (an analogue of the 808's 4 V).
const UNACCENTED_PULSE_HEIGHT: f64 = 3.0;
/// Plaits scales the resonator's input by `0.001 / f0`, with `f0` as a share
/// of its 48 kHz rate. In Hz that's this over Tune, which keeps the level
/// the same at every pitch.
const INPUT_SCALE_HZ: f64 = 48.0;
/// Tone's range: the low-pass after the resonator, on a log scale.
const MIN_TONE_HZ: f64 = 200.0;
const MAX_TONE_HZ: f64 = 8000.0;
/// The output level that puts a default kick at full accent at the kit's
/// reference peak (see [`super::REFERENCE_PEAK`]). Measured, not derived: see
/// the `the_kick_at_full_accent_peaks_at_the_reference` test.
const OUTPUT_GAIN: f32 = 0.6695;
/// Below this, summed over its state, the kick has died away and stops
/// doing work. Far under anything audible (its loudest internal swings are
/// around 1), and well above the slow "denormal" numbers.
const SILENT: f64 = 1.0e-7;

/// The kick's controls, as they glide.
#[derive(Debug, Clone, Copy)]
struct Controls {
    /// Tune, in octaves (log2 of Hz), so it glides on a log scale.
    tune_octaves: Ramp,
    tone: Ramp,
    /// The resonator's time constant, as its log, so it glides evenly.
    log_tau: Ramp,
    level: Ramp,
}

impl Controls {
    fn new(settings: KickSettings, smoothing: u32) -> Self {
        Self {
            tune_octaves: Ramp::new(settings.tune_hz.log2(), smoothing),
            tone: Ramp::new(settings.tone, smoothing),
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
}

impl Rates {
    fn new(sample_rate: f64) -> Self {
        let samples = |seconds: f64| (seconds * sample_rate).round().max(1.0);
        Self {
            sample_rate,
            trigger_samples: samples(TRIGGER_PULSE_SECONDS) as u32,
            pulse_decay: 1.0 - 1.0 / samples(PULSE_DECAY_SECONDS),
            pulse_filter: 1.0 / samples(PULSE_FILTER_SECONDS),
        }
    }
}

/// What's ringing: all zero when it's silent.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    /// Samples left of the trigger pulse and of the attack shift.
    pulse_remaining: u32,
    fm_pulse_remaining: u32,
    pulse_height: f64,
    /// Plaits' softest hit is a pulse 3 high (an unaccented 808 hit), and it
    /// gives two parts of the pulse a fixed size: the step down as it ends,
    /// and the retrigger pulse. Uta's hits go softer than that, so for them
    /// those parts shrink with the hit, by this share, so soft hits get
    /// softer all the way down. 1 from an unaccented hit up: Plaits exactly.
    soft_scale: f64,
    /// How fast the pulse's edges rise and fall: a one-pole coefficient.
    edge: f64,
    /// The trigger pulse before and after its edges are smoothed.
    pulse_raw: f64,
    pulse: f64,
    pulse_lp: f64,
    fm_pulse_lp: f64,
    retrig_pulse: f64,
    /// The resonator: a trapezoidal state-variable filter (Andrew Simper's,
    /// as Plaits' is), whose band-pass output is the kick.
    s1: f64,
    s2: f64,
    lp_out: f64,
    tone_lp: f64,
    /// Whether it's ringing, or about to.
    active: bool,
}

/// What the controls' values work out to, kept until they move, so a kick
/// whose knobs are still costs no `exp` or `powf` a sample.
#[derive(Debug, Clone, Copy)]
struct Derived {
    /// The control values these are for.
    tune_octaves: f32,
    log_tau: f32,
    tone: f32,
    tune_hz: f64,
    tau: f64,
    tone_coefficient: f64,
    exciter_leak: f64,
}

impl Derived {
    /// None yet: the first update works them out.
    fn new() -> Self {
        Self {
            tune_octaves: f32::NAN,
            log_tau: f32::NAN,
            tone: f32::NAN,
            tune_hz: 0.0,
            tau: 0.0,
            tone_coefficient: 0.0,
            exciter_leak: 0.0,
        }
    }

    /// These values for the controls at this sample.
    #[inline]
    fn update(&mut self, tune_octaves: f32, log_tau: f32, tone: f32, sample_rate: f64) -> Self {
        if tune_octaves != self.tune_octaves {
            self.tune_octaves = tune_octaves;
            self.tune_hz = f64::from(tune_octaves).exp2();
        }
        if log_tau != self.log_tau {
            self.log_tau = log_tau;
            self.tau = f64::from(log_tau).exp();
        }
        if tone != self.tone {
            self.tone = tone;
            let tone = f64::from(tone);
            let tone_hz = MIN_TONE_HZ * (MAX_TONE_HZ / MIN_TONE_HZ).powf(tone);
            self.tone_coefficient = one_pole(tone_hz, sample_rate);
            self.exciter_leak = 0.08 * (tone + 0.25);
        }
        *self
    }
}

/// One 808 kick circuit. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Kick {
    settings: KickSettings,
    controls: Controls,
    rates: Rates,
    derived: Derived,
    state: State,
}

impl Kick {
    pub(crate) fn new(settings: KickSettings, sample_rate: f64) -> Self {
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
    pub(crate) fn load(&mut self, settings: KickSettings) {
        self.settings = settings.clamped();
        self.controls = Controls::new(
            self.settings,
            super::smoothing_samples(self.rates.sample_rate),
        );
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: KickSettings) {
        let settings = settings.clamped();
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        let controls = &mut self.controls;
        controls.tune_octaves.set_target(settings.tune_hz.log2());
        controls.tone.set_target(settings.tone);
        controls.log_tau.set_target(log_tau(settings.decay_seconds));
        controls
            .level
            .set_target(super::db_to_gain(settings.level_db));
    }

    /// A hit with this strength (see [`super::velocity_to_strength`]): a new
    /// trigger pulse and attack shift. The resonator carries on from where it
    /// is.
    pub(crate) fn hit(&mut self, strength: f32) {
        let tune_hz = f64::from(self.controls.tune_octaves.value()).exp2();
        let fm_seconds = FM_PULSE_SECONDS * NATIVE_TUNE_HZ / tune_hz;
        let state = &mut self.state;
        state.pulse_remaining = self.rates.trigger_samples;
        state.fm_pulse_remaining = (fm_seconds * self.rates.sample_rate).round().max(1.0) as u32;
        state.pulse_height = MAX_PULSE_HEIGHT * f64::from(strength);
        state.soft_scale = (state.pulse_height / UNACCENTED_PULSE_HEIGHT).min(1.0);
        // A softer hit is like a softer stick: it's in contact longer, so its
        // edges are rounder and it's duller as well as quieter.
        let edge_seconds = PULSE_EDGE_SECONDS / f64::from(strength).max(0.01).sqrt();
        state.edge = one_pole(1.0 / (2.0 * PI * edge_seconds), self.rates.sample_rate);
        state.active = true;
    }

    /// Whether it's ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.state.active
    }

    /// The next sample. A kick that has died away returns silence without
    /// doing the work.
    #[inline]
    pub(crate) fn next_sample(&mut self) -> f32 {
        // The controls glide whether or not it's ringing.
        let controls = &mut self.controls;
        let tune_octaves = controls.tune_octaves.next_value();
        let tone = controls.tone.next_value();
        let log_tau = controls.log_tau.next_value();
        let level = controls.level.next_value();
        let rates = &self.rates;
        let state = &mut self.state;
        if !state.active {
            return 0.0;
        }
        let derived = self
            .derived
            .update(tune_octaves, log_tau, tone, rates.sample_rate);
        let (tune_hz, tau) = (derived.tune_hz, derived.tau);

        // Q39 / Q40: the trigger pulse, its edges smoothed.
        let raw = if state.pulse_remaining > 0 {
            state.pulse_remaining -= 1;
            if state.pulse_remaining > 0 {
                state.pulse_height
            } else {
                // Plaits steps down by 1 here (see `soft_scale`).
                state.pulse_height - state.soft_scale
            }
        } else {
            state.pulse_raw * rates.pulse_decay
        };
        state.pulse_raw = raw;
        state.pulse += state.edge * (raw - state.pulse);
        let pulse = state.pulse;

        // C40 / R163 / R162 / D83: the pulse shaper. A high-pass picks out
        // its edges, and the diode clips the falling one.
        state.pulse_lp += rates.pulse_filter * (pulse - state.pulse_lp);
        let pulse = diode((pulse - state.pulse_lp) + pulse * 0.044);

        // Q41 / Q42: the attack shift, and the retrigger pulse as it ends.
        let fm_pulse = if state.fm_pulse_remaining > 0 {
            state.fm_pulse_remaining -= 1;
            // C39 / C52 (see `soft_scale`)
            state.retrig_pulse = if state.fm_pulse_remaining > 0 {
                0.0
            } else {
                -0.8 * state.soft_scale
            };
            1.0
        } else {
            // C39 / R161, but never outlasting the ring (see
            // `RETRIG_PULSE_SECONDS`).
            let retrig_seconds = RETRIG_PULSE_SECONDS.min(tau);
            state.retrig_pulse *= 1.0 - 1.0 / (retrig_seconds * rates.sample_rate).max(1.0);
            0.0
        };
        state.fm_pulse_lp += rates.pulse_filter * (fm_pulse - state.fm_pulse_lp);

        // Q43 and R170 leakage: the pitch follows the level (the sigh).
        let punch = 0.7 + diode(10.0 * state.lp_out - 1.0);
        let self_fm = SELF_FM * (punch - PUNCH_AT_REST);
        // Q43 / R165
        let attack_fm = state.fm_pulse_lp * ATTACK_FM;
        let frequency = (tune_hz * (1.0 + attack_fm + self_fm)).clamp(1.0, 0.4 * rates.sample_rate);

        // The resonator. Its Q follows the frequency, so it always dies
        // away with the time constant `tau`.
        let g = tan(PI * frequency / rates.sample_rate);
        let r = 1.0 / (PI * frequency * tau).max(0.5);
        let h = 1.0 / (1.0 + r * g + g * g);
        let input = (pulse - state.retrig_pulse * 0.2) * INPUT_SCALE_HZ / tune_hz;
        let hp = (input - r * state.s1 - g * state.s1 - state.s2) * h;
        let bp = g * hp + state.s1;
        state.s1 = g * hp + bp;
        let lp = g * bp + state.s2;
        state.s2 = g * bp + lp;
        state.lp_out = lp;

        // The Tone low-pass, which also lets some of the pulse through: the
        // click.
        state.tone_lp +=
            derived.tone_coefficient * (pulse * derived.exciter_leak + bp - state.tone_lp);

        let out = state.tone_lp as f32 * level * OUTPUT_GAIN;
        if state.pulse_remaining == 0
            && state.fm_pulse_remaining == 0
            && state.s1.abs()
                + state.s2.abs()
                + state.tone_lp.abs()
                + state.pulse_raw.abs()
                + state.pulse.abs()
                + state.retrig_pulse.abs()
                < SILENT
        {
            // Died away: stop, and stop doing work until the next hit.
            *state = State::default();
        }
        out
    }
}

/// Plaits' diode: passes positive swings, and clips negative ones softly to
/// about -0.7.
#[inline]
fn diode(x: f64) -> f64 {
    if x >= 0.0 {
        x
    } else {
        let x = 2.0 * x;
        0.7 * x / (1.0 + x.abs())
    }
}

/// tan(x), by its series where it's accurate to better than one part in a
/// million: the resonator's angle stays under 0.02 at 48 kHz, so it never
/// needs the library's `tan`.
#[inline]
fn tan(x: f64) -> f64 {
    if x < 0.2 {
        let x2 = x * x;
        x * (1.0 + x2 * (1.0 / 3.0 + x2 * (2.0 / 15.0 + x2 * (17.0 / 315.0))))
    } else {
        x.tan()
    }
}

/// The coefficient of a one-pole low-pass at `cutoff_hz`.
#[inline]
fn one_pole(cutoff_hz: f64, sample_rate: f64) -> f64 {
    1.0 - (-2.0 * PI * cutoff_hz / sample_rate).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::REFERENCE_PEAK;

    const RATE: f64 = 48_000.0;

    /// Runs `kick` for `seconds`.
    fn run(kick: &mut Kick, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|_| kick.next_sample())
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn the_kick_at_full_accent_peaks_at_the_reference() {
        let mut kick = Kick::new(KickSettings::default(), RATE);
        kick.hit(1.0);
        let peak = peak(&run(&mut kick, 0.5));
        assert!((peak - REFERENCE_PEAK).abs() < 0.005, "peak {peak}");
    }

    /// Once it has died away far below hearing it stops, and does no work
    /// until the next hit. At the longest Decay, 0.8 s to fall 40 dB, it's
    /// about 140 dB down after 2.8 s.
    #[test]
    fn a_kick_that_has_died_away_stops() {
        let settings = KickSettings {
            decay_seconds: 0.8,
            ..KickSettings::default()
        };
        let mut kick = Kick::new(settings, RATE);
        assert!(!kick.is_sounding());
        kick.hit(1.0);
        assert!(kick.is_sounding());
        let samples = run(&mut kick, 3.5);
        assert!(!kick.is_sounding(), "still ringing after 3.5 s");
        let stopped = samples.iter().rposition(|&s| s != 0.0).unwrap();
        let seconds = stopped as f64 / RATE;
        assert!((2.0..3.5).contains(&seconds), "stopped after {seconds} s");
        // Its last sound was far below hearing: no click as it stops.
        let last = peak(&samples[stopped - 4800..=stopped]);
        assert!(last < 1e-6, "last 0.1 s peaked at {last}");
        // A new hit starts it again.
        kick.hit(1.0);
        assert!(peak(&run(&mut kick, 0.1)) > 0.4);
    }

    /// A second hit adds to the ringing rather than restarting it: the
    /// sound carries on through the hit, and the second hit doesn't come
    /// out a copy of the first.
    #[test]
    fn a_hit_while_it_rings_adds_to_it() {
        let mut kick = Kick::new(KickSettings::default(), RATE);
        kick.hit(0.3);
        let first = run(&mut kick, 0.05);
        let before = *first.last().unwrap();
        kick.hit(0.3);
        let second = run(&mut kick, 0.05);
        // The first samples after the hit carry on from the ringing: the
        // pulse edge is smoothed, so the hit only bends the waveform.
        assert!(
            (second[0] - before).abs() < 0.01,
            "{before} then {}",
            second[0]
        );
        let difference = first
            .iter()
            .zip(&second)
            .fold(0.0f32, |max, (a, b)| max.max((a - b).abs()));
        assert!(difference > 0.05, "the second hit copied the first");
    }
}
