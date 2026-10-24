//! The Reverb's settings: an early burst and a late tail you balance, in a
//! mode's arrangement. See RFC-010, "The controls".
//!
//! Every setting RFC-010 lists is here from the start, with its range, unit
//! and default, so the command format goes up once. UTA-99's engine plays
//! the Mode, Predelay, Decay, Depth, the sizes, Diffusion and Early Send;
//! the tone, the crosses and the modulation are played from UTA-100.

use serde::{Deserialize, Serialize};

use crate::setting::{Choice, Description, Scale, Unit};

/// The Reverb's settings. Serialises with the same names as
/// [`ReverbParam`]. They arrived whole in command format 15, so every one
/// is always there.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReverbSettings {
    pub mode: ReverbMode,
    /// The gap before the space answers, from 0 to 0.5 s.
    pub predelay_seconds: f32,
    /// How long the tail takes to fall 60 dB (RT60), from 0.1 to 30 s.
    pub decay_seconds: f32,
    /// The blend of the early burst and the late tail, from 0 (only early)
    /// to 1 (only late).
    pub depth: f32,
    /// The low-pass on the whole reverb, from 1 to 20 kHz.
    pub high_cut_hz: f32,
    /// The low shelf on the whole reverb, from 20 Hz to 1 kHz: off at
    /// 20 Hz, its lowest.
    pub low_cut_hz: f32,
    /// The early burst's length, from 2 ms to 1 s.
    pub early_size_seconds: f32,
    /// How much left and right swap in the early burst, from 0 to 1.
    pub early_cross: f32,
    /// From 0 (separate early echoes) to 1 (a smooth burst).
    pub diffusion: f32,
    /// How much of the early burst feeds the tail, from 0 to 1.
    pub early_send: f32,
    /// The early burst's slow random wobble: its rate, from 0.05 to 5 Hz,
    /// and its depth, from 0 to 1.
    pub early_mod_rate_hz: f32,
    pub early_mod_depth: f32,
    /// The room's size, from 0 to 1.
    pub late_size: f32,
    /// How much the tail's sides feed each other, from 0 to 1.
    pub late_cross: f32,
    /// The tail's slow random drift: its rate, from 0.05 to 5 Hz, and its
    /// depth, from 0 to 1.
    pub late_mod_rate_hz: f32,
    pub late_mod_depth: f32,
    /// The lows' decay as a multiple of Decay, from 0.25 to 4, below Bass
    /// Xover, from 100 Hz to 2 kHz.
    pub bass_mult: f32,
    pub bass_xover_hz: f32,
    /// The highs' decay as a multiple of Decay, from 0.1 to 2, above High
    /// Xover, from 1 to 16 kHz.
    pub high_mult: f32,
    pub high_xover_hz: f32,
}

impl ReverbSettings {
    pub const MIN_PREDELAY_SECONDS: f32 = 0.0;
    pub const MAX_PREDELAY_SECONDS: f32 = 0.5;
    pub const MIN_DECAY_SECONDS: f32 = 0.1;
    /// RFC-010, open question 2: past about 30 s a tail barely fades within
    /// a song, and outlasts `uta render`'s 30 s limit.
    pub const MAX_DECAY_SECONDS: f32 = 30.0;
    pub const MIN_HIGH_CUT_HZ: f32 = 1_000.0;
    pub const MAX_HIGH_CUT_HZ: f32 = 20_000.0;
    /// The Low Cut's lowest setting, where it's off.
    pub const MIN_LOW_CUT_HZ: f32 = 20.0;
    pub const MAX_LOW_CUT_HZ: f32 = 1_000.0;
    pub const MIN_EARLY_SIZE_SECONDS: f32 = 0.002;
    pub const MAX_EARLY_SIZE_SECONDS: f32 = 1.0;
    pub const MIN_MOD_RATE_HZ: f32 = 0.05;
    pub const MAX_MOD_RATE_HZ: f32 = 5.0;
    pub const MIN_BASS_MULT: f32 = 0.25;
    pub const MAX_BASS_MULT: f32 = 4.0;
    pub const MIN_BASS_XOVER_HZ: f32 = 100.0;
    pub const MAX_BASS_XOVER_HZ: f32 = 2_000.0;
    pub const MIN_HIGH_MULT: f32 = 0.1;
    pub const MAX_HIGH_MULT: f32 = 2.0;
    pub const MIN_HIGH_XOVER_HZ: f32 = 1_000.0;
    pub const MAX_HIGH_XOVER_HZ: f32 = 16_000.0;
    /// The limits of every setting that's a share of something: Depth, the
    /// crosses, Diffusion, Early Send, the sizes and the mod depths.
    pub const MIN_AMOUNT: f32 = 0.0;
    pub const MAX_AMOUNT: f32 = 1.0;

