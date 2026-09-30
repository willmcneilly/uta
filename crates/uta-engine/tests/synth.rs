//! Synth sound tests: notes are started through the controller, rendered
//! offline through the real processor, and measured from the waveform. See
//! `CLAUDE.md`, "Proving audio code works".
//!
//! The transport stays stopped, so only the notes started here sound. The
//! master volume is 0 dB, so a voice at full velocity
//! peaks at [`VOICE_LEVEL`].

mod common;

use std::f64::consts::PI;

use common::*;
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, NoteKey, Snapshot, SynthSettings, VOICE_LEVEL, VOICES, Waveform, pitch_to_hz,
    velocity_to_gain,
};

const RATE: u32 = 48_000;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn renderer(settings: SynthSettings, block_size: usize) -> Renderer {
    let snapshot = with_synth(Snapshot::default().with_volume_db(0.0), settings);
    Renderer::new(config(), snapshot, block_size)
}

fn sine() -> SynthSettings {
    SynthSettings {
        waveform: Waveform::Sine,
        ..SynthSettings::default()
    }
}

fn seconds(s: f64) -> usize {
    (s * f64::from(RATE)).round() as usize
}

/// Holds one note for `seconds` and returns everything rendered.
fn held_note(settings: SynthSettings, pitch: u8, velocity: u8, hold: f64) -> Vec<f32> {
    let mut renderer = renderer(settings, 128);
    renderer
        .controller
        .note_on(0, NoteKey(1), pitch, velocity)
        .unwrap();
    renderer.render_seconds(hold);
    renderer.into_samples()
}

/// The steady part of a held note, after its attack and decay.
fn steady(settings: SynthSettings, pitch: u8, velocity: u8) -> Vec<f32> {
    held_note(settings, pitch, velocity, 1.0)[seconds(0.4)..].to_vec()
}

#[test]
fn notes_are_silent_until_played() {
    let mut renderer = renderer(SynthSettings::default(), 128);
    renderer.render_seconds(0.1);
    assert_eq!(peak(renderer.samples()), 0.0);
}

#[test]
fn notes_play_at_their_pitch() {
    for waveform in [
        Waveform::Sine,
        Waveform::Triangle,
        Waveform::Saw,
        Waveform::Square,
    ] {
        for pitch in [21, 33, 45, 57, 69, 81, 93, 105] {
            let settings = SynthSettings {
                waveform,
                ..SynthSettings::default()
            };
            let frequency = measure_frequency(&steady(settings, pitch, 100), RATE);
            let expected = pitch_to_hz(pitch);
            assert!(
                (frequency / expected - 1.0).abs() < 1e-4,
                "{waveform:?} note {pitch}: measured {frequency} Hz, expected {expected} Hz"
            );
        }
    }
}

/// A full-velocity sine peaks at the voice level times the sustain level, and
/// each velocity is `40 × log10(velocity / 127)` dB below that.
#[test]
fn velocity_sets_the_level() {
    let settings = SynthSettings {
        sustain: 1.0,
        ..sine()
    };
    let full = rms(&steady(settings, 69, 127));
    let expected_full = f64::from(VOICE_LEVEL) / std::f64::consts::SQRT_2;
    assert!(
        (full / expected_full - 1.0).abs() < 1e-3,
        "full velocity: RMS {full}, expected {expected_full}"
    );
    for velocity in [100, 64, 32, 1] {
        let level_db = 20.0 * (rms(&steady(settings, 69, velocity)) / full).log10();
        let expected_db = 40.0 * (f64::from(velocity) / 127.0).log10();
        assert!(
            (level_db - expected_db).abs() < 0.01,
            "velocity {velocity}: {level_db} dB, expected {expected_db} dB"
        );
        let gain_db = 20.0 * f64::from(velocity_to_gain(velocity)).log10();
        assert!((gain_db - expected_db).abs() < 1e-3);
    }
}

