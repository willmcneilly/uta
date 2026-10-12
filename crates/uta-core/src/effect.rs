//! Insert effects and the chains that hold them. See RFC-007, "The chain"
//! and "Patterns for every effect after this one", point 1.
//!
//! An effect is its permanent ID, its settings (which say its kind), an
//! on/off switch and a Mix. A chain is an ordered list of up to
//! [`MAX_EFFECTS`] of them, on a track or the master. It belongs to the
//! place it sits rather than assuming it's a track's, so later a return or a
//! group can have one too: [`Chain`] names which.

use serde::{Deserialize, Serialize};

use crate::setting::{Choice, Description, Scale, Unit};
use crate::time::NoteDivision;
use crate::{EffectId, TrackId};

/// The most effects a chain holds. The audio thread sets aside a table of
/// this many effect states for each chain when it starts, and its work per
/// block is bounded by it. Raising it later is one number and its tests;
/// lowering it would stop projects with more from loading (RFC-007, open
/// question 5).
pub const MAX_EFFECTS: usize = 8;

/// Which chain a command means. Serialises as `{"track":"<id>"}` or
/// `"master"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chain {
    /// The chain between a track's source and its mixer strip.
    Track(TrackId),
    /// The master's chain, between the sum of the tracks and the master
    /// volume.
    Master,
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

/// The Delay's settings: a tape echo. See RFC-007, "The Delay".
///
/// Serialises with the same names as [`DelayParam`]. Command formats 5 to 7
/// had only `time_seconds` and `feedback`: the settings they didn't have
/// read as the plain Delay they made, unsynced, with its cuts wide open and
/// no Drive or Wobble ([`DelaySettings::plain`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelaySettings {
    /// Whether the time follows the tempo: [`Self::division`] when it does,
    /// [`Self::time_seconds`] when it doesn't.
    #[serde(default)]
    pub sync: bool,
    /// The echoes' spacing as a note length, when synced.
    #[serde(default = "plain_division")]
    pub division: NoteDivision,
    /// The echoes' spacing when not synced, from 1 ms to 5 s.
    pub time_seconds: f32,
    #[serde(default)]
    pub mode: DelayMode,
    /// How much of each echo comes round again, from 0 to 1.1. Past 1, the
    /// echoes grow until the tape's saturation holds them.
    pub feedback: f32,
    /// The high-pass filter in the feedback loop, in Hz: each echo is
    /// thinner than the last.
    #[serde(default = "plain_low_cut")]
    pub low_cut_hz: f32,
    /// The low-pass filter in the feedback loop, in Hz: each echo is darker
    /// than the last.
    #[serde(default = "plain_high_cut")]
    pub high_cut_hz: f32,
    /// How hard the echoes hit the tape's saturation, from 0 (clean) to 1.
    #[serde(default)]
    pub drive: f32,
    /// How much the playback head wavers, from 0 (exactly on time) to 1.
    #[serde(default)]
    pub wobble: f32,
}

fn plain_division() -> NoteDivision {
    DelaySettings::default().division
}

fn plain_low_cut() -> f32 {
    DelaySettings::MIN_LOW_CUT_HZ
}

fn plain_high_cut() -> f32 {
    DelaySettings::MAX_HIGH_CUT_HZ
}

impl DelaySettings {
    /// The shortest time: 1 ms, for slapback and effects off the grid.
    pub const MIN_TIME_SECONDS: f32 = 0.001;
    /// The longest time: the length of the tape loop the engine sets aside.
    /// A synced time longer than this is held at it.
    pub const MAX_TIME_SECONDS: f32 = 5.0;
    pub const MIN_FEEDBACK: f32 = 0.0;
    /// Past 100%, the echoes grow until the saturation in the loop holds
    /// them: a sustained howl, not an ever louder blast. Bitwig's Delay+
    /// goes to 122% the same way.
    pub const MAX_FEEDBACK: f32 = 1.1;
    pub const MIN_LOW_CUT_HZ: f32 = 20.0;
    pub const MAX_LOW_CUT_HZ: f32 = 2_000.0;
    pub const MIN_HIGH_CUT_HZ: f32 = 500.0;
    pub const MAX_HIGH_CUT_HZ: f32 = 20_000.0;
    pub const MIN_AMOUNT: f32 = 0.0;
    pub const MAX_AMOUNT: f32 = 1.0;

