//! A track's mixer strip as the engine plays it. See RFC-003, "The mixer in
//! the track headers".

use crate::db_to_gain;

/// A track's volume, pan, mute and solo: the core's mixer strip, carried in
/// the snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerStrip {
    /// The track's volume, in dB. Clamped to [`MixerStrip::MAX_VOLUME_DB`].
    pub volume_db: f32,
    /// From -1 (left) through 0 (centre) to 1 (right). Clamped to that range.
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
}

impl MixerStrip {
    /// The loudest a track can be, in dB. Tracks are mixed as floating-point
    /// numbers, so a track above full scale only clips if the master is too.
    pub const MAX_VOLUME_DB: f32 = 6.0;

    /// The gain for the left and right speakers: the volume, then the pan,
    /// then the mute and solo. NaN volume or pan is silence.
    ///
    /// `soloing` says whether any track is soloed. A track plays if it isn't
    /// muted, and either no track is soloed or it is, so mute wins even on a
    /// soloed track. See RFC-003, "The mixer in the track headers".
    ///
    /// Pan uses the "−3 dB compensated" constant-power law: centred, both
    /// speakers get the track unchanged; panned hard to one side, that
    /// speaker gets it 3 dB louder and the other nothing. Worked out in `f64`,
    /// so the centre comes out at exactly 1 and the far side at exactly 0.
    pub fn gains(&self, soloing: bool) -> [f32; 2] {
        if self.mute || (soloing && !self.solo) || self.volume_db.is_nan() || self.pan.is_nan() {
            return [0.0, 0.0];
        }
        let volume = f64::from(db_to_gain(self.volume_db.min(Self::MAX_VOLUME_DB)));
        let pan = f64::from(self.pan.clamp(-1.0, 1.0));
        let quarter = std::f64::consts::FRAC_PI_4;
        let side = |towards: f64| {
            (volume * std::f64::consts::SQRT_2 * ((1.0 + towards) * quarter).sin()) as f32
        };
        [side(-pan), side(pan)]
    }
}

impl Default for MixerStrip {
    /// Full volume, centred, not muted or soloed: the track passes through
    /// unchanged.
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
        }
    }
}

/// The core's mixer strip as the engine plays it.
impl From<&uta_core::MixerStrip> for MixerStrip {
    fn from(strip: &uta_core::MixerStrip) -> Self {
        Self {
            volume_db: strip.volume_db,
            pan: strip.pan,
            mute: strip.mute,
            solo: strip.solo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(volume_db: f32, pan: f32) -> MixerStrip {
        MixerStrip {
            volume_db,
            pan,
            ..MixerStrip::default()
        }
    }

    #[test]
    fn centred_at_0_db_passes_through_exactly() {
        assert_eq!(MixerStrip::default().gains(false), [1.0, 1.0]);
    }

    #[test]
    fn panned_hard_is_3_db_up_on_one_side_and_silent_on_the_other() {
        let root_two = std::f32::consts::SQRT_2;
        assert_eq!(strip(0.0, -1.0).gains(false), [root_two, 0.0]);
        assert_eq!(strip(0.0, 1.0).gains(false), [0.0, root_two]);
        // Past the ends is the ends.
        assert_eq!(strip(0.0, -3.0).gains(false), [root_two, 0.0]);
        assert_eq!(strip(0.0, 3.0).gains(false), [0.0, root_two]);
    }

    #[test]
    fn in_between_the_power_stays_the_same() {
        for pan in [-0.75, -0.5, -0.1, 0.0, 0.3, 0.5, 0.9] {
            let [left, right] = strip(0.0, pan).gains(false);
            let power = left * left + right * right;
            assert!((power - 2.0).abs() < 1e-6, "pan {pan}: power {power}");
            assert_eq!(strip(0.0, -pan).gains(false), [right, left], "pan {pan}");
        }
    }

    #[test]
    fn volume_scales_both_sides_up_to_6_db() {
        let [left, right] = strip(-6.0, 0.0).gains(false);
        assert!((left - db_to_gain(-6.0)).abs() < 1e-7);
        assert_eq!(left, right);
        assert_eq!(strip(6.0, 0.0).gains(false), [db_to_gain(6.0); 2]);
        assert_eq!(strip(20.0, 0.0).gains(false), [db_to_gain(6.0); 2]);
        assert_eq!(strip(-120.0, 0.0).gains(false), [0.0, 0.0]);
    }

    #[test]
    fn mute_and_nan_are_silent() {
        let muted = MixerStrip {
            mute: true,
            ..MixerStrip::default()
        };
        assert_eq!(muted.gains(false), [0.0, 0.0]);
        assert_eq!(strip(f32::NAN, 0.0).gains(false), [0.0, 0.0]);
        assert_eq!(strip(0.0, f32::NAN).gains(false), [0.0, 0.0]);
    }

    #[test]
    fn solo_silences_the_tracks_that_are_not_soloed_and_mute_wins() {
        let soloed = MixerStrip {
            solo: true,
            ..MixerStrip::default()
        };
        let muted_and_soloed = MixerStrip {
            mute: true,
            ..soloed
        };
        // Nothing soloed: solo makes no difference.
        assert_eq!(soloed.gains(false), [1.0, 1.0]);
        // Something soloed: only soloed tracks play, unless they're muted.
        assert_eq!(soloed.gains(true), [1.0, 1.0]);
        assert_eq!(MixerStrip::default().gains(true), [0.0, 0.0]);
        assert_eq!(muted_and_soloed.gains(true), [0.0, 0.0]);
        assert_eq!(muted_and_soloed.gains(false), [0.0, 0.0]);
    }

    #[test]
    fn copies_the_cores_strip() {
        let core = uta_core::MixerStrip {
            volume_db: -3.0,
            pan: 0.25,
            mute: true,
            solo: true,
        };
        let strip = MixerStrip::from(&core);
        assert_eq!(
            (strip.volume_db, strip.pan, strip.mute, strip.solo),
            (-3.0, 0.25, true, true)
        );
        assert_eq!(
            MixerStrip::from(&uta_core::MixerStrip::default()),
            MixerStrip::default()
        );
    }
}