/// Attack rises in a straight line to full level, decay reaches the sustain
/// level, and release reaches silence, each exactly when its time is up.
#[test]
fn envelope_stages_take_their_set_times() {
    let (attack, decay, sustain, release) = (0.1, 0.3, 0.5, 0.4);
    let settings = SynthSettings {
        attack_seconds: attack,
        decay_seconds: decay,
        sustain,
        release_seconds: release,
        ..sine()
    };
    // Block size 32 divides every event time below, so the note starts and
    // releases on those exact samples.
    let mut renderer = renderer(settings, 32);
    let hold = seconds(0.8);
    renderer.controller.note_on(0, NoteKey(1), 81, 127).unwrap();
    renderer.render(hold);
    renderer.controller.note_off(0, NoteKey(1)).unwrap();
    renderer.render(seconds(0.6));
    let samples = renderer.samples();

    // One cycle of the 880 Hz sine, for reading its level.
    let cycle = (f64::from(RATE) / 880.0).ceil() as usize;
    let level = |at_seconds: f64| {
        f64::from(level_at(samples, seconds(at_seconds), cycle)) / f64::from(VOICE_LEVEL)
    };
    let (attack, decay, sustain, release) = (
        f64::from(attack),
        f64::from(decay),
        f64::from(sustain),
        f64::from(release),
    );
    // A cycle of a straight rise, in level.
    let slack = cycle as f64 / (attack * f64::from(RATE));

    assert_eq!(samples[0], 0.0, "the attack starts from silence");
    for share in [0.25, 0.5, 0.75] {
        let at = level(attack * share);
        assert!(
            (at - share).abs() < slack,
            "{share} of the way through the attack the level is {at}"
        );
    }
    assert!(level(attack) > 0.99, "full level at the end of the attack");

    let early_decay = level(attack + decay * 0.1);
    assert!(early_decay > sustain + 0.2, "decay starts at {early_decay}");
    let end_of_decay = level(attack + decay);
    assert!(
        (end_of_decay - sustain).abs() < 0.005,
        "decay ends at {end_of_decay}, expected {sustain}"
    );
    let sustained = level(0.75);
    assert!(
        (sustained - sustain).abs() < 1e-3,
        "sustains at {sustained}"
    );

    // Release: still sounding until its time is up, and silent after it.
    let released = hold + seconds(release);
    assert!(peak(&samples[released - seconds(0.05)..released]) > 0.0);
    assert!(
        samples[released..].iter().all(|&s| s == 0.0),
        "still sounding after the release"
    );
}

/// The filter's level at `frequency_hz` relative to an open filter: a
/// Butterworth low-pass with the cutoff pre-warped, as a trapezoidal filter
/// is. It's -3 dB at the cutoff and falls 12 dB per octave above it.
fn expected_filter_db(frequency_hz: f64, cutoff_hz: f64) -> f64 {
    let warp = |f: f64| (PI * f / f64::from(RATE)).tan();
    let ratio = warp(frequency_hz) / warp(cutoff_hz);
    -10.0 * (1.0 + ratio.powi(4)).log10()
}

#[test]
fn the_filter_cuts_above_the_cutoff() {
    let cutoff = pitch_to_hz(81); // 880 Hz
    let filtered = SynthSettings {
        cutoff_hz: cutoff as f32,
        sustain: 1.0,
        ..sine()
    };
    let open = SynthSettings {
        sustain: 1.0,
        ..sine()
    };
    // An octave below, at, one and two octaves above the cutoff.
    for pitch in [69, 81, 93, 105] {
        let level_db =
            20.0 * (rms(&steady(filtered, pitch, 127)) / rms(&steady(open, pitch, 127))).log10();
        let expected = expected_filter_db(pitch_to_hz(pitch), cutoff);
        assert!(
            (level_db - expected).abs() < 0.1,
            "note {pitch}: {level_db} dB, expected {expected} dB"
        );
    }
}

/// The loudest partial in `spectrum` that isn't a harmonic of
/// `frequency_hz`, in dB relative to the fundamental.
fn loudest_alias_db(spectrum: &[f64], frequency_hz: f64, fft_len: usize) -> f64 {
    let bin_hz = f64::from(RATE) / fft_len as f64;
    // The window's main lobe is 4 bins either side; allow a little more.
    let lobe = 6.0;
    let fundamental_bin = (frequency_hz / bin_hz).round() as usize;
    let fundamental = spectrum[fundamental_bin - 6..fundamental_bin + 6]
        .iter()
        .fold(0.0f64, |a, &b| a.max(b));
    let near_harmonic = |bin: usize| {
        let harmonic = (bin as f64 * bin_hz / frequency_hz).round();
        (bin as f64 - harmonic * frequency_hz / bin_hz).abs() <= lobe
    };
    let alias = spectrum
        .iter()
        .enumerate()
        .filter(|&(bin, _)| !near_harmonic(bin))
        .fold(0.0f64, |a, (_, &b)| a.max(b));
    20.0 * (alias / fundamental).log10()
}

