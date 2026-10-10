//! The 808's metal: six square waves at odd, unrelated frequencies, whose
//! sum is the clangy cluster the hats (and, later, the cymbal) filter down
//! to their top end. See RFC-006, "How each sound is made" (Hats).
//!
//! From Werner, Abel and Smith's analysis of the 808 cymbal (ICMC/SMC 2014,
//! section 3): six Schmitt-trigger oscillators on one chip, shared by the
//! cymbal and both hats, each a rectangle with a 47.98% duty, at 205.3,
//! 304.4, 369.6, 522.7, 540 and 800 Hz. Plaits' hi-hat uses other
//! frequencies, so only its idea is used here, not its numbers.
//!
//! Where it differs from the 808 and Plaits, and why:
//! - **Each square is band-limited with PolyBLEP,** as the synth's are. Both
//!   make naive squares, whose harmonics above half the sample rate fold
//!   back as false tones (aliasing), and the hats keep exactly the top end
//!   where they land. See RFC-006, "What makes it sound good", point 4.
//! - **Tune,** which the 808 doesn't have, moves all six together, keeping
//!   their ratios.
//!
//! It's free-running: it runs whether or not anything is sounding, so each
//! hit catches it at a different point and comes out a little different, as
//! on the 808, where the oscillators are never reset by a trigger. It
//! restarts from the same point when playback starts, so playing from the
//! top sounds the same as a render. If the hats are ringing then, it
//! crossfades from where it was to the restarted metal over a few
//! milliseconds, so their ring doesn't jump.

use crate::ramp::Ramp;
use crate::synth::oscillator::poly_blep;

/// The 808's six oscillators, in Hz (Werner, section 3).
pub(crate) const FREQUENCIES: [f64; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

/// The share of each cycle a square spends high (Werner, section 3).
const DUTY: f64 = 0.4798;

/// Where each oscillator starts, as a share of its cycle: spread out, so they
/// don't all rise on the same sample when playback starts.
const START_PHASES: [f64; 6] = [0.0, 0.17, 0.41, 0.59, 0.73, 0.89];

/// How long a restart while the hats ring crossfades for.
const CROSSFADE_SECONDS: f64 = 0.005;

/// The six oscillators. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Metal {
    /// Where each is in its cycle, 0..1.
    phases: [f64; 6],
    /// Where each was before a restart while the hats rang, still running
    /// while the crossfade away from it lasts.
    old_phases: [f64; 6],
    /// Steps left in the crossfade, and its length.
    crossfade: u32,
    crossfade_steps: u32,
    /// How far each moves a step, in cycles.
    increments: [f64; 6],
    /// Tune, in octaves above the 808's, as it glides.
    tune_octaves: Ramp,
    /// The value of `tune_octaves` the increments were worked out for.
    tuned_to: f32,
    /// The steps it takes a second: the kit's sample rate times its
    /// oversampling.
    rate: f64,
}

/// Tune in octaves above the 808's lowest oscillator.
fn octaves(tune_hz: f32) -> f32 {
    (f64::from(tune_hz) / FREQUENCIES[0]).log2() as f32
}

impl Metal {
    /// Six oscillators tuned so the lowest is at `tune_hz`, taking `rate`
    /// steps a second, whose Tune glides over `smoothing` steps.
    pub(crate) fn new(tune_hz: f32, rate: f64, smoothing: u32) -> Self {
        Self {
            phases: START_PHASES,
            old_phases: START_PHASES,
            crossfade: 0,
            crossfade_steps: (CROSSFADE_SECONDS * rate).round().max(1.0) as u32,
            increments: [0.0; 6],
            tune_octaves: Ramp::new(octaves(tune_hz), smoothing),
            tuned_to: f32::NAN,
            rate,
        }
    }

    /// Starts again from the same point. If something is listening to it
    /// (`ringing`), it crossfades there from where it was, so the sound
    /// doesn't jump.
    pub(crate) fn restart(&mut self, ringing: bool) {
        if ringing {
            // A restart during a crossfade fades from whichever metal is
            // louder in it now, so the sound jumps by the quieter's share at
            // most.
            if self.crossfade <= self.crossfade_steps / 2 {
                self.old_phases = self.phases;
            }
            self.crossfade = self.crossfade_steps;
        } else {
            self.crossfade = 0;
        }
        self.phases = START_PHASES;
    }

    /// Takes on a new Tune straight away.
    pub(crate) fn load_tune(&mut self, tune_hz: f32) {
        self.tune_octaves.jump_to(octaves(tune_hz));
    }

    /// Glides to a new Tune.
    pub(crate) fn set_tune(&mut self, tune_hz: f32) {
        self.tune_octaves.set_target(octaves(tune_hz));
    }

    /// Moves Tune on a step, and the increments with it if it moved.
    #[inline]
    fn tune(&mut self) {
        let tune = self.tune_octaves.next_value();
        if tune != self.tuned_to {
            self.tuned_to = tune;
            let ratio = f64::from(tune).exp2();
            for (increment, frequency) in self.increments.iter_mut().zip(FREQUENCIES) {
                *increment = frequency * ratio / self.rate;
            }
        }
    }

    /// The next step: the six squares added together, from -1 to 1.
    #[inline]
    pub(crate) fn next_sample(&mut self) -> f64 {
        self.tune();
        let new = squares(&mut self.phases, &self.increments);
        if self.crossfade == 0 {
            return new;
        }
        let old = squares(&mut self.old_phases, &self.increments);
        let old_share = f64::from(self.crossfade) / f64::from(self.crossfade_steps + 1);
        self.crossfade -= 1;
        old * old_share + new * (1.0 - old_share)
    }

