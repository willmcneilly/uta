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

    /// The gain in dB of a steady sine at a whole number of Hz, measured
    /// over a second once the filter has settled. Correlating with a sine
    /// and a cosine over whole cycles gives the exact amplitude, where the
    /// peak sample would miss it at high frequencies.
    fn measured_db(frequency_hz: u32, coefficients: &Coefficients, rate: u32) -> f64 {
        let mut filter = Filter::default();
        let (mut in_phase, mut quadrature) = (0.0f64, 0.0f64);
        for n in 0..2 * rate {
            let phase = 2.0 * PI * f64::from(frequency_hz) * f64::from(n) / f64::from(rate);
            let output = f64::from(filter.process(phase.sin() as f32, coefficients));
            if n >= rate {
                in_phase += output * phase.sin();
                quadrature += output * phase.cos();
            }
        }
        let amplitude = 2.0 * in_phase.hypot(quadrature) / f64::from(rate);
        20.0 * amplitude.log10()
    }

    /// The synth panel draws the filter's response from the same formula as
    /// this filter, worked out in the UI (`app/src/synth/filterCurve.ts`).
    /// Measuring the real filter is far too slow for a drawing that follows
    /// a slider, and asking Rust would be a round trip on every step, so the
    /// UI keeps its own copy of the maths. The risk is that the copy drifts
    /// from the sound, as it did for HISE's filter display (research:
    /// https://app.notion.com/p/3f23af969b6f81f9a4d9f08a1bc97317).
    ///
    /// So this test drives the real filter with sine waves and keeps the
    /// gains it measures in `app/src/synth/filter-response.json` (beside
    /// the curve, where the UI's tests can read it), and a TypeScript test
    /// checks the drawn curve against that file within
    /// 0.5 dB. If the filter changes, this test fails until the file is
    /// regenerated with `UTA_GOLDEN=1 cargo test -p uta-engine filter`, and
    /// then the TypeScript test fails until the drawing matches. A human
    /// approves every change to the file, as with the golden WAVs.
    #[test]
    fn response_matches_the_fixture() {
        let mut points = Vec::new();
        for rate in [44_100, 48_000] {
            for cutoff_hz in [100.0, 1_000.0, 5_000.0, 20_000.0] {
                for resonance in [0.0, 0.5, 1.0] {
                    let c = Coefficients::new(cutoff_hz, resonance, f64::from(rate));
                    for frequency_hz in [
                        20, 50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000, 15_000, 20_000,
                    ] {
                        let gain_db = measured_db(frequency_hz, &c, rate);
                        points.push(serde_json::json!({
                            "sampleRate": rate,
                            "cutoffHz": cutoff_hz,
                            "resonance": resonance,
                            "frequencyHz": frequency_hz,
                            "gainDb": (gain_db * 1e4).round() / 1e4,
                        }));
                    }
                }
            }
        }

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../app/src/synth/filter-response.json");
        if std::env::var_os("UTA_GOLDEN").is_some() {
            let fixture = serde_json::json!({
                "about": "The engine's low-pass filter, measured with sine waves by \
                          response_matches_the_fixture in crates/uta-engine/src/synth/filter.rs. \
                          Regenerate with UTA_GOLDEN=1; a human approves every change.",
                "points": points,
            });
            let text = serde_json::to_string_pretty(&fixture).unwrap() + "\n";
            std::fs::write(&path, text).unwrap();
            eprintln!("Wrote {}", path.display());
            return;
        }

        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "can't read {} ({e}); regenerate with UTA_GOLDEN=1",
                path.display()
            )
        });
        let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
        let saved = fixture["points"].as_array().unwrap();
        assert_eq!(saved.len(), points.len(), "regenerate with UTA_GOLDEN=1");
        for (saved, measured) in saved.iter().zip(&points) {
            for key in ["sampleRate", "cutoffHz", "resonance", "frequencyHz"] {
                assert_eq!(saved[key], measured[key], "regenerate with UTA_GOLDEN=1");
            }
            let (was, now) = (
                saved["gainDb"].as_f64().unwrap(),
                measured["gainDb"].as_f64().unwrap(),
            );
            assert!(
                (was - now).abs() < 0.01,
                "{measured}: the filter now measures {now} dB, not {was} dB; \
                 regenerate with UTA_GOLDEN=1 and check the drawing still matches"
            );
        }
    }
}
