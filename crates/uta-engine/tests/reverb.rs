//! The Reverb through the whole engine: added by a command, its state made
//! on the control side and handed to the audio thread, played by the real
//! processor, and rendered until its tail has rung out. Its own sound is
//! tested on impulse responses in `effects/reverb/tests.rs`. See RFC-010,
//! "Fitting the chain".

mod common;

use common::{PLAIN_SINE, note, peak, project};
use uta_core::{
    Chain, Command, Effect, EffectId, EffectSettings, PlacedEffect, Project, ReverbSettings,
};
use uta_engine::offline::{self, MAX_TAIL_SECONDS};
use uta_engine::{EngineConfig, Snapshot};
use uuid::Uuid;

/// Below this, −120 dB, a sample is silence.
const SILENCE: f32 = 1e-6;

fn stereo() -> EngineConfig {
    EngineConfig {
        sample_rate: 48_000,
        channels: 2,
    }
}

/// A short A4, played once.
fn stab() -> Project {
    let mut project = project(120.0, 4, &PLAIN_SINE, vec![note(0, 69, 0, 240)]);
    project
        .apply(&Command::SetLoopEnabled { enabled: false })
        .unwrap();
    project
}

/// `project` with a Reverb with `decay_seconds` at the end of `chain`.
fn with_reverb(mut project: Project, chain: Chain, decay_seconds: f32) -> Project {
    let settings = ReverbSettings {
        decay_seconds,
        ..ReverbSettings::default()
    };
    let effect = Effect::new(
        EffectId::from_uuid(Uuid::from_u128(9000)),
        EffectSettings::Reverb(settings),
    )
    .with_mix(0.5);
    project
        .apply(&Command::AddEffects {
            effects: vec![PlacedEffect {
                chain,
                index: 0,
                effect,
            }],
        })
        .unwrap();
    project
}

/// `uta render` runs on until the Reverb sleeps: on a track or the master,
/// a 2 s tail falls 120 dB in 4 s, then what's left in its lines passes
/// through, and the render ends in silence, not cut off.
#[test]
fn a_render_runs_on_until_the_tail_has_rung_out() {
    let seconds = 0.5;
    for chain in [Chain::Track(stab().tracks()[0].id()), Chain::Master] {
        let song = offline::render_song(
            stereo(),
            Snapshot::from(&with_reverb(stab(), chain, 2.0)),
            seconds,
        );
        assert!(!song.cut_off, "{chain:?}");
        let tail = song.samples.len() as f64 / 2.0 / 48_000.0 - seconds;
        assert!(
            (4.0..7.0).contains(&tail),
            "a tail of {tail} s on {chain:?}"
        );
        let end = song.samples.len();
        assert!(peak(&song.samples[end - 2 * 4800..]) < SILENCE);
        assert!(peak(&song.samples[end - 2 * 48_000 * 3..]) > 1e-8);
    }
}

/// A 30 s tail is still ringing at about −60 dB when the render's limit
/// stops it, and the render says so (RFC-010's Risks, "Long tails and the
/// render limit").
#[test]
fn a_30_second_tail_is_cut_off_at_the_limit_and_says_so() {
    let track = Chain::Track(stab().tracks()[0].id());
    let song = offline::render_song(
        stereo(),
        Snapshot::from(&with_reverb(
            stab(),
            track,
            ReverbSettings::MAX_DECAY_SECONDS,
        )),
        0.5,
    );
    assert!(song.cut_off);
    let tail = song.samples.len() as f64 / 2.0 / 48_000.0 - 0.5;
    assert!(
        (tail - MAX_TAIL_SECONDS).abs() < 0.01,
        "a tail of {tail} s, not {MAX_TAIL_SECONDS}"
    );
    let end = song.samples.len();
    assert!(peak(&song.samples[end - 2 * 48_000..]) > SILENCE);
}