    /// The plain Delay of command formats 5 to 7: `time_seconds` apart,
    /// unsynced, in stereo, with its cuts wide open and no Drive or Wobble.
    pub fn plain(time_seconds: f32, feedback: f32) -> Self {
        Self {
            sync: false,
            division: plain_division(),
            time_seconds,
            mode: DelayMode::Stereo,
            feedback,
            low_cut_hz: Self::MIN_LOW_CUT_HZ,
            high_cut_hz: Self::MAX_HIGH_CUT_HZ,
            drive: 0.0,
            wobble: 0.0,
        }
    }

    /// The echoes' spacing in seconds at `bpm`: the division's length when
    /// synced, held at [`Self::MAX_TIME_SECONDS`], or the time in seconds
    /// when not.
    pub fn time_seconds_at(&self, bpm: f32) -> f32 {
        if self.sync {
            (self.division.seconds_at(bpm) as f32)
                .clamp(Self::MIN_TIME_SECONDS, Self::MAX_TIME_SECONDS)
        } else {
            self.time_seconds
        }
    }

    /// Every setting, as `SetDelayParam` carries them, in the panel's
    /// order.
    pub fn params(&self) -> [DelayParam; 9] {
        [
            DelayParam::Sync(self.sync),
            DelayParam::Division(self.division),
            DelayParam::TimeSeconds(self.time_seconds),
            DelayParam::Mode(self.mode),
            DelayParam::Feedback(self.feedback),
            DelayParam::LowCutHz(self.low_cut_hz),
            DelayParam::HighCutHz(self.high_cut_hz),
            DelayParam::Drive(self.drive),
            DelayParam::Wobble(self.wobble),
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
        use std::mem::replace;
        Ok(match param {
            DelayParam::Sync(on) => DelayParam::Sync(replace(&mut self.sync, on)),
            DelayParam::Division(division) => {
                DelayParam::Division(replace(&mut self.division, division))
            }
            DelayParam::TimeSeconds(value) => {
                DelayParam::TimeSeconds(replace(&mut self.time_seconds, value))
            }
            DelayParam::Mode(mode) => DelayParam::Mode(replace(&mut self.mode, mode)),
            DelayParam::Feedback(value) => DelayParam::Feedback(replace(&mut self.feedback, value)),
            DelayParam::LowCutHz(hz) => DelayParam::LowCutHz(replace(&mut self.low_cut_hz, hz)),
            DelayParam::HighCutHz(hz) => DelayParam::HighCutHz(replace(&mut self.high_cut_hz, hz)),
            DelayParam::Drive(value) => DelayParam::Drive(replace(&mut self.drive, value)),
            DelayParam::Wobble(value) => DelayParam::Wobble(replace(&mut self.wobble, value)),
        })
    }
}

impl Default for DelaySettings {
    /// RFC-007's starting point: a dotted eighth, synced, 40% feedback,
    /// gentle cuts, a little Drive and a little Wobble. Tuned by ear in a
    /// real song in UTA-77.
    fn default() -> Self {
        Self {
            sync: true,
            division: NoteDivision::DOTTED_EIGHTH,
            // A dotted eighth at the default 120 BPM, for when Sync goes off.
            time_seconds: 0.375,
            mode: DelayMode::Stereo,
            feedback: 0.4,
            low_cut_hz: 100.0,
            high_cut_hz: 8_000.0,
            drive: 0.2,
            wobble: 0.1,
        }
    }
}

/// How the Delay's echoes sit across the two sides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelayMode {
    /// Each side echoes on itself.
    #[default]
    Stereo,
    /// Both sides go into the left tape, and each echo crosses to the
    /// other side: left, right, left.
    PingPong,
}

impl DelayMode {
    /// The mode as `SetDelayParam` carries it: `"ping_pong"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::Stereo => "stereo",
            Self::PingPong => "ping_pong",
        }
    }
}

/// Every mode, as the panel offers them.
const DELAY_MODES: [Choice; 2] = [
    Choice {
        value: "stereo",
        label: "Stereo",
    },
    Choice {
        value: "ping_pong",
        label: "Ping-pong",
    },
];

/// Every note division, as a synced time's menu offers them.
const DIVISIONS: [Choice; 18] = {
    let mut choices = [Choice {
        value: "",
        label: "",
    }; 18];
    let mut index = 0;
    while index < 18 {
        let division = NoteDivision::ALL[index];
        choices[index] = Choice {
            value: division.value(),
            label: division.label(),
        };
        index += 1;
    }
    choices
};

