//! The drum machine: one circuit per sound, not a pool of voices. See
//! RFC-006, "In the engine".
//!
//! Everything here runs on the audio thread. Each slot has a [`Kit`] beside
//! its synth, built at stream setup, so a slot can play either kind of
//! track. Every circuit is plain numbers and keeps running between hits: a
//! hit adds energy to it, never restarts it. Drum notes are one-shots: the
//! end of a note does nothing.

mod clap;
mod cymbal;
mod filter;
mod hats;
mod kick;
mod kick909;
mod metal;
mod noise;
mod resonator;
mod snare;
mod tom;

use clap::Clap;
use cymbal::Cymbal;
use hats::Hats;
use kick::Kick;
use kick909::Kick909;
use metal::Metal;
use noise::Noise;
use snare::Snare;
use tom::{Tom, TomKind, TomSettings};
use uta_core::{DrumSound, KickModel};

use crate::snapshot::db_to_gain;

/// How long a change to a drum setting takes to glide to its new value, as
/// the synth's do.
pub const DRUM_SMOOTHING_SECONDS: f64 = crate::SYNTH_SMOOTHING_SECONDS;

/// The peak a sound reaches at full accent with its default settings,
/// before the track's and master's volumes: -6 dB. The kit's levels are
/// balanced against it in the tuning ticket.
pub const REFERENCE_PEAK: f32 = 0.5;

/// A hit's strength, from its velocity: how hard the excitation is, from 0
/// to 1. Every sound feeds it into its excitation, never only into its
/// output level, so a harder hit has a different tone, not just more level.
/// See RFC-006, "What makes it sound good", point 2.
///
/// - Velocity 127 is a full accent: strength 1, as the 808's 14 V trigger.
/// - Velocity 100 is an unaccented hit: strength 0.3, as the 808's 4 V
///   trigger is about 0.29 of its 14 V, and Plaits' unaccented pulse is 3
///   of its 10.
/// - In between, the strength in dB is a straight line, about 0.39 dB a
///   step.
/// - Below 100, it falls 0.25 dB a step, gentler than above, so velocity 64
///   is about 9 dB under an unaccented hit and velocity 1 about 25 dB, still
///   a hit you can hear.
pub fn velocity_to_strength(velocity: u8) -> f32 {
    const UNACCENTED: f32 = 0.3;
    let velocity = f32::from(velocity.clamp(1, 127));
    let unaccented_db = 20.0 * UNACCENTED.log10();
    let db = if velocity >= 100.0 {
        unaccented_db * (127.0 - velocity) / 27.0
    } else {
        unaccented_db - 0.25 * (100.0 - velocity)
    };
    db_to_gain(db)
}

/// The kit's settings, as the engine plays them. The project core keeps its
/// own copy for `SetDrumParam` to check, with the same names, units, ranges
/// and defaults, so the snapshot only has to copy values across.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct KitSettings {
    /// Which kick plays.
    pub kick_model: KickModel,
    /// The 808 kick's settings, and the 909's.
    pub kick: KickSettings,
    pub kick_909: Kick909Settings,
    pub snare: SnareSettings,
    pub clap: ClapSettings,
    pub closed_hat: ClosedHatSettings,
    pub open_hat: OpenHatSettings,
    pub low_tom: LowTomSettings,
    pub high_tom: HighTomSettings,
    pub cymbal: CymbalSettings,
}

