//! A band-limited oscillator using PolyBLEP.
//!
//! A naive saw or square jumps once or twice a cycle, and those jumps carry
//! harmonics far above half the sample rate, which fold back down as
//! "aliasing": tones that aren't in tune with the note. PolyBLEP smooths each
//! jump over the two samples around it with a small polynomial, which removes
//! most of the aliasing cheaply. The triangle has corners rather than jumps,
//! and gets the integrated version (PolyBLAMP) at each corner.
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
    /// Restarts at the start of a cycle at `frequency_hz`.
    pub(crate) fn start(&mut self, frequency_hz: f64, sample_rate: f64) {
        self.phase = 0.0;
        self.increment = frequency_hz / sample_rate;
    }

    /// The output for this sample, from -1 to 1.
    #[inline]
    pub(crate) fn value(&self, waveform: Waveform) -> f32 {
        let t = self.phase;
        let dt = self.increment;
        let value = match waveform {
            Waveform::Sine => (t * TAU).sin(),
            Waveform::Saw => 2.0 * t - 1.0 - poly_blep(t, dt),
            Waveform::Square => {
                let naive = if t < 0.5 { 1.0 } else { -1.0 };
                naive + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt)
            }
            Waveform::Triangle => {
                // Rises from -1 to 1 over the first half, falls back over the
                // second. Its slope changes by 8 per cycle at each corner.
                let naive = 1.0 - 4.0 * (t - 0.5).abs();
                naive + 8.0 * dt * (poly_blamp(t, dt) - poly_blamp((t + 0.5).fract(), dt))
            }
        };
        value as f32
    }

    /// Moves on by one sample.
    #[inline]
    pub(crate) fn advance(&mut self) {
        self.phase = (self.phase + self.increment).fract();
    }
}

/// The PolyBLEP correction for an upward jump of 2 at phase 0, for a sample
/// at phase `t` with `dt` cycles per sample. Zero except within a sample
/// either side of the jump.
#[inline]
fn poly_blep(t: f64, dt: f64) -> f64 {
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
        oscillator.start(frequency_hz, 48_000.0);
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
            Waveform::Square,
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
            Waveform::Square,
        ] {
            let samples = cycle(waveform, 100.0);
            let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(mean.abs() < 1e-3, "{waveform:?} has an offset of {mean}");
        }
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
