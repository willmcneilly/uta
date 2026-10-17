//! Each voice's LFO: a slow wave that moves the pitch, the pulse width and
//! the cutoff. See RFC-008, "The instrument" (LFO) and "In the engine".
//!
//! Where a voice's LFO is in its cycle (its phase) comes from one of three
//! places, which the synth works out each sample:
//! - with Retrigger on, the voice's own phase, from 0 at its note's start;
//! - off, a phase every voice shares: synced, the song's position, so its
//!   cycles line up with the bars; not synced, one that runs from Play.
//!
//! Its shape is worked out at the control rate, every
//! [`super::FILTER_CONTROL_SAMPLES`], as the per-voice filter is, for the
//! next control point, and the output glides there in a straight line. A
//! wave of at most 20 Hz barely bends in a third of a millisecond, and that
//! keeps a voice's LFO to a few additions a sample. See RFC-008, "Control
//! rate".
//!
//! What the voice's LFO keeps for itself is the held level of its
//! sample-and-hold, its own seeded random generator for it, and what's
//! left of its last step, so that a stepped shape never jumps.

use super::settings::LfoShape;
use crate::noise::Noise;

/// How quickly what's left of a step dies away, as the time it takes to fall
/// to about a third: a few milliseconds, so a square or a sample-and-hold
/// steps crisply but never clicks. See RFC-008, "In the engine" (Everything
/// glides).
pub const LFO_SMOOTHING_SECONDS: f64 = 0.002;

/// A change from one control point to the next bigger than this is a step,
/// to be smoothed. No smooth shape moves this fast: a sine at the fastest
/// rate, 20 Hz, moves at most 0.05 in 16 samples at 44.1 kHz.
const STEP: f32 = 0.1;

/// One voice's LFO.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lfo {
    /// The voice's own phase, from 0 to 1, for Retrigger.
    phase: f64,
    /// The phase at the last control point, to tell when a new cycle
    /// starts.
    last_phase: f64,
    /// The sample-and-hold's level, from -1 to 1.
    held: f32,
    /// Where the sample-and-hold's levels come from: the voice's own seed,
    /// so a render is the same every time.
    random: Noise,
    /// The shape's value at the last control point, before smoothing.
    last_raw: f32,
    /// What's left of the steps so far, added to the shape so that the
    /// output carries on from where it was. It dies away at each control
    /// point.
    offset: f32,
    /// The output this sample, the step it moves by each sample, and where
    /// it's heading: its value at the next control point.
    out: f32,
    out_step: f32,
    target: f32,
}

impl Lfo {
    /// A voice's LFO, whose sample-and-hold starts from `seed` (not zero).
    pub(crate) fn seeded(seed: u32) -> Self {
        Self {
            phase: 0.0,
            last_phase: 0.0,
            held: 0.0,
            random: Noise::seeded(seed),
            last_raw: 0.0,
            offset: 0.0,
            out: 0.0,
            out_step: 0.0,
            target: 0.0,
        }
    }

    /// Starts the sample-and-hold's levels again from the seed: on Play.
    pub(crate) fn restart(&mut self) {
        self.random.restart();
    }

    /// Starts afresh for a new note, `phase` into its cycle, with no step
    /// left over from the last. The sample-and-hold takes a new level. It
    /// holds its value until the next control point.
    pub(crate) fn start(&mut self, phase: f64, shape: LfoShape) {
        self.phase = phase;
        self.last_phase = phase;
        self.held = self.random.next_sample();
        self.last_raw = shape_value(shape, phase, self.held);
        self.offset = 0.0;
        self.out = self.last_raw;
        self.out_step = 0.0;
        self.target = self.last_raw;
    }

    /// The voice's own phase this sample, which then moves on by
    /// `increment` (a share of a cycle) for the next.
    #[inline]
    pub(crate) fn advance_own(&mut self, increment: f64) -> f64 {
        let phase = self.phase;
        self.phase = wrap(phase + increment);
        phase
    }