/// C7 at 2093 Hz: its 11th harmonic is already past half the sample rate, so
/// a naive saw folds strong aliases back down.
const HIGH_NOTE: u8 = 96;
const FFT_LEN: usize = 32_768;
/// The loudest alias allowed in the synth's high saw, relative to its
/// fundamental. The synth's PolyBLEP saw measures -37 dB here, and a naive
/// saw -21.5 dB, which `aliasing_measure_catches_a_naive_saw` checks.
const ALIAS_LIMIT_DB: f64 = -33.0;

#[test]
fn high_saw_aliasing_is_below_the_limit() {
    let samples = held_note(SynthSettings::default(), HIGH_NOTE, 127, 1.2)[seconds(0.4)..].to_vec();
    let alias_db = loudest_alias_db(
        &spectrum(&samples[..FFT_LEN]),
        pitch_to_hz(HIGH_NOTE),
        FFT_LEN,
    );
    assert!(
        alias_db < ALIAS_LIMIT_DB,
        "loudest alias is {alias_db} dB, limit {ALIAS_LIMIT_DB} dB"
    );
}

/// Guards the aliasing test: a naive saw at the same pitch must fail it.
#[test]
fn aliasing_measure_catches_a_naive_saw() {
    let frequency = pitch_to_hz(HIGH_NOTE);
    let naive: Vec<f32> = (0..FFT_LEN)
        .map(|n| {
            let phase = (n as f64 * frequency / f64::from(RATE)).fract();
            (2.0 * phase - 1.0) as f32
        })
        .collect();
    let alias_db = loudest_alias_db(&spectrum(&naive), frequency, FFT_LEN);
    assert!(
        alias_db > ALIAS_LIMIT_DB + 8.0,
        "a naive saw's loudest alias is only {alias_db} dB"
    );
}

/// The steepest `voices` sine voices at `frequency_hz` can move in one
/// sample, at full velocity and up to `gain` times the voice level, while
/// each voice's envelope moves up to 1/`attack_samples` a sample, plus 10%.
/// Anything above it is a click, not the notes.
fn click_limit(voices: usize, frequency_hz: f64, gain: f32, attack_samples: usize) -> f32 {
    let per_voice = sine_max_step(frequency_hz, RATE) * gain + 1.0 / attack_samples as f32;
    voices as f32 * VOICE_LEVEL * per_voice * 1.1
}

/// Every voice starts, is taken over and released, in overlapping waves and
/// at odd spacings, all at 110 Hz and full velocity.
#[test]
fn no_clicks_when_notes_start_release_and_are_taken_over() {
    let settings = SynthSettings {
        release_seconds: 0.05,
        ..sine()
    };
    let mut renderer = renderer(settings, 128);
    let pitch = 45; // 110 Hz
    for wave in 0..4u128 {
        // One more note than there are voices, so the last takes one over.
        for i in 0..=VOICES as u128 {
            let key = NoteKey(wave * 100 + i);
            renderer.controller.note_on(0, key, pitch, 127).unwrap();
            renderer.render(37 + (i as usize * 53) % 300);
            if i % 3 == 0 {
                renderer.controller.note_off(0, key).unwrap();
            }
        }
        // More take-overs, some while the last ones are still fading.
        for i in 0..8u128 {
            renderer
                .controller
                .note_on(0, NoteKey(wave * 100 + 50 + i), pitch, 127)
                .unwrap();
            renderer.render(97 * (i as usize + 1));
        }
        // Release everything.
        for i in 0..60 {
            renderer
                .controller
                .note_off(0, NoteKey(wave * 100 + i))
                .unwrap();
        }
        renderer.render(seconds(0.03));
    }
    renderer.render(seconds(0.1));

    let samples = renderer.samples();
    let limit = click_limit(VOICES, 110.0, 1.0, seconds(0.005));
    let (jump, at) = max_jump(samples);
    assert!(
        jump <= limit,
        "jump of {jump} at sample {at}, limit {limit}"
    );
    assert_eq!(
        peak(&samples[samples.len() - seconds(0.04)..]),
        0.0,
        "notes still sounding after they were all released"
    );
}

/// Guards the click limit: one voice cut off at its loudest point, with
/// fifteen others sounding, must fail it.
#[test]
fn click_limit_catches_a_voice_cut_off() {
    let mut samples = steady(sine(), 45, 127);
    let loudest = samples
        .iter()
        .position(|s| s.abs() > VOICE_LEVEL * 0.7 * 0.999)
        .unwrap();
    samples[loudest + 1..].fill(0.0);
    let limit = click_limit(VOICES, 110.0, 1.0, seconds(0.005));
    assert!(max_jump(&samples).0 > limit * 2.0);
}

