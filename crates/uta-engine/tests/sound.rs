//! Sound tests: everything is rendered offline through the real processor and
//! measured from the waveform. See `CLAUDE.md`, "Proving audio code works".
//!
//! These cover the transport and the master volume, with the loop playing,
//! and the demo loop and demo song as a whole.
//! The synth's own sound is in `synth.rs`, and note timing in `sequencer.rs`.

mod common;

use std::path::Path;

use common::*;
use uta_core::time::Ticks;
use uta_core::{ClipPosition, Command, Project};
use uta_engine::offline::{self, Renderer};
use uta_engine::{
    EngineConfig, Snapshot, VOICE_LEVEL, VOLUME_SMOOTHING_SECONDS, db_to_gain, pitch_to_hz,
};

const RATE: u32 = 48_000;
/// A 1-bar loop at 120 BPM, in samples.
const LOOP: usize = 96_000;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn stereo() -> EngineConfig {
    EngineConfig {
        channels: 2,
        ..config()
    }
}

/// A 1-bar loop of one A4 sine note filling the bar, at full velocity: a
/// steady tone at exactly [`VOICE_LEVEL`] times the master volume between
/// the note's attack and its release at the loop's end.
fn held_a4() -> Project {
    let a4 = uta_core::Note {
        velocity: 127,
        ..note(0, 69, 0, 3840)
    };
    project(120.0, 1, &PLAIN_SINE, vec![a4])
}

/// The click limit for one sine voice at `level`: the steepest step the sine
/// takes, plus the steepest the 5 ms attack adds, plus 10%. A click is a
/// jump of a large part of the waveform in one sample.
fn click_limit(level: f32) -> f32 {
    let attack = 0.005 * f64::from(RATE);
    (sine_max_step(pitch_to_hz(69), RATE) * level + level / attack as f32) * 1.1
}

/// Renders the steady part of the held note at `volume_db`: from 0.1 s into
/// the loop to 0.1 s before its end.
fn steady_note(volume_db: f32) -> Vec<f32> {
    let snapshot = Snapshot::from(&held_a4()).with_volume_db(volume_db);
    let mut renderer = Renderer::new(config(), snapshot, 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP);
    renderer.samples()[LOOP / 20..LOOP - LOOP / 20].to_vec()
}

#[test]
fn level_matches_the_volume() {
    for volume_db in [0.0, -6.0, -12.0, -40.0] {
        let tone = steady_note(volume_db);
        let gain = f64::from(db_to_gain(volume_db) * VOICE_LEVEL);
        let rms = rms(&tone);
        let expected_rms = gain / std::f64::consts::SQRT_2;
        assert!(
            (rms / expected_rms - 1.0).abs() < 1e-3,
            "{volume_db} dB: RMS {rms}, expected {expected_rms} within 0.1%"
        );
        let peak = f64::from(peak(&tone));
        assert!(
            (peak / gain - 1.0).abs() < 1e-3,
            "{volume_db} dB: peak {peak}, expected {gain} within 0.1%"
        );
    }
}

#[test]
fn silent_until_play() {
    let mut renderer = Renderer::new(config(), Snapshot::from(&demo_loop()), 128);
    renderer.render_seconds(1.0);
    assert_eq!(peak(renderer.samples()), 0.0);
}