    /// Every setting, as `SetReverbParam` carries them, in RFC-010's order.
    pub fn params(&self) -> [ReverbParam; 20] {
        [
            ReverbParam::Mode(self.mode),
            ReverbParam::PredelaySeconds(self.predelay_seconds),
            ReverbParam::DecaySeconds(self.decay_seconds),
            ReverbParam::Depth(self.depth),
            ReverbParam::HighCutHz(self.high_cut_hz),
            ReverbParam::LowCutHz(self.low_cut_hz),
            ReverbParam::EarlySizeSeconds(self.early_size_seconds),
            ReverbParam::EarlyCross(self.early_cross),
            ReverbParam::Diffusion(self.diffusion),
            ReverbParam::EarlySend(self.early_send),
            ReverbParam::EarlyModRateHz(self.early_mod_rate_hz),
            ReverbParam::EarlyModDepth(self.early_mod_depth),
            ReverbParam::LateSize(self.late_size),
            ReverbParam::LateCross(self.late_cross),
            ReverbParam::LateModRateHz(self.late_mod_rate_hz),
            ReverbParam::LateModDepth(self.late_mod_depth),
            ReverbParam::BassMult(self.bass_mult),
            ReverbParam::BassXoverHz(self.bass_xover_hz),
            ReverbParam::HighMult(self.high_mult),
            ReverbParam::HighXoverHz(self.high_xover_hz),
        ]
    }

    /// Checks every setting is in range, or returns the first that isn't.
    pub(crate) fn validate(&self) -> Result<(), ReverbParam> {
        match self.params().into_iter().find(|param| !param.in_range()) {
            Some(param) => Err(param),
            None => Ok(()),
        }
    }

    /// Sets one setting and returns the old value, or the setting back if
    /// it's out of range. On error, nothing changes.
    pub(crate) fn set(&mut self, param: ReverbParam) -> Result<ReverbParam, ReverbParam> {
        if !param.in_range() {
            return Err(param);
        }
        use ReverbParam as P;
        use std::mem::replace;
        Ok(match param {
            P::Mode(mode) => P::Mode(replace(&mut self.mode, mode)),
            P::PredelaySeconds(v) => P::PredelaySeconds(replace(&mut self.predelay_seconds, v)),
            P::DecaySeconds(v) => P::DecaySeconds(replace(&mut self.decay_seconds, v)),
            P::Depth(v) => P::Depth(replace(&mut self.depth, v)),
            P::HighCutHz(v) => P::HighCutHz(replace(&mut self.high_cut_hz, v)),
            P::LowCutHz(v) => P::LowCutHz(replace(&mut self.low_cut_hz, v)),
            P::EarlySizeSeconds(v) => P::EarlySizeSeconds(replace(&mut self.early_size_seconds, v)),
            P::EarlyCross(v) => P::EarlyCross(replace(&mut self.early_cross, v)),
            P::Diffusion(v) => P::Diffusion(replace(&mut self.diffusion, v)),
            P::EarlySend(v) => P::EarlySend(replace(&mut self.early_send, v)),
            P::EarlyModRateHz(v) => P::EarlyModRateHz(replace(&mut self.early_mod_rate_hz, v)),
            P::EarlyModDepth(v) => P::EarlyModDepth(replace(&mut self.early_mod_depth, v)),
            P::LateSize(v) => P::LateSize(replace(&mut self.late_size, v)),
            P::LateCross(v) => P::LateCross(replace(&mut self.late_cross, v)),
            P::LateModRateHz(v) => P::LateModRateHz(replace(&mut self.late_mod_rate_hz, v)),
            P::LateModDepth(v) => P::LateModDepth(replace(&mut self.late_mod_depth, v)),
            P::BassMult(v) => P::BassMult(replace(&mut self.bass_mult, v)),
            P::BassXoverHz(v) => P::BassXoverHz(replace(&mut self.bass_xover_hz, v)),
            P::HighMult(v) => P::HighMult(replace(&mut self.high_mult, v)),
            P::HighXoverHz(v) => P::HighXoverHz(replace(&mut self.high_xover_hz, v)),
        })
    }
}

