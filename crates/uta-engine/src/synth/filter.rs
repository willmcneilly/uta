//! Andrew Simper's state-variable low-pass filter.
//!
//! A two-pole (12 dB per octave) filter built by trapezoidal integration. It's
//! used widely because it stays stable and smooth while its cutoff moves every
//! sample, which is exactly what a filter sweep does. The equations are the
//! ones in Simper's "Linear Trapezoidal Integrated State Variable Filter"
//! (Cytomic, 2013), low-pass output only.

use std::f64::consts::{PI, SQRT_2};

/// The damping at full resonance. 0 would ring forever; 0.1 is a Q of 10,
/// a strong peak at the cutoff that still dies away.
const MIN_DAMPING: f64 = 0.1;
/// The highest cutoff the filter uses, as a share of the sample rate. The
/// maths needs it below half the rate; at 44.1 kHz and up, 20 kHz is under it.
const MAX_CUTOFF_RATIO: f64 = 0.49;

/// Filter coefficients for one cutoff and resonance. The synth works these
/// out once per sample and every voice's filter shares them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Coefficients {
    a1: f64,
    a2: f64,
    a3: f64,
}

impl Coefficients {
    /// Resonance 0 gives a Butterworth response: flat, then 3 dB down at the
    /// cutoff and falling 12 dB per octave above it.
    pub(crate) fn new(cutoff_hz: f64, resonance: f64, sample_rate: f64) -> Self {
        let cutoff_hz = cutoff_hz.min(sample_rate * MAX_CUTOFF_RATIO);
        let g = (PI * cutoff_hz / sample_rate).tan();
        let k = SQRT_2 + (MIN_DAMPING - SQRT_2) * resonance;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Self { a1, a2, a3 }
    }
}

/// One voice's filter memory.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Filter {
    ic1eq: f64,
    ic2eq: f64,
}

impl Filter {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Filters one sample.
    #[inline]
    pub(crate) fn process(&mut self, input: f32, c: &Coefficients) -> f32 {
        let v3 = f64::from(input) - self.ic2eq;
        let v1 = c.a1 * self.ic1eq + c.a2 * v3;
        let v2 = self.ic2eq + c.a2 * self.ic1eq + c.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        v2 as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The level a steady sine at `frequency_hz` comes out at.
    fn gain_at(frequency_hz: f64, coefficients: &Coefficients) -> f64 {
        let rate = 48_000.0;
        let mut filter = Filter::default();
        let mut peak = 0.0f64;
        for n in 0..48_000 {
            let input = (2.0 * PI * frequency_hz * f64::from(n) / rate).sin() as f32;
            let output = filter.process(input, coefficients);
            if n > 24_000 {
                peak = peak.max(f64::from(output.abs()));
            }
        }
        peak
    }

    #[test]
    fn butterworth_at_zero_resonance() {
        let c = Coefficients::new(1_000.0, 0.0, 48_000.0);
        assert!(
            (gain_at(50.0, &c) - 1.0).abs() < 1e-3,
            "passband isn't flat"
        );
        let at_cutoff = 20.0 * gain_at(1_000.0, &c).log10();
        assert!((at_cutoff + 3.01).abs() < 0.05, "{at_cutoff} dB at cutoff");
    }

    #[test]
    fn resonance_peaks_at_the_cutoff() {
        let c = Coefficients::new(1_000.0, 1.0, 48_000.0);
        let at_cutoff = 20.0 * gain_at(1_000.0, &c).log10();
        assert!((at_cutoff - 20.0).abs() < 0.5, "{at_cutoff} dB at cutoff");
    }

    #[test]
    fn cutoff_is_kept_below_nyquist() {
        let c = Coefficients::new(20_000.0, 0.0, 32_000.0);
        assert!(c.a1.is_finite() && c.a2.is_finite() && c.a3.is_finite());
    }
}