/// Cutoff, resonance and sustain jump between extremes faster than they can
/// glide, and the waveform flips between sine and triangle, while four notes
/// hold. Everything glides, so nothing clicks.
#[test]
fn no_clicks_when_settings_change_while_notes_sound() {
    let base = SynthSettings {
        sustain: 1.0,
        ..sine()
    };
    let mut renderer = renderer(base, 128);
    for key in 0..4 {
        renderer
            .controller
            .note_on(0, NoteKey(key), 45, 127)
            .unwrap();
    }
    renderer.render_seconds(0.1);
    let changes = [
        (200.0, 0.5, 0.2, Waveform::Triangle),
        (20_000.0, 0.0, 1.0, Waveform::Sine),
        (400.0, 0.5, 0.3, Waveform::Sine),
        (20.0, 0.0, 1.0, Waveform::Triangle),
        (20_000.0, 0.5, 0.2, Waveform::Sine),
        (150.0, 0.2, 0.9, Waveform::Triangle),
        (20_000.0, 0.0, 1.0, Waveform::Sine),
    ];
    for _ in 0..3 {
        for &(cutoff_hz, resonance, sustain, waveform) in &changes {
            renderer
                .controller
                .set_synth_settings(
                    0,
                    SynthSettings {
                        waveform,
                        cutoff_hz,
                        resonance,
                        sustain,
                        ..base
                    },
                )
                .unwrap();
            renderer.render(300);
        }
    }
    renderer.render_seconds(0.1);

    // At resonance 0.5 the filter peaks about 1.4x at its cutoff. A triangle
    // moves at most 4 × f / rate a sample, less than a sine's steepest step.
    let limit = click_limit(4, 110.0, 1.5, usize::MAX);
    let (jump, at) = max_jump(renderer.samples());
    assert!(
        jump <= limit,
        "jump of {jump} at sample {at}, limit {limit}"
    );
}

/// Guards the settings click test: a sustain level that jumps rather than
/// glides must fail it.
#[test]
fn click_limit_catches_a_level_jump() {
    let mut samples = steady(
        SynthSettings {
            sustain: 1.0,
            ..sine()
        },
        45,
        127,
    );
    let loudest = samples
        .iter()
        .position(|s| s.abs() > VOICE_LEVEL * 0.999)
        .unwrap();
    for s in &mut samples[loudest + 1..] {
        *s *= 0.2;
    }
    let limit = click_limit(4, 110.0, 1.5, usize::MAX);
    assert!(max_jump(&samples).0 > limit * 2.0);
}

/// Renders the same session in blocks of `block_size`. Every event lands on a
/// multiple of 1024 frames, which every tested block size divides, so they
/// hit the same sample in every render.
fn session(block_size: usize) -> Vec<f32> {
    let mut renderer = renderer(SynthSettings::default(), block_size);
    renderer.render(1024);
    for i in 0..VOICES as u8 + 4 {
        renderer
            .controller
            .note_on(0, NoteKey(u128::from(i)), 40 + i * 3, 30 + i * 5)
            .unwrap();
        renderer.render(1024);
        if i % 4 == 0 {
            renderer
                .controller
                .note_off(0, NoteKey(u128::from(i)))
                .unwrap();
        }
    }
    for (i, waveform) in [
        Waveform::Square,
        Waveform::Triangle,
        Waveform::Sine,
        Waveform::Saw,
    ]
    .into_iter()
    .enumerate()
    {
        renderer
            .controller
            .set_synth_settings(
                0,
                SynthSettings {
                    waveform,
                    cutoff_hz: 300.0 * (i + 1) as f32,
                    resonance: 0.25 * i as f32,
                    sustain: 0.2 * (i + 1) as f32,
                    ..SynthSettings::default()
                },
            )
            .unwrap();
        renderer.render(1024 * 2);
    }
    for i in 0..VOICES as u128 + 4 {
        renderer.controller.note_off(0, NoteKey(i)).unwrap();
    }
    renderer.render(1024 * 12);
    renderer.into_samples()
}

#[test]
fn block_size_does_not_change_the_synth() {
    let reference = session(1024);
    assert!(peak(&reference) > 0.1, "the session made no sound");
    for block_size in [32, 128] {
        let difference = max_difference(&session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}
