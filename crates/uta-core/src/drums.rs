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
/// Serialises as `{"kick":{...},"snare":{...},"clap":{...},
/// "closed_hat":{...},"open_hat":{...},"low_tom":{...},"high_tom":{...}}`.
/// Every sound may be left out, and
/// takes its defaults, so a sound added later doesn't stop older command
/// lists loading. The sounds that have no settings yet aren't here.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KitSettings {
    pub kick: KickSettings,
    pub snare: SnareSettings,
    pub clap: ClapSettings,
    pub closed_hat: ClosedHatSettings,
    pub open_hat: OpenHatSettings,
    pub low_tom: LowTomSettings,
    pub high_tom: HighTomSettings,
}

/// The 808 kick's settings: its front panel, plus Tune. See RFC-006, "The
/// kit".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KickSettings {
    /// The note it settles on, in Hz.
    pub tune_hz: f32,
    /// How bright it is, from 0 to 1: the low-pass after the resonator,
    /// from 200 Hz to 8 kHz on a log scale, which also lets more of the
    /// click through.
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
            // Round and deep, with a soft click: about a 420 Hz low-pass.
            tone: 0.2,
            decay_seconds: 0.3,
            level_db: 0.0,
        }
    }
}

/// The 909 snare's settings: its front panel. See RFC-006, "The kit".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SnareSettings {
    /// The shell's pitch, in Hz: the lower of its two tones.
    pub tune_hz: f32,
    /// Tone, as on the 909: the wires' length, in seconds. It's how long
    /// the wires take to die away by 40 dB once their hold ends.
    pub tone_seconds: f32,
    /// How much rattle there is against the shell, from 0 (all shell) to 1
    /// (all wires).
    pub snappy: f32,
    /// Its level, in dB.
    pub level_db: f32,
}

impl SnareSettings {
    pub const MIN_TUNE_HZ: f32 = 140.0;
    pub const MAX_TUNE_HZ: f32 = 260.0;
    pub const MIN_TONE_SECONDS: f32 = 0.04;
    pub const MAX_TONE_SECONDS: f32 = 0.4;
    pub const MIN_SNAPPY: f32 = 0.0;
    pub const MAX_SNAPPY: f32 = 1.0;
}

impl Default for SnareSettings {
    fn default() -> Self {
        Self {
            // The 909's shell, from the research's recipe.
            tune_hz: 180.0,
            // A tight 909 snare: the wires hold, then are gone in a little
            // over a fifth of a second.
            tone_seconds: 0.16,
            snappy: 0.5,
            level_db: 0.0,
        }
    }
}

/// The 808 clap's settings: its front panel, plus Tone and Decay. See
/// RFC-006, "The kit".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClapSettings {
    /// Tone: the band-pass's centre, in Hz.
    pub tone_hz: f32,
    /// How long the tail (the "reverb") rings: the seconds it takes to die
    /// away by 40 dB.
    pub decay_seconds: f32,
    /// Its level, in dB.
    pub level_db: f32,
}

impl ClapSettings {
    pub const MIN_TONE_HZ: f32 = 700.0;
    pub const MAX_TONE_HZ: f32 = 2000.0;
    pub const MIN_DECAY_SECONDS: f32 = 0.05;
    pub const MAX_DECAY_SECONDS: f32 = 0.4;
}

impl Default for ClapSettings {
    fn default() -> Self {
        Self {
            // The 808's band-pass, from the research's recipe.
            tone_hz: 1000.0,
            // A tail you hear for about 100 ms, as on the 808.
            decay_seconds: 0.2,
            level_db: 0.0,
        }
    }
}

/// The 808 closed hat's settings: its front panel, plus Tune, Tone and
/// Decay. Its Tune and Tone are the open hat's too: the two hats are one
/// circuit on the 808, which is how a closed hit cuts off a ringing open
/// hat. See RFC-006, "The kit".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClosedHatSettings {
    /// The metal's pitch: the lowest of its six oscillators, in Hz. The
    /// other five keep the 808's ratios to it.
    pub tune_hz: f32,
    /// Tone: where the filters that keep the metal's top end sit, in Hz.
    pub tone_hz: f32,
    /// How long it rings: the seconds it takes to die away by 40 dB.
    pub decay_seconds: f32,
    /// Its level, in dB.
    pub level_db: f32,
}

