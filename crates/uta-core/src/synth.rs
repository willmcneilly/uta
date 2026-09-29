//! The built-in synth's settings. See RFC-002, "The shared model", point 6.
//!
//! The engine keeps its own copy of these, with the same names, units, ranges
//! and defaults (the "Synth settings" table in `docs/plans/make-a-loop.md`),
//! so the snapshot only has to copy values across.

use serde::{Deserialize, Serialize};

/// The oscillator's wave shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
}

/// Everything that shapes the synth's sound.
///
/// Serialises with the same names as [`SynthParam`], as `AddTracks` carries
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthSettings {
    pub waveform: Waveform,
    /// The low-pass filter's cutoff, in Hz. Controls move it on a log scale.
    pub cutoff_hz: f32,
    /// The filter's resonance, from 0 to 1.
    pub resonance: f32,
    pub attack_seconds: f32,
    pub decay_seconds: f32,
    /// The level held after the decay, from 0 to 1 (linear, not dB).
    pub sustain: f32,
    pub release_seconds: f32,
}

impl SynthSettings {
    pub const MIN_CUTOFF_HZ: f32 = 20.0;
    pub const MAX_CUTOFF_HZ: f32 = 20_000.0;
    pub const MIN_RESONANCE: f32 = 0.0;
    pub const MAX_RESONANCE: f32 = 1.0;
    /// The shortest attack, decay or release: 1 ms.
    pub const MIN_ENVELOPE_SECONDS: f32 = 0.001;
    /// The longest attack, decay or release.
    pub const MAX_ENVELOPE_SECONDS: f32 = 10.0;
    pub const MIN_SUSTAIN: f32 = 0.0;
    pub const MAX_SUSTAIN: f32 = 1.0;

    /// Every setting, as `SetSynthParam` carries them.
    pub fn params(&self) -> [SynthParam; 7] {
        [
            SynthParam::Waveform(self.waveform),
            SynthParam::CutoffHz(self.cutoff_hz),
            SynthParam::Resonance(self.resonance),
            SynthParam::AttackSeconds(self.attack_seconds),
            SynthParam::DecaySeconds(self.decay_seconds),
            SynthParam::Sustain(self.sustain),
            SynthParam::ReleaseSeconds(self.release_seconds),
        ]
    }

    /// Checks every setting is in range, or returns the first that isn't.
    pub(crate) fn validate(&self) -> Result<(), SynthParam> {
        match self.params().into_iter().find(|param| !param.in_range()) {
            Some(param) => Err(param),
            None => Ok(()),
        }
    }

    /// Sets one setting and returns the old value, or an error if `param`
    /// is out of range. On error, nothing changes.
    pub(crate) fn set(&mut self, param: SynthParam) -> Result<SynthParam, SynthParam> {
        if !param.in_range() {
            return Err(param);
        }
        Ok(match param {
            SynthParam::Waveform(waveform) => {
                SynthParam::Waveform(std::mem::replace(&mut self.waveform, waveform))
            }
            SynthParam::CutoffHz(value) => {
                SynthParam::CutoffHz(std::mem::replace(&mut self.cutoff_hz, value))
            }
            SynthParam::Resonance(value) => {
                SynthParam::Resonance(std::mem::replace(&mut self.resonance, value))
            }
            SynthParam::AttackSeconds(value) => {
                SynthParam::AttackSeconds(std::mem::replace(&mut self.attack_seconds, value))
            }
            SynthParam::DecaySeconds(value) => {
                SynthParam::DecaySeconds(std::mem::replace(&mut self.decay_seconds, value))
            }
            SynthParam::Sustain(value) => {
                SynthParam::Sustain(std::mem::replace(&mut self.sustain, value))
            }
            SynthParam::ReleaseSeconds(value) => {
                SynthParam::ReleaseSeconds(std::mem::replace(&mut self.release_seconds, value))
            }
        })
    }
}