impl KitSettings {
    /// How long the kit's longest sound takes to die away to -60 dB once
    /// hit, in seconds: how long a render has to run on for it to end in
    /// silence.
    pub fn ring_seconds(&self) -> f32 {
        let (kick, snare, clap) = (
            self.kick.clamped(),
            self.snare.clamped(),
            self.clap.clamped(),
        );
        let (closed_hat, open_hat) = (self.closed_hat.clamped(), self.open_hat.clamped());
        // Each Decay is 40 dB, so 60 dB is half as long again. The snare's
        // shell rings about 0.5 s with its long tail, and its wires hold for
        // up to 70 ms first; the clap's tail swells for a few ms after its
        // last burst, 30 ms in.
        // Either kick, since one can still be ringing out after the model
        // changes. The 909's envelope holds for 1 ms first.
        let kick =
            (kick.decay_seconds * 1.5).max(0.001 + self.kick_909.clamped().decay_seconds * 1.5);
        let snare = (0.07 + snare.tone_seconds * 1.5).max(0.5);
        let clap = 0.05 + clap.decay_seconds * 1.5;
        let hats = closed_hat.decay_seconds.max(open_hat.decay_seconds) * 1.5;
        // The toms' skin dies away a little slower than their body, but
        // starts well under it, so it's gone by then too.
        let toms = self
            .low_tom
            .clamped()
            .decay_seconds
            .max(self.high_tom.clamped().decay_seconds)
            * 1.5;
        // The cymbal's low band is its longest, and its mid band's long
        // envelope follows it, well under it.
        let cymbal = self.cymbal.clamped().decay_seconds * 1.5;
        kick.max(snare).max(clap).max(hats).max(toms).max(cymbal)
    }
}

/// The 808 kick's settings. See `uta_core::KickSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KickSettings {
    /// The note it settles on, in Hz, [`Self::TUNE_HZ`].
    pub tune_hz: f32,
    /// How bright it is, [`Self::TONE`].
    pub tone: f32,
    /// The seconds it takes to die away by 40 dB, [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

/// Every sound's Level range, in dB.
pub const LEVEL_DB: std::ops::RangeInclusive<f32> = uta_core::MIN_LEVEL_DB..=uta_core::MAX_LEVEL_DB;

/// `value` inside `range`, or `fallback` if it isn't a number, so a bad value
/// can never reach the audio.
fn clamp(value: f32, range: std::ops::RangeInclusive<f32>, fallback: f32) -> f32 {
    if value.is_nan() {
        fallback
    } else {
        value.clamp(*range.start(), *range.end())
    }
}

impl KickSettings {
    pub const TUNE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::KickSettings::MIN_TUNE_HZ..=uta_core::KickSettings::MAX_TUNE_HZ;
    pub const TONE: std::ops::RangeInclusive<f32> =
        uta_core::KickSettings::MIN_TONE..=uta_core::KickSettings::MAX_TONE;
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::KickSettings::MIN_DECAY_SECONDS..=uta_core::KickSettings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default, so a bad value can never reach the audio.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tune_hz: clamp(self.tune_hz, Self::TUNE_HZ, default.tune_hz),
            tone: clamp(self.tone, Self::TONE, default.tone),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }
}

impl Default for KickSettings {
    fn default() -> Self {
        Self::from(&uta_core::KickSettings::default())
    }
}