impl Default for ReverbSettings {
    /// RFC-010's starting point: Room, a 2 s Decay, Depth at 50% and a High
    /// Mult of 0.5 (Mix, 25%, is the effect's). The rest are this ticket's
    /// first guesses, to be tuned by ear in UTA-101: the cuts open, the
    /// crosses full and the modulation still, as the engine plays them until
    /// UTA-100, and a short, fairly smooth burst half sent into the tail.
    fn default() -> Self {
        Self {
            mode: ReverbMode::Room,
            predelay_seconds: 0.01,
            decay_seconds: 2.0,
            depth: 0.5,
            high_cut_hz: Self::MAX_HIGH_CUT_HZ,
            low_cut_hz: Self::MIN_LOW_CUT_HZ,
            early_size_seconds: 0.03,
            early_cross: 1.0,
            diffusion: 0.8,
            early_send: 0.5,
            early_mod_rate_hz: 0.5,
            early_mod_depth: 0.0,
            late_size: 0.5,
            late_cross: 1.0,
            late_mod_rate_hz: 0.5,
            late_mod_depth: 0.0,
            bass_mult: 1.0,
            bass_xover_hz: 500.0,
            high_mult: 0.5,
            high_xover_hz: 4_000.0,
        }
    }
}

/// The Reverb's arrangement of its building blocks (RFC-010, "Modes"). Room
/// is the first; Chamber and Space follow in UTA-102 and UTA-103.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReverbMode {
    /// A natural, medium to large room: a little sparse at first, building
    /// to a dense tail.
    #[default]
    Room,
}

impl ReverbMode {
    /// Every mode, as the panel offers them.
    pub const ALL: [ReverbMode; 1] = [Self::Room];

    /// The mode as `SetReverbParam` carries it: `"room"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::Room => "room",
        }
    }
}

/// Every mode, as the panel offers them.
const REVERB_MODES: [Choice; 1] = [Choice {
    value: "room",
    label: "Room",
}];

/// One Reverb setting with its new value, as `SetReverbParam` carries it.
///
/// Serialises as `{"name":"decay_seconds","value":2.0}` or
/// `{"name":"mode","value":"room"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum ReverbParam {
    Mode(ReverbMode),
    PredelaySeconds(f32),
    DecaySeconds(f32),
    Depth(f32),
    HighCutHz(f32),
    LowCutHz(f32),
    EarlySizeSeconds(f32),
    EarlyCross(f32),
    Diffusion(f32),
    EarlySend(f32),
    EarlyModRateHz(f32),
    EarlyModDepth(f32),
    LateSize(f32),
    LateCross(f32),
    LateModRateHz(f32),
    LateModDepth(f32),
    BassMult(f32),
    BassXoverHz(f32),
    HighMult(f32),
    HighXoverHz(f32),
}

impl ReverbParam {
    /// Whether `other` is the same setting, whatever its value.
    pub fn same_setting(&self, other: &ReverbParam) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The setting's value and its limits (inclusive), or `None` for Mode,
    /// which has no range.
    pub fn range(&self) -> Option<(f32, f32, f32)> {
        use ReverbSettings as S;
        let amount = |value| Some((value, S::MIN_AMOUNT, S::MAX_AMOUNT));
        match *self {
            Self::Mode(_) => None,
            Self::PredelaySeconds(v) => Some((v, S::MIN_PREDELAY_SECONDS, S::MAX_PREDELAY_SECONDS)),
            Self::DecaySeconds(v) => Some((v, S::MIN_DECAY_SECONDS, S::MAX_DECAY_SECONDS)),
            Self::HighCutHz(v) => Some((v, S::MIN_HIGH_CUT_HZ, S::MAX_HIGH_CUT_HZ)),
            Self::LowCutHz(v) => Some((v, S::MIN_LOW_CUT_HZ, S::MAX_LOW_CUT_HZ)),
            Self::EarlySizeSeconds(v) => {
                Some((v, S::MIN_EARLY_SIZE_SECONDS, S::MAX_EARLY_SIZE_SECONDS))
            }
            Self::EarlyModRateHz(v) | Self::LateModRateHz(v) => {
                Some((v, S::MIN_MOD_RATE_HZ, S::MAX_MOD_RATE_HZ))
            }
            Self::BassMult(v) => Some((v, S::MIN_BASS_MULT, S::MAX_BASS_MULT)),
            Self::BassXoverHz(v) => Some((v, S::MIN_BASS_XOVER_HZ, S::MAX_BASS_XOVER_HZ)),
            Self::HighMult(v) => Some((v, S::MIN_HIGH_MULT, S::MAX_HIGH_MULT)),
            Self::HighXoverHz(v) => Some((v, S::MIN_HIGH_XOVER_HZ, S::MAX_HIGH_XOVER_HZ)),
            Self::Depth(v)
            | Self::EarlyCross(v)
            | Self::Diffusion(v)
            | Self::EarlySend(v)
            | Self::EarlyModDepth(v)
            | Self::LateSize(v)
            | Self::LateCross(v)
            | Self::LateModDepth(v) => amount(v),
        }
    }

