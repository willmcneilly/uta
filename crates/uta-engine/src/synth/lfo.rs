//! Each voice's LFO: a slow wave that moves the pitch, the pulse width and
//! the cutoff. See RFC-008, "The instrument" (LFO) and "In the engine".
//!
//! Where a voice's LFO is in its cycle (its phase) comes from one of three
//! places, which the synth works out each sample:
//! - with Retrigger on, the voice's own phase, from 0 at its note's start;
//! - off, a phase every voice shares: synced, the song's position, so its
//!   cycles line up with the bars; not synced, one that runs from Play.
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

/// A change in one sample bigger than this is a step, to be smoothed. No
/// smooth shape moves this fast: a sine at the fastest rate, 20 Hz, moves at
/// most 0.003 a sample at 44.1 kHz.
const STEP: f32 = 0.05;

/// One voice's LFO.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lfo {
    /// The voice's own phase, from 0 to 1, for Retrigger.
    phase: f64,
    /// The phase on the last sample, to tell when a new cycle starts.
    last_phase: f64,
    /// The sample-and-hold's level, from -1 to 1.
    held: f32,
    /// Where the sample-and-hold's levels come from: the voice's own seed,
    /// so a render is the same every time.
    random: Noise,
    /// The shape's value on the last sample, before smoothing.
    last_raw: f32,
    /// What's left of the steps so far, added to the shape so that the
    /// output carries on from where it was. It dies away each sample.
    offset: f32,
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
        }
    }

    /// Starts the sample-and-hold's levels again from the seed: on Play.
    pub(crate) fn restart(&mut self) {
        self.random.restart();
    }

    /// Starts afresh for a new note, `phase` into its cycle, with no step
    /// left over from the last. The sample-and-hold takes a new level.
    pub(crate) fn start(&mut self, phase: f64, shape: LfoShape) {
        self.phase = phase;
        self.last_phase = phase;
        self.held = self.random.next_sample();
        self.last_raw = shape_value(shape, phase, self.held);
        self.offset = 0.0;
    }

    /// The voice's own phase this sample, which then moves on by
    /// `increment` (a share of a cycle) for the next.
    #[inline]
    pub(crate) fn advance_own(&mut self, increment: f64) -> f64 {
        let phase = self.phase;
        self.phase = wrap(phase + increment);
        phase
    }

    /// The output at `phase`, from -1 to 1: `shape`, with any step since the
    /// last sample smoothed. A new cycle (the phase going back) takes a new
    /// sample-and-hold level. `decay` is how much of a step is left after a
    /// sample.
    #[inline]
    pub(crate) fn next(&mut self, phase: f64, shape: LfoShape, decay: f32) -> f32 {
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
        raw + self.offset
    }

    /// The output on the last sample.
    #[inline]
    pub(crate) fn value(&self) -> f32 {
        self.last_raw + self.offset
    }
}

/// How much of a step is left after one sample at `sample_rate`.
pub(crate) fn decay(sample_rate: f64) -> f32 {
    (-1.0 / (LFO_SMOOTHING_SECONDS * sample_rate)).exp() as f32
}

/// A phase moved back into 0 to 1.
#[inline]
pub(crate) fn wrap(phase: f64) -> f64 {
    phase - phase.floor()
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
        let decay = decay(RATE);
        let mut lfo = Lfo::seeded(1);
        lfo.start(0.0, LfoShape::Square);
        let increment = 1.0 / RATE; // 1 Hz
        let mut phase = 0.0;
        let mut out = Vec::new();
        for _ in 0..RATE as usize {
            out.push(lfo.next(phase, LfoShape::Square, decay));
            phase = wrap(phase + increment);
        }
        let biggest = out
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(biggest < 0.03, "the biggest step a sample is {biggest}");
        // 20 ms after the edge at half a second, it's all but there.
        let edge = RATE as usize / 2;
        assert!(out[edge + 960] < -0.999, "{}", out[edge + 960]);
        assert!(out[edge - 1] > 0.99);
    }

    /// A sine passes through untouched: there's no step to smooth, so there's
    /// no lag either.
    #[test]
    fn a_smooth_shape_is_untouched() {
        let decay = decay(RATE);
        let mut lfo = Lfo::seeded(1);
        lfo.start(0.0, LfoShape::Sine);
        let increment = 20.0 / RATE;
        let mut phase = 0.0;
        for _ in 0..RATE as usize {
            phase = wrap(phase + increment);
            let out = lfo.next(phase, LfoShape::Sine, decay);
            let exact = shape_value(LfoShape::Sine, phase, 0.0);
            assert!((out - exact).abs() < 1e-3, "{out} against {exact}");
        }
    }

    /// The sample-and-hold takes a new level at the start of each cycle and
    /// holds it, the same levels from the same seed, and new ones after a
    /// restart only if the seed's sequence says so.
    #[test]
    fn sample_and_hold_takes_a_new_level_each_cycle_from_its_seed() {
        let decay = decay(RATE);
        let levels = |seed| {
            let mut lfo = Lfo::seeded(seed);
            lfo.start(0.0, LfoShape::SampleAndHold);
            let mut phase = 0.0;
            let mut held = Vec::new();
            for n in 0..4 * 4_800 {
                phase = wrap(phase + 10.0 / RATE);
                lfo.next(phase, LfoShape::SampleAndHold, decay);
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
}