impl ClosedHatSettings {
    /// The 808's lowest oscillator, and an octave either side of it.
    pub const DEFAULT_TUNE_HZ: f32 = 205.3;
    pub const MIN_TUNE_HZ: f32 = Self::DEFAULT_TUNE_HZ / 2.0;
    pub const MAX_TUNE_HZ: f32 = Self::DEFAULT_TUNE_HZ * 2.0;
    pub const MIN_TONE_HZ: f32 = 4000.0;
    pub const MAX_TONE_HZ: f32 = 12_000.0;
    pub const MIN_DECAY_SECONDS: f32 = 0.02;
    pub const MAX_DECAY_SECONDS: f32 = 0.15;
}

impl Default for ClosedHatSettings {
    fn default() -> Self {
        Self {
            tune_hz: Self::DEFAULT_TUNE_HZ,
            // The 808's band-pass, from the research's recipe.
            tone_hz: 7100.0,
            // The 808's fixed closed hat.
            decay_seconds: 0.05,
            level_db: 0.0,
        }
    }
}

/// The 808 open hat's settings: its front panel. Its Tune and Tone are the
/// closed hat's. See RFC-006, "The kit".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenHatSettings {
    /// How long it rings: the seconds it takes to die away by 40 dB.
    pub decay_seconds: f32,
    /// Its level, in dB.
    pub level_db: f32,
}

impl OpenHatSettings {
    /// The 808's Decay knob.
    pub const MIN_DECAY_SECONDS: f32 = 0.09;
    pub const MAX_DECAY_SECONDS: f32 = 0.6;
}

impl Default for OpenHatSettings {
    fn default() -> Self {
        Self {
            // A little past the knob's middle: a hat you hear ring for about
            // half a second.
            decay_seconds: 0.35,
            level_db: 0.0,
        }
    }
}

/// The 808 low tom's settings: its front panel, plus Tune and Decay (the
/// 808's toms have neither knob; the 909's have both). See RFC-006, "The
/// kit".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LowTomSettings {
    /// The note it settles on, in Hz.
    pub tune_hz: f32,
    /// How long it rings: the seconds it takes to die away by 40 dB.
    pub decay_seconds: f32,
    /// Its level, in dB.
    pub level_db: f32,
}

impl LowTomSettings {
    /// The 808's own range for its low tom.
    pub const MIN_TUNE_HZ: f32 = 80.0;
    pub const MAX_TUNE_HZ: f32 = 100.0;
    pub const MIN_DECAY_SECONDS: f32 = TOM_MIN_DECAY_SECONDS;
    pub const MAX_DECAY_SECONDS: f32 = TOM_MAX_DECAY_SECONDS;
}

impl Default for LowTomSettings {
    fn default() -> Self {
        Self {
            // The 808's low tom, from the research's recipe.
            tune_hz: 90.0,
            decay_seconds: 0.2,
            level_db: 0.0,
        }
    }
}

/// The 808 high tom's settings: its front panel, plus Tune and Decay. See
/// [`LowTomSettings`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HighTomSettings {
    /// The note it settles on, in Hz.
    pub tune_hz: f32,
    /// How long it rings: the seconds it takes to die away by 40 dB.
    pub decay_seconds: f32,
    /// Its level, in dB.
    pub level_db: f32,
}

impl HighTomSettings {
    /// The 808's own range for its high tom.
    pub const MIN_TUNE_HZ: f32 = 165.0;
    pub const MAX_TUNE_HZ: f32 = 220.0;
    pub const MIN_DECAY_SECONDS: f32 = TOM_MIN_DECAY_SECONDS;
    pub const MAX_DECAY_SECONDS: f32 = TOM_MAX_DECAY_SECONDS;
}

impl Default for HighTomSettings {
    fn default() -> Self {
        Self {
            // The 808's high tom, from the research's recipe.
            tune_hz: 185.0,
            decay_seconds: 0.1,
            level_db: 0.0,
        }
    }
}

/// Both toms' Decay range, from the research's recipe.
const TOM_MIN_DECAY_SECONDS: f32 = 0.1;
const TOM_MAX_DECAY_SECONDS: f32 = 0.6;

/// The quietest and loudest any sound's Level goes, in dB, as a track's
/// volume does.
pub const MIN_LEVEL_DB: f32 = -60.0;
pub const MAX_LEVEL_DB: f32 = 6.0;

