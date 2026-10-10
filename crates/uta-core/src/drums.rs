//! The drum machine's kit and its settings. See RFC-006, "The kit" and "In
//! the project".
//!
//! The kit is fixed: eight sounds, each played by its General MIDI drum
//! note, so a MIDI file or keyboard plays the right sound later. This file
//! is the one place the kit's rows are defined; the engine and the UI take
//! them from here.
//!
//! The engine keeps its own copy of the settings, with the same names,
//! units, ranges and defaults, so the snapshot only has to copy values
//! across.

use serde::{Deserialize, Serialize};

/// One sound of the kit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrumSound {
    Kick,
    Snare,
    Clap,
    LowTom,
    HighTom,
    ClosedHat,
    OpenHat,
    Cymbal,
}

/// One row of the kit: a sound, its name, and the note that plays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KitRow {
    pub sound: DrumSound,
    pub name: &'static str,
    /// The General MIDI drum note that plays it.
    pub pitch: u8,
}

/// The kit's rows, from the bottom of the piano roll to the top: kick at
/// C2 (36) to cymbal at C♯3 (49).
pub const KIT: [KitRow; 8] = [
    KitRow {
        sound: DrumSound::Kick,
        name: "Kick",
        pitch: 36,
    },
    KitRow {
        sound: DrumSound::Snare,
        name: "Snare",
        pitch: 38,
    },
    KitRow {
        sound: DrumSound::Clap,
        name: "Clap",
        pitch: 39,
    },
    KitRow {
        sound: DrumSound::LowTom,
        name: "Low tom",
        pitch: 45,
    },
    KitRow {
        sound: DrumSound::HighTom,
        name: "High tom",
        pitch: 50,
    },
    KitRow {
        sound: DrumSound::ClosedHat,
        name: "Closed hat",
        pitch: 42,
    },
    KitRow {
        sound: DrumSound::OpenHat,
        name: "Open hat",
        pitch: 46,
    },
    KitRow {
        sound: DrumSound::Cymbal,
        name: "Cymbal",
        pitch: 49,
    },
];

impl DrumSound {
    /// The sound `pitch` plays, if it's one of the kit's notes.
    pub fn at_pitch(pitch: u8) -> Option<Self> {
        KIT.iter()
            .find(|row| row.pitch == pitch)
            .map(|row| row.sound)
    }

    /// Its row in [`KIT`].
    pub fn row(self) -> &'static KitRow {
        KIT.iter()
            .find(|row| row.sound == self)
            .expect("every sound has a row")
    }
}

/// Every setting of a drum track's kit.
///
/// Serialises as `{"kick":{...}}`. Every sound may be left out, and takes
/// its defaults, so a sound added later doesn't stop older command lists
/// loading. The sounds that have no settings yet aren't here.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KitSettings {
    pub kick: KickSettings,
}

/// The 808 kick's settings: its front panel, plus Tune. See RFC-006, "The
/// kit".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KickSettings {
    /// The note it settles on, in Hz.
    pub tune_hz: f32,
    /// How bright it is, from 0 to 1: the low-pass after the resonator,
    /// which also lets more of the click through.
    pub tone: f32,
    /// How long it rings: the seconds it takes to die away by 40 dB, about
    /// where it's lost in a mix.
    pub decay_seconds: f32,
    /// Its level, in dB.
    pub level_db: f32,
}

impl KickSettings {
    pub const MIN_TUNE_HZ: f32 = 40.0;
    pub const MAX_TUNE_HZ: f32 = 80.0;
    pub const MIN_TONE: f32 = 0.0;
    pub const MAX_TONE: f32 = 1.0;
    pub const MIN_DECAY_SECONDS: f32 = 0.05;
    pub const MAX_DECAY_SECONDS: f32 = 0.8;
}

impl Default for KickSettings {
    fn default() -> Self {
        Self {
            tune_hz: 49.0,
            tone: 0.5,
            decay_seconds: 0.3,
            level_db: 0.0,
        }
    }
}

/// The quietest and loudest any sound's Level goes, in dB, as a track's
/// volume does.
pub const MIN_LEVEL_DB: f32 = -60.0;
pub const MAX_LEVEL_DB: f32 = 6.0;

