//! Halves a sample rate: the way back down from the hats' 2× oversampling.
//! See RFC-006, "What makes it sound good", point 4.
//!
//! Before every other sample is dropped, everything above the lower rate's
//! highest frequency has to go, or it folds back as false tones, which is
//! what oversampling is there to avoid. This is a windowed-sinc low-pass at
//! a quarter of the higher rate (a "half-band" filter): 63 taps under a
//! Blackman window, flat to within 0.01 dB up to 19 kHz at 48 kHz and more
//! than 70 dB down from 29 kHz. Every other tap of a half-band filter is
//! zero, so it costs 16 multiplies a sample out, plus the middle one.

use std::f64::consts::PI;

/// Taps either side of the middle.
const HALF: usize = 31;
const TAPS: usize = 2 * HALF + 1;

/// A half-band low-pass that takes two samples in and gives one out. Plain
/// numbers only.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HalfBand {
    /// The taps at odd distances from the middle, 1, 3, …, 31: the filter
    /// is symmetric, and the taps at even distances are zero but the
    /// middle's, which is 1/2.
    odd: [f64; HALF.div_ceil(2)],
    /// The last `TAPS` samples in, twice over, so the window ending at any
    /// point is one run.
    history: [f64; 2 * TAPS],
    /// Where the next sample goes.
    position: usize,
}

impl HalfBand {
    pub(crate) fn new() -> Self {
        // A sinc cut off at a quarter of the rate, under a Blackman window,
        // scaled so it passes a steady level unchanged.
        let tap = |m: usize| {
            let n = (HALF + m) as f64;
            let x = 2.0 * PI * n / (TAPS - 1) as f64;
            let window = 0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos();
            let sinc = if m == 0 {
                0.5
            } else {
                (PI * m as f64 / 2.0).sin() / (PI * m as f64)
            };
            sinc * window
        };
        let mut odd = [0.0; HALF.div_ceil(2)];
        for (i, value) in odd.iter_mut().enumerate() {
            *value = tap(2 * i + 1);
        }
        let sum = tap(0) + 2.0 * odd.iter().sum::<f64>();
        let middle = tap(0) / sum;
        for value in &mut odd {
            *value /= sum;
        }
        debug_assert!((middle - 0.5).abs() < 1e-3);
        Self {
            odd,
            history: [0.0; 2 * TAPS],
            position: 0,
        }
    }

    /// Takes two samples at the higher rate and gives the one at the lower.
    #[inline]
    pub(crate) fn process(&mut self, pair: [f64; 2]) -> f64 {
        for sample in pair {
            self.history[self.position] = sample;
            self.history[self.position + TAPS] = sample;
            self.position = (self.position + 1) % TAPS;
        }
        // The last `TAPS` samples, oldest first.
        let window = &self.history[self.position..self.position + TAPS];
        let mut out = 0.5 * window[HALF];
        for (i, &tap) in self.odd.iter().enumerate() {
            let m = 2 * i + 1;
            out += tap * (window[HALF - m] + window[HALF + m]);
        }
        out
    }

    /// Forgets what came in, for a sound that has died away.
    pub(crate) fn clear(&mut self) {
        self.history = [0.0; 2 * TAPS];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 96_000.0;

    /// The level out of a steady sine at `hz`, at the higher rate.
    fn gain(hz: f64) -> f64 {
        let mut filter = HalfBand::new();
        let mut peak: f64 = 0.0;
        for n in 0..20_000 {
            let at = |k: usize| (2.0 * PI * hz * k as f64 / RATE).sin();
            let out = filter.process([at(2 * n), at(2 * n + 1)]);
            if n > 1000 {
                peak = peak.max(out.abs());
            }
        }
        peak
    }

    #[test]
    fn it_passes_the_audible_band_and_stops_what_would_fold_back() {
        for hz in [100.0, 1000.0, 10_000.0, 19_000.0] {
            let db = 20.0 * gain(hz).log10();
            assert!(db.abs() < 0.01, "{hz} Hz: {db:.3} dB");
        }
        for hz in [29_000.0, 33_000.0, 40_000.0, 47_000.0] {
            let db = 20.0 * gain(hz).log10();
            assert!(db < -70.0, "{hz} Hz: {db:.1} dB");
        }
    }
}