#[test]
fn an_empty_loop_is_silent() {
    let mut renderer = Renderer::new(config(), Snapshot::default(), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(1.0);
    assert_eq!(peak(renderer.samples()), 0.0);
    assert!(renderer.controller.poll().playing);
}

#[test]
fn stop_releases_rather_than_cuts() {
    let snapshot = Snapshot::from(&held_a4()).with_volume_db(0.0);
    let mut renderer = Renderer::new(config(), snapshot, 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP / 2);
    renderer.controller.stop().unwrap();
    renderer.render(LOOP / 2);
    let samples = renderer.samples();
    let release = renderer.frames_for(0.001);

    assert!(
        peak(&samples[LOOP / 2..LOOP / 2 + release]) > 0.0,
        "cut off without a release"
    );
    assert_eq!(
        peak(&samples[LOOP / 2 + release + 1..]),
        0.0,
        "still sounding after Stop"
    );
    let (jump, at) = max_jump(samples);
    let limit = click_limit(VOICE_LEVEL);
    assert!(
        jump <= limit,
        "jump of {jump} at sample {at}, limit {limit}"
    );
}

#[test]
fn volume_change_glides() {
    let snapshot = Snapshot::from(&held_a4()).with_volume_db(0.0);
    let mut renderer = Renderer::new(config(), snapshot, 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.2);
    renderer.controller.set_volume_db(-40.0).unwrap();
    renderer.render_seconds(0.2);
    let change = renderer.frames_for(0.2);
    let glide = renderer.frames_for(VOLUME_SMOOTHING_SECONDS);
    let samples = renderer.samples();

    // Halfway through the glide the level is between the two volumes.
    let mid = &samples[change + glide / 2 - 60..change + glide / 2 + 60];
    let level = peak(mid) / VOICE_LEVEL;
    assert!(level > 0.3 && level < 0.7, "mid-glide level {level}");
    // After the glide it has arrived.
    let after = &samples[change + glide + 100..];
    assert!((peak(after) / (db_to_gain(-40.0) * VOICE_LEVEL) - 1.0).abs() < 1e-3);
}

/// Play and Stop over and over, big volume jumps, and the loop point, with
/// the note at full level: nothing may jump more than the note itself does.
#[test]
fn no_clicks_across_play_stop_volume_and_the_loop_point() {
    let snapshot = Snapshot::from(&held_a4()).with_volume_db(0.0);
    let mut renderer = Renderer::new(config(), snapshot, 128);

    // Twenty quick Play/Stop pairs with different spacings, some shorter than
    // the attack, so notes are released part-way in.
    for i in 0..20 {
        renderer.controller.play().unwrap();
        renderer.render(37 + i * 53);
        renderer.controller.stop().unwrap();
        renderer.render(11 + i * 29);
    }
    // Round the loop a few times, with volume jumps while it plays,
    // including mid-glide reversals.
    renderer.controller.play().unwrap();
    for volume_db in [-60.0, 0.0, -20.0, 0.0, -120.0, 0.0] {
        renderer.controller.set_volume_db(volume_db).unwrap();
        renderer.render(300);
    }
    renderer.render(3 * LOOP);
    renderer.controller.stop().unwrap();
    renderer.render_seconds(0.05);

    let limit = click_limit(VOICE_LEVEL);
    let (jump, at) = max_jump(renderer.samples());
    assert!(
        jump <= limit,
        "jump of {jump} at sample {at}, limit {limit}"
    );
}

/// Guards the click test itself: a hard cut in a full-level note must fail it.
#[test]
fn click_limit_catches_a_hard_cut() {
    let mut tone = steady_note(0.0);
    // Cut at the note's loudest point.
    let loudest = tone
        .iter()
        .position(|s| s.abs() > 0.999 * VOICE_LEVEL)
        .unwrap();
    tone[loudest + 1..].fill(0.0);
    assert!(max_jump(&tone).0 > click_limit(VOICE_LEVEL) * 5.0);
}

/// Renders the demo loop in blocks of `block_size`, round the loop and on,
/// with a volume change, a Stop and a Play. The notes land wherever the tempo
/// puts them; the commands land on multiples of 1024 frames, which every
/// tested block size divides, so they hit the same sample in every render.
fn session(block_size: usize) -> Vec<f32> {
    let mut renderer = Renderer::new(config(), Snapshot::from(&demo_loop()), block_size);
    renderer.render(1024);
    renderer.controller.play().unwrap();
    renderer.render(1024 * 150);
    renderer.controller.set_volume_db(-3.0).unwrap();
    renderer.render(1024 * 100);
    renderer.controller.stop().unwrap();
    renderer.render(1024 * 10);
    renderer.controller.play().unwrap();
    renderer.render(1024 * 50);
    renderer.into_samples()
}

#[test]
fn block_size_does_not_change_the_audio() {
    let reference = session(1024);
    // Round the demo loop (4.3 s) at least once.
    assert!(reference.len() > 3 * 48_000 * 2);
    for block_size in [32, 128] {
        let difference = max_difference(&session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}

/// Renders the demo song in stereo in blocks of `block_size`, round the
/// loop and on, while a track is muted, soloed and moved, a clip is moved and
/// the volume changes. As in [`session`], every change lands on a multiple
/// of 1024 frames.
fn song_session(block_size: usize) -> Vec<f32> {
    let mut project = demo_song();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), block_size);
    let ids: Vec<_> = project.tracks().iter().map(|track| track.id()).collect();
    let lead = project.tracks()[2].clips()[1].clone();
    let mixer = |mute, solo| uta_core::MixerStrip {
        mute,
        solo,
        ..uta_core::MixerStrip::default()
    };
    let changes = [
        Command::SetTrackMixer {
            track: ids[1],
            mixer: mixer(true, false),
        },
        Command::MoveTrack {
            track: ids[0],
            index: 2,
        },
        Command::SetTrackMixer {
            track: ids[2],
            mixer: mixer(false, true),
        },
        Command::SetClips {
            clips: vec![ClipPosition {
                id: lead.id(),
                track: ids[2],
                start: lead.start() + 960,
                length: lead.length(),
            }],
        },
        Command::SetTrackMixer {
            track: ids[2],
            mixer: mixer(false, false),
        },
    ];
    renderer.controller.play().unwrap();
    renderer.render(1024 * 100);
    for command in &changes {
        project.apply(command).unwrap();
        renderer.controller.set_project(&project).unwrap();
        renderer.render(1024 * 60);
    }
    renderer.controller.set_volume_db(-3.0).unwrap();
    renderer.render(1024 * 60);
    renderer.controller.stop().unwrap();
    renderer.render(1024 * 40);
    renderer.into_samples()
}

#[test]
fn block_size_does_not_change_the_audio_with_several_tracks() {
    let reference = song_session(1024);
    // Round the demo song's loop (8.6 s) at least once while playing.
    assert!(reference.len() > 10 * 48_000 * 2);
    assert!(peak(&reference) > 0.1);
    for block_size in [32, 128] {
        let difference = max_difference(&song_session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}

/// How long a golden render plays: once round the loop and a beat more, so
/// it includes the loop point.
fn golden_seconds(project: &Project) -> f64 {
    let transport = project.transport();
    let ticks: Ticks = transport.loop_length() + 960;
    transport.tempo_map().ticks_to_samples(ticks, RATE) as f64 / f64::from(RATE)
}

/// Compares `project`'s render with `tests/golden/<name>.wav`, or writes
/// the file with `UTA_GOLDEN=1`. A human approves every change to a golden
/// file.
fn check_golden(name: &str, config: EngineConfig, project: &Project) {
    let rendered = offline::render_loop(config, Snapshot::from(project), golden_seconds(project));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.wav"));
    if std::env::var_os("UTA_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        offline::write_wav(&path, config, &rendered).unwrap();
        eprintln!("Wrote {}", path.display());
        return;
    }
    let (spec, golden) = offline::read_wav(&path).unwrap_or_else(|e| {
        panic!(
            "can't read {} ({e}); regenerate with UTA_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!(spec.sample_rate, config.sample_rate);
    assert_eq!(usize::from(spec.channels), config.channels);
    let difference = max_difference(&rendered, &golden);
    assert!(
        difference < 1e-5,
        "{name}: render differs from the golden WAV by {difference} (limit 1e-5)"
    );
}

/// The demo loop, as `uta render --commands examples/demo-loop.json` writes
/// it but shorter and in mono. Regenerate with
/// `UTA_GOLDEN=1 cargo test -p uta-engine --test sound`; a human approves
/// every change to the file.
#[test]
fn matches_the_golden_wav() {
    check_golden("demo-loop", config(), &demo_loop());
}

/// The demo song, three tracks with their own sounds panned apart, as
/// `uta render --commands examples/demo-song.json` writes it but shorter.
/// Regenerated the same way as the demo loop's, and approved the same way.
#[test]
fn the_demo_song_matches_its_golden_wav() {
    let project = demo_song();
    assert_eq!(project.tracks().len(), 3);
    check_golden("demo-song", stereo(), &project);
}

#[test]
fn a_render_ends_in_silence() {
    let rendered = offline::render_loop(config(), Snapshot::from(&demo_loop()), 1.0);
    assert!(peak(&rendered[..rendered.len() / 2]) > 0.1, "too quiet");
    assert_eq!(*rendered.last().unwrap(), 0.0);
}

#[test]
fn every_channel_carries_the_sound() {
    let snapshot = Snapshot::from(&demo_loop());
    let samples = offline::render_loop(stereo(), snapshot.clone(), 0.5);
    let mono = offline::render_loop(config(), snapshot, 0.5);
    assert_eq!(samples.len(), mono.len() * 2);
    assert!(peak(&mono) > 0.0);
    let (frames, rest) = samples.as_chunks::<2>();
    assert!(rest.is_empty());
    for (frame, &expected) in frames.iter().zip(&mono) {
        assert_eq!(*frame, [expected, expected]);
    }
}
