//! The "what to play" snapshot.

use uta_core::Project;

use crate::SynthSettings;

/// Everything the audio thread needs to know about what to play.
///
/// The control side builds a whole new snapshot for every change, and the
/// audio thread swaps it in at the start of a block. The old one goes back to
/// the control side to be freed. It lives in a `Box` on its way through the
/// queues, so the audio thread only ever moves a pointer.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The tone's frequency in Hz.
    pub frequency_hz: f64,
    /// The output volume as a linear gain (1.0 is full scale).
    pub gain: f32,
    /// How the synth sounds. UTA-10 fills this from the project.
    pub synth: SynthSettings,
}

impl Snapshot {
    /// The milestone 0 tone: 440 Hz at the default volume.
    pub const DEFAULT_FREQUENCY_HZ: f64 = 440.0;
    /// The default volume, in dB.
    pub const DEFAULT_VOLUME_DB: f32 = -12.0;
    /// The loudest volume, in dB. At 0 dB the tone peaks at full scale, so no
    /// volume can make it clip.
    pub const MAX_VOLUME_DB: f32 = 0.0;

    /// A snapshot with the given volume in dB, clamped to
    /// [`Self::MAX_VOLUME_DB`]. NaN is silence.
    pub fn with_volume_db(self, volume_db: f32) -> Self {
        let gain = if volume_db.is_nan() {
            0.0
        } else {
            db_to_gain(volume_db.min(Self::MAX_VOLUME_DB))
        };
        Self { gain, ..self }
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            frequency_hz: Self::DEFAULT_FREQUENCY_HZ,
            gain: db_to_gain(Self::DEFAULT_VOLUME_DB),
            synth: SynthSettings::default(),
        }
    }
}

/// What the engine plays for a project. The control side sends the result
/// with [`crate::Controller::set_snapshot`] after each change.
impl From<&Project> for Snapshot {
    fn from(project: &Project) -> Self {
        Self::default().with_volume_db(project.master_volume_db())
    }
}

/// Converts decibels to a linear gain. Anything at or below the project's
/// minimum volume, [`Project::MIN_VOLUME_DB`], is silence.
pub fn db_to_gain(db: f32) -> f32 {
    if db <= Project::MIN_VOLUME_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_to_gain_known_values() {
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(-6.0) - 0.501_187).abs() < 1e-6);
        assert!((db_to_gain(-20.0) - 0.1).abs() < 1e-7);
        assert_eq!(db_to_gain(-120.0), 0.0);
        assert_eq!(db_to_gain(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn default_is_440_hz_at_minus_12_db() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.frequency_hz, 440.0);
        assert_eq!(snapshot.gain, db_to_gain(-12.0));
    }

    #[test]
    fn volume_is_clamped_to_the_ceiling() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.clone().with_volume_db(40.0).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(f32::INFINITY).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(0.0).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(f32::NAN).gain, 0.0);
        assert_eq!(snapshot.with_volume_db(-6.0).gain, db_to_gain(-6.0));
    }

    #[test]
    fn a_new_project_plays_the_default_snapshot() {
        assert_eq!(Snapshot::from(&Project::new()), Snapshot::default());
    }

    #[test]
    fn the_snapshot_follows_the_master_volume() {
        let mut project = Project::new();
        project
            .apply(&uta_core::Command::SetMasterVolume { volume_db: -6.0 })
            .unwrap();
        let snapshot = Snapshot::from(&project);
        assert_eq!(snapshot.gain, db_to_gain(-6.0));
        assert_eq!(snapshot.frequency_hz, Snapshot::DEFAULT_FREQUENCY_HZ);
    }
}