/// One drum setting with its new value, as `SetDrumParam` carries it. Which
/// sound it's for travels beside it, and not every sound has every setting.
///
/// Serialises as `{"name":"tune_hz","value":55.0}`.
///
/// Each value is in its sound's own unit. Tone is the one that differs: from
/// 0 to 1 on the kick, the wires' length in seconds on the snare, and the
/// filters' centre in Hz on the clap and the closed hat, as each sound's
/// settings say. The toms have no Tone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum DrumParam {
    TuneHz(f32),
    Tone(f32),
    DecaySeconds(f32),
    Snappy(f32),
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
            | Self::Snappy(value)
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
        use ClapSettings as C;
        use ClosedHatSettings as CH;
        use DrumParam as P;
        use DrumSound as D;
        use HighTomSettings as HT;
        use KickSettings as K;
        use LowTomSettings as LT;
        use OpenHatSettings as OH;
        use SnareSettings as S;
        match (sound, param) {
            (D::Kick, P::TuneHz(_)) => Some((K::MIN_TUNE_HZ, K::MAX_TUNE_HZ)),
            (D::Kick, P::Tone(_)) => Some((K::MIN_TONE, K::MAX_TONE)),
            (D::Kick, P::DecaySeconds(_)) => Some((K::MIN_DECAY_SECONDS, K::MAX_DECAY_SECONDS)),
            (D::Snare, P::TuneHz(_)) => Some((S::MIN_TUNE_HZ, S::MAX_TUNE_HZ)),
            (D::Snare, P::Tone(_)) => Some((S::MIN_TONE_SECONDS, S::MAX_TONE_SECONDS)),
            (D::Snare, P::Snappy(_)) => Some((S::MIN_SNAPPY, S::MAX_SNAPPY)),
            (D::Clap, P::Tone(_)) => Some((C::MIN_TONE_HZ, C::MAX_TONE_HZ)),
            (D::Clap, P::DecaySeconds(_)) => Some((C::MIN_DECAY_SECONDS, C::MAX_DECAY_SECONDS)),
            (D::ClosedHat, P::TuneHz(_)) => Some((CH::MIN_TUNE_HZ, CH::MAX_TUNE_HZ)),
            (D::ClosedHat, P::Tone(_)) => Some((CH::MIN_TONE_HZ, CH::MAX_TONE_HZ)),
            (D::ClosedHat, P::DecaySeconds(_)) => {
                Some((CH::MIN_DECAY_SECONDS, CH::MAX_DECAY_SECONDS))
            }
            (D::OpenHat, P::DecaySeconds(_)) => {
                Some((OH::MIN_DECAY_SECONDS, OH::MAX_DECAY_SECONDS))
            }
            (D::LowTom, P::TuneHz(_)) => Some((LT::MIN_TUNE_HZ, LT::MAX_TUNE_HZ)),
            (D::LowTom, P::DecaySeconds(_)) => Some((LT::MIN_DECAY_SECONDS, LT::MAX_DECAY_SECONDS)),
            (D::HighTom, P::TuneHz(_)) => Some((HT::MIN_TUNE_HZ, HT::MAX_TUNE_HZ)),
            (D::HighTom, P::DecaySeconds(_)) => {
                Some((HT::MIN_DECAY_SECONDS, HT::MAX_DECAY_SECONDS))
            }
            (
                D::Kick | D::Snare | D::Clap | D::ClosedHat | D::OpenHat | D::LowTom | D::HighTom,
                P::LevelDb(_),
            ) => Some((MIN_LEVEL_DB, MAX_LEVEL_DB)),
            _ => None,
        }
    }

    /// Every setting of every sound, as `SetDrumParam` carries them.
    pub fn params(&self) -> Vec<(DrumSound, DrumParam)> {
        let (kick, snare, clap) = (&self.kick, &self.snare, &self.clap);
        let (closed, open) = (&self.closed_hat, &self.open_hat);
        let (low, high) = (&self.low_tom, &self.high_tom);
        vec![
            (DrumSound::Kick, DrumParam::TuneHz(kick.tune_hz)),
            (DrumSound::Kick, DrumParam::Tone(kick.tone)),
            (DrumSound::Kick, DrumParam::DecaySeconds(kick.decay_seconds)),
            (DrumSound::Kick, DrumParam::LevelDb(kick.level_db)),
            (DrumSound::Snare, DrumParam::TuneHz(snare.tune_hz)),
            (DrumSound::Snare, DrumParam::Tone(snare.tone_seconds)),
            (DrumSound::Snare, DrumParam::Snappy(snare.snappy)),
            (DrumSound::Snare, DrumParam::LevelDb(snare.level_db)),
            (DrumSound::Clap, DrumParam::Tone(clap.tone_hz)),
            (DrumSound::Clap, DrumParam::DecaySeconds(clap.decay_seconds)),
            (DrumSound::Clap, DrumParam::LevelDb(clap.level_db)),
            (DrumSound::ClosedHat, DrumParam::TuneHz(closed.tune_hz)),
            (DrumSound::ClosedHat, DrumParam::Tone(closed.tone_hz)),
            (
                DrumSound::ClosedHat,
                DrumParam::DecaySeconds(closed.decay_seconds),
            ),
            (DrumSound::ClosedHat, DrumParam::LevelDb(closed.level_db)),
            (
                DrumSound::OpenHat,
                DrumParam::DecaySeconds(open.decay_seconds),
            ),
            (DrumSound::OpenHat, DrumParam::LevelDb(open.level_db)),
            (DrumSound::LowTom, DrumParam::TuneHz(low.tune_hz)),
            (
                DrumSound::LowTom,
                DrumParam::DecaySeconds(low.decay_seconds),
            ),
            (DrumSound::LowTom, DrumParam::LevelDb(low.level_db)),
            (DrumSound::HighTom, DrumParam::TuneHz(high.tune_hz)),
            (
                DrumSound::HighTom,
                DrumParam::DecaySeconds(high.decay_seconds),
            ),
            (DrumSound::HighTom, DrumParam::LevelDb(high.level_db)),
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
        use DrumParam as P;
        use DrumSound as D;
        let (kick, snare, clap) = (&mut self.kick, &mut self.snare, &mut self.clap);
        let (closed, open) = (&mut self.closed_hat, &mut self.open_hat);
        let (low, high) = (&mut self.low_tom, &mut self.high_tom);
        let slot = match (sound, param) {
            (D::Kick, P::TuneHz(_)) => &mut kick.tune_hz,
            (D::Kick, P::Tone(_)) => &mut kick.tone,
            (D::Kick, P::DecaySeconds(_)) => &mut kick.decay_seconds,
            (D::Kick, P::LevelDb(_)) => &mut kick.level_db,
            (D::Snare, P::TuneHz(_)) => &mut snare.tune_hz,
            (D::Snare, P::Tone(_)) => &mut snare.tone_seconds,
            (D::Snare, P::Snappy(_)) => &mut snare.snappy,
            (D::Snare, P::LevelDb(_)) => &mut snare.level_db,
            (D::Clap, P::Tone(_)) => &mut clap.tone_hz,
            (D::Clap, P::DecaySeconds(_)) => &mut clap.decay_seconds,
            (D::Clap, P::LevelDb(_)) => &mut clap.level_db,
            (D::ClosedHat, P::TuneHz(_)) => &mut closed.tune_hz,
            (D::ClosedHat, P::Tone(_)) => &mut closed.tone_hz,
            (D::ClosedHat, P::DecaySeconds(_)) => &mut closed.decay_seconds,
            (D::ClosedHat, P::LevelDb(_)) => &mut closed.level_db,
            (D::OpenHat, P::DecaySeconds(_)) => &mut open.decay_seconds,
            (D::OpenHat, P::LevelDb(_)) => &mut open.level_db,
            (D::LowTom, P::TuneHz(_)) => &mut low.tune_hz,
            (D::LowTom, P::DecaySeconds(_)) => &mut low.decay_seconds,
            (D::LowTom, P::LevelDb(_)) => &mut low.level_db,
            (D::HighTom, P::TuneHz(_)) => &mut high.tune_hz,
            (D::HighTom, P::DecaySeconds(_)) => &mut high.decay_seconds,
            (D::HighTom, P::LevelDb(_)) => &mut high.level_db,
            _ => unreachable!("check found the setting"),
        };
        let previous = std::mem::replace(slot, param.value());
        Ok(match param {
            DrumParam::TuneHz(_) => DrumParam::TuneHz(previous),
            DrumParam::Tone(_) => DrumParam::Tone(previous),
            DrumParam::DecaySeconds(_) => DrumParam::DecaySeconds(previous),
            DrumParam::Snappy(_) => DrumParam::Snappy(previous),
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
    fn the_snare_and_claps_defaults_and_limits_are_the_rfcs() {
        let snare = SnareSettings::default();
        assert_eq!((snare.tune_hz, snare.level_db), (180.0, 0.0));
        let clap = ClapSettings::default();
        assert_eq!((clap.tone_hz, clap.level_db), (1000.0, 0.0));
        let range = |sound, param| KitSettings::range(sound, &param);
        use DrumParam as P;
        use DrumSound as D;
        assert_eq!(range(D::Snare, P::TuneHz(0.0)), Some((140.0, 260.0)));
        assert_eq!(range(D::Snare, P::Tone(0.0)), Some((0.04, 0.4)));
        assert_eq!(range(D::Snare, P::Snappy(0.0)), Some((0.0, 1.0)));
        assert_eq!(range(D::Clap, P::Tone(0.0)), Some((700.0, 2000.0)));
        assert_eq!(range(D::Clap, P::DecaySeconds(0.0)), Some((0.05, 0.4)));
        for sound in [D::Kick, D::Snare, D::Clap] {
            assert_eq!(range(sound, P::LevelDb(0.0)), Some((-60.0, 6.0)));
        }
    }

    #[test]
    fn the_hats_defaults_and_limits_are_the_rfcs() {
        let closed = ClosedHatSettings::default();
        assert_eq!((closed.tune_hz, closed.decay_seconds), (205.3, 0.05));
        assert_eq!(closed.level_db, 0.0);
        let range = |sound, param| KitSettings::range(sound, &param);
        use DrumParam as P;
        use DrumSound as D;
        assert_eq!(range(D::ClosedHat, P::TuneHz(0.0)), Some((102.65, 410.6)));
        assert_eq!(range(D::ClosedHat, P::Tone(0.0)), Some((4000.0, 12_000.0)));
        assert_eq!(
            range(D::ClosedHat, P::DecaySeconds(0.0)),
            Some((0.02, 0.15))
        );
        assert_eq!(range(D::OpenHat, P::DecaySeconds(0.0)), Some((0.09, 0.6)));
        for sound in [D::ClosedHat, D::OpenHat] {
            assert_eq!(range(sound, P::LevelDb(0.0)), Some((-60.0, 6.0)));
        }
        // The open hat shares the closed hat's Tune and Tone.
        assert_eq!(range(D::OpenHat, P::TuneHz(0.0)), None);
        assert_eq!(range(D::OpenHat, P::Tone(0.0)), None);
    }

    #[test]
    fn the_toms_defaults_and_limits_are_the_rfcs() {
        let (low, high) = (LowTomSettings::default(), HighTomSettings::default());
        assert_eq!(
            (low.tune_hz, low.decay_seconds, low.level_db),
            (90.0, 0.2, 0.0)
        );
        assert_eq!(
            (high.tune_hz, high.decay_seconds, high.level_db),
            (185.0, 0.1, 0.0)
        );
        let range = |sound, param| KitSettings::range(sound, &param);
        use DrumParam as P;
        use DrumSound as D;
        assert_eq!(range(D::LowTom, P::TuneHz(0.0)), Some((80.0, 100.0)));
        assert_eq!(range(D::HighTom, P::TuneHz(0.0)), Some((165.0, 220.0)));
        for sound in [D::LowTom, D::HighTom] {
            assert_eq!(range(sound, P::DecaySeconds(0.0)), Some((0.1, 0.6)));
            assert_eq!(range(sound, P::LevelDb(0.0)), Some((-60.0, 6.0)));
            // Neither has a Tone, or a Bend (RFC-006 open question 4).
            assert_eq!(range(sound, P::Tone(0.0)), None);
        }
    }

    #[test]
    fn every_setting_of_the_snare_and_clap_sets_and_undoes() {
        let mut kit = KitSettings::default();
        let changes = [
            (DrumSound::Snare, DrumParam::TuneHz(260.0)),
            (DrumSound::Snare, DrumParam::Tone(0.04)),
            (DrumSound::Snare, DrumParam::Snappy(1.0)),
            (DrumSound::Snare, DrumParam::LevelDb(-60.0)),
            (DrumSound::Clap, DrumParam::Tone(2000.0)),
            (DrumSound::Clap, DrumParam::DecaySeconds(0.05)),
            (DrumSound::Clap, DrumParam::LevelDb(6.0)),
            (DrumSound::ClosedHat, DrumParam::TuneHz(410.6)),
            (DrumSound::ClosedHat, DrumParam::Tone(4000.0)),
            (DrumSound::ClosedHat, DrumParam::DecaySeconds(0.15)),
            (DrumSound::ClosedHat, DrumParam::LevelDb(-60.0)),
            (DrumSound::OpenHat, DrumParam::DecaySeconds(0.09)),
            (DrumSound::OpenHat, DrumParam::LevelDb(6.0)),
            (DrumSound::LowTom, DrumParam::TuneHz(100.0)),
            (DrumSound::LowTom, DrumParam::DecaySeconds(0.6)),
            (DrumSound::LowTom, DrumParam::LevelDb(-60.0)),
            (DrumSound::HighTom, DrumParam::TuneHz(165.0)),
            (DrumSound::HighTom, DrumParam::DecaySeconds(0.1)),
            (DrumSound::HighTom, DrumParam::LevelDb(6.0)),
        ];
        let mut undo = Vec::new();
        for (sound, param) in changes {
            let previous = kit.set(sound, param).unwrap();
            assert!(previous.same_setting(&param));
            undo.push((sound, previous));
        }
        assert!(kit.validate().is_ok());
        let changed = kit;
        for (sound, param) in changes {
            assert!(changed.params().contains(&(sound, param)), "{param:?}");
        }
        for (sound, previous) in undo.into_iter().rev() {
            kit.set(sound, previous).unwrap();
        }
        assert_eq!(kit, KitSettings::default());
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
        // A sound only has its own settings, and the sounds without a
        // circuit yet have none.
        for (sound, param) in [
            (DrumSound::Kick, DrumParam::Snappy(0.5)),
            (DrumSound::Snare, DrumParam::DecaySeconds(0.2)),
            (DrumSound::Clap, DrumParam::TuneHz(200.0)),
            (DrumSound::Clap, DrumParam::Snappy(0.5)),
            (DrumSound::OpenHat, DrumParam::TuneHz(205.3)),
            (DrumSound::OpenHat, DrumParam::Tone(7100.0)),
            (DrumSound::LowTom, DrumParam::Tone(0.5)),
            (DrumSound::HighTom, DrumParam::Snappy(0.5)),
            (DrumSound::Cymbal, DrumParam::TuneHz(200.0)),
        ] {
            assert_eq!(
                kit.set(sound, param),
                Err(DrumParamError::NoSuchSetting),
                "{sound:?} {param:?}"
            );
        }
        // Each tom has its own Tune range.
        assert!(
            kit.set(DrumSound::LowTom, DrumParam::TuneHz(185.0))
                .is_err()
        );
        assert!(
            kit.set(DrumSound::HighTom, DrumParam::TuneHz(90.0))
                .is_err()
        );
        // Tone is in each sound's own unit, so its limits differ.
        assert!(kit.set(DrumSound::Snare, DrumParam::Tone(0.5)).is_err());
        assert!(kit.set(DrumSound::Clap, DrumParam::Tone(0.5)).is_err());
        assert_eq!(kit, KitSettings::default());
    }

    #[test]
    fn settings_serialise_and_leave_out_nothing_they_need() {
        let json = concat!(
            r#"{"kick":{"tune_hz":49.0,"tone":0.2,"decay_seconds":0.3,"level_db":0.0},"#,
            r#""snare":{"tune_hz":180.0,"tone_seconds":0.16,"snappy":0.5,"level_db":0.0},"#,
            r#""clap":{"tone_hz":1000.0,"decay_seconds":0.2,"level_db":0.0},"#,
            r#""closed_hat":{"tune_hz":205.3,"tone_hz":7100.0,"decay_seconds":0.05,"level_db":0.0},"#,
            r#""open_hat":{"decay_seconds":0.35,"level_db":0.0},"#,
            r#""low_tom":{"tune_hz":90.0,"decay_seconds":0.2,"level_db":0.0},"#,
            r#""high_tom":{"tune_hz":185.0,"decay_seconds":0.1,"level_db":0.0}}"#,
        );
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
        // A kit from before the snare and clap had settings (format 4 as
        // UTA-47 wrote it) takes their defaults.
        assert_eq!(tuned.snare, SnareSettings::default());
        assert_eq!(tuned.clap, ClapSettings::default());
        assert_eq!(tuned.closed_hat, ClosedHatSettings::default());
        assert_eq!(tuned.open_hat, OpenHatSettings::default());
        assert_eq!(tuned.low_tom, LowTomSettings::default());
        assert_eq!(tuned.high_tom, HighTomSettings::default());
        let param: DrumParam = serde_json::from_str(r#"{"name":"snappy","value":0.7}"#).unwrap();
        assert_eq!(param, DrumParam::Snappy(0.7));
        assert!(serde_json::from_str::<KitSettings>(r#"{"cowbell":{}}"#).is_err());
    }
}