/// One drum setting with its new value, as `SetDrumParam` carries it. Which
/// sound it's for travels beside it, and not every sound has every setting.
///
/// Serialises as `{"name":"tune_hz","value":55.0}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum DrumParam {
    TuneHz(f32),
    Tone(f32),
    DecaySeconds(f32),
    LevelDb(f32),
}

impl DrumParam {
    /// Whether `other` is the same setting, whatever its value.
    pub fn same_setting(&self, other: &DrumParam) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The value it carries.
    pub fn value(&self) -> f32 {
        match *self {
            Self::TuneHz(value)
            | Self::Tone(value)
            | Self::DecaySeconds(value)
            | Self::LevelDb(value) => value,
        }
    }
}

/// Why a drum setting couldn't be set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrumParamError {
    /// The sound doesn't have this setting.
    NoSuchSetting,
    /// The value is outside the setting's limits, or not a number. They're
    /// the limits it has on this sound.
    OutOfRange { min: f32, max: f32 },
}

impl KitSettings {
    /// The limits (inclusive) of `param` on `sound`, or `None` if the sound
    /// doesn't have that setting.
    pub fn range(sound: DrumSound, param: &DrumParam) -> Option<(f32, f32)> {
        use KickSettings as K;
        match (sound, param) {
            (DrumSound::Kick, DrumParam::TuneHz(_)) => Some((K::MIN_TUNE_HZ, K::MAX_TUNE_HZ)),
            (DrumSound::Kick, DrumParam::Tone(_)) => Some((K::MIN_TONE, K::MAX_TONE)),
            (DrumSound::Kick, DrumParam::DecaySeconds(_)) => {
                Some((K::MIN_DECAY_SECONDS, K::MAX_DECAY_SECONDS))
            }
            (DrumSound::Kick, DrumParam::LevelDb(_)) => Some((MIN_LEVEL_DB, MAX_LEVEL_DB)),
            _ => None,
        }
    }

    /// Every setting of every sound, as `SetDrumParam` carries them.
    pub fn params(&self) -> Vec<(DrumSound, DrumParam)> {
        let kick = &self.kick;
        vec![
            (DrumSound::Kick, DrumParam::TuneHz(kick.tune_hz)),
            (DrumSound::Kick, DrumParam::Tone(kick.tone)),
            (DrumSound::Kick, DrumParam::DecaySeconds(kick.decay_seconds)),
            (DrumSound::Kick, DrumParam::LevelDb(kick.level_db)),
        ]
    }

    /// Checks every setting is in range, or returns the first that isn't.
    pub(crate) fn validate(&self) -> Result<(), (DrumSound, DrumParam, DrumParamError)> {
        for (sound, param) in self.params() {
            check(sound, &param).map_err(|error| (sound, param, error))?;
        }
        Ok(())
    }

    /// Sets one setting of one sound and returns its old value, or an error
    /// if the sound has no such setting or the value is out of range. On
    /// error, nothing changes.
    pub(crate) fn set(
        &mut self,
        sound: DrumSound,
        param: DrumParam,
    ) -> Result<DrumParam, DrumParamError> {
        check(sound, &param)?;
        let kick = &mut self.kick;
        let slot = match (sound, param) {
            (DrumSound::Kick, DrumParam::TuneHz(_)) => &mut kick.tune_hz,
            (DrumSound::Kick, DrumParam::Tone(_)) => &mut kick.tone,
            (DrumSound::Kick, DrumParam::DecaySeconds(_)) => &mut kick.decay_seconds,
            (DrumSound::Kick, DrumParam::LevelDb(_)) => &mut kick.level_db,
            _ => unreachable!("check found the setting"),
        };
        let previous = std::mem::replace(slot, param.value());
        Ok(match param {
            DrumParam::TuneHz(_) => DrumParam::TuneHz(previous),
            DrumParam::Tone(_) => DrumParam::Tone(previous),
            DrumParam::DecaySeconds(_) => DrumParam::DecaySeconds(previous),
            DrumParam::LevelDb(_) => DrumParam::LevelDb(previous),
        })
    }
}