    /// At a control point: from where it is now, heads for `shape` at
    /// `phase`, where it will be at the next control point `samples` on,
    /// with any step since this point smoothed. A new cycle (the phase going
    /// back) takes a new sample-and-hold level. `decay` is how much of a
    /// step is left after `samples`.
    #[inline]
    pub(crate) fn control(&mut self, phase: f64, shape: LfoShape, decay: f32, samples: u32) {
        let new_cycle = phase < self.last_phase;
        self.last_phase = phase;
        if new_cycle {
            self.held = self.random.next_sample();
        }
        let raw = shape_value(shape, phase, self.held);
        // A step carries on from where the output was, and dies away from
        // there. The sample-and-hold's new level is one, however small.
        let new_level = new_cycle && shape == LfoShape::SampleAndHold;
        if new_level || (raw - self.last_raw).abs() > STEP {
            self.offset += self.last_raw - raw;
        }
        self.last_raw = raw;
        self.offset *= decay;
        // Exactly where it was heading, so the straight lines never drift.
        self.out = self.target;
        self.target = raw + self.offset;
        self.out_step = (self.target - self.out) / samples as f32;
    }

    /// The output this sample, from -1 to 1, and on by a sample towards the
    /// next control point.
    #[inline]
    pub(crate) fn next_sample(&mut self) -> f32 {
        let out = self.out;
        self.out += self.out_step;
        out
    }

    /// The output this sample.
    #[inline]
    pub(crate) fn value(&self) -> f32 {
        self.out
    }
}

/// How much of a step is left after `samples` at `sample_rate`.
pub(crate) fn decay(samples: u32, sample_rate: f64) -> f32 {
    (-f64::from(samples) / (LFO_SMOOTHING_SECONDS * sample_rate)).exp() as f32
}

/// A phase moved back into 0 to 1.
#[inline]
pub(crate) fn wrap(phase: f64) -> f64 {
    phase - phase.floor()
}

/// 2 to the power `x`, for `x` within a sixth either side of zero, as a
/// vibrato's pitch ratio is: a few multiplications rather than `exp2`, and
/// within a millionth of a semitone of it there.
#[inline]
pub(crate) fn exp2_small(x: f64) -> f64 {
    let y = x * std::f64::consts::LN_2;
    1.0 + y * (1.0 + y * (0.5 + y * (1.0 / 6.0 + y * (1.0 / 24.0 + y / 120.0))))
}

