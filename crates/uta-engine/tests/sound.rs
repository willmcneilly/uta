//! Sound tests: everything is rendered offline through the real processor and
//! measured from the waveform. See `CLAUDE.md`, "Proving audio code works".

mod common;

use std::path::{Path, PathBuf};

use common::*;
use uta_engine::offline::{self, Renderer};
use uta_engine::{EngineConfig, FADE_SECONDS, Snapshot, VOLUME_SMOOTHING_SECONDS, db_to_gain};

const RATE: u32 = 48_000;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn full_scale() -> Snapshot {
    Snapshot::default().with_volume_db(0.0)
}

/// The click limit: the steepest step a full-scale 440 Hz tone takes, plus
/// 10%. A fade or glide adds a little on top of the tone's own slope; a click
/// is a jump of a large part of the waveform in one sample.
fn click_limit() -> f32 {
    sine_max_step(440.0, RATE) * 1.1
}

/// Renders a steady tone at `volume_db` and returns a stretch well after the
/// fade-in.
fn steady_tone(volume_db: f32) -> Vec<f32> {
    let mut renderer = Renderer::new(config(), Snapshot::default().with_volume_db(volume_db), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(1.1);
    renderer.samples()[RATE as usize / 10..].to_vec()
}

#[test]
fn pitch_is_440_hz() {
    let tone = steady_tone(-12.0);
    let frequency = measure_frequency(&tone, RATE);
    assert!(
        (frequency - 440.0).abs() < 0.01,
        "measured {frequency} Hz, expected 440 Hz within 0.01 Hz"
    );
}

#[test]
fn level_matches_the_volume() {
    for volume_db in [0.0, -6.0, -12.0, -40.0] {
        let tone = steady_tone(volume_db);
        let gain = f64::from(db_to_gain(volume_db));
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
    let mut renderer = Renderer::new(config(), full_scale(), 128);
    renderer.render_seconds(0.1);
    assert_eq!(peak(renderer.samples()), 0.0);
}

#[test]
fn play_fades_in_and_stop_fades_out() {
    let mut renderer = Renderer::new(config(), full_scale(), 128);
    let fade = renderer.frames_for(FADE_SECONDS);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.1);
    renderer.controller.stop().unwrap();
    renderer.render_seconds(0.1);
    let samples = renderer.samples();

    // The fade-in's first samples are quieter than the tone can be.
    assert!(peak(&samples[..fade / 4]) < 0.3, "no fade-in");
    // Stopped: silent once the fade-out is over.
    let stop = renderer.frames_for(0.1);
    assert!(
        peak(&samples[stop..stop + fade]) > 0.0,
        "cut off without a fade-out"
    );
    assert_eq!(
        peak(&samples[stop + fade..]),
        0.0,
        "still sounding after Stop"
    );
}

#[test]
fn volume_change_glides() {
    let mut renderer = Renderer::new(config(), full_scale(), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.2);
    renderer.controller.set_volume_db(-40.0).unwrap();
    renderer.render_seconds(0.2);
    let change = renderer.frames_for(0.2);
    let glide = renderer.frames_for(VOLUME_SMOOTHING_SECONDS);
    let samples = renderer.samples();

    // Halfway through the glide the level is between the two volumes.
    let mid = &samples[change + glide / 2 - 60..change + glide / 2 + 60];
    let level = peak(mid);
    assert!(level > 0.3 && level < 0.7, "mid-glide peak {level}");
    // After the glide it has arrived.
    let after = &samples[change + glide + 100..];
    assert!((peak(after) / db_to_gain(-40.0) - 1.0).abs() < 1e-3);
}

/// Play and Stop over and over, including before a fade finishes, plus big
/// volume jumps, all at full scale: nothing may jump more than the tone does.
#[test]
fn no_clicks_across_play_stop_and_volume() {
    let mut renderer = Renderer::new(config(), full_scale(), 128);
    let limit = click_limit();

    // Twenty quick Play/Stop pairs with different spacings, some shorter than
    // the fade, so the fades are interrupted part-way.
    for i in 0..20 {
        renderer.controller.play().unwrap();
        renderer.render(37 + i * 53);
        renderer.controller.stop().unwrap();
        renderer.render(11 + i * 29);
    }
    // Volume jumps while playing, including mid-glide reversals.
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.05);
    for volume_db in [-60.0, 0.0, -20.0, 0.0, -120.0, 0.0] {
        renderer.controller.set_volume_db(volume_db).unwrap();
        renderer.render(300);
    }
    renderer.controller.stop().unwrap();
    renderer.render_seconds(0.05);

    let (jump, at) = max_jump(renderer.samples());
    assert!(
        jump <= limit,
        "jump of {jump} at sample {at}, limit {limit}"
    );
}

/// Guards the click test itself: a hard cut in a full-scale tone must fail it.
#[test]
fn click_limit_catches_a_hard_cut() {
    let mut tone = steady_tone(0.0);
    // Cut at the tone's loudest point.
    let loudest = tone.iter().position(|s| s.abs() > 0.999).unwrap();
    tone[loudest + 1..].fill(0.0);
    assert!(max_jump(&tone).0 > click_limit() * 5.0);
}

/// Renders the same session in blocks of `block_size`. Events land on
/// multiples of 1024 frames, which every tested block size divides, so they
/// hit the same sample in every render.
fn session(block_size: usize) -> Vec<f32> {
    let mut renderer = Renderer::new(config(), Snapshot::default(), block_size);
    renderer.render(1024);
    renderer.controller.play().unwrap();
    renderer.render(1024 * 10);
    renderer.controller.set_volume_db(-3.0).unwrap();
    renderer.render(1024 * 5);
    renderer
        .controller
        .set_snapshot(Snapshot {
            frequency_hz: 660.0,
            ..renderer.controller.snapshot().clone()
        })
        .unwrap();
    renderer.render(1024 * 5);
    renderer.controller.stop().unwrap();
    renderer.render(1024 * 3);
    renderer.into_samples()
}

#[test]
fn block_size_does_not_change_the_audio() {
    let reference = session(1024);
    for block_size in [32, 128] {
        let difference = max_difference(&session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/tone.wav")
}

/// The milestone 0 render: what `uta render` writes, half a second long.
/// Regenerate with `UTA_GOLDEN=1 cargo test -p uta-engine --test sound`; a
/// human approves every change to the file.
#[test]
fn matches_the_golden_wav() {
    let rendered = offline::render_tone(config(), Snapshot::default(), 0.5);
    let path = golden_path();
    if std::env::var_os("UTA_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        offline::write_wav(&path, config(), &rendered).unwrap();
        eprintln!("Wrote {}", path.display());
        return;
    }
    let (spec, golden) = offline::read_wav(&path).unwrap_or_else(|e| {
        panic!(
            "can't read {} ({e}); regenerate with UTA_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!(spec.sample_rate, RATE);
    assert_eq!(spec.channels, 1);
    let difference = max_difference(&rendered, &golden);
    assert!(
        difference < 1e-5,
        "render differs from the golden WAV by {difference} (limit 1e-5)"
    );
}

#[test]
fn every_channel_carries_the_tone() {
    let stereo = EngineConfig {
        channels: 2,
        ..config()
    };
    let samples = offline::render_tone(stereo, Snapshot::default(), 0.1);
    let mono = offline::render_tone(config(), Snapshot::default(), 0.1);
    assert_eq!(samples.len(), mono.len() * 2);
    for (frame, &expected) in samples.chunks_exact(2).zip(&mono) {
        assert_eq!(frame, [expected, expected]);
    }
}