/// One Delay setting with its new value, as `SetDelayParam` carries it.
///
/// Serialises as `{"name":"feedback","value":0.5}`, `{"name":"sync",
/// "value":true}`, `{"name":"division","value":"1/8d"}` or
/// `{"name":"mode","value":"ping_pong"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum DelayParam {
    Sync(bool),
    Division(NoteDivision),
    TimeSeconds(f32),
    Mode(DelayMode),
    Feedback(f32),
    LowCutHz(f32),
    HighCutHz(f32),
    Drive(f32),
    Wobble(f32),
}

impl DelayParam {
    /// Whether `other` is the same setting, whatever its value.
    pub fn same_setting(&self, other: &DelayParam) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The first command format that has this setting.
    pub(crate) fn since_format(&self) -> u32 {
        match self {
            Self::TimeSeconds(_) | Self::Feedback(_) => 5,
            _ => 8,
        }
    }

    /// The setting's value and its limits (inclusive), or `None` for Sync,
    /// a division or a mode, which have no range.
    pub fn range(&self) -> Option<(f32, f32, f32)> {
        use DelaySettings as S;
        match *self {
            Self::Sync(_) | Self::Division(_) | Self::Mode(_) => None,
            Self::TimeSeconds(seconds) => Some((seconds, S::MIN_TIME_SECONDS, S::MAX_TIME_SECONDS)),
            Self::Feedback(amount) => Some((amount, S::MIN_FEEDBACK, S::MAX_FEEDBACK)),
            Self::LowCutHz(hz) => Some((hz, S::MIN_LOW_CUT_HZ, S::MAX_LOW_CUT_HZ)),
            Self::HighCutHz(hz) => Some((hz, S::MIN_HIGH_CUT_HZ, S::MAX_HIGH_CUT_HZ)),
            Self::Drive(amount) | Self::Wobble(amount) => {
                Some((amount, S::MIN_AMOUNT, S::MAX_AMOUNT))
            }
        }
    }

