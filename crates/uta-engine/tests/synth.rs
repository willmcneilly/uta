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
        osc1_wave: Waveform::Sine,
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
        Waveform::Pulse,
    ] {
        for pitch in [21, 33, 45, 57, 69, 81, 93, 105] {
            let settings = SynthSettings {
                osc1_wave: waveform,
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
/// `frequency_hz`, in dB relative to the loudest harmonic: for a saw, its
/// fundamental, and for a synced wave, whichever harmonic its restarts make
/// loudest.
fn loudest_alias_db(spectrum: &[f64], frequency_hz: f64, fft_len: usize) -> f64 {
    let bin_hz = f64::from(RATE) / fft_len as f64;
    // The window's main lobe is 4 bins either side; allow a little more.
    let lobe = 6.0;
    // The harmonic each bin is on, if it's on one: 0 is any offset, which
    // isn't an alias but isn't a harmonic either.
    let harmonic = |bin: usize| {
        let harmonic = (bin as f64 * bin_hz / frequency_hz).round();
        ((bin as f64 - harmonic * frequency_hz / bin_hz).abs() <= lobe).then_some(harmonic)
    };
    let loudest = |on: fn(Option<f64>) -> bool| {
        spectrum
            .iter()
            .enumerate()
            .filter(|&(bin, _)| on(harmonic(bin)))
            .fold(0.0f64, |a, (_, &b)| a.max(b))
    };
    let alias = loudest(|harmonic| harmonic.is_none());
    let harmonics = loudest(|harmonic| harmonic.is_some_and(|h| h >= 1.0));
    20.0 * (alias / harmonics).log10()
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
                        osc1_wave: waveform,
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
        Waveform::Pulse,
        Waveform::Pulse,
        Waveform::Sine,
        Waveform::Saw,
    ]
    .into_iter()
    .enumerate()
    {
        // Every setting moves: oscillator 1's pulse narrows, oscillator 2
        // detunes, changes octave and syncs on and off, the sub and noise
        // come in, and the oscillators run free from the second change on.
        renderer
            .controller
            .set_synth_settings(
                0,
                SynthSettings {
                    osc1_wave: waveform,
                    osc1_octave: i as i8 - 1,
                    osc1_width: 0.2 + 0.2 * i as f32,
                    osc2_wave: [Waveform::Saw, Waveform::Pulse][i % 2],
                    osc2_octave: 1 - i as i8,
                    osc2_pitch_semitones: 0.07 + 3.0 * i as f32,
                    osc2_sync: i % 2 == 1,
                    sub_octave: -1 - (i % 2) as i8,
                    osc1_level: 1.0 - 0.2 * i as f32,
                    osc2_level: 0.3 * i as f32,
                    sub_level: 0.25 * i as f32,
                    noise_level: 0.1 * i as f32,
                    cutoff_hz: 300.0 * (i + 1) as f32,
                    resonance: 0.25 * i as f32,
                    sustain: 0.2 * (i + 1) as f32,
                    osc_retrigger: i == 0,
                    ..SynthSettings::default()
                },
            )
            .unwrap();
        renderer.render(1024);
        // Notes starting with these settings: free-running from the second.
        let key = 100 + i as u128;
        renderer
            .controller
            .note_on(0, NoteKey(key), 50 + i as u8, 90)
            .unwrap();
        renderer.render(1024);
    }
    for i in (0..VOICES as u128 + 4).chain(100..104) {
        renderer.controller.note_off(0, NoteKey(i)).unwrap();
    }
    renderer.render(1024 * 12);
    renderer.into_samples()
}

/// Rendering twice gives identical audio, noise and free-running
/// oscillators and all, and so do blocks of 32, 128 and 1024.
#[test]
fn block_size_does_not_change_the_synth() {
    let reference = session(1024);
    assert!(peak(&reference) > 0.1, "the session made no sound");
    assert_eq!(session(1024), reference, "rendered twice");
    for block_size in [32, 128] {
        let difference = max_difference(&session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}

/// Oscillator 1, `osc1_level` of it, and the rest of the mixer at zero.
fn only_osc1(level: f32) -> SynthSettings {
    SynthSettings {
        osc1_level: level,
        sustain: 1.0,
        ..SynthSettings::default()
    }
}

/// Oscillator 2 alone, at full level: a sine at `octave` and `semitones`.
fn only_osc2(octave: i8, semitones: f32) -> SynthSettings {
    SynthSettings {
        osc2_wave: Waveform::Sine,
        osc2_octave: octave,
        osc2_pitch_semitones: semitones,
        osc2_level: 1.0,
        ..only_osc1(0.0)
    }
}

/// Each oscillator's measured pitch is the note's, moved by its Octave and,
/// for oscillator 2, its Pitch, across both ends of each.
#[test]
fn the_oscillators_play_at_their_octave_and_pitch() {
    let note = 57; // A3, 220 Hz
    for octave in -2..=2i8 {
        let osc1 = SynthSettings {
            osc1_wave: Waveform::Sine,
            osc1_octave: octave,
            ..only_osc1(1.0)
        };
        for (settings, semitones, which) in [
            (osc1, 0.0, "oscillator 1"),
            (only_osc2(octave, -12.0), -12.0, "oscillator 2"),
            (only_osc2(octave, -0.1), -0.1, "oscillator 2"),
            (only_osc2(octave, 0.0), 0.0, "oscillator 2"),
            (only_osc2(octave, 7.0), 7.0, "oscillator 2"),
            (only_osc2(octave, 12.0), 12.0, "oscillator 2"),
        ] {
            let measured = measure_frequency(&steady(settings, note, 100), RATE);
            let expected = pitch_to_hz(note) * (f64::from(octave) + semitones / 12.0).exp2();
            assert!(
                (measured / expected - 1.0).abs() < 1e-4,
                "{which} at octave {octave}, {semitones} semitones: {measured} Hz, expected {expected} Hz"
            );
        }
    }
}

/// The sub plays one or two octaves under oscillator 1, wherever oscillator
/// 1's Octave puts it.
#[test]
fn the_sub_is_one_or_two_octaves_under_oscillator_1() {
    let note = 69; // A4, 440 Hz
    for osc1_octave in [-1, 0, 2] {
        for sub_octave in [-1, -2] {
            let settings = SynthSettings {
                osc1_octave,
                sub_octave,
                sub_level: 1.0,
                ..only_osc1(0.0)
            };
            let osc1 = SynthSettings {
                osc1_octave,
                ..only_osc1(1.0)
            };
            let sub = measure_frequency(&steady(settings, note, 100), RATE);
            let above = measure_frequency(&steady(osc1, note, 100), RATE);
            let expected = pitch_to_hz(note) * f64::from(osc1_octave + sub_octave).exp2();
            assert!(
                (sub / expected - 1.0).abs() < 1e-4,
                "the sub at {sub_octave} under octave {osc1_octave}: {sub} Hz, expected {expected} Hz"
            );
            let octaves = (above / sub).log2();
            assert!(
                (octaves + f64::from(sub_octave)).abs() < 1e-4,
                "the sub is {octaves} octaves under oscillator 1"
            );
        }
    }
}

/// The noise alone, at `level`, held on one note with the filter open, for
/// long enough to measure its spectrum.
fn noise(level: f32) -> Vec<f32> {
    let settings = SynthSettings {
        noise_level: level,
        ..only_osc1(0.0)
    };
    held_note(settings, 60, 127, 1.2)[seconds(0.4)..].to_vec()
}

/// White: the same power in every band of the spectrum, from 100 Hz to
/// 10 kHz, where the open filter is flat.
#[test]
fn noise_is_white() {
    let samples = noise(1.0);
    let spectrum = spectrum(&samples[..FFT_LEN]);
    let bin_hz = f64::from(RATE) / FFT_LEN as f64;
    // The mean power per bin in each octave band.
    let band_db = |low: f64| {
        let bins = &spectrum[(low / bin_hz) as usize..(2.0 * low / bin_hz) as usize];
        let power = bins.iter().map(|m| m * m).sum::<f64>() / bins.len() as f64;
        10.0 * power.log10()
    };
    let bands: Vec<f64> = [100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 5000.0]
        .iter()
        .map(|&low| band_db(low))
        .collect();
    let mean = bands.iter().sum::<f64>() / bands.len() as f64;
    for (band, db) in bands.iter().enumerate() {
        assert!(
            (db - mean).abs() < 1.0,
            "band {band} is {:.2} dB from the mean: {bands:?}",
            db - mean
        );
    }
}

/// Its level follows the mixer: half the level is 6 dB down, a quarter 12,
/// and at full level its RMS is a full-scale uniform noise's, 1/√3, at the
/// note's level, less what the open filter takes from the top of it.
#[test]
fn noise_follows_its_level() {
    let full = rms(&noise(1.0));
    // The share of white noise's power the filter passes: its response,
    // averaged over every frequency up to half the sample rate.
    let steps = 10_000;
    let passed = (0..steps)
        .map(|i| {
            let hz = (i as f64 + 0.5) / steps as f64 * f64::from(RATE) / 2.0;
            10f64.powf(expected_filter_db(hz, 20_000.0) / 10.0)
        })
        .sum::<f64>()
        / steps as f64;
    let expected = f64::from(VOICE_LEVEL * velocity_to_gain(127)) / 3f64.sqrt() * passed.sqrt();
    let db = 20.0 * (full / expected).log10();
    assert!(
        db.abs() < 0.1,
        "full level: RMS {full}, expected {expected} ({db:.2} dB off)"
    );
    for (level, expected_db) in [(0.5, -6.02), (0.25, -12.04)] {
        let db = 20.0 * (rms(&noise(level)) / full).log10();
        assert!(
            (db - expected_db).abs() < 0.01,
            "level {level}: {db} dB, expected {expected_db} dB"
        );
    }
    assert_eq!(peak(&noise(0.0)), 0.0);
}

/// Each voice's noise comes from its own seed: two noise notes at once add
/// up as two different noises, 3 dB louder than one, not as one noise twice
/// over, 6 dB louder.
#[test]
fn each_voice_has_its_own_noise() {
    let settings = SynthSettings {
        noise_level: 1.0,
        ..only_osc1(0.0)
    };
    let notes = |count: u128| {
        let mut renderer = renderer(settings, 128);
        for key in 0..count {
            renderer
                .controller
                .note_on(0, NoteKey(key), 60, 127)
                .unwrap();
        }
        renderer.render_seconds(1.0);
        rms(&renderer.into_samples()[seconds(0.4)..])
    };
    let db = 20.0 * (notes(2) / notes(1)).log10();
    assert!(
        (db - 3.01).abs() < 0.5,
        "two noise notes are {db:.2} dB louder than one"
    );
}

/// Two oscillators at full level, detuned, or one with the sub, sound about
/// as loud as one oscillator alone: the mixer turns the mix down once its
/// levels add up to more than one. Oscillator 2 alone at full level is as
/// loud as oscillator 1 alone. Each is measured over two seconds, several
/// times round the detuned oscillators' beat.
#[test]
fn a_typical_mix_is_about_as_loud_as_one_oscillator() {
    let level = |settings| rms(&held_note(settings, 57, 100, 2.4)[seconds(0.4)..]);
    let one = level(only_osc1(1.0));
    let mixes = [
        (
            "both saws, detuned",
            SynthSettings {
                osc2_pitch_semitones: 0.3,
                osc2_level: 1.0,
                ..only_osc1(1.0)
            },
            1.0,
        ),
        (
            "oscillator 1 with the sub at half",
            SynthSettings {
                sub_level: 0.5,
                ..only_osc1(1.0)
            },
            2.0,
        ),
        (
            "everything at full",
            SynthSettings {
                osc2_pitch_semitones: 0.3,
                osc2_level: 1.0,
                sub_level: 1.0,
                noise_level: 1.0,
                ..only_osc1(1.0)
            },
            2.0,
        ),
        (
            "oscillator 2 alone",
            SynthSettings {
                osc2_level: 1.0,
                ..only_osc1(0.0)
            },
            0.01,
        ),
    ];
    for (what, settings, within_db) in mixes {
        let db = 20.0 * (level(settings) / one).log10();
        assert!(
            db.abs() < within_db,
            "{what}: {db:.2} dB against oscillator 1 alone"
        );
    }
}

/// The same note twice, a little over half a second apart, with Osc
/// retrigger `retrigger`: each note's first 50 ms.
fn the_same_note_twice(retrigger: bool) -> (Vec<f32>, Vec<f32>) {
    let settings = SynthSettings {
        osc2_level: 1.0,
        osc2_pitch_semitones: 7.0,
        sub_level: 0.5,
        osc_retrigger: retrigger,
        ..SynthSettings::default()
    };
    let mut renderer = renderer(settings, 128);
    renderer.render(1000);
    renderer.controller.note_on(0, NoteKey(1), 57, 100).unwrap();
    renderer.render(seconds(0.3));
    renderer.controller.note_off(0, NoteKey(1)).unwrap();
    // A gap that isn't a whole number of the note's cycles.
    renderer.render(seconds(0.3) + 77);
    renderer.controller.note_on(0, NoteKey(2), 57, 100).unwrap();
    renderer.render(seconds(0.3));
    let samples = renderer.into_samples();
    let first = 1000;
    let second = first + seconds(0.6) + 77;
    let length = seconds(0.05);
    (
        samples[first..first + length].to_vec(),
        samples[second..second + length].to_vec(),
    )
}

/// With Osc retrigger off the oscillators run free, so the same note twice
/// starts somewhere different in the wave each time; on, it starts the
/// same. Either way, a render is the same every time.
#[test]
fn free_running_oscillators_differ_note_to_note_yet_render_the_same() {
    let (first, second) = the_same_note_twice(false);
    let difference = max_difference(&first, &second);
    assert!(
        difference > 0.1 * peak(&first),
        "two free-running notes differ by only {difference}"
    );
    assert_eq!(
        the_same_note_twice(false),
        (first, second),
        "rendered twice"
    );

    let (first, second) = the_same_note_twice(true);
    assert_eq!(first, second, "retriggered, the same note starts the same");
}

/// Play starts the free-running parts again, the oscillators and each
/// voice's noise, so playing from the top a second time sounds the same as
/// the first.
#[test]
fn play_restarts_the_noise_and_free_running_oscillators() {
    let notes = (0..8)
        .map(|i| note(i, 48 + 5 * i as u8, i as u64 * 240, 600))
        .collect();
    let project = project(
        120.0,
        1,
        &[
            uta_core::SynthParam::Osc2Level(1.0),
            uta_core::SynthParam::Osc2PitchSemitones(0.13),
            uta_core::SynthParam::SubLevel(0.5),
            uta_core::SynthParam::NoiseLevel(0.3),
            uta_core::SynthParam::OscRetrigger(false),
        ],
        notes,
    );
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(seconds(1.0));
    renderer.controller.stop().unwrap();
    // Long enough for every note to fall silent.
    renderer.render(seconds(1.0));
    renderer.controller.play().unwrap();
    renderer.render(seconds(1.0));
    let samples = renderer.into_samples();
    let (first, second) = (&samples[..seconds(1.0)], &samples[seconds(2.0)..]);
    assert!(peak(first) > 0.1, "the notes played");
    assert_eq!(first, second);
}

/// How a change to the synth's settings sounds, against the same render
/// without it: a note held from the start with `base`, and `turned` set at
/// sample `at` (a whole number of 128-sample blocks). Returns `with` minus
/// `without`, sample by sample, from `at` on.
fn turn_difference(base: SynthSettings, turned: SynthSettings, at: usize) -> Vec<f32> {
    turn_difference_at(45, base, turned, at)
}

/// [`turn_difference`] for a note at `pitch`.
fn turn_difference_at(
    pitch: u8,
    base: SynthSettings,
    turned: SynthSettings,
    at: usize,
) -> Vec<f32> {
    assert_eq!(at % 128, 0, "turns land on a block boundary");
    let render = |turn: Option<SynthSettings>| {
        let mut renderer = renderer(base, 128);
        renderer
            .controller
            .note_on(0, NoteKey(1), pitch, 127)
            .unwrap();
        renderer.render(at);
        if let Some(turned) = turn {
            renderer.controller.set_synth_settings(0, turned).unwrap();
        }
        renderer.render(seconds(0.2));
        renderer.into_samples()
    };
    let (with, without) = (render(Some(turned)), render(None));
    with[at..]
        .iter()
        .zip(&without[at..])
        .map(|(a, b)| a - b)
        .collect()
}

/// The sub and the noise, whose own waves jump, glide in and out with their
/// levels, and the sub's octave and oscillator 2's wave crossfade: each
/// change makes its difference over the glide, not at once.
#[test]
fn the_sub_noise_and_waves_glide_when_turned() {
    let base = SynthSettings {
        sustain: 1.0,
        ..SynthSettings::default()
    };
    let with_sub = SynthSettings {
        sub_level: 1.0,
        ..base
    };
    let with_osc2 = SynthSettings {
        osc2_level: 1.0,
        osc2_pitch_semitones: 0.1,
        ..base
    };
    let turns = [
        ("the sub coming in", base, with_sub),
        ("the sub going out", with_sub, base),
        (
            "the sub's octave",
            with_sub,
            SynthSettings {
                sub_octave: -2,
                ..with_sub
            },
        ),
        (
            "the noise coming in",
            base,
            SynthSettings {
                noise_level: 1.0,
                ..base
            },
        ),
        (
            "oscillator 2's wave",
            with_osc2,
            SynthSettings {
                osc2_wave: Waveform::Pulse,
                ..with_osc2
            },
        ),
        (
            "oscillator 2's level",
            base,
            SynthSettings {
                osc2_level: 1.0,
                ..base
            },
        ),
    ];
    let glide = (uta_engine::SYNTH_SMOOTHING_SECONDS * f64::from(RATE)).round() as usize;
    // Where the sub an octave down and the sub two down disagree, so the
    // sub's Octave changes the sound at once unless it crossfades: the note
    // is at 110 Hz, so sample 2816 is in oscillator 1's seventh cycle, from
    // sample 2618 to 3054, where the first is high and the second low.
    let at = 128 * 22;
    for (what, from, to) in turns {
        assert_glides(&turn_difference(from, to, at), glide, what);
    }
}

/// Every new control turned while four sine notes hold: oscillator 1's and
/// 2's Octave (each change at once, the wave carrying on from where it is),
/// oscillator 2's Pitch, wave and level, oscillator 1's level, and Osc
/// retrigger. Nothing clicks.
#[test]
fn no_clicks_when_the_oscillators_and_mixer_change_while_notes_sound() {
    let base = SynthSettings {
        osc1_wave: Waveform::Sine,
        osc2_wave: Waveform::Sine,
        osc2_level: 0.5,
        sustain: 1.0,
        ..SynthSettings::default()
    };
    let mut renderer = renderer(base, 128);
    for key in 0..4 {
        renderer
            .controller
            .note_on(0, NoteKey(key), 45, 127)
            .unwrap();
    }
    renderer.render_seconds(0.1);
    // (osc 1 octave, osc 2 octave, osc 2 pitch, osc 2 wave, osc 1 level,
    // osc 2 level, retrigger)
    let changes = [
        (1, 0, 0.0, Waveform::Sine, 1.0, 0.5, true),
        (2, -2, 12.0, Waveform::Triangle, 0.0, 1.0, false),
        (-2, 2, -12.0, Waveform::Sine, 1.0, 0.0, true),
        (0, 2, 12.0, Waveform::Triangle, 0.3, 1.0, false),
        (-1, -1, 0.05, Waveform::Sine, 1.0, 1.0, true),
        (2, 1, -7.0, Waveform::Triangle, 0.0, 0.0, false),
        (0, 0, 0.0, Waveform::Sine, 1.0, 0.5, true),
    ];
    for _ in 0..3 {
        for &(osc1_octave, osc2_octave, semitones, osc2_wave, osc1_level, osc2_level, retrigger) in
            &changes
        {
            renderer
                .controller
                .set_synth_settings(
                    0,
                    SynthSettings {
                        osc1_octave,
                        osc2_octave,
                        osc2_pitch_semitones: semitones,
                        osc2_wave,
                        osc1_level,
                        osc2_level,
                        osc_retrigger: retrigger,
                        ..base
                    },
                )
                .unwrap();
            renderer.render(300);
        }
    }
    renderer.render_seconds(0.1);

    // The highest either oscillator goes is 110 Hz up three octaves, and the
    // two together, balanced by the mixer, are at most √2 times one. A
    // triangle moves less in a sample than a sine.
    let limit = click_limit(4, 880.0, std::f32::consts::SQRT_2, usize::MAX);
    let (jump, at) = max_jump(renderer.samples());
    assert!(
        jump <= limit,
        "jump of {jump} at sample {at}, limit {limit}"
    );
}

/// Guards the oscillator click test: four sines at full level whose phase
/// jumps a quarter of a cycle, as an octave change that restarted the wave
/// would, must fail it.
#[test]
fn click_limit_catches_a_phase_jump() {
    let cycle = f64::from(RATE) / 110.0;
    let samples: Vec<f32> = (0..seconds(0.1))
        .map(|n| {
            // A quarter of a cycle on, from the tenth upward zero crossing.
            let jump = if n as f64 >= 10.0 * cycle { 0.25 } else { 0.0 };
            let phase = n as f64 / cycle + jump;
            (4.0 * f64::from(VOICE_LEVEL) * (std::f64::consts::TAU * phase).sin()) as f32
        })
        .collect();
    let limit = click_limit(4, 880.0, std::f32::consts::SQRT_2, usize::MAX);
    assert!(max_jump(&samples).0 > limit * 2.0);
}

/// Oscillator 1 alone, a pulse `width` wide.
fn pulse(width: f32) -> SynthSettings {
    SynthSettings {
        osc1_wave: Waveform::Pulse,
        osc1_width: width,
        ..only_osc1(1.0)
    }
}

/// The level of the `harmonic`th harmonic of `frequency_hz` in `spectrum`,
/// in dB relative to the fundamental: the loudest bin within the window's
/// main lobe of it.
fn harmonic_db(spectrum: &[f64], frequency_hz: f64, harmonic: usize, fft_len: usize) -> f64 {
    let bin_hz = f64::from(RATE) / fft_len as f64;
    let level = |harmonic: usize| {
        let bin = (harmonic as f64 * frequency_hz / bin_hz).round() as usize;
        spectrum[bin - 4..=bin + 4]
            .iter()
            .fold(0.0f64, |a, &b| a.max(b))
    };
    20.0 * (level(harmonic) / level(1)).log10()
}

/// A pulse a quarter of a cycle wide has no 4th, 8th or 12th harmonic, and
/// a square no even ones, as their widths say they should: the Width puts
/// the second jump where it says.
#[test]
fn at_a_quarter_width_every_fourth_harmonic_is_missing() {
    let frequency = pitch_to_hz(45); // 110 Hz
    let harmonics = |width: f32| {
        let samples = held_note(pulse(width), 45, 127, 1.2)[seconds(0.4)..].to_vec();
        let spectrum = spectrum(&samples[..FFT_LEN]);
        (1..=12)
            .map(|harmonic| harmonic_db(&spectrum, frequency, harmonic, FFT_LEN))
            .collect::<Vec<_>>()
    };
    let quarter = harmonics(0.25);
    for (index, db) in quarter.iter().enumerate() {
        let harmonic = index + 1;
        if harmonic % 4 == 0 {
            assert!(db < &-50.0, "harmonic {harmonic} is at {db} dB");
        } else {
            // A pulse's nth harmonic is |sin(n π w)| / n of the first's.
            let expected = 20.0
                * ((harmonic as f64 * PI * 0.25).sin().abs()
                    / (harmonic as f64 * (PI * 0.25).sin()))
                .log10();
            assert!(
                (db - expected).abs() < 1.0,
                "harmonic {harmonic} is at {db} dB, not {expected}"
            );
        }
    }
    let square = harmonics(0.5);
    for harmonic in [2, 4, 6, 8] {
        assert!(square[harmonic - 1] < -50.0, "the square's {harmonic}");
    }
    assert!(square[2] > -10.0, "the square's 3rd is at {}", square[2]);
}

/// However narrow or wide the pulse, it averages zero: a narrow one is
/// high briefly and far, and low for long and near, rather than sitting
/// below the middle.
#[test]
fn the_pulse_has_no_offset_at_any_width() {
    for width in [0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95] {
        let samples = held_note(pulse(width), 45, 127, 2.4)[seconds(0.4)..].to_vec();
        let mean = samples.iter().map(|&s| f64::from(s)).sum::<f64>() / samples.len() as f64;
        // A pulse that drifted off centre would average 1 - 2 × its width
        // of its level.
        let level = rms(&samples);
        assert!(
            mean.abs() < 0.005 * level,
            "width {width}: an offset of {mean} at a level of {level}"
        );
    }
}

/// The pitch of a steady tone from its period: the lag, between the
/// periods of `high_hz` and `low_hz`, at which the tone matches itself
/// best, refined between samples. Unlike counting zero crossings, it works
/// for a wave that crosses zero several times a cycle, as a synced one
/// does.
fn pitch_from_period(samples: &[f32], low_hz: f64, high_hz: f64) -> f64 {
    let rate = f64::from(RATE);
    let (shortest, longest) = ((rate / high_hz) as usize, (rate / low_hz).ceil() as usize);
    let span = samples.len() - longest - 1;
    let difference = |lag: usize| -> f64 {
        (0..span)
            .map(|n| f64::from(samples[n] - samples[n + lag]).powi(2))
            .sum()
    };
    let differences: Vec<f64> = (shortest - 1..=longest + 1).map(difference).collect();
    let best = (1..differences.len() - 1)
        .min_by(|&a, &b| differences[a].total_cmp(&differences[b]))
        .unwrap();
    let (a, b, c) = (
        differences[best - 1],
        differences[best],
        differences[best + 1],
    );
    let offset = 0.5 * (a - c) / (a - 2.0 * b + c);
    rate / ((shortest - 1 + best) as f64 + offset)
}

/// Oscillator 2 alone, `wave` at `octave` and `semitones`, synced to
/// oscillator 1, which isn't heard.
fn synced(wave: Waveform, octave: i8, semitones: f32) -> SynthSettings {
    SynthSettings {
        osc2_wave: wave,
        osc2_sync: true,
        ..only_osc2(octave, semitones)
    }
}

/// Under sync, oscillator 2 sounds at oscillator 1's pitch, whatever its
/// own Octave and Pitch, below oscillator 1 or above it, for every wave.
/// Without sync, the same settings play oscillator 2's own pitch.
#[test]
fn under_sync_the_pitch_follows_oscillator_1() {
    let note = 45; // 110 Hz
    let frequency = pitch_to_hz(note);
    let measure = |settings: SynthSettings| {
        let samples = steady(settings, note, 127);
        pitch_from_period(&samples[..seconds(0.2)], 0.8 * frequency, 5.0 * frequency)
    };
    for wave in Waveform::ALL {
        for (octave, semitones) in [
            (0, 3.3),
            (0, 7.0),
            (0, -7.0),
            (-1, 5.0),
            (1, -5.0),
            (1, 4.5),
            (2, -1.0),
        ] {
            let synced = measure(synced(wave, octave, semitones));
            assert!(
                (synced / frequency - 1.0).abs() < 0.002,
                "{wave:?} at octave {octave} and {semitones}: {synced} Hz"
            );
            let free = measure(SynthSettings {
                osc2_sync: false,
                ..self::synced(wave, octave, semitones)
            });
            // Its own pitch, or (as a whole number of its cycles is a period
            // too) a half or a third of it, where that's the closest match.
            let own = frequency * (f64::from(octave) + f64::from(semitones) / 12.0).exp2();
            if own > 0.8 * frequency {
                assert!(
                    (1..=3).any(|k| (free * f64::from(k) / own - 1.0).abs() < 0.002),
                    "{wave:?} at octave {octave} and {semitones} without sync: {free} Hz, not {own}"
                );
            }
        }
    }
}

/// The loudest alias allowed in a sync sweep, for each wave, relative to the
/// loudest harmonic: oscillator 2 synced to C7 (2093 Hz) and swept up two
/// octaves. Measured here, with the synced oscillator taking four steps a
/// sample, as the saw and pulse -46.0 dB, the triangle -55.6 and the sine
/// -65.5. Taking one step a sample, PolyBLEP and PolyBLAMP alone, they
/// measured -25.6, -24.3, -35.9 and -42.9: about 20 dB worse, and the
/// difference the sync A/B heard. Uncorrected, as
/// `sync_aliasing_measure_catches_a_naive_sync` checks, they're worse
/// still.
const SYNC_ALIAS_LIMIT_DB: [(Waveform, f64); 4] = [
    (Waveform::Saw, -45.5),
    (Waveform::Pulse, -45.5),
    (Waveform::Triangle, -55.0),
    (Waveform::Sine, -65.0),
];

/// Oscillator 2's Octave and Pitch across a sync sweep: up two octaves from
/// oscillator 1, in steps of a semitone and a half.
fn sync_sweep() -> impl Iterator<Item = (i8, f32)> {
    (0..=16).map(|step| {
        let semitones = 1.5 * step as f32;
        if semitones > 12.0 {
            (1, semitones - 12.0)
        } else {
            (0, semitones)
        }
    })
}

/// The aliasing anywhere in a sync sweep to a high pitch, sync's hardest
/// case, stays at or under the figure measured for each wave when it was
/// built, so it can't quietly get worse. See RFC-008, "Risks & unknowns"
/// (Sync aliasing at extreme settings).
#[test]
fn sync_sweep_aliasing_is_below_the_limit() {
    let frequency = pitch_to_hz(HIGH_NOTE);
    for (wave, limit) in SYNC_ALIAS_LIMIT_DB {
        for (octave, semitones) in sync_sweep() {
            let settings = synced(wave, octave, semitones);
            let samples = held_note(settings, HIGH_NOTE, 127, 1.2)[seconds(0.4)..].to_vec();
            let alias_db = loudest_alias_db(&spectrum(&samples[..FFT_LEN]), frequency, FFT_LEN);
            assert!(
                alias_db <= limit,
                "{wave:?} at octave {octave} and {semitones}: {alias_db} dB, limit {limit} dB"
            );
        }
    }
}

/// Guards the sync aliasing test: oscillator 2 restarted where oscillator 1
/// wraps with nothing corrected, its own jumps included, must fail it
/// clearly somewhere in the sweep.
#[test]
fn sync_aliasing_measure_catches_a_naive_sync() {
    let frequency = pitch_to_hz(HIGH_NOTE);
    let increment = frequency / f64::from(RATE);
    for (wave, limit) in SYNC_ALIAS_LIMIT_DB {
        let worst = sync_sweep()
            .map(|(octave, semitones)| {
                let ratio = (f64::from(octave) + f64::from(semitones) / 12.0).exp2();
                let (mut leader, mut phase) = (0.0f64, 0.0f64);
                let naive: Vec<f32> = (0..FFT_LEN)
                    .map(|_| {
                        let value = match wave {
                            Waveform::Sine => (std::f64::consts::TAU * phase).sin(),
                            Waveform::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
                            Waveform::Saw => 2.0 * phase - 1.0,
                            Waveform::Pulse => {
                                if phase < 0.5 {
                                    1.0
                                } else {
                                    -1.0
                                }
                            }
                        };
                        leader += increment;
                        if leader >= 1.0 {
                            leader -= 1.0;
                            phase = (leader * ratio).fract();
                        } else {
                            phase = (phase + increment * ratio).fract();
                        }
                        value as f32
                    })
                    .collect();
                loudest_alias_db(&spectrum(&naive), frequency, FFT_LEN)
            })
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            worst > limit + 8.0,
            "a naive synced {wave:?}'s loudest alias is only {worst} dB"
        );
    }
}

/// Fails unless a change that moves where a wave jumps glides in: a pulse's
/// width, or a synced oscillator's pitch, which moves its own wrap against
/// its restart. Moving a jump by more than a sample makes a full-height
/// difference, so the peak can't tell a glide from a jump (as
/// `assert_glides` does); the difference's energy can. Gliding, the jump
/// has moved a twentieth of the way on average in the first tenth of the
/// glide, so the RMS there is about a quarter of the change's whole RMS,
/// after the glide. Jumping, it's all the way there at once.
fn assert_jumps_glide(difference: &[f32], glide: usize, what: &str) {
    let early = rms(&difference[..glide / 10]);
    let full = rms(&difference[glide..2 * glide]);
    assert!(full > 1e-3, "{what}: the turn changed nothing");
    assert!(
        early < 0.45 * full,
        "{what}: {:.0}% of its RMS in the first tenth of the glide",
        100.0 * early / full
    );
}

/// Guards [`assert_jumps_glide`]: a pulse whose width jumps from a half to
/// a tenth, against the same pulse left alone, must fail it.
#[test]
#[should_panic(expected = "of its RMS in the first tenth")]
fn width_glide_check_catches_a_jump() {
    let glide = (uta_engine::SYNTH_SMOOTHING_SECONDS * f64::from(RATE)).round() as usize;
    let cycle = f64::from(RATE) / pitch_to_hz(81);
    let difference: Vec<f32> = (0..3 * glide)
        .map(|n| {
            let phase = (n as f64 / cycle).fract();
            let level = |width: f64| {
                if phase < width {
                    2.0 - 2.0 * width
                } else {
                    -2.0 * width
                }
            };
            (level(0.1) - level(0.5)) as f32
        })
        .collect();
    assert_jumps_glide(&difference, glide, "a jump");
}

/// Oscillator 1's Width glides, switching Sync on or off crossfades between
/// oscillator 2 free and synced, and a synced oscillator's Pitch glides:
/// each changes the sound over the glide, not at once, so turning them
/// while a note plays doesn't click. The width and the synced pitch are
/// turned at A5, so many of their cycles fall in the glide.
#[test]
fn width_and_sync_glide_when_turned() {
    let glide = (uta_engine::SYNTH_SMOOTHING_SECONDS * f64::from(RATE)).round() as usize;
    let at = 128 * 22;
    let square = SynthSettings {
        sustain: 1.0,
        ..pulse(0.5)
    };
    for width in [0.1, 0.9] {
        let difference = turn_difference_at(
            81,
            square,
            SynthSettings {
                osc1_width: width,
                ..square
            },
            at,
        );
        assert_jumps_glide(&difference, glide, &format!("the width to {width}"));
    }
    let synced = SynthSettings {
        sustain: 1.0,
        ..synced(Waveform::Saw, 0, 7.0)
    };
    let difference = turn_difference_at(
        81,
        synced,
        SynthSettings {
            osc2_pitch_semitones: 12.0,
            ..synced
        },
        at,
    );
    assert_jumps_glide(&difference, glide, "the synced oscillator's pitch");
    let free = SynthSettings {
        osc1_level: 0.0,
        osc2_level: 1.0,
        osc2_pitch_semitones: 7.0,
        sustain: 1.0,
        ..SynthSettings::default()
    };
    let with_sync = SynthSettings {
        osc2_sync: true,
        ..free
    };
    let turns = [
        ("sync switched on", free, with_sync),
        ("sync switched off", with_sync, free),
    ];
    for (what, from, to) in turns {
        assert_glides(&turn_difference(from, to, at), glide, what);
    }
}
