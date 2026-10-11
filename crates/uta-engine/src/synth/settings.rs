//! The synth's settings, as the engine plays them.
//!
//! The project core keeps its own copy of these for `SetSynthParam` to check.
//! Both follow the "Synth settings" table in `docs/plans/make-a-loop.md` and
//! RFC-008's "The instrument": the same names, units, ranges and defaults, so
//! the engine's snapshot can be filled from the project field by field.

use std::ops::RangeInclusive;

/// An oscillator's waveform.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
    #[default]
    Saw,
    /// A square, until it has a Width.
    Pulse,
}

/// How the synth sounds. It travels in the [`crate::Snapshot`], and changes
/// glide rather than jump (see [`crate::SYNTH_SMOOTHING_SECONDS`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthSettings {
    pub osc1_wave: Waveform,
    /// Oscillator 1's octave from the note, [`Self::OCTAVE`].
    pub osc1_octave: i8,
    pub osc2_wave: Waveform,
    /// Oscillator 2's octave from the note, [`Self::OCTAVE`].
    pub osc2_octave: i8,
    /// Oscillator 2's offset from its octave in semitones,
    /// [`Self::PITCH_SEMITONES`]. It glides.
    pub osc2_pitch_semitones: f32,
    /// How far under oscillator 1 the sub plays, in octaves,
    /// [`Self::SUB_OCTAVE`].
    pub sub_octave: i8,
    /// How much of each goes into the filter, as linear levels,
    /// [`Self::LEVEL`]. Together they're balanced so that the mix is never
    /// louder than one oscillator at full level, in power (see `mix_gain`).
    pub osc1_level: f32,
    pub osc2_level: f32,
    pub sub_level: f32,
    pub noise_level: f32,
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
    /// Whether the oscillators start at the beginning of their wave on each
    /// note. Off, each note picks them up where they would be had they been
    /// running since Play.
    pub osc_retrigger: bool,
}

impl SynthSettings {
    pub const OCTAVE: RangeInclusive<i8> = -2..=2;
    pub const PITCH_SEMITONES: RangeInclusive<f32> = -12.0..=12.0;
    pub const SUB_OCTAVE: RangeInclusive<i8> = -2..=-1;
    pub const LEVEL: RangeInclusive<f32> = 0.0..=1.0;
    pub const CUTOFF_HZ: RangeInclusive<f32> = 20.0..=20_000.0;
    pub const RESONANCE: RangeInclusive<f32> = 0.0..=1.0;
    pub const TIME_SECONDS: RangeInclusive<f32> = 0.001..=10.0;
    pub const SUSTAIN: RangeInclusive<f32> = 0.0..=1.0;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default. The synth plays what this returns, so a
    /// bad value can never reach the audio.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        let clamp = |value: f32, range: RangeInclusive<f32>, fallback: f32| {
            if value.is_nan() {
                fallback
            } else {
                value.clamp(*range.start(), *range.end())
            }
        };
        let octave =
            |value: i8, range: RangeInclusive<i8>| value.clamp(*range.start(), *range.end());
        Self {
            osc1_wave: self.osc1_wave,
            osc1_octave: octave(self.osc1_octave, Self::OCTAVE),
            osc2_wave: self.osc2_wave,
            osc2_octave: octave(self.osc2_octave, Self::OCTAVE),
            osc2_pitch_semitones: clamp(
                self.osc2_pitch_semitones,
                Self::PITCH_SEMITONES,
                default.osc2_pitch_semitones,
            ),
            sub_octave: octave(self.sub_octave, Self::SUB_OCTAVE),
            osc1_level: clamp(self.osc1_level, Self::LEVEL, default.osc1_level),
            osc2_level: clamp(self.osc2_level, Self::LEVEL, default.osc2_level),
            sub_level: clamp(self.sub_level, Self::LEVEL, default.sub_level),
            noise_level: clamp(self.noise_level, Self::LEVEL, default.noise_level),
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
            osc_retrigger: self.osc_retrigger,
        }
    }
}

impl Default for SynthSettings {
    /// Today's sound: oscillator 1 a saw at full level, the rest of the
    /// mixer at zero, and the oscillators restarting on each note.
    fn default() -> Self {
        Self {
            osc1_wave: Waveform::Saw,
            osc1_octave: 0,
            osc2_wave: Waveform::Saw,
            osc2_octave: 0,
            osc2_pitch_semitones: 0.0,
            sub_octave: -1,
            osc1_level: 1.0,
            osc2_level: 0.0,
            sub_level: 0.0,
            noise_level: 0.0,
            cutoff_hz: 20_000.0,
            resonance: 0.0,
            attack_seconds: 0.005,
            decay_seconds: 0.2,
            sustain: 0.7,
            release_seconds: 0.2,
            osc_retrigger: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_plan() {
        let settings = SynthSettings::default();
        assert_eq!(settings.osc1_wave, Waveform::Saw);
        assert_eq!((settings.osc1_octave, settings.osc2_octave), (0, 0));
        assert_eq!(settings.osc2_wave, Waveform::Saw);
        assert_eq!(settings.osc2_pitch_semitones, 0.0);
        assert_eq!(settings.sub_octave, -1);
        assert_eq!(
            [
                settings.osc1_level,
                settings.osc2_level,
                settings.sub_level,
                settings.noise_level
            ],
            [1.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(settings.cutoff_hz, 20_000.0);
        assert_eq!(settings.resonance, 0.0);
        assert_eq!(settings.attack_seconds, 0.005);
        assert_eq!(settings.decay_seconds, 0.2);
        assert_eq!(settings.sustain, 0.7);
        assert_eq!(settings.release_seconds, 0.2);
        assert!(settings.osc_retrigger);
        assert_eq!(settings.clamped(), settings);
    }

    #[test]
    fn clamped_keeps_values_in_range() {
        let wild = SynthSettings {
            osc1_wave: Waveform::Pulse,
            osc1_octave: -9,
            osc2_wave: Waveform::Sine,
            osc2_octave: 9,
            osc2_pitch_semitones: f32::NAN,
            sub_octave: 0,
            osc1_level: 2.0,
            osc2_level: -1.0,
            sub_level: f32::INFINITY,
            noise_level: f32::NAN,
            cutoff_hz: 1.0e9,
            resonance: -3.0,
            attack_seconds: 0.0,
            decay_seconds: f32::INFINITY,
            sustain: f32::NAN,
            release_seconds: -1.0,
            osc_retrigger: false,
        }
        .clamped();
        assert_eq!(wild.osc1_wave, Waveform::Pulse);
        assert_eq!(wild.osc1_octave, -2);
        assert_eq!(wild.osc2_octave, 2);
        assert_eq!(wild.osc2_pitch_semitones, 0.0);
        assert_eq!(wild.sub_octave, -1);
        assert_eq!(
            [
                wild.osc1_level,
                wild.osc2_level,
                wild.sub_level,
                wild.noise_level
            ],
            [1.0, 0.0, 1.0, 0.0]
        );
        assert_eq!(wild.cutoff_hz, 20_000.0);
        assert_eq!(wild.resonance, 0.0);
        assert_eq!(wild.attack_seconds, 0.001);
        assert_eq!(wild.decay_seconds, 10.0);
        assert_eq!(wild.sustain, 0.7);
        assert_eq!(wild.release_seconds, 0.001);
        assert!(!wild.osc_retrigger);
    }
}
