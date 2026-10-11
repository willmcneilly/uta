//! Insert effects and the chains that hold them. See RFC-007, "The chain"
//! and "Patterns for every effect after this one", point 1.
//!
//! An effect is its permanent ID, its settings (which say its kind), an
//! on/off switch and a Mix. A chain is an ordered list of up to
//! [`MAX_EFFECTS`] of them, on a track for now. It belongs to the place it
//! sits rather than assuming it's a track's, so the master, and later a
//! return or a group, can have one: [`Chain`] names which.

use serde::{Deserialize, Serialize};

use crate::setting::{Description, Scale, Unit};
use crate::{EffectId, TrackId};

/// The most effects a chain holds. The audio thread sets aside a table of
/// this many effect states for each chain when it starts, and its work per
/// block is bounded by it. Raising it later is one number and its tests;
/// lowering it would stop projects with more from loading (RFC-007, open
/// question 5).
pub const MAX_EFFECTS: usize = 8;

/// Which chain a command means. Serialises as `{"track":"<id>"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chain {
    /// The chain between a track's source and its mixer strip.
    Track(TrackId),
}

/// One insert effect: its ID, its kind with its settings, whether it's on,
/// and its Mix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effect {
    pub(crate) id: EffectId,
    pub(crate) settings: EffectSettings,
    pub(crate) enabled: bool,
    pub(crate) mix: f32,
}

impl Effect {
    /// The lowest Mix: the sound unchanged.
    pub const MIN_MIX: f32 = 0.0;
    /// The highest Mix: only the effect.
    pub const MAX_MIX: f32 = 1.0;

    /// A new effect of the kind `settings` says, switched on, at its kind's
    /// default Mix.
    pub fn new(id: EffectId, settings: EffectSettings) -> Self {
        Self {
            id,
            mix: settings.kind().default_mix(),
            settings,
            enabled: true,
        }
    }

    /// A new effect of `kind`, with its defaults.
    pub fn default_of(id: EffectId, kind: EffectKind) -> Self {
        Self::new(id, EffectSettings::default_of(kind))
    }

    /// This effect with the given Mix.
    pub fn with_mix(self, mix: f32) -> Self {
        Self { mix, ..self }
    }

    /// This effect switched on or off.
    pub fn with_enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    pub fn id(&self) -> EffectId {
        self.id
    }

    pub fn kind(&self) -> EffectKind {
        self.settings.kind()
    }

    pub fn settings(&self) -> &EffectSettings {
        &self.settings
    }

    /// Whether it's switched on. Off, the sound passes through unchanged.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// How much of the effect you hear, from 0 (the sound unchanged) to 1
    /// (only the effect). Every effect has it, blended equal-power by the
    /// chain.
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// A copy of this effect with a new ID, for duplicating its track.
    pub fn copy(&self, id: EffectId) -> Self {
        Self { id, ..*self }
    }

    /// Mix, described. The same for every kind, apart from its default.
    pub fn describe_mix(&self) -> Description {
        Description::number(
            "Mix",
            "How much of the effect you hear against the sound going into it. \
             None leaves the sound unchanged; all of it is only the effect. \
             Halfway keeps both at nearly full strength, without a dip.",
            Unit::Fraction,
            Scale::Linear,
            (Self::MIN_MIX, Self::MAX_MIX),
            self.kind().default_mix(),
        )
    }

    /// Checks the Mix and every setting are in range.
    pub(crate) fn validate(&self) -> Result<(), crate::CommandError> {
        check_mix(self.mix)?;
        match &self.settings {
            EffectSettings::Delay(settings) => settings
                .validate()
                .map_err(crate::CommandError::DelayParamOutOfRange),
        }
    }
}

/// Checks a Mix is from 0 to 1. NaN never is.
pub(crate) fn check_mix(mix: f32) -> Result<(), crate::CommandError> {
    if (Effect::MIN_MIX..=Effect::MAX_MIX).contains(&mix) {
        Ok(())
    } else {
        Err(crate::CommandError::EffectMixOutOfRange(mix))
    }
}

/// The kinds of effect there are: just Delay for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Delay,
}

impl EffectKind {
    /// Every kind, in the order the **+** menu lists them.
    pub const ALL: [EffectKind; 1] = [Self::Delay];