impl From<&uta_core::KickSettings> for KickSettings {
    fn from(settings: &uta_core::KickSettings) -> Self {
        Self {
            tune_hz: settings.tune_hz,
            tone: settings.tone,
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

/// The 909 kick's settings. See `uta_core::Kick909Settings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kick909Settings {
    /// The note it settles on, in Hz, [`Self::TUNE_HZ`].
    pub tune_hz: f32,
    /// How far the pitch drops at the start, [`Self::SWEEP`].
    pub sweep: f32,
    /// How much click, [`Self::ATTACK`].
    pub attack: f32,
    /// The seconds it takes to die away by 40 dB, [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl Kick909Settings {
    pub const TUNE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::Kick909Settings::MIN_TUNE_HZ..=uta_core::Kick909Settings::MAX_TUNE_HZ;
    pub const SWEEP: std::ops::RangeInclusive<f32> =
        uta_core::Kick909Settings::MIN_SWEEP..=uta_core::Kick909Settings::MAX_SWEEP;
    pub const ATTACK: std::ops::RangeInclusive<f32> =
        uta_core::Kick909Settings::MIN_ATTACK..=uta_core::Kick909Settings::MAX_ATTACK;
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::Kick909Settings::MIN_DECAY_SECONDS..=uta_core::Kick909Settings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tune_hz: clamp(self.tune_hz, Self::TUNE_HZ, default.tune_hz),
            sweep: clamp(self.sweep, Self::SWEEP, default.sweep),
            attack: clamp(self.attack, Self::ATTACK, default.attack),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }
}

impl Default for Kick909Settings {
    fn default() -> Self {
        Self::from(&uta_core::Kick909Settings::default())
    }
}

impl From<&uta_core::Kick909Settings> for Kick909Settings {
    fn from(settings: &uta_core::Kick909Settings) -> Self {
        Self {
            tune_hz: settings.tune_hz,
            sweep: settings.sweep,
            attack: settings.attack,
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

/// The 909 snare's settings. See `uta_core::SnareSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnareSettings {
    /// The shell's pitch, in Hz, [`Self::TUNE_HZ`].
    pub tune_hz: f32,
    /// The wires' length: the seconds they take to die away by 40 dB once
    /// their hold ends, [`Self::TONE_SECONDS`].
    pub tone_seconds: f32,
    /// How much rattle against the shell, [`Self::SNAPPY`].
    pub snappy: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl SnareSettings {
    pub const TUNE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::SnareSettings::MIN_TUNE_HZ..=uta_core::SnareSettings::MAX_TUNE_HZ;
    pub const TONE_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::SnareSettings::MIN_TONE_SECONDS..=uta_core::SnareSettings::MAX_TONE_SECONDS;
    pub const SNAPPY: std::ops::RangeInclusive<f32> =
        uta_core::SnareSettings::MIN_SNAPPY..=uta_core::SnareSettings::MAX_SNAPPY;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tune_hz: clamp(self.tune_hz, Self::TUNE_HZ, default.tune_hz),
            tone_seconds: clamp(self.tone_seconds, Self::TONE_SECONDS, default.tone_seconds),
            snappy: clamp(self.snappy, Self::SNAPPY, default.snappy),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }
}

impl Default for SnareSettings {
    fn default() -> Self {
        Self::from(&uta_core::SnareSettings::default())
    }
}

impl From<&uta_core::SnareSettings> for SnareSettings {
    fn from(settings: &uta_core::SnareSettings) -> Self {
        Self {
            tune_hz: settings.tune_hz,
            tone_seconds: settings.tone_seconds,
            snappy: settings.snappy,
            level_db: settings.level_db,
        }
    }
}

/// The 808 clap's settings. See `uta_core::ClapSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClapSettings {
    /// The band-pass's centre, in Hz, [`Self::TONE_HZ`].
    pub tone_hz: f32,
    /// The seconds its tail takes to die away by 40 dB,
    /// [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl ClapSettings {
    pub const TONE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::ClapSettings::MIN_TONE_HZ..=uta_core::ClapSettings::MAX_TONE_HZ;
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::ClapSettings::MIN_DECAY_SECONDS..=uta_core::ClapSettings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tone_hz: clamp(self.tone_hz, Self::TONE_HZ, default.tone_hz),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }
}

impl Default for ClapSettings {
    fn default() -> Self {
        Self::from(&uta_core::ClapSettings::default())
    }
}

impl From<&uta_core::ClapSettings> for ClapSettings {
    fn from(settings: &uta_core::ClapSettings) -> Self {
        Self {
            tone_hz: settings.tone_hz,
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

/// The 808 closed hat's settings. Its Tune and Tone are the open hat's too.
/// See `uta_core::ClosedHatSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClosedHatSettings {
    /// The metal's lowest oscillator, in Hz, [`Self::TUNE_HZ`].
    pub tune_hz: f32,
    /// Where the filters sit, in Hz, [`Self::TONE_HZ`].
    pub tone_hz: f32,
    /// The seconds it takes to die away by 40 dB, [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl ClosedHatSettings {
    pub const TUNE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::ClosedHatSettings::MIN_TUNE_HZ..=uta_core::ClosedHatSettings::MAX_TUNE_HZ;
    pub const TONE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::ClosedHatSettings::MIN_TONE_HZ..=uta_core::ClosedHatSettings::MAX_TONE_HZ;
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::ClosedHatSettings::MIN_DECAY_SECONDS
            ..=uta_core::ClosedHatSettings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tune_hz: clamp(self.tune_hz, Self::TUNE_HZ, default.tune_hz),
            tone_hz: clamp(self.tone_hz, Self::TONE_HZ, default.tone_hz),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }
}

impl Default for ClosedHatSettings {
    fn default() -> Self {
        Self::from(&uta_core::ClosedHatSettings::default())
    }
}

impl From<&uta_core::ClosedHatSettings> for ClosedHatSettings {
    fn from(settings: &uta_core::ClosedHatSettings) -> Self {
        Self {
            tune_hz: settings.tune_hz,
            tone_hz: settings.tone_hz,
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

/// The 808 open hat's settings. It shares the closed hat's Tune and Tone.
/// See `uta_core::OpenHatSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenHatSettings {
    /// The seconds it takes to die away by 40 dB, [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl OpenHatSettings {
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::OpenHatSettings::MIN_DECAY_SECONDS..=uta_core::OpenHatSettings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }
}

impl Default for OpenHatSettings {
    fn default() -> Self {
        Self::from(&uta_core::OpenHatSettings::default())
    }
}

impl From<&uta_core::OpenHatSettings> for OpenHatSettings {
    fn from(settings: &uta_core::OpenHatSettings) -> Self {
        Self {
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

/// The 808 low tom's settings. See `uta_core::LowTomSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LowTomSettings {
    /// The note it settles on, in Hz, [`Self::TUNE_HZ`].
    pub tune_hz: f32,
    /// The seconds it takes to die away by 40 dB, [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl LowTomSettings {
    pub const TUNE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::LowTomSettings::MIN_TUNE_HZ..=uta_core::LowTomSettings::MAX_TUNE_HZ;
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::LowTomSettings::MIN_DECAY_SECONDS..=uta_core::LowTomSettings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tune_hz: clamp(self.tune_hz, Self::TUNE_HZ, default.tune_hz),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }

    /// The tom circuit's settings, clamped.
    fn tom(self) -> TomSettings {
        let Self {
            tune_hz,
            decay_seconds,
            level_db,
        } = self.clamped();
        TomSettings {
            tune_hz,
            decay_seconds,
            level_db,
        }
    }
}

impl Default for LowTomSettings {
    fn default() -> Self {
        Self::from(&uta_core::LowTomSettings::default())
    }
}

impl From<&uta_core::LowTomSettings> for LowTomSettings {
    fn from(settings: &uta_core::LowTomSettings) -> Self {
        Self {
            tune_hz: settings.tune_hz,
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

/// The 808 high tom's settings. See `uta_core::HighTomSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighTomSettings {
    /// The note it settles on, in Hz, [`Self::TUNE_HZ`].
    pub tune_hz: f32,
    /// The seconds it takes to die away by 40 dB, [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl HighTomSettings {
    pub const TUNE_HZ: std::ops::RangeInclusive<f32> =
        uta_core::HighTomSettings::MIN_TUNE_HZ..=uta_core::HighTomSettings::MAX_TUNE_HZ;
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::HighTomSettings::MIN_DECAY_SECONDS..=uta_core::HighTomSettings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tune_hz: clamp(self.tune_hz, Self::TUNE_HZ, default.tune_hz),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }

    /// The tom circuit's settings, clamped.
    fn tom(self) -> TomSettings {
        let Self {
            tune_hz,
            decay_seconds,
            level_db,
        } = self.clamped();
        TomSettings {
            tune_hz,
            decay_seconds,
            level_db,
        }
    }
}

impl Default for HighTomSettings {
    fn default() -> Self {
        Self::from(&uta_core::HighTomSettings::default())
    }
}

impl From<&uta_core::HighTomSettings> for HighTomSettings {
    fn from(settings: &uta_core::HighTomSettings) -> Self {
        Self {
            tune_hz: settings.tune_hz,
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

/// The 808 cymbal's settings. Its Tune is the closed hat's. See
/// `uta_core::CymbalSettings`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CymbalSettings {
    /// How bright it is, mainly its high band's level, [`Self::TONE`].
    pub tone: f32,
    /// The seconds its low band takes to die away by 40 dB,
    /// [`Self::DECAY_SECONDS`].
    pub decay_seconds: f32,
    /// Its level, in dB, [`LEVEL_DB`].
    pub level_db: f32,
}

impl CymbalSettings {
    pub const TONE: std::ops::RangeInclusive<f32> =
        uta_core::CymbalSettings::MIN_TONE..=uta_core::CymbalSettings::MAX_TONE;
    pub const DECAY_SECONDS: std::ops::RangeInclusive<f32> =
        uta_core::CymbalSettings::MIN_DECAY_SECONDS..=uta_core::CymbalSettings::MAX_DECAY_SECONDS;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        Self {
            tone: clamp(self.tone, Self::TONE, default.tone),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::DECAY_SECONDS,
                default.decay_seconds,
            ),
            level_db: clamp(self.level_db, LEVEL_DB, default.level_db),
        }
    }
}

impl Default for CymbalSettings {
    fn default() -> Self {
        Self::from(&uta_core::CymbalSettings::default())
    }
}

impl From<&uta_core::CymbalSettings> for CymbalSettings {
    fn from(settings: &uta_core::CymbalSettings) -> Self {
        Self {
            tone: settings.tone,
            decay_seconds: settings.decay_seconds,
            level_db: settings.level_db,
        }
    }
}

impl From<&uta_core::KitSettings> for KitSettings {
    fn from(kit: &uta_core::KitSettings) -> Self {
        Self {
            kick_model: kit.kick.model,
            kick: KickSettings::from(&kit.kick),
            kick_909: Kick909Settings::from(&kit.kick.tr909),
            snare: SnareSettings::from(&kit.snare),
            clap: ClapSettings::from(&kit.clap),
            closed_hat: ClosedHatSettings::from(&kit.closed_hat),
            open_hat: OpenHatSettings::from(&kit.open_hat),
            low_tom: LowTomSettings::from(&kit.low_tom),
            high_tom: HighTomSettings::from(&kit.high_tom),
            cymbal: CymbalSettings::from(&kit.cymbal),
        }
    }
}

/// How long the sounds smooth every envelope edge over, so even an instant
/// attack is a fast curve: about the 808 cymbal's attack (Werner).
const EDGE_SECONDS: f64 = 0.1e-3;

/// How much a decay of 40 dB is in time constants: ln(100).
const LN_100: f64 = 4.605_170_185_988_091;

/// The time constant for a decay of 40 dB in `decay_seconds`, as its log, so
/// it glides evenly.
fn log_tau(decay_seconds: f32) -> f32 {
    (f64::from(decay_seconds) / LN_100).ln() as f32
}

/// The multiplier a sample for an exponential decay with time constant
/// `tau_seconds`.
fn decay_per_sample(tau_seconds: f64, sample_rate: f64) -> f64 {
    (-1.0 / (tau_seconds * sample_rate)).exp()
}

/// The coefficient of a one-pole smoother with time constant `seconds`.
fn smoothing(seconds: f64, sample_rate: f64) -> f64 {
    1.0 - decay_per_sample(seconds, sample_rate)
}

/// Samples for a setting to glide, at `sample_rate`.
fn smoothing_samples(sample_rate: f64) -> u32 {
    (DRUM_SMOOTHING_SECONDS * sample_rate).round().max(1.0) as u32
}

/// One drum track's kit on the audio thread: a circuit per sound. Owned by
/// its slot.
pub(crate) struct Kit {
    /// Which kick a hit plays. The other rings on if it was ringing, so
    /// changing the model never cuts a kick off.
    kick_model: KickModel,
    kick: Kick,
    kick_909: Kick909,
    snare: Snare,
    clap: Clap,
    /// Both hats: one circuit.
    hats: Hats,
    low_tom: Tom,
    high_tom: Tom,
    cymbal: Cymbal,
    /// The noise the snare, the clap, the toms and the 909 kick's click share.
    noise: Noise,
    /// The metal the hats and the cymbal filter, tuned by the closed hat's
    /// Tune.
    metal: Metal,
}

/// The metal for a kit at `sample_rate`, tuned to `tune_hz`.
fn metal(tune_hz: f32, sample_rate: f64) -> Metal {
    Metal::new(tune_hz, sample_rate, smoothing_samples(sample_rate))
}

impl Kit {
    pub(crate) fn new(settings: KitSettings, sample_rate: f64) -> Self {
        let hats = Hats::new(settings.closed_hat, settings.open_hat, sample_rate);
        Self {
            kick_model: settings.kick_model,
            kick: Kick::new(settings.kick, sample_rate),
            kick_909: Kick909::new(settings.kick_909, sample_rate),
            snare: Snare::new(settings.snare, sample_rate),
            clap: Clap::new(settings.clap, sample_rate),
            low_tom: Tom::new(TomKind::Low, settings.low_tom.tom(), sample_rate),
            high_tom: Tom::new(TomKind::High, settings.high_tom.tom(), sample_rate),
            cymbal: Cymbal::new(settings.cymbal, sample_rate),
            metal: metal(hats.tune_hz(), sample_rate),
            hats,
            noise: Noise::new(),
        }
    }

