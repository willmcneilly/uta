//! Measurements for the sound tests, taken from the rendered waveform.

#![allow(dead_code)] // Each test binary uses a different subset.

/// The frequency of a steady tone, from the time between its first and last
/// upward zero crossings (interpolated between samples).
pub fn measure_frequency(samples: &[f32], sample_rate: u32) -> f64 {
    let mut first = None;
    let mut last = 0.0;
    let mut crossings = 0usize;
    for (i, pair) in samples.windows(2).enumerate() {
        let (a, b) = (f64::from(pair[0]), f64::from(pair[1]));
        if a < 0.0 && b >= 0.0 {
            let at = i as f64 + a / (a - b);
            first.get_or_insert(at);
            last = at;
            crossings += 1;
        }
    }
    let first = first.expect("no zero crossings: is the tone playing?");
    assert!(crossings >= 2, "need at least two cycles to measure pitch");
    (crossings - 1) as f64 * f64::from(sample_rate) / (last - first)
}

pub fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|&s| f64::from(s).powi(2)).sum();
    (sum / samples.len() as f64).sqrt()
}

pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
}

/// The biggest sample-to-sample jump, and where it happens.
pub fn max_jump(samples: &[f32]) -> (f32, usize) {
    samples
        .windows(2)
        .enumerate()
        .map(|(i, pair)| ((pair[1] - pair[0]).abs(), i + 1))
        .fold(
            (0.0, 0),
            |max, jump| if jump.0 > max.0 { jump } else { max },
        )
}

/// The steepest a full-scale sine at `frequency_hz` ever moves in one sample.
/// Any jump well above this is a click, not the tone.
pub fn sine_max_step(frequency_hz: f64, sample_rate: u32) -> f32 {
    (2.0 * (std::f64::consts::PI * frequency_hz / f64::from(sample_rate)).sin()) as f32
}

/// The largest difference between two renders, which must be the same length.
pub fn max_difference(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "renders differ in length");
    a.iter()
        .zip(b)
        .fold(0.0f32, |max, (x, y)| max.max((x - y).abs()))
}