    /// What the panel and the **+** menu call it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Delay => "Delay",
        }
    }

    /// A new effect's Mix.
    pub fn default_mix(self) -> f32 {
        match self {
            Self::Delay => 0.3,
        }
    }
}

/// An effect's own settings, which also say its kind. Serialises as
/// `{"delay":{...}}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectSettings {
    Delay(DelaySettings),
}

impl EffectSettings {
    /// The settings a new effect of `kind` starts with.
    pub fn default_of(kind: EffectKind) -> Self {
        match kind {
            EffectKind::Delay => Self::Delay(DelaySettings::default()),
        }
    }

    pub fn kind(&self) -> EffectKind {
        match self {
            Self::Delay(_) => EffectKind::Delay,
        }
    }
}

/// The Delay's settings: a tape echo. See RFC-007, "The Delay". This is the
/// plain Delay: its time in seconds and its Feedback. Sync, Mode, the cuts,
/// Drive and Wobble arrive with its tape loop (UTA-70).
///
/// Serialises with the same names as [`DelayParam`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelaySettings {
    /// The echoes' spacing, from 1 ms to 5 s.
    pub time_seconds: f32,
    /// How much of each echo comes round again, from 0 to 1.
    pub feedback: f32,
}

impl DelaySettings {
    /// The shortest time: 1 ms, for slapback and effects off the grid.
    pub const MIN_TIME_SECONDS: f32 = 0.001;
    /// The longest time: the length of the tape loop the engine sets aside.
    pub const MAX_TIME_SECONDS: f32 = 5.0;
    pub const MIN_FEEDBACK: f32 = 0.0;
    /// Without the tape loop's saturation to hold them, echoes past 100%
    /// would grow without end. UTA-70 raises this to 110% with it.
    pub const MAX_FEEDBACK: f32 = 1.0;

    /// Every setting, as `SetDelayParam` carries them.
    pub fn params(&self) -> [DelayParam; 2] {
        [
            DelayParam::TimeSeconds(self.time_seconds),
            DelayParam::Feedback(self.feedback),
        ]
    }

    /// Checks every setting is in range, or returns the first that isn't.
    pub(crate) fn validate(&self) -> Result<(), DelayParam> {
        match self.params().into_iter().find(|param| !param.in_range()) {
            Some(param) => Err(param),
            None => Ok(()),
        }
    }

    /// Sets one setting and returns the old value, or the setting back if
    /// it's out of range. On error, nothing changes.
    pub(crate) fn set(&mut self, param: DelayParam) -> Result<DelayParam, DelayParam> {
        if !param.in_range() {
            return Err(param);
        }
        Ok(match param {
            DelayParam::TimeSeconds(value) => {
                DelayParam::TimeSeconds(std::mem::replace(&mut self.time_seconds, value))
            }
            DelayParam::Feedback(value) => {
                DelayParam::Feedback(std::mem::replace(&mut self.feedback, value))
            }
        })
    }
}

impl Default for DelaySettings {
    /// A dotted eighth at the default 120 BPM, and 40% feedback: RFC-007's
    /// starting point, until the Delay can sync to the tempo (UTA-70).
    fn default() -> Self {
        Self {
            time_seconds: 0.375,
            feedback: 0.4,
        }
    }
}

/// One Delay setting with its new value, as `SetDelayParam` carries it.
///
/// Serialises as `{"name":"feedback","value":0.5}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum DelayParam {
    TimeSeconds(f32),
    Feedback(f32),
}

impl DelayParam {
    /// Whether `other` is the same setting, whatever its value.
    pub fn same_setting(&self, other: &DelayParam) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The setting's value and its limits (inclusive).
    pub fn range(&self) -> (f32, f32, f32) {
        use DelaySettings as S;
        match *self {
            Self::TimeSeconds(seconds) => (seconds, S::MIN_TIME_SECONDS, S::MAX_TIME_SECONDS),
            Self::Feedback(amount) => (amount, S::MIN_FEEDBACK, S::MAX_FEEDBACK),
        }
    }

