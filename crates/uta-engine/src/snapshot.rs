//! The "what to play" snapshot.

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
}

impl Snapshot {
    /// The milestone 0 tone: 440 Hz at the default volume.
    pub const DEFAULT_FREQUENCY_HZ: f64 = 440.0;
    /// The default volume, in dB.
    pub const DEFAULT_VOLUME_DB: f32 = -12.0;

    /// A snapshot with the given volume in dB.
    pub fn with_volume_db(self, volume_db: f32) -> Self {
        Self {
            gain: db_to_gain(volume_db),
            ..self
        }
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            frequency_hz: Self::DEFAULT_FREQUENCY_HZ,
            gain: db_to_gain(Self::DEFAULT_VOLUME_DB),
        }
    }
}

/// Converts decibels to a linear gain. Anything at or below -120 dB is silence.
pub fn db_to_gain(db: f32) -> f32 {
    if db <= -120.0 {
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
}