    /// The setting described: its label, how it sounds, its unit, range,
    /// scale and default, or its choices. The value it carries doesn't
    /// matter.
    pub fn describe(&self) -> Description {
        let defaults = ReverbSettings::default();
        let default = defaults
            .params()
            .into_iter()
            .find(|param| param.same_setting(self))
            .expect("every setting has a default");
        let (value, min, max) = default.range().unwrap_or_default();
        let number = |label, sounds, unit, scale| {
            Description::number(label, sounds, unit, scale, (min, max), value)
        };
        use {Scale::*, Unit::*};
        match self {
            Self::Mode(_) => Description::choice(
                "Mode",
                "The kind of space: how its echoes are arranged. Room is a \
                 natural room that starts a little sparse and builds to a \
                 dense tail.",
                &REVERB_MODES,
                defaults.mode.value(),
            ),
            Self::PredelaySeconds(_) => number(
                "Predelay",
                "The gap before the space answers. None puts the sound right \
                 inside the room; a few tens of milliseconds keeps the hit \
                 clear before the reverb, and more pushes the space away \
                 behind it.",
                Seconds,
                Linear,
            ),
            Self::DecaySeconds(_) => number(
                "Decay",
                "How long the tail rings, as the time it takes to fall 60 dB. \
                 Half a second is a small room; two seconds a hall; ten or \
                 more a cathedral, or a wash that hangs under the next chord.",
                Seconds,
                Log,
            ),
            Self::Depth(_) => number(
                "Depth",
                "The balance of the early burst and the tail. None is only \
                 the early reflections, a tight slap of a space; all of it \
                 is only the tail, smooth and distant.",
                Fraction,
                Linear,
            ),
            Self::HighCutHz(_) => number(
                "High cut",
                "Darkens the whole reverb, taking away the highs above it, so \
                 the space sits behind the sound instead of fizzing over it. \
                 All the way up, it stays bright.",
                Hz,
                Log,
            ),
            Self::LowCutHz(_) => number(
                "Low cut",
                "Thins the whole reverb, turning down the lows below it, so the \
                 space doesn't muddy the bass and the kick. All the way down, \
                 it's off.",
                Hz,
                Log,
            ),
            Self::EarlySizeSeconds(_) => number(
                "Early size",
                "How long the early burst lasts. A few milliseconds is a small, \
                 close room; a hundred or more is a gated, eighties burst.",
                Seconds,
                Log,
            ),
            Self::EarlyCross(_) => number(
                "Early cross",
                "How much the left and right swap in the early burst. None \
                 keeps each side's reflections on its own side; more widens \
                 and thickens them.",
                Fraction,
                Linear,
            ),
            Self::Diffusion(_) => number(
                "Diffusion",
                "How smooth the early burst is. Low, you hear separate early \
                 echoes, like a hard, bare room; high, they blur into one \
                 dense burst.",
                Fraction,
                Linear,
            ),
            Self::EarlySend(_) => number(
                "Early send",
                "How much of the early burst feeds the tail. None runs them \
                 side by side; more makes the tail start denser, or, with a \
                 long early size, swell in slowly.",
                Fraction,
                Linear,
            ),
            Self::EarlyModRateHz(_) => number(
                "Early mod rate",
                "How fast the early burst's slow wobble moves. Slow just \
                 smooths it; fast starts to sound like chorus.",
                Hz,
                Log,
            ),
            Self::EarlyModDepth(_) => number(
                "Early mod depth",
                "How far the early burst's delays wobble. None keeps them \
                 still; a little smears its echoes so nothing rings.",
                Fraction,
                Linear,
            ),
            Self::LateSize(_) => number(
                "Late size",
                "The size of the room the tail rings in. Small puts the \
                 reflections close together, like a box; large spreads them \
                 out, like a hall.",
                Fraction,
                Linear,
            ),
            Self::LateCross(_) => number(
                "Late cross",
                "How much the tail's left and right feed each other. None \
                 keeps the sound's stereo image in the tail; all of it \
                 spreads everything across both sides.",
                Fraction,
                Linear,
            ),
            Self::LateModRateHz(_) => number(
                "Late mod rate",
                "How fast the tail's slow drift moves. Around half a hertz it \
                 just smooths the tail; above one it starts to sound like \
                 chorus.",
                Hz,
                Log,
            ),
            Self::LateModDepth(_) => number(
                "Late mod depth",
                "How far the tail's delays drift. None leaves it still, where \
                 a ring can sing out; a little makes it lush; a lot makes it \
                 waver in pitch.",
                Fraction,
                Linear,
            ),
            Self::BassMult(_) => number(
                "Bass mult",
                "How long the lows ring, as a multiple of Decay. Above 1 the \
                 bass booms on after the rest; below, the tail thins out as it \
                 fades.",
                Amount,
                Log,
            ),
            Self::BassXoverHz(_) => number(
                "Bass xover",
                "Where the lows start, for Bass mult: everything below it \
                 rings for Bass mult times Decay.",
                Hz,
                Log,
            ),
            Self::HighMult(_) => number(
                "High mult",
                "How long the highs ring, as a multiple of Decay. Below 1 the \
                 tail darkens as it fades, the way air and soft furnishings \
                 swallow treble; above, it stays bright and glassy.",
                Amount,
                Log,
            ),
            Self::HighXoverHz(_) => number(
                "High xover",
                "Where the highs start, for High mult: everything above it \
                 rings for High mult times Decay.",
                Hz,
                Log,
            ),
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
    use crate::setting::Kind;

    /// RFC-010's defaults: Room, a 2 s Decay, Depth 50% and a High Mult of
    /// 0.5.
    #[test]
    fn a_new_reverb_has_rfc_010s_starting_point() {
        let settings = ReverbSettings::default();
        assert_eq!(settings.mode, ReverbMode::Room);
        assert_eq!(settings.decay_seconds, 2.0);
        assert_eq!(settings.depth, 0.5);
        assert_eq!(settings.high_mult, 0.5);
        assert!(settings.validate().is_ok());
    }

    /// Every setting in RFC-010's table, with its range and unit.
    #[test]
    fn every_reverb_setting_is_described_with_rfc_010s_range_and_its_default() {
        let settings = ReverbSettings::default();
        let described: Vec<_> = settings
            .params()
            .iter()
            .map(|param| {
                let description = param.describe();
                assert!(description.sounds.ends_with('.'), "{param:?}");
                let kind = match (description.kind, param.range()) {
                    (
                        Kind::Number {
                            unit,
                            scale,
                            limits,
                            default,
                            step: None,
                        },
                        Some((value, min, max)),
                    ) => {
                        assert_eq!(default, value, "{param:?}");
                        assert_eq!(limits, (min, max), "{param:?}");
                        format!("{unit:?} {scale:?} {min}..{max}")
                    }
                    (Kind::Choice { options, default }, None) => {
                        assert_eq!(default, settings.mode.value());
                        format!("{} choices", options.len())
                    }
                    (kind, range) => panic!("{param:?}: {kind:?} with {range:?}"),
                };
                (description.label, kind)
            })
            .collect();
        let expected = [
            ("Mode", "1 choices"),
            ("Predelay", "Seconds Linear 0..0.5"),
            ("Decay", "Seconds Log 0.1..30"),
            ("Depth", "Fraction Linear 0..1"),
            ("High cut", "Hz Log 1000..20000"),
            ("Low cut", "Hz Log 20..1000"),
            ("Early size", "Seconds Log 0.002..1"),
            ("Early cross", "Fraction Linear 0..1"),
            ("Diffusion", "Fraction Linear 0..1"),
            ("Early send", "Fraction Linear 0..1"),
            ("Early mod rate", "Hz Log 0.05..5"),
            ("Early mod depth", "Fraction Linear 0..1"),
            ("Late size", "Fraction Linear 0..1"),
            ("Late cross", "Fraction Linear 0..1"),
            ("Late mod rate", "Hz Log 0.05..5"),
            ("Late mod depth", "Fraction Linear 0..1"),
            ("Bass mult", "Amount Log 0.25..4"),
            ("Bass xover", "Hz Log 100..2000"),
            ("High mult", "Amount Log 0.1..2"),
            ("High xover", "Hz Log 1000..16000"),
        ]
        .map(|(label, kind)| (label, kind.to_string()));
        assert_eq!(described, expected);
    }

    #[test]
    fn reverb_limits_are_inclusive_and_out_of_range_changes_nothing() {
        let mut settings = ReverbSettings::default();
        let ends: Vec<ReverbParam> = settings
            .params()
            .iter()
            .filter_map(|param| param.range().map(|(_, min, max)| (param, min, max)))
            .flat_map(|(param, min, max)| [with_value(param, min), with_value(param, max)])
            .collect();
        for param in ends {
            assert!(settings.set(param).is_ok(), "{param:?} was rejected");
        }
        let before = settings;
        for param in settings.params() {
            let Some((_, min, max)) = param.range() else {
                continue;
            };
            for value in [min - 0.001 * (max - min) - 1e-6, max * 1.001, f32::NAN] {
                let param = with_value(&param, value);
                assert!(settings.set(param).is_err(), "{param:?} was accepted");
                assert_eq!(settings, before);
            }
        }
    }

    /// `param` carrying `value` instead.
    fn with_value(param: &ReverbParam, v: f32) -> ReverbParam {
        use ReverbParam as P;
        match param {
            P::Mode(_) => panic!("Mode has no value"),
            P::PredelaySeconds(_) => P::PredelaySeconds(v),
            P::DecaySeconds(_) => P::DecaySeconds(v),
            P::Depth(_) => P::Depth(v),
            P::HighCutHz(_) => P::HighCutHz(v),
            P::LowCutHz(_) => P::LowCutHz(v),
            P::EarlySizeSeconds(_) => P::EarlySizeSeconds(v),
            P::EarlyCross(_) => P::EarlyCross(v),
            P::Diffusion(_) => P::Diffusion(v),
            P::EarlySend(_) => P::EarlySend(v),
            P::EarlyModRateHz(_) => P::EarlyModRateHz(v),
            P::EarlyModDepth(_) => P::EarlyModDepth(v),
            P::LateSize(_) => P::LateSize(v),
            P::LateCross(_) => P::LateCross(v),
            P::LateModRateHz(_) => P::LateModRateHz(v),
            P::LateModDepth(_) => P::LateModDepth(v),
            P::BassMult(_) => P::BassMult(v),
            P::BassXoverHz(_) => P::BassXoverHz(v),
            P::HighMult(_) => P::HighMult(v),
            P::HighXoverHz(_) => P::HighXoverHz(v),
        }
    }

    #[test]
    fn reverb_settings_serialise_like_this() {
        assert_eq!(
            serde_json::to_string(&ReverbParam::DecaySeconds(2.5)).unwrap(),
            r#"{"name":"decay_seconds","value":2.5}"#
        );
        assert_eq!(
            serde_json::to_string(&ReverbParam::Mode(ReverbMode::Room)).unwrap(),
            r#"{"name":"mode","value":"room"}"#
        );
        let json = serde_json::to_string(&ReverbSettings::default()).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"mode":"room","predelay_seconds":0.01,"decay_seconds":2.0,"depth":0.5,"#,
                r#""high_cut_hz":20000.0,"low_cut_hz":20.0,"early_size_seconds":0.03,"#,
                r#""early_cross":1.0,"diffusion":0.8,"early_send":0.5,"early_mod_rate_hz":0.5,"#,
                r#""early_mod_depth":0.0,"late_size":0.5,"late_cross":1.0,"late_mod_rate_hz":0.5,"#,
                r#""late_mod_depth":0.0,"bass_mult":1.0,"bass_xover_hz":500.0,"high_mult":0.5,"#,
                r#""high_xover_hz":4000.0}"#
            )
        );
        let back: ReverbSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ReverbSettings::default());
    }
}