    /// Moves to a new sample rate. Every sound falls silent: the stream it
    /// was playing on has already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        self.kick.prepare(sample_rate);
        self.kick_909.prepare(sample_rate);
        self.snare.prepare(sample_rate);
        self.clap.prepare(sample_rate);
        self.hats.prepare(sample_rate);
        self.low_tom.prepare(sample_rate);
        self.high_tom.prepare(sample_rate);
        self.cymbal.prepare(sample_rate);
        self.noise.restart();
        self.metal = metal(self.hats.tune_hz(), sample_rate);
    }

    /// Takes on new settings straight away, with no glide: for a kit that
    /// isn't sounding.
    pub(crate) fn load(&mut self, settings: KitSettings) {
        self.kick_model = settings.kick_model;
        self.kick.load(settings.kick);
        self.kick_909.load(settings.kick_909);
        self.snare.load(settings.snare);
        self.clap.load(settings.clap);
        self.hats.load(settings.closed_hat, settings.open_hat);
        self.low_tom.load(settings.low_tom.tom());
        self.high_tom.load(settings.high_tom.tom());
        self.cymbal.load(settings.cymbal);
        self.metal.load_tune(self.hats.tune_hz());
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: KitSettings) {
        self.kick_model = settings.kick_model;
        self.kick.set_settings(settings.kick);
        self.kick_909.set_settings(settings.kick_909);
        self.snare.set_settings(settings.snare);
        self.clap.set_settings(settings.clap);
        self.hats
            .set_settings(settings.closed_hat, settings.open_hat);
        self.low_tom.set_settings(settings.low_tom.tom());
        self.high_tom.set_settings(settings.high_tom.tom());
        self.cymbal.set_settings(settings.cymbal);
        self.metal.set_tune(self.hats.tune_hz());
    }