    /// The setting described: its label, how it sounds, its unit, range,
    /// scale and default. The value it carries doesn't matter.
    pub fn describe(&self) -> Description {
        let default = DelaySettings::default()
            .params()
            .into_iter()
            .find(|param| param.same_setting(self))
            .expect("every setting has a default");
        let (value, min, max) = default.range();
        let number = |label, sounds, unit, scale| {
            Description::number(label, sounds, unit, scale, (min, max), value)
        };
        match self {
            Self::TimeSeconds(_) => number(
                "Time",
                "How far apart the echoes are. A few milliseconds thickens the \
                 sound into a slapback or a buzz; a quarter of a second is a \
                 rhythmic echo; seconds are long, separate repeats.",
                Unit::Seconds,
                Scale::Log,
            ),
            Self::Feedback(_) => number(
                "Feedback",
                "How much of each echo comes round again, so how many repeats \
                 you hear. None is a single echo; a little is a few that fade \
                 quickly; all of it barely fades at all.",
                Unit::Fraction,
                Scale::Linear,
            ),
        }
    }

    /// Whether the value is within the setting's range. NaN never is.
    fn in_range(&self) -> bool {
        let (value, min, max) = self.range();
        (min..=max).contains(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::Kind;

    #[test]
    fn a_new_delay_has_rfc_007s_starting_point() {
        let effect = Effect::default_of(EffectId::random(), EffectKind::Delay);
        assert!(effect.enabled());
        assert_eq!(effect.mix(), 0.3);
        assert_eq!(
            effect.settings(),
            &EffectSettings::Delay(DelaySettings {
                time_seconds: 0.375,
                feedback: 0.4,
            })
        );
    }

    #[test]
    fn every_delay_setting_and_mix_is_described_with_its_range_and_default() {
        let settings = DelaySettings::default();
        let described: Vec<_> = settings
            .params()
            .iter()
            .map(|param| {
                let description = param.describe();
                let (value, min, max) = param.range();
                let Kind::Number {
                    unit,
                    scale,
                    limits,
                    default,
                } = description.kind
                else {
                    panic!("{param:?} is a number");
                };
                assert_eq!(limits, (min, max), "{param:?}");
                assert_eq!(default, value, "{param:?}");
                assert!(description.sounds.ends_with('.'), "{param:?}");
                (description.label, unit, scale)
            })
            .collect();
        assert_eq!(
            described,
            [
                ("Time", Unit::Seconds, Scale::Log),
                ("Feedback", Unit::Fraction, Scale::Linear),
            ]
        );
        let mix = Effect::default_of(EffectId::random(), EffectKind::Delay).describe_mix();
        assert_eq!(mix.label, "Mix");
        assert_eq!(
            mix.kind,
            Kind::Number {
                unit: Unit::Fraction,
                scale: Scale::Linear,
                limits: (0.0, 1.0),
                default: 0.3,
            }
        );
    }

    #[test]
    fn delay_limits_are_inclusive_and_out_of_range_changes_nothing() {
        let mut settings = DelaySettings::default();
        for param in [
            DelayParam::TimeSeconds(0.001),
            DelayParam::TimeSeconds(5.0),
            DelayParam::Feedback(0.0),
            DelayParam::Feedback(1.0),
        ] {
            assert!(settings.set(param).is_ok(), "{param:?} was rejected");
        }
        let before = settings;
        for param in [
            DelayParam::TimeSeconds(0.0009),
            DelayParam::TimeSeconds(5.01),
            DelayParam::TimeSeconds(f32::NAN),
            DelayParam::Feedback(-0.01),
            DelayParam::Feedback(1.01),
        ] {
            assert!(settings.set(param).is_err(), "{param:?} was accepted");
            assert_eq!(settings, before);
        }
    }

    #[test]
    fn a_chain_and_settings_serialise_like_this() {
        let id = TrackId::random();
        assert_eq!(
            serde_json::to_string(&Chain::Track(id)).unwrap(),
            format!(r#"{{"track":"{id}"}}"#)
        );
        assert_eq!(
            serde_json::to_string(&DelayParam::Feedback(0.5)).unwrap(),
            r#"{"name":"feedback","value":0.5}"#
        );
        assert_eq!(
            serde_json::to_string(&EffectSettings::default_of(EffectKind::Delay)).unwrap(),
            r#"{"delay":{"time_seconds":0.375,"feedback":0.4}}"#
        );
    }
}
