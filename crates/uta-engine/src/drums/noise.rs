//! The kit's noise source: one per kit, shared by the snare and the clap,
//! as on the 909, where the two "produce a phasing effect when played
//! together", and by the toms' skin. See RFC-006, "What makes it sound good", point 6.
//!
//! It's free-running: it runs whether or not anything is sounding, so each
//! hit catches it at a different point and comes out a little different, as
//! on the hardware. But it's deterministic: it restarts from the same seed
//! when playback starts, so playing from the top always sounds the same as a
//! render.

/// Where the noise starts. Any number but zero.
const SEED: u32 = 0x9E37_79B9;

/// White noise from a 32-bit xorshift generator (George Marsaglia's
/// "Xorshift RNGs", 2003): three shifts and three XORs a sample, with a
/// period of 2^32 - 1 samples, a day at 48 kHz.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Noise {
    state: u32,
}

impl Noise {
    pub(crate) fn new() -> Self {
        Self { state: SEED }
    }

    /// Starts again from the seed.
    pub(crate) fn restart(&mut self) {
        self.state = SEED;
    }

    /// The next sample, evenly spread over -1 to 1.
    #[inline]
    pub(crate) fn next_sample(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        // The top 24 bits, which a float holds exactly, over -1 to 1.
        (x >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_white_and_evenly_spread_over_minus_one_to_one() {
        let mut noise = Noise::new();
        let samples: Vec<f32> = (0..480_000).map(|_| noise.next_sample()).collect();
        assert!(samples.iter().all(|s| (-1.0..1.0).contains(s)));
        let n = samples.len() as f64;
        let mean = samples.iter().map(|&s| f64::from(s)).sum::<f64>() / n;
        let variance = samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / n;
        // Even over -1 to 1: mean 0, variance 1/3.
        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((variance - 1.0 / 3.0).abs() < 0.01, "variance {variance}");
        // White: next to no correlation from one sample to the next.
        let lag: f64 = samples
            .windows(2)
            .map(|pair| f64::from(pair[0]) * f64::from(pair[1]))
            .sum::<f64>()
            / n
            / variance;
        assert!(lag.abs() < 0.01, "lag-1 correlation {lag}");
    }

    #[test]
    fn restarting_starts_it_again_from_the_same_point() {
        let mut noise = Noise::new();
        let first: Vec<f32> = (0..100).map(|_| noise.next_sample()).collect();
        noise.restart();
        let again: Vec<f32> = (0..100).map(|_| noise.next_sample()).collect();
        assert_eq!(first, again);
    }
}
