//! The engine plays what the project core says: a change made through the
//! core, and its undo, reach the audio. Rendered offline through the real
//! processor.

mod common;

use common::*;
use uta_core::{Command, Project, Session};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, Snapshot, db_to_gain};

const RATE: u32 = 48_000;

/// The RMS of the next 0.2 s, rendered after 0.1 s for any glide to settle.
fn next_level(renderer: &mut Renderer) -> f64 {
    renderer.render_seconds(0.1);
    let start = renderer.samples().len();
    renderer.render_seconds(0.2);
    rms(&renderer.samples()[start..])
}

fn assert_level(rms: f64, volume_db: f32) {
    let expected = f64::from(db_to_gain(volume_db)) / std::f64::consts::SQRT_2;
    assert!(
        (rms / expected - 1.0).abs() < 1e-3,
        "{volume_db} dB: RMS {rms}, expected {expected} within 0.1%"
    );
}

#[test]
fn core_changes_and_their_undo_reach_the_audio() {
    let mut session = Session::new(Project::new());
    let config = EngineConfig {
        sample_rate: RATE,
        channels: 1,
    };
    let mut renderer = Renderer::new(config, Snapshot::from(session.project()), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.1); // past the fade-in
    assert_level(next_level(&mut renderer), Project::DEFAULT_MASTER_VOLUME_DB);

    session
        .apply(Command::SetMasterVolume { volume_db: -3.0 })
        .unwrap();
    renderer
        .controller
        .set_snapshot(Snapshot::from(session.project()))
        .unwrap();
    assert_level(next_level(&mut renderer), -3.0);

    session.undo().unwrap();
    renderer
        .controller
        .set_snapshot(Snapshot::from(session.project()))
        .unwrap();
    assert_level(next_level(&mut renderer), Project::DEFAULT_MASTER_VOLUME_DB);
}
