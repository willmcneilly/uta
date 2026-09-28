//! The engine plays what the project core says: changes made through the
//! core, and their undo, reach the audio. Rendered offline through the real
//! processor.

mod common;

use common::*;
use uta_core::{Command, Note, Project, Session};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, Snapshot, VOICE_LEVEL, db_to_gain};

const RATE: u32 = 48_000;

/// The RMS of the next 0.2 s, rendered after 0.1 s for any glide to settle.
fn next_level(renderer: &mut Renderer) -> f64 {
    renderer.render_seconds(0.1);
    let start = renderer.samples().len();
    renderer.render_seconds(0.2);
    rms(&renderer.samples()[start..])
}

fn assert_level(rms: f64, volume_db: f32) {
    let expected = f64::from(db_to_gain(volume_db) * VOICE_LEVEL) / std::f64::consts::SQRT_2;
    assert!(
        (rms / expected - 1.0).abs() < 1e-3,
        "{volume_db} dB: RMS {rms}, expected {expected} within 0.1%"
    );
}

fn send(renderer: &mut Renderer, session: &Session) {
    renderer
        .controller
        .set_snapshot(Snapshot::from(session.project()))
        .unwrap();
}

#[test]
fn core_changes_and_their_undo_reach_the_audio() {
    // A 1-bar loop of one full-velocity sine note filling the bar.
    let held = Note {
        velocity: 127,
        ..note(0, 69, 0, 3840)
    };
    let mut session = Session::new(project(120.0, 1, &PLAIN_SINE, vec![held]));
    let config = EngineConfig {
        sample_rate: RATE,
        channels: 1,
    };
    let mut renderer = Renderer::new(config, Snapshot::from(session.project()), 128);
    renderer.controller.play().unwrap();
    assert_level(next_level(&mut renderer), Project::DEFAULT_MASTER_VOLUME_DB);

    session
        .apply(Command::SetMasterVolume { volume_db: -3.0 })
        .unwrap();
    send(&mut renderer, &session);
    assert_level(next_level(&mut renderer), -3.0);

    assert!(!session.undo().is_empty());
    send(&mut renderer, &session);
    assert_level(next_level(&mut renderer), Project::DEFAULT_MASTER_VOLUME_DB);
}

#[test]
fn notes_added_through_the_core_play_and_undo_removes_them() {
    let mut session = Session::new(project(120.0, 1, &PLAIN_SINE, vec![]));
    let clip = session.project().tracks()[0].clips()[0].id();
    let config = EngineConfig {
        sample_rate: RATE,
        channels: 1,
    };
    let mut renderer = Renderer::new(config, Snapshot::from(session.project()), 128);
    renderer.controller.play().unwrap();

    // Added half way through the pass, after the note's place: it waits
    // for the next pass.
    renderer.render(48_000);
    session
        .apply(Command::AddNotes {
            clip,
            notes: vec![note(0, 69, 0, 960)],
        })
        .unwrap();
    send(&mut renderer, &session);
    renderer.render(48_000);
    assert_eq!(peak(renderer.samples()), 0.0, "added mid-pass: waits");
    renderer.render(24_000 + 100);
    assert!(peak(&renderer.samples()[96_000..]) > 0.0, "the note played");

    // Undone once the note has ended (releasing a note deleted while it
    // sounds is UTA-11): the next pass is silent.
    assert!(!session.undo().is_empty());
    send(&mut renderer, &session);
    renderer.render(72_000 + 96_000);
    assert_eq!(
        first_sound(&renderer.samples()[96_000 + 24_000 + 100..]),
        None
    );
}