/// Whether `sound` has `param`, and its value is in range. NaN never is.
fn check(sound: DrumSound, param: &DrumParam) -> Result<(), DrumParamError> {
    let (min, max) = KitSettings::range(sound, param).ok_or(DrumParamError::NoSuchSetting)?;
    if (min..=max).contains(&param.value()) {
        Ok(())
    } else {
        Err(DrumParamError::OutOfRange { min, max })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn the_kit_has_eight_rows_from_kick_at_36_to_cymbal_at_49() {
        assert_eq!(KIT.len(), 8);
        assert_eq!((KIT[0].name, KIT[0].pitch), ("Kick", 36));
        assert_eq!((KIT[7].name, KIT[7].pitch), ("Cymbal", 49));
        let pitches: HashSet<u8> = KIT.iter().map(|row| row.pitch).collect();
        let sounds: HashSet<DrumSound> = KIT.iter().map(|row| row.sound).collect();
        assert_eq!(pitches.len(), 8, "one note per row");
        assert_eq!(sounds.len(), 8, "one row per sound");
        for row in KIT {
            assert_eq!(DrumSound::at_pitch(row.pitch), Some(row.sound));
            assert_eq!(row.sound.row(), &row);
        }
        assert_eq!(DrumSound::at_pitch(37), None);
        assert_eq!(DrumSound::at_pitch(60), None);
    }

    #[test]
    fn the_kicks_defaults_and_limits_are_the_rfcs() {
        let kick = KickSettings::default();
        assert_eq!((kick.tune_hz, kick.decay_seconds), (49.0, 0.3));
        assert_eq!(kick.level_db, 0.0);
        let range = |param| KitSettings::range(DrumSound::Kick, &param);
        assert_eq!(range(DrumParam::TuneHz(0.0)), Some((40.0, 80.0)));
        assert_eq!(range(DrumParam::DecaySeconds(0.0)), Some((0.05, 0.8)));
        assert!(KitSettings::default().validate().is_ok());
    }

    #[test]
    fn set_returns_the_old_value_and_limits_are_inclusive() {
        let mut kit = KitSettings::default();
        assert_eq!(
            kit.set(DrumSound::Kick, DrumParam::TuneHz(55.0)),
            Ok(DrumParam::TuneHz(49.0))
        );
        assert_eq!(kit.kick.tune_hz, 55.0);
        for param in [
            DrumParam::TuneHz(40.0),
            DrumParam::TuneHz(80.0),
            DrumParam::Tone(0.0),
            DrumParam::Tone(1.0),
            DrumParam::DecaySeconds(0.05),
            DrumParam::DecaySeconds(0.8),
            DrumParam::LevelDb(-60.0),
            DrumParam::LevelDb(6.0),
        ] {
            assert!(kit.set(DrumSound::Kick, param).is_ok(), "{param:?}");
        }
    }

    #[test]
    fn out_of_range_values_and_missing_settings_are_refused_and_change_nothing() {
        let mut kit = KitSettings::default();
        for param in [
            DrumParam::TuneHz(39.9),
            DrumParam::TuneHz(f32::NAN),
            DrumParam::Tone(1.01),
            DrumParam::DecaySeconds(0.0),
            DrumParam::LevelDb(f32::INFINITY),
        ] {
            assert!(matches!(
                kit.set(DrumSound::Kick, param),
                Err(DrumParamError::OutOfRange { .. })
            ));
        }
        // The other sounds have no settings until their tickets.
        assert_eq!(
            kit.set(DrumSound::Snare, DrumParam::TuneHz(200.0)),
            Err(DrumParamError::NoSuchSetting)
        );
        assert_eq!(kit, KitSettings::default());
    }

    #[test]
    fn settings_serialise_and_leave_out_nothing_they_need() {
        let json = r#"{"kick":{"tune_hz":49.0,"tone":0.5,"decay_seconds":0.3,"level_db":0.0}}"#;
        assert_eq!(
            serde_json::to_string(&KitSettings::default()).unwrap(),
            json
        );
        // A kit, or a sound, can be left out and takes its defaults.
        let empty: KitSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, KitSettings::default());
        let tuned: KitSettings = serde_json::from_str(r#"{"kick":{"tune_hz":60.0}}"#).unwrap();
        assert_eq!(tuned.kick.tune_hz, 60.0);
        assert_eq!(tuned.kick.decay_seconds, 0.3);
        assert!(serde_json::from_str::<KitSettings>(r#"{"cowbell":{}}"#).is_err());
    }
}
