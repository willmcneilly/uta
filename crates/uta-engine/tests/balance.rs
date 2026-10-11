//! The default kit's balance, measured: RFC-006, "What makes it sound
//! good", point 7. Each sound's default Level sets it against the kick by
//! loudness, and the whole kit hitting at once leaves headroom.

mod common;

use common::*;
use uta_core::{Command, DrumParam, DrumSound, KickModel, KitSettings, Note, Project};

/// How long a hit's loudness is measured over. The ear sums a sound's
/// energy over about this long, so a closed hat that's over in 60 ms and an
/// open hat that rings both count as heard; the research suggests 100 to
/// 200 ms.
const LOUDNESS_SECONDS: f64 = 0.15;

/// The kit's eight notes.
const EVERY_NOTE: [u8; 8] = [
    KICK, SNARE, CLAP, CLOSED_HAT, OPEN_HAT, LOW_TOM, HIGH_TOM, CYMBAL,
];

const PLAY_909: (DrumSound, DrumParam) = (DrumSound::Kick, DrumParam::Model(KickModel::Tr909));

/// [`drum_project`] with the kit at its real defaults, default Levels and
/// all.
fn default_kit_project(params: &[(DrumSound, DrumParam)], hits: Vec<Note>) -> Project {
    drum_project_with_kit(KitSettings::default(), 60.0, 1, params, hits)
}

/// A hit on `pitch` at full accent, with the kit at its defaults except for
/// `params`: its loudness, in dB.
fn loudness(pitch: u8, params: &[(DrumSound, DrumParam)]) -> f64 {
    let project = default_kit_project(params, vec![hit(0, pitch, 127, 0)]);
    hit_loudness_db(&render_drums(&project, 0.5, 128), LOUDNESS_SECONDS)
}

/// Each sound's default Level puts it at its target against the kick: the
/// snare and clap 2 to 4 dB under it, the toms 6 dB (within half a dB), and
/// the hats and cymbal 10 to 12 dB. The 909 kick is as loud as the 808's,
/// within half a dB, so choosing it doesn't change the balance.
#[test]
fn the_default_levels_balance_the_kit_by_loudness() {
    let kick = loudness(KICK, &[]);
    let targets = [
        ("snare", SNARE, -4.0..=-2.0),
        ("clap", CLAP, -4.0..=-2.0),
        ("low tom", LOW_TOM, -6.5..=-5.5),
        ("high tom", HIGH_TOM, -6.5..=-5.5),
        ("closed hat", CLOSED_HAT, -12.0..=-10.0),
        ("open hat", OPEN_HAT, -12.0..=-10.0),
        ("cymbal", CYMBAL, -12.0..=-10.0),
    ];
    for (name, pitch, target) in targets {
        let against_kick = loudness(pitch, &[]) - kick;
        assert!(
            target.contains(&against_kick),
            "{name}: {against_kick:.2} dB against the kick, wanted {target:?}"
        );
    }
    let kick_909 = loudness(KICK, &[PLAY_909]) - kick;
    assert!(
        kick_909.abs() <= 0.5,
        "909 kick: {kick_909:.2} dB against the 808's"
    );
}

/// Every sound at full accent at once, with either kick, peaks under
/// -1 dBFS with the track and master at 0 dB. The output stops at full
/// scale, so the master is turned down 20 dB to see past it, and the 20 dB
/// added back.
#[test]
fn the_whole_kit_at_once_peaks_under_minus_1_dbfs() {
    for params in [vec![], vec![PLAY_909]] {
        let hits = EVERY_NOTE
            .iter()
            .enumerate()
            .map(|(i, &pitch)| hit(i as u128, pitch, 127, 0))
            .collect();
        let mut project = default_kit_project(&params, hits);
        project
            .apply(&Command::SetMasterVolume { volume_db: -20.0 })
            .unwrap();
        let samples = render_drums(&project, 2.0, 128);
        let peak_dbfs = 20.0 * f64::from(peak(&samples)).log10() + 20.0;
        assert!(peak_dbfs < -1.0, "{params:?}: peaks at {peak_dbfs:.2} dBFS");
    }
}