    /// Hits the sound `pitch` plays, at `velocity`. A pitch off the kit does
    /// nothing.
    pub(crate) fn hit(&mut self, pitch: u8, velocity: u8) {
        let strength = velocity_to_strength(velocity);
        match DrumSound::at_pitch(pitch) {
            Some(DrumSound::Kick) => match self.kick_model {
                KickModel::Tr808 => self.kick.hit(strength),
                KickModel::Tr909 => self.kick_909.hit(strength),
            },
            Some(DrumSound::Snare) => self.snare.hit(strength),
            Some(DrumSound::Clap) => self.clap.hit(strength),
            Some(DrumSound::ClosedHat) => self.hats.hit_closed(strength),
            Some(DrumSound::OpenHat) => self.hats.hit_open(strength),
            Some(DrumSound::LowTom) => self.low_tom.hit(strength),
            Some(DrumSound::HighTom) => self.high_tom.hit(strength),
            Some(DrumSound::Cymbal) => self.cymbal.hit(strength),
            None => {}
        }
    }

    /// Restarts the free-running parts, the noise and oscillators that run
    /// between hits, from the same point, so playing from the top always
    /// sounds the same as a render. Called when playback starts. See RFC-006,
    /// resolved open question 3.
    ///
    /// The noise restarts here. A sound ringing as playback starts carries on
    /// without a click: noise has no waveform to break, and the filters it
    /// runs through aren't touched; the toms' skin is noise of this kind. The
    /// kick's and toms' resonators aren't free-running, and the snare's
    /// oscillators start from the same point whenever it has died away, so
    /// none of them is touched.
    ///
    /// The metal restarts too. If the hats or the cymbal are ringing, it
    /// crossfades to its restarted self over a few milliseconds, since its
    /// square waves jumping would make them click.
    pub(crate) fn restart(&mut self) {
        self.noise.restart();
        self.metal.restart(self.metal_heard());
    }

