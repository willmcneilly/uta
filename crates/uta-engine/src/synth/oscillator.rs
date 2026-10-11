//! A band-limited oscillator using PolyBLEP.
//!
//! A naive saw or square jumps once or twice a cycle, and those jumps carry
//! harmonics far above half the sample rate, which fold back down as
//! "aliasing": tones that aren't in tune with the note. PolyBLEP smooths each
//! jump over the two samples around it with a small polynomial, which removes
//! most of the aliasing cheaply. The triangle has corners rather than jumps,
//! and gets the integrated version (PolyBLAMP) at each corner.
//!
//! The sub-oscillator is a square that flips each time oscillator 1 wraps
//! (every second wrap for two octaves down), like the Juno's divider. Its
//! jumps fall where oscillator 1 wraps, so it gets its PolyBLEP correction
//! from oscillator 1's own phase. See RFC-008, "In the engine".
//!
//! References: Välimäki and Huovilainen, "Antialiasing oscillators in
//! subtractive synthesis" (2007), and Esqueda, Välimäki and Bilbao,
//! "Rounding corners with BLAMP" (DAFx 2016).

use std::f64::consts::TAU;

use super::Waveform;

/// An oscillator's phase and speed. Its output for any waveform is worked out
/// from the same phase, so the voice can crossfade between two waveforms.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Oscillator {
    /// Where it is in the cycle, 0..1.
    phase: f64,
    /// How far it moves per sample, in cycles.
    increment: f64,
}

impl Oscillator {
    /// Starts at `phase` in the cycle, from 0 to 1: 0 is the start.
    pub(crate) fn start_at(&mut self, phase: f64) {
        self.phase = phase;
    }

    /// Sets how far it moves each sample, in cycles: its frequency over the
    /// sample rate.
    #[inline]
    pub(crate) fn set_increment(&mut self, increment: f64) {
        self.increment = increment;
    }

    /// The output for this sample, from -1 to 1.
    #[inline]
    pub(crate) fn value(&self, waveform: Waveform) -> f32 {
        let t = self.phase;
        let dt = self.increment;
        let value = match waveform {
            Waveform::Sine => (t * TAU).sin(),
            Waveform::Saw => 2.0 * t - 1.0 - poly_blep(t, dt),
            Waveform::Pulse => square(t, dt),
            Waveform::Triangle => {
                // Rises from -1 to 1 over the first half, falls back over the
                // second. Its slope changes by 8 per cycle at each corner.
                let naive = 1.0 - 4.0 * (t - 0.5).abs();
                naive + 8.0 * dt * (poly_blamp(t, dt) - poly_blamp((t + 0.5).fract(), dt))
            }
        };
        value as f32
    }

    /// The sub-oscillator under this one: a square `divide` times lower (2
    /// or 4), from -1 to 1. `cycle` counts this oscillator's wraps; the sub
    /// is the same at `cycle` and `cycle + divide`.
    #[inline]
    pub(crate) fn sub_value(&self, cycle: u32, divide: u32) -> f32 {
        let divide = f64::from(divide);
        let t = (f64::from(cycle) % divide + self.phase) / divide;
        square(t, self.increment / divide) as f32
    }

    /// Moves on by one sample, and says whether it wrapped round to the
    /// start of a cycle.
    #[inline]
    pub(crate) fn advance(&mut self) -> bool {
        let next = self.phase + self.increment;
        self.phase = next.fract();
        next >= 1.0
    }
}

/// A square at phase `t` with `dt` cycles per sample: up for the first half
/// of the cycle, down for the second, with a PolyBLEP correction at each
/// jump.
#[inline]
fn square(t: f64, dt: f64) -> f64 {
    let naive = if t < 0.5 { 1.0 } else { -1.0 };
    naive + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt)
}