    /// Whether it's crossfading from where it was before a restart.
    #[cfg(test)]
    pub(crate) fn is_crossfading(&self) -> bool {
        self.crossfade > 0
    }

    /// Moves on a step without working out the sound, for when nothing is
    /// listening: it keeps running free.
    #[inline]
    pub(crate) fn skip(&mut self) {
        self.tune();
        advance(&mut self.phases, &self.increments);
        if self.crossfade > 0 {
            advance(&mut self.old_phases, &self.increments);
            self.crossfade -= 1;
        }
    }
}

/// Six squares at `phases`, added together, from -1 to 1, then moved on a
/// step.
#[inline]
fn squares(phases: &mut [f64; 6], increments: &[f64; 6]) -> f64 {
    let mut sum = 0.0;
    for (phase, &dt) in phases.iter_mut().zip(increments) {
        let t = *phase;
        // High for the first 47.98% of the cycle, low for the rest: a jump
        // up at the start of the cycle and down at the duty.
        let naive = if t < DUTY { 1.0 } else { -1.0 };
        sum += naive + poly_blep(t, dt) - poly_blep((t + 1.0 - DUTY).fract(), dt);
        *phase = (t + dt).fract();
    }
    sum / 6.0
}

/// Moves `phases` on a step.
#[inline]
fn advance(phases: &mut [f64; 6], increments: &[f64; 6]) {
    for (phase, &dt) in phases.iter_mut().zip(increments) {
        *phase = (*phase + dt).fract();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    /// Each oscillator rises once a cycle: counted over 10 s, the lowest
    /// rises 2053 times at the 808's tuning and twice that an octave up, and
    /// the others in their ratios. The 808's frequencies are written out
    /// here (Werner, section 3), so a wrong one in the code fails.
    #[test]
    fn the_oscillators_are_at_the_808s_frequencies_and_tune_moves_them() {
        const WERNER: [f64; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
        for (tune_hz, ratio) in [(205.3, 1.0), (410.6, 2.0)] {
            let mut metal = Metal::new(tune_hz, RATE, 1);
            let mut rises = [0u32; 6];
            for _ in 0..(10.0 * RATE) as usize {
                let before = metal.phases;
                metal.skip();
                for (i, rise) in rises.iter_mut().enumerate() {
                    if metal.phases[i] < before[i] {
                        *rise += 1;
                    }
                }
            }
            for (rises, frequency) in rises.iter().zip(WERNER) {
                let expected = 10.0 * frequency * ratio;
                assert!(
                    (f64::from(*rises) - expected).abs() <= 1.0,
                    "{frequency} Hz at {tune_hz}: {rises}"
                );
            }
        }
    }

    /// Each square spends 47.98% of its cycle high (Werner, section 3): the
    /// sum's average over a long time is what six such squares average,
    /// 2 × 0.4798 - 1, about -0.040. A square at 50% would average 0.
    #[test]
    fn the_squares_have_the_808s_duty() {
        let mut metal = Metal::new(205.3, RATE, 1);
        let n = (20.0 * RATE) as usize;
        let mean = (0..n).map(|_| metal.next_sample()).sum::<f64>() / n as f64;
        assert!((mean - (2.0 * 0.4798 - 1.0)).abs() < 1e-3, "mean {mean}");
    }

    /// Tune glides rather than jumping: a tenth of the way through its
    /// glide, the oscillators have moved a tenth of the way, in octaves.
    #[test]
    fn tune_glides() {
        let mut metal = Metal::new(205.3, RATE, 960);
        metal.skip();
        metal.set_tune(410.6);
        for _ in 0..96 {
            metal.skip();
        }
        let octaves = (metal.increments[0] * RATE / FREQUENCIES[0]).log2();
        assert!((octaves - 0.1).abs() < 0.002, "{octaves}");
        for _ in 0..960 {
            metal.skip();
        }
        let octaves = (metal.increments[0] * RATE / FREQUENCIES[0]).log2();
        assert!((octaves - 1.0).abs() < 1e-6, "{octaves}");
    }

    /// Skipping steps moves it on exactly as working them out does, so a
    /// hit catches it where it would have been.
    #[test]
    fn skipping_keeps_it_running() {
        let (mut heard, mut skipped) = (Metal::new(300.0, RATE, 1), Metal::new(300.0, RATE, 1));
        for _ in 0..12_345 {
            heard.next_sample();
            skipped.skip();
        }
        assert_eq!(heard.next_sample(), skipped.next_sample());
        heard.restart(false);
        let first = heard.next_sample();
        heard.restart(false);
        assert_eq!(heard.next_sample(), first);
    }

    /// A restart while the hats ring crossfades over 5 ms from where the
    /// metal was to where it restarts, rather than jumping, and after it
    /// the metal is exactly where a restart while quiet would put it.
    #[test]
    fn a_restart_while_ringing_crossfades() {
        let mut ringing = Metal::new(205.3, RATE, 1);
        for _ in 0..1000 {
            ringing.next_sample();
        }
        let (mut old, mut quiet) = (ringing, Metal::new(205.3, RATE, 1));
        ringing.restart(true);
        quiet.next_sample();
        quiet.restart(false);
        let steps = (0.005 * RATE) as u32;
        for step in 1..=steps {
            let (from, to) = (old.next_sample(), quiet.next_sample());
            let share = f64::from(steps + 1 - step) / f64::from(steps + 1);
            let mixed = ringing.next_sample();
            assert!((mixed - (from * share + to * (1.0 - share))).abs() < 1e-12);
        }
        assert_eq!(ringing.next_sample(), quiet.next_sample());
    }
}