/// `shape` at `phase` (0 to 1), from -1 to 1, with `held` as the
/// sample-and-hold's level. Each starts its cycle where RFC-008 has it start
/// on the bar: the sine and triangle at zero, rising; the saw at the bottom;
/// the square high.
#[inline]
pub(crate) fn shape_value(shape: LfoShape, phase: f64, held: f32) -> f32 {
    let phase = phase as f32;
    match shape {
        LfoShape::Sine => (std::f32::consts::TAU * phase).sin(),
        LfoShape::Triangle => {
            if phase < 0.25 {
                4.0 * phase
            } else if phase < 0.75 {
                2.0 - 4.0 * phase
            } else {
                4.0 * phase - 4.0
            }
        }
        LfoShape::Saw => 2.0 * phase - 1.0,
        LfoShape::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        LfoShape::SampleAndHold => held,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;
    const CONTROL: u32 = 16;

    /// `samples` of `shape` at `hz` from the start of its cycle, run as the
    /// synth runs a voice's LFO.
    fn run(shape: LfoShape, hz: f64, samples: usize) -> Vec<f32> {
        let decay = decay(CONTROL, RATE);
        let increment = hz / RATE;
        let mut lfo = Lfo::seeded(1);
        lfo.start(0.0, shape);
        let mut out = Vec::with_capacity(samples);
        for n in 0..samples {
            let phase = lfo.advance_own(increment);
            if n % CONTROL as usize == 0 {
                let ahead = wrap(phase + f64::from(CONTROL) * increment);
                lfo.control(ahead, shape, decay, CONTROL);
            }
            out.push(lfo.next_sample());
        }
        out
    }

    #[test]
    fn each_shape_starts_its_cycle_as_rfc_008_has_it() {
        let at = |shape, phase| shape_value(shape, phase, 0.5);
        assert_eq!(at(LfoShape::Sine, 0.0), 0.0);
        assert!((at(LfoShape::Sine, 0.25) - 1.0).abs() < 1e-6);
        assert_eq!(at(LfoShape::Triangle, 0.0), 0.0);
        assert_eq!(at(LfoShape::Triangle, 0.25), 1.0);
        assert_eq!(at(LfoShape::Triangle, 0.75), -1.0);
        assert_eq!(at(LfoShape::Saw, 0.0), -1.0);
        assert_eq!(at(LfoShape::Saw, 0.5), 0.0);
        assert_eq!(at(LfoShape::Square, 0.0), 1.0);
        assert_eq!(at(LfoShape::Square, 0.5), -1.0);
        assert_eq!(at(LfoShape::SampleAndHold, 0.3), 0.5);
    }

    /// A square's edge, run through the LFO, doesn't jump: it heads for the
    /// new level and is most of the way there within a few milliseconds.
    #[test]
    fn a_step_is_smoothed_over_a_few_milliseconds() {
        let out = run(LfoShape::Square, 1.0, RATE as usize);
        let biggest = out
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(biggest < 0.03, "the biggest step a sample is {biggest}");
        // 20 ms after the edge at half a second, it's all but there.
        let edge = RATE as usize / 2;
        assert!(out[edge + 960] < -0.999, "{}", out[edge + 960]);
        assert!(out[edge - CONTROL as usize] > 0.99);
    }

    /// A sine passes through all but untouched: there's no step to smooth,
    /// and the straight lines between control points don't lag it.
    #[test]
    fn a_smooth_shape_is_untouched() {
        let increment = 20.0 / RATE;
        let out = run(LfoShape::Sine, 20.0, RATE as usize);
        for (n, &value) in out.iter().enumerate().skip(CONTROL as usize) {
            let exact = shape_value(LfoShape::Sine, n as f64 * increment, 0.0);
            assert!((value - exact).abs() < 1e-3, "{value} against {exact}");
        }
    }

    /// The sample-and-hold takes a new level at the start of each cycle and
    /// holds it, the same levels from the same seed.
    #[test]
    fn sample_and_hold_takes_a_new_level_each_cycle_from_its_seed() {
        let levels = |seed| {
            let decay = decay(CONTROL, RATE);
            let increment = 10.0 / RATE;
            let mut lfo = Lfo::seeded(seed);
            lfo.start(0.0, LfoShape::SampleAndHold);
            let mut held = Vec::new();
            for n in 0..4 * 4_800 {
                let phase = lfo.advance_own(increment);
                if n % CONTROL as usize == 0 {
                    let ahead = wrap(phase + f64::from(CONTROL) * increment);
                    lfo.control(ahead, LfoShape::SampleAndHold, decay, CONTROL);
                }
                lfo.next_sample();
                if n % 4_800 == 4_000 {
                    held.push(lfo.held);
                }
            }
            held
        };
        let first = levels(7);
        assert_eq!(first, levels(7), "the same seed, the same levels");
        assert_ne!(first, levels(8));
        assert!(first.windows(2).all(|w| w[0] != w[1]), "{first:?}");
        assert!(first.iter().all(|level| (-1.0..=1.0).contains(level)));
    }

    #[test]
    fn exp2_small_is_exp2_where_vibrato_uses_it() {
        for i in -100..=100 {
            let x = f64::from(i) / 600.0;
            let semitones = 12.0 * (exp2_small(x).log2() - x);
            assert!(semitones.abs() < 1e-6, "{x}: {semitones} semitones off");
        }
    }
}