    /// The setting described: its label, how it sounds, its unit, range,
    /// scale and default, or its choices. The value it carries doesn't
    /// matter.
    pub fn describe(&self) -> Description {
        let defaults = DelaySettings::default();
        let default = defaults
            .params()
            .into_iter()
            .find(|param| param.same_setting(self))
            .expect("every setting has a default");
        let (value, min, max) = default.range().unwrap_or_default();
        let number = |label, sounds, unit, scale| {
            Description::number(label, sounds, unit, scale, (min, max), value)
        };
        match self {
            Self::Sync(_) => Description::toggle(
                "Sync",
                "Whether the time follows the tempo. On, it's a note length, so \
                 the echoes stay in time with the song and drift with it when \
                 the tempo changes; off, it's in milliseconds, for slapback and \
                 echoes off the grid.",
                defaults.sync,
            ),
            Self::Division(_) => Description::choice(
                "Time",
                "How far apart the echoes are, as a note length. A dotted \
                 eighth weaves between the beats; a triplet swings against \
                 them; a quarter or longer is a separate, rhythmic repeat.",
                &DIVISIONS,
                defaults.division.value(),
            ),
            Self::TimeSeconds(_) => number(
                "Time",
                "How far apart the echoes are. A few milliseconds thickens the \
                 sound into a slapback or a buzz; a quarter of a second is a \
                 rhythmic echo; seconds are long, separate repeats. Turning it \
                 while echoes are on their way bends their pitch, as on tape.",
                Unit::Seconds,
                Scale::Log,
            ),
            Self::Mode(_) => Description::choice(
                "Mode",
                "Where the echoes sit. Stereo echoes each side on itself; \
                 ping-pong bounces them left, right, left, even from a mono \
                 sound.",
                &DELAY_MODES,
                defaults.mode.value(),
            ),
            Self::Feedback(_) => number(
                "Feedback",
                "How much of each echo comes round again, so how many repeats \
                 you hear. None is a single echo; a little is a few that fade \
                 quickly; all of it barely fades at all. Past that they swell \
                 until the tape holds them, into a dub howl.",
                Unit::Fraction,
                Scale::Linear,
            ),
            Self::LowCutHz(_) => number(
                "Low cut",
                "Thins each repeat a little more than the last, taking away the \
                 lows below it, so echoes stay out of the way of the bass and \
                 the kick.",
                Unit::Hz,
                Scale::Log,
            ),
            Self::HighCutHz(_) => number(
                "High cut",
                "Darkens each repeat a little more than the last, taking away \
                 the highs above it, so the echoes fade into murk like tape's. \
                 All the way up, they stay bright.",
                Unit::Hz,
                Scale::Log,
            ),
            Self::Drive(_) => number(
                "Drive",
                "How hard the echoes hit the tape. None is clean; a little \
                 warms and rounds them; a lot squashes and crunches them, and \
                 holds high feedback at a steady roar.",
                Unit::Fraction,
                Scale::Linear,
            ),
            Self::Wobble(_) => number(
                "Wobble",
                "Wow and flutter: the slow sway and fast waver in pitch of a \
                 worn tape machine. A little makes the echoes feel analogue; a \
                 lot makes them seasick.",
                Unit::Fraction,
                Scale::Linear,
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

    #[test]
    fn a_new_delay_has_rfc_007s_starting_point() {
        let effect = Effect::default_of(EffectId::random(), EffectKind::Delay);
        assert!(effect.enabled());
        assert_eq!(effect.mix(), 0.3);
        let EffectSettings::Delay(settings) = *effect.settings();
        assert!(settings.sync);
        assert_eq!(settings.division, NoteDivision::DOTTED_EIGHTH);
        assert_eq!(settings.mode, DelayMode::Stereo);
        assert_eq!(settings.feedback, 0.4);
        assert_eq!(settings.time_seconds_at(120.0), 0.375);
    }

    #[test]
    fn every_delay_setting_and_mix_is_described_with_its_range_and_default() {
        let settings = DelaySettings::default();
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
                        assert_eq!(limits, (min, max), "{param:?}");
                        assert_eq!(default, value, "{param:?}");
                        format!("{unit:?} {scale:?}")
                    }
                    (Kind::Choice { options, default }, None) => {
                        let value = match param {
                            DelayParam::Division(division) => division.value(),
                            DelayParam::Mode(mode) => mode.value(),
                            _ => panic!("{param:?} is a choice"),
                        };
                        assert_eq!(default, value, "{param:?}");
                        format!("{} choices", options.len())
                    }
                    (Kind::Toggle { default }, None) => {
                        assert_eq!(default, settings.sync);
                        "toggle".to_string()
                    }
                    (kind, range) => panic!("{param:?}: {kind:?} with {range:?}"),
                };
                (description.label, kind)
            })
            .collect();
        let expected: Vec<(&str, String)> = [
            ("Sync", "toggle"),
            ("Time", "18 choices"),
            ("Time", "Seconds Log"),
            ("Mode", "2 choices"),
            ("Feedback", "Fraction Linear"),
            ("Low cut", "Hz Log"),
            ("High cut", "Hz Log"),
            ("Drive", "Fraction Linear"),
            ("Wobble", "Fraction Linear"),
        ]
        .into_iter()
        .map(|(label, kind)| (label, kind.to_string()))
        .collect();
        assert_eq!(described, expected);
        let mix = Effect::default_of(EffectId::random(), EffectKind::Delay).describe_mix();
        assert_eq!(mix.label, "Mix");
        assert_eq!(
            mix.kind,
            Kind::Number {
                unit: Unit::Fraction,
                scale: Scale::Linear,
                limits: (0.0, 1.0),
                default: 0.3,
                step: None,
            }
        );
    }

    /// The synced time's menu is every division, shortest note first, with
    /// the values a command carries.
    #[test]
    fn the_synced_times_choices_are_every_division() {
        let Kind::Choice { options, .. } = DelayParam::Division(NoteDivision::DOTTED_EIGHTH)
            .describe()
            .kind
        else {
            panic!("the synced time is a choice");
        };
        let values: Vec<_> = options.iter().map(|choice| choice.value).collect();
        let all: Vec<_> = NoteDivision::ALL.iter().map(|d| d.value()).collect();
        assert_eq!(values, all);
        assert_eq!(options[8].label, "1/8 dotted");
    }

    #[test]
    fn delay_limits_are_inclusive_and_out_of_range_changes_nothing() {
        let mut settings = DelaySettings::default();
        for param in [
            DelayParam::TimeSeconds(0.001),
            DelayParam::TimeSeconds(5.0),
            DelayParam::Feedback(0.0),
            DelayParam::Feedback(1.1),
            DelayParam::LowCutHz(20.0),
            DelayParam::LowCutHz(2_000.0),
            DelayParam::HighCutHz(500.0),
            DelayParam::HighCutHz(20_000.0),
            DelayParam::Drive(0.0),
            DelayParam::Drive(1.0),
            DelayParam::Wobble(0.0),
            DelayParam::Wobble(1.0),
            DelayParam::Sync(false),
            DelayParam::Mode(DelayMode::PingPong),
        ] {
            assert!(settings.set(param).is_ok(), "{param:?} was rejected");
        }
        let before = settings;
        for param in [
            DelayParam::TimeSeconds(0.0009),
            DelayParam::TimeSeconds(5.01),
            DelayParam::TimeSeconds(f32::NAN),
            DelayParam::Feedback(-0.01),
            DelayParam::Feedback(1.11),
            DelayParam::LowCutHz(19.0),
            DelayParam::LowCutHz(2_001.0),
            DelayParam::HighCutHz(499.0),
            DelayParam::HighCutHz(20_001.0),
            DelayParam::Drive(1.01),
            DelayParam::Wobble(-0.01),
            DelayParam::Wobble(f32::NAN),
        ] {
            assert!(settings.set(param).is_err(), "{param:?} was accepted");
            assert_eq!(settings, before);
        }
    }

    /// Synced, the time is the division at the tempo, held at 5 s: a half
    /// note at 40 BPM is 3 s, but at 20 BPM it would be 6 s. Off, it's the
    /// time in seconds, whatever the tempo.
    #[test]
    fn a_synced_time_follows_the_tempo_and_is_held_at_5_seconds() {
        let mut settings = DelaySettings::default();
        assert_eq!(settings.time_seconds_at(120.0), 0.375);
        assert_eq!(settings.time_seconds_at(60.0), 0.75);
        settings.division = NoteDivision::from_value("1/2").unwrap();
        assert_eq!(settings.time_seconds_at(40.0), 3.0);
        assert_eq!(settings.time_seconds_at(20.0), 5.0);
        settings.sync = false;
        settings.time_seconds = 0.2;
        assert_eq!(settings.time_seconds_at(40.0), 0.2);
        assert_eq!(settings.time_seconds_at(300.0), 0.2);
    }

    #[test]
    fn a_chain_and_settings_serialise_like_this() {
        let id = TrackId::random();
        assert_eq!(
            serde_json::to_string(&Chain::Track(id)).unwrap(),
            format!(r#"{{"track":"{id}"}}"#)
        );
        assert_eq!(
            serde_json::to_string(&Chain::Master).unwrap(),
            r#""master""#
        );
        assert_eq!(
            serde_json::to_string(&DelayParam::Feedback(0.5)).unwrap(),
            r#"{"name":"feedback","value":0.5}"#
        );
        assert_eq!(
            serde_json::to_string(&DelayParam::Division(NoteDivision::DOTTED_EIGHTH)).unwrap(),
            r#"{"name":"division","value":"1/8d"}"#
        );
        assert_eq!(
            serde_json::to_string(&DelayParam::Mode(DelayMode::PingPong)).unwrap(),
            r#"{"name":"mode","value":"ping_pong"}"#
        );
        assert_eq!(
            serde_json::to_string(&EffectSettings::default_of(EffectKind::Delay)).unwrap(),
            r#"{"delay":{"sync":true,"division":"1/8d","time_seconds":0.375,"mode":"stereo","feedback":0.4,"low_cut_hz":100.0,"high_cut_hz":8000.0,"drive":0.2,"wobble":0.1}}"#
        );
    }

    /// Command formats 5 to 7 carried only the time and Feedback: they read
    /// as the plain Delay they made.
    #[test]
    fn a_delay_from_before_its_tape_loop_reads_as_the_plain_delay() {
        let old: EffectSettings =
            serde_json::from_str(r#"{"delay":{"time_seconds":0.25,"feedback":0.6}}"#).unwrap();
        assert_eq!(old, EffectSettings::Delay(DelaySettings::plain(0.25, 0.6)));
        let EffectSettings::Delay(plain) = old;
        assert!(!plain.sync);
        assert_eq!(plain.time_seconds_at(60.0), 0.25);
        assert_eq!((plain.low_cut_hz, plain.high_cut_hz), (20.0, 20_000.0));
        assert_eq!((plain.drive, plain.wobble), (0.0, 0.0));
        assert_eq!(plain.mode, DelayMode::Stereo);
    }
}