impl Default for SynthSettings {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            cutoff_hz: Self::MAX_CUTOFF_HZ,
            resonance: 0.0,
            attack_seconds: 0.005,
            decay_seconds: 0.2,
            sustain: 0.7,
            release_seconds: 0.2,
        }
    }
}

/// One synth setting with its new value, as `SetSynthParam` carries it.
///
/// Serialises as `{"name":"cutoff_hz","value":1000.0}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum SynthParam {
    Waveform(Waveform),
    CutoffHz(f32),
    Resonance(f32),
    AttackSeconds(f32),
    DecaySeconds(f32),
    Sustain(f32),
    ReleaseSeconds(f32),
}

impl SynthParam {
    /// Whether `other` is the same setting, whatever its value.
    pub fn same_setting(&self, other: &SynthParam) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The setting's value and its limits (inclusive), or `None` for the
    /// waveform, which has no range.
    pub fn range(&self) -> Option<(f32, f32, f32)> {
        use SynthSettings as S;
        match *self {
            Self::Waveform(_) => None,
            Self::CutoffHz(hz) => Some((hz, S::MIN_CUTOFF_HZ, S::MAX_CUTOFF_HZ)),
            Self::Resonance(value) => Some((value, S::MIN_RESONANCE, S::MAX_RESONANCE)),
            Self::AttackSeconds(seconds)
            | Self::DecaySeconds(seconds)
            | Self::ReleaseSeconds(seconds) => {
                Some((seconds, S::MIN_ENVELOPE_SECONDS, S::MAX_ENVELOPE_SECONDS))
            }
            Self::Sustain(level) => Some((level, S::MIN_SUSTAIN, S::MAX_SUSTAIN)),
        }
    }

    /// Whether the value is within the setting's range. NaN never is.
    fn in_range(&self) -> bool {
        self.range()
            .is_none_or(|(value, min, max)| (min..=max).contains(&value))
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
    }

    #[test]
    fn set_returns_the_old_value() {
        let mut settings = SynthSettings::default();
        assert_eq!(
            settings.set(SynthParam::CutoffHz(440.0)),
            Ok(SynthParam::CutoffHz(20_000.0))
        );
        assert_eq!(settings.cutoff_hz, 440.0);
        assert_eq!(
            settings.set(SynthParam::Waveform(Waveform::Square)),
            Ok(SynthParam::Waveform(Waveform::Saw))
        );
        assert_eq!(settings.waveform, Waveform::Square);
    }

    #[test]
    fn limits_are_inclusive() {
        let mut settings = SynthSettings::default();
        for param in [
            SynthParam::CutoffHz(20.0),
            SynthParam::CutoffHz(20_000.0),
            SynthParam::Resonance(0.0),
            SynthParam::Resonance(1.0),
            SynthParam::AttackSeconds(0.001),
            SynthParam::DecaySeconds(10.0),
            SynthParam::Sustain(0.0),
            SynthParam::Sustain(1.0),
            SynthParam::ReleaseSeconds(0.001),
        ] {
            assert!(settings.set(param).is_ok(), "{param:?} was rejected");
        }
    }

    #[test]
    fn out_of_range_values_are_rejected_and_change_nothing() {
        let mut settings = SynthSettings::default();
        for param in [
            SynthParam::CutoffHz(19.9),
            SynthParam::CutoffHz(20_001.0),
            SynthParam::CutoffHz(f32::NAN),
            SynthParam::Resonance(-0.01),
            SynthParam::Resonance(1.01),
            SynthParam::AttackSeconds(0.0),
            SynthParam::DecaySeconds(10.5),
            SynthParam::Sustain(f32::INFINITY),
            SynthParam::ReleaseSeconds(-1.0),
        ] {
            assert!(settings.set(param).is_err(), "{param:?} was accepted");
            assert_eq!(settings, SynthSettings::default());
        }
    }
}