    /// Whether anything is listening to the metal.
    fn metal_heard(&self) -> bool {
        self.hats.is_sounding() || self.cymbal.is_sounding()
    }

    /// Whether any sound is ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.kick.is_sounding()
            || self.kick_909.is_sounding()
            || self.snare.is_sounding()
            || self.clap.is_sounding()
            || self.hats.is_sounding()
            || self.low_tom.is_sounding()
            || self.high_tom.is_sounding()
            || self.cymbal.is_sounding()
    }

    /// The next sample: every sound, added together. A sound that has died
    /// away does no work, but the noise and the metal run on, free.
    #[inline]
    pub(crate) fn next_sample(&mut self) -> f32 {
        let noise = self.noise.next_sample();
        let metal = if self.metal_heard() {
            self.metal.next_sample()
        } else {
            self.metal.skip();
            0.0
        };
        self.kick.next_sample()
            + self.kick_909.next_sample(noise)
            + self.snare.next_sample(noise)
            + self.clap.next_sample(noise)
            + self.hats.next_sample(metal)
            + self.low_tom.next_sample(noise)
            + self.high_tom.next_sample(noise)
            + self.cymbal.next_sample(metal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_100_is_unaccented_and_127_a_full_accent() {
        assert_eq!(velocity_to_strength(127), 1.0);
        assert!((velocity_to_strength(100) - 0.3).abs() < 1e-6);
        let db = |velocity| 20.0 * velocity_to_strength(velocity).log10();
        assert!((db(64) - (db(100) - 9.0)).abs() < 0.01, "{}", db(64));
        // Every step up is a harder hit.
        for velocity in 1..127 {
            assert!(velocity_to_strength(velocity) < velocity_to_strength(velocity + 1));
        }
        assert_eq!(velocity_to_strength(0), velocity_to_strength(1));
    }

    #[test]
    fn settings_out_of_range_are_clamped_and_nan_is_the_default() {
        let wild = KickSettings {
            tune_hz: 1000.0,
            tone: -1.0,
            decay_seconds: f32::NAN,
            level_db: 100.0,
        }
        .clamped();
        assert_eq!(wild.tune_hz, 80.0);
        assert_eq!(wild.tone, 0.0);
        assert_eq!(wild.decay_seconds, KickSettings::default().decay_seconds);
        assert_eq!(wild.level_db, 6.0);
    }

    /// Play restarts the metal, crossfading if the hats are ringing and
    /// not if they're quiet.
    #[test]
    fn a_restart_crossfades_the_metal_only_while_the_hats_ring() {
        let mut kit = Kit::new(KitSettings::default(), 48_000.0);
        kit.restart();
        assert!(!kit.metal.is_crossfading());
        kit.hit(46, 127);
        for _ in 0..4800 {
            kit.next_sample();
        }
        kit.restart();
        assert!(kit.metal.is_crossfading());
        // Once the hats have died away, a restart doesn't crossfade.
        for _ in 0..(3.0 * 48_000.0) as usize {
            kit.next_sample();
        }
        assert!(!kit.is_sounding());
        kit.restart();
        assert!(!kit.metal.is_crossfading());
    }

    /// The cymbal listens to the same metal, so Play crossfades it while
    /// the cymbal rings, with the hats quiet.
    #[test]
    fn a_restart_crossfades_the_metal_while_the_cymbal_rings() {
        let mut kit = Kit::new(KitSettings::default(), 48_000.0);
        kit.hit(49, 127);
        for _ in 0..4800 {
            kit.next_sample();
        }
        assert!(!kit.hats.is_sounding());
        kit.restart();
        assert!(kit.metal.is_crossfading());
    }

    #[test]
    fn the_engines_defaults_are_the_cores() {
        assert_eq!(
            KitSettings::from(&uta_core::KitSettings::default()),
            KitSettings::default()
        );
    }
}
