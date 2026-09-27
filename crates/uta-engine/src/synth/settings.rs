//! The synth's settings, as the engine plays them.
//!
//! The project core keeps its own copy of these for `SetSynthParam` to check.
//! Both follow the "Synth settings" table in `docs/plans/make-a-loop.md`: the
//! same names, units, ranges and defaults, so the engine's snapshot can be
//! filled from the project field by field.

/// The oscillator's waveform.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
    #[default]
    Saw,
    Square,
}

/// How the synth sounds. It travels in the [`crate::Snapshot`], and changes
/// glide rather than jump (see [`crate::SYNTH_SMOOTHING_SECONDS`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthSettings {
    pub waveform: Waveform,
    /// The low-pass filter's cutoff in Hz, [`Self::CUTOFF_HZ`]. It glides on
    /// a log scale.
    pub cutoff_hz: f32,
    /// The filter's resonance, [`Self::RESONANCE`]: 0 is a plain
    /// (Butterworth) low-pass, 1 rings strongly at the cutoff.
    pub resonance: f32,
    /// Seconds to rise from silence to full level, [`Self::TIME_SECONDS`].
    pub attack_seconds: f32,
    /// Seconds to fall from full level to the sustain level,
    /// [`Self::TIME_SECONDS`].
    pub decay_seconds: f32,
    /// The level held while the note is down, as a linear level,
    /// [`Self::SUSTAIN`].
    pub sustain: f32,
    /// Seconds to fall to silence once the note is released,
    /// [`Self::TIME_SECONDS`].
    pub release_seconds: f32,
}

impl SynthSettings {
    pub const CUTOFF_HZ: std::ops::RangeInclusive<f32> = 20.0..=20_000.0;
    pub const RESONANCE: std::ops::RangeInclusive<f32> = 0.0..=1.0;
    pub const TIME_SECONDS: std::ops::RangeInclusive<f32> = 0.001..=10.0;
    pub const SUSTAIN: std::ops::RangeInclusive<f32> = 0.0..=1.0;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default. The synth plays what this returns, so a
    /// bad value can never reach the audio.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        let clamp = |value: f32, range: std::ops::RangeInclusive<f32>, fallback: f32| {
            if value.is_nan() {
                fallback
            } else {
                value.clamp(*range.start(), *range.end())
            }
        };
        Self {
            waveform: self.waveform,
            cutoff_hz: clamp(self.cutoff_hz, Self::CUTOFF_HZ, default.cutoff_hz),
            resonance: clamp(self.resonance, Self::RESONANCE, default.resonance),
            attack_seconds: clamp(
                self.attack_seconds,
                Self::TIME_SECONDS,
                default.attack_seconds,
            ),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::TIME_SECONDS,
                default.decay_seconds,
            ),
            sustain: clamp(self.sustain, Self::SUSTAIN, default.sustain),
            release_seconds: clamp(
                self.release_seconds,
                Self::TIME_SECONDS,
                default.release_seconds,
            ),
        }
    }
}

impl Default for SynthSettings {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            cutoff_hz: 20_000.0,
            resonance: 0.0,
            attack_seconds: 0.005,
            decay_seconds: 0.2,
            sustain: 0.7,
            release_seconds: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_plan() {
        let settings = SynthSettings::default();
        assert_eq!(settings.waveform, Waveform::Saw);
        assert_eq!(settings.cutoff_hz, 20_000.0);
        assert_eq!(settings.resonance, 0.0);
        assert_eq!(settings.attack_seconds, 0.005);
        assert_eq!(settings.decay_seconds, 0.2);
        assert_eq!(settings.sustain, 0.7);
        assert_eq!(settings.release_seconds, 0.2);
        assert_eq!(settings.clamped(), settings);
    }

    #[test]
    fn clamped_keeps_values_in_range() {
        let wild = SynthSettings {
            waveform: Waveform::Square,
            cutoff_hz: 1.0e9,
            resonance: -3.0,
            attack_seconds: 0.0,
            decay_seconds: f32::INFINITY,
            sustain: f32::NAN,
            release_seconds: -1.0,
        }
        .clamped();
        assert_eq!(wild.waveform, Waveform::Square);
        assert_eq!(wild.cutoff_hz, 20_000.0);
        assert_eq!(wild.resonance, 0.0);
        assert_eq!(wild.attack_seconds, 0.001);
        assert_eq!(wild.decay_seconds, 10.0);
        assert_eq!(wild.sustain, 0.7);
        assert_eq!(wild.release_seconds, 0.001);
    }
}