/// The PolyBLEP correction for an upward jump of 2 at phase 0, for a sample
/// at phase `t` with `dt` cycles per sample. Zero except within a sample
/// either side of the jump.
#[inline]
pub(crate) fn poly_blep(t: f64, dt: f64) -> f64 {
    let dt = dt.min(0.5);
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// The PolyBLAMP correction for a corner at phase 0 whose slope rises by one
/// per sample: the integral of the PolyBLEP correction, in samples.
#[inline]
fn poly_blamp(t: f64, dt: f64) -> f64 {
    let dt = dt.min(0.5);
    if t < dt {
        let x = 1.0 - t / dt;
        x * x * x / 6.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt + 1.0;
        x * x * x / 6.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle(waveform: Waveform, frequency_hz: f64) -> Vec<f32> {
        let mut oscillator = Oscillator::default();
        oscillator.set_increment(frequency_hz / 48_000.0);
        let samples = (48_000.0 / frequency_hz).round() as usize;
        (0..samples)
            .map(|_| {
                let value = oscillator.value(waveform);
                oscillator.advance();
                value
            })
            .collect()
    }

    #[test]
    fn every_waveform_stays_near_full_scale() {
        for waveform in [
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Saw,
            Waveform::Pulse,
        ] {
            let peak = cycle(waveform, 100.0)
                .iter()
                .fold(0.0f32, |p, s| p.max(s.abs()));
            assert!((0.97..=1.0).contains(&peak), "{waveform:?} peaks at {peak}");
        }
    }

    #[test]
    fn waveforms_have_no_offset() {
        for waveform in [
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Saw,
            Waveform::Pulse,
        ] {
            let samples = cycle(waveform, 100.0);
            let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(mean.abs() < 1e-3, "{waveform:?} has an offset of {mean}");
        }
    }

    /// The sub flips only where the oscillator wraps: every wrap for an
    /// octave down, every second for two, starting high.
    #[test]
    fn the_sub_flips_where_the_oscillator_wraps() {
        for divide in [2, 4] {
            let mut oscillator = Oscillator::default();
            // A power of two, so the phase adds up exactly and it wraps
            // every 128 samples.
            oscillator.set_increment(1.0 / 128.0);
            let mut cycle = 0;
            let mut high = true;
            let mut flips = Vec::new();
            for sample in 0..1024 {
                let value = oscillator.sub_value(cycle, divide);
                // Exactly on a jump the correction makes it zero, half way.
                if value != 0.0 && (value > 0.0) != high {
                    high = value > 0.0;
                    flips.push(sample);
                }
                if oscillator.advance() {
                    cycle += 1;
                }
            }
            // A sample after each jump, which is at a wrap.
            let half = 128 * divide as usize / 2;
            let expected: Vec<usize> = (1..1024 / half).map(|n| n * half + 1).collect();
            assert_eq!(flips, expected, "dividing by {divide}");
        }
    }

    #[test]
    fn the_sub_is_corrected_at_its_jumps_and_has_no_offset() {
        let mut oscillator = Oscillator::default();
        let increment = 440.0 / 48_000.0;
        oscillator.set_increment(increment);
        let mut cycle = 0;
        let samples: Vec<f32> = (0..48_000)
            .map(|_| {
                let value = oscillator.sub_value(cycle, 2);
                if oscillator.advance() {
                    cycle += 1;
                }
                value
            })
            .collect();
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 1e-3, "an offset of {mean}");
        // Each jump is spread over a sample either side, so no step is the
        // whole jump of 2.
        let step = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(step < 1.9, "a step of {step}");
    }

    #[test]
    fn advance_says_when_it_wraps() {
        let mut oscillator = Oscillator::default();
        oscillator.set_increment(0.25);
        let wraps: Vec<bool> = (0..8).map(|_| oscillator.advance()).collect();
        assert_eq!(
            wraps,
            [false, false, false, true, false, false, false, true]
        );
        oscillator.start_at(0.9);
        assert!(oscillator.advance());
    }

    #[test]
    fn corrections_are_continuous_at_the_jump() {
        let dt = 0.1;
        // Either side of the jump the saw's correction meets in the middle.
        assert!((poly_blep(0.0, dt) + 1.0).abs() < 1e-12);
        assert!((poly_blep(1.0 - 1e-12, dt) - 1.0).abs() < 1e-9);
        // Both halves of the corner's correction meet at 1/6.
        assert!((poly_blamp(0.0, dt) - 1.0 / 6.0).abs() < 1e-12);
        assert!((poly_blamp(1.0 - 1e-12, dt) - 1.0 / 6.0).abs() < 1e-9);
        assert_eq!(poly_blep(0.5, dt), 0.0);
        assert_eq!(poly_blamp(0.5, dt), 0.0);
    }
}
