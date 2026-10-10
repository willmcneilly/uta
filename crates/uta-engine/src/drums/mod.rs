//! The drum machine: one circuit per sound, not a pool of voices. See
//! RFC-006, "In the engine".
//!
//! Everything here runs on the audio thread. Each slot has a [`Kit`] beside
//! its synth, built at stream setup, so a slot can play either kind of
//! track. Every circuit is plain numbers and keeps running between hits: a
//! hit adds energy to it, never restarts it. Drum notes are one-shots: the
//! end of a note does nothing.
//!
//! The sounds without a circuit yet (all but the kick, for now) are silent.

mod kick;

use kick::Kick;
use uta_core::DrumSound;

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
    pub kick: KickSettings,
}

impl KitSettings {
    /// How long the kit's longest sound takes to die away to -60 dB once
    /// hit, in seconds: how long a render has to run on for it to end in
    /// silence.
    pub fn ring_seconds(&self) -> f32 {
        let kick = self.kick.clamped();
        // The Decay is 40 dB, so 60 dB is half as long again.
        kick.decay_seconds * 1.5
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
        let clamp = |value: f32, range: std::ops::RangeInclusive<f32>, fallback: f32| {
            if value.is_nan() {
                fallback
            } else {
                value.clamp(*range.start(), *range.end())
            }
        };
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

impl From<&uta_core::KitSettings> for KitSettings {
    fn from(kit: &uta_core::KitSettings) -> Self {
        Self {
            kick: KickSettings::from(&kit.kick),
        }
    }
}

/// Samples for a setting to glide, at `sample_rate`.
fn smoothing_samples(sample_rate: f64) -> u32 {
    (DRUM_SMOOTHING_SECONDS * sample_rate).round().max(1.0) as u32
}

/// One drum track's kit on the audio thread: a circuit per sound. Owned by
/// its slot.
pub(crate) struct Kit {
    kick: Kick,
}

impl Kit {
    pub(crate) fn new(settings: KitSettings, sample_rate: f64) -> Self {
        Self {
            kick: Kick::new(settings.kick, sample_rate),
        }
    }

    /// Moves to a new sample rate. Every sound falls silent: the stream it
    /// was playing on has already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        self.kick.prepare(sample_rate);
    }

    /// Takes on new settings straight away, with no glide: for a kit that
    /// isn't sounding.
    pub(crate) fn load(&mut self, settings: KitSettings) {
        self.kick.load(settings.kick);
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: KitSettings) {
        self.kick.set_settings(settings.kick);
    }

    /// Hits the sound `pitch` plays, at `velocity`. A pitch off the kit, or
    /// a sound without a circuit yet, does nothing.
    pub(crate) fn hit(&mut self, pitch: u8, velocity: u8) {
        let strength = velocity_to_strength(velocity);
        if let Some(DrumSound::Kick) = DrumSound::at_pitch(pitch) {
            self.kick.hit(strength);
        }
    }

    /// Restarts the free-running parts, the noise and oscillators that run
    /// between hits, from the same point, so playing from the top always
    /// sounds the same as a render. Called when playback starts. See RFC-006,
    /// resolved open question 3.
    ///
    /// The kick has no free-running parts: its resonator only rings after a
    /// hit, and isn't touched here, so a kick still ringing as playback
    /// starts carries on without a click. The noise (snare and clap) and the
    /// metal bank (hats and cymbal) will restart here.
    pub(crate) fn restart(&mut self) {}

    /// Whether any sound is ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.kick.is_sounding()
    }

    /// The next sample: every sound, added together. A sound that has died
    /// away does no work.
    #[inline]
    pub(crate) fn next_sample(&mut self) -> f32 {
        self.kick.next_sample()
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

    #[test]
    fn the_engines_defaults_are_the_cores() {
        assert_eq!(
            KitSettings::from(&uta_core::KitSettings::default()),
            KitSettings::default()
        );
    }
}
