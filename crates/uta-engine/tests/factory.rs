//! The factory sounds, measured against their briefs: RFC-008, "Factory
//! sounds, and turning an idea into settings", and "How we'll verify it"
//! (Factory sounds). Each sound's audition is rendered as the app would play
//! it, and measured as `uta analyse` measures a render.
//!
//! Run with `--nocapture` to see every sound's measurements against its
//! targets: the numbers the brief → settings → render → measure loop reads.

mod common;

use common::*;
use uta_core::time::TICKS_PER_QUARTER;
use uta_core::{
    Audition, Command, FactorySound, LOUDNESS_SPREAD_DB, NoteId, Project, SynthSettings, Targets,
};
use uuid::Uuid;

/// The tempo the auditions play at: two beats a second.
const BPM: f32 = 120.0;

/// How long the render carries on after the notes are let go: long enough
/// for the longest release to die away.
const TAIL_SECONDS: f64 = 4.0;

/// What `uta analyse` reads from a sound's audition, as its brief's
/// targets name them.
#[derive(Debug)]
struct Measured {
    brightness_hz: f64,
    attack_ms: f64,
    fade_seconds: f64,
    loudness_db: f64,
}

/// A new project playing `sound`'s audition: its notes from the top, held
/// for its time, with the sound chosen as the app chooses it.
fn audition_project(sound: &FactorySound) -> Project {
    let mut project = synth_project();
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let ticks_per_second = f64::from(BPM) / 60.0 * TICKS_PER_QUARTER as f64;
    let length = (sound.audition.held_seconds * ticks_per_second).round() as u64;
    let notes = sound
        .audition
        .pitches
        .iter()
        .enumerate()
        .map(|(i, &pitch)| uta_core::Note {
            id: NoteId::from_uuid(Uuid::from_u128(1000 + i as u128)),
            pitch,
            velocity: 100,
            start: 0,
            length,
        })
        .collect();
    for command in [
        Command::SetTempo { bpm: BPM },
        Command::SetLoopEnabled { enabled: false },
        Command::SetSynthSettings {
            track,
            settings: sound.settings,
        },
        Command::AddNotes { clip, notes },
    ] {
        project.apply(&command).expect("a valid audition");
    }
    project
}

/// The default sound, a new track's saw, playing middle C for a second:
/// what the factory sounds' loudness is measured against.
fn default_sound() -> FactorySound {
    FactorySound {
        key: "default",
        name: "Default",
        brief: "",
        audition: Audition {
            pitches: &[60],
            held_seconds: 1.0,
        },
        targets: Targets {
            brightness_hz: 0.0..=0.0,
            attack_ms: 0.0..=0.0,
            fade_seconds: 0.0..=0.0,
            loudness_db: 0.0..=0.0,
        },
        settings: SynthSettings::default(),
    }
}

/// `sound`'s audition, rendered from its first sound until it has died
/// away, and measured, with its loudness against the default sound's.
fn measure(sound: &FactorySound) -> Measured {
    let mut measured = measure_alone(sound);
    measured.loudness_db -= measure_alone(&default_sound()).loudness_db;
    measured
}

/// `sound`'s audition, rendered from its first sound until it has died
/// away, and measured.
fn measure_alone(sound: &FactorySound) -> Measured {
    let rendered = render_drums(
        &audition_project(sound),
        sound.audition.held_seconds + TAIL_SECONDS,
        128,
    );
    // To hear an audition, or run `uta analyse` on it: UTA_AUDITIONS=<folder>.
    if let Some(folder) = std::env::var_os("UTA_AUDITIONS") {
        let path = std::path::Path::new(&folder).join(format!("{}.wav", sound.key));
        let config = uta_engine::EngineConfig {
            sample_rate: 48_000,
            channels: 1,
        };
        uta_engine::offline::write_wav(&path, config, &rendered).expect("the audition is written");
    }
    let start = first_sound(&rendered).expect("the audition sounds");
    let samples = &rendered[start..];
    Measured {
        brightness_hz: spectral_centroid(samples, 48_000),
        attack_ms: 1000.0 * attack_seconds(samples, 48_000).expect("it sounds"),
        fade_seconds: fade_seconds(samples),
        loudness_db: hit_loudness_db(samples, sound.audition.held_seconds),
    }
}

/// How long `samples` last: from the start until their level, over 5 ms
/// windows, has fallen for good to 40 dB under their loudest.
fn fade_seconds(samples: &[f32]) -> f64 {
    let envelope = envelope_db(samples, 48_000, 0.005);
    let loudest = envelope.iter().fold(f64::MIN, |a, &(_, db)| a.max(db));
    let last = envelope
        .iter()
        .rposition(|&(_, db)| db >= loudest - 40.0)
        .expect("the loudest window is within 40 dB of itself");
    // The end of the last window still within 40 dB.
    envelope[last].0 + 0.0025
}

/// Each factory sound meets every target in its brief: how bright it is,
/// how fast it starts, how long it takes to fade and how loud it is.
#[test]
fn every_factory_sound_meets_its_briefs_targets() {
    let mut misses = Vec::new();
    for sound in FactorySound::all() {
        let measured = measure(&sound);
        let targets = &sound.targets;
        let checks = [
            (
                "brightness Hz",
                measured.brightness_hz,
                &targets.brightness_hz,
            ),
            ("attack ms", measured.attack_ms, &targets.attack_ms),
            ("fade s", measured.fade_seconds, &targets.fade_seconds),
            ("loudness dB", measured.loudness_db, &targets.loudness_db),
        ];
        let line: Vec<String> = checks
            .iter()
            .map(|(what, value, range)| {
                let mark = if range.contains(value) { "" } else { " ✗" };
                format!(
                    "{what} {value:.1} ({:.1} to {:.1}){mark}",
                    range.start(),
                    range.end()
                )
            })
            .collect();
        eprintln!("{:<10} {}", sound.key, line.join(" · "));
        misses.extend(
            checks
                .iter()
                .filter(|(_, value, range)| !range.contains(value))
                .map(|(what, value, range)| {
                    format!("{}: {what} {value:.2}, wanted {range:?}", sound.key)
                }),
        );
    }
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}

/// The factory sounds are within [`LOUDNESS_SPREAD_DB`] of each other,
/// quietest to loudest, each played as its audition, so switching sounds in
/// a loop doesn't need the volume touched.
#[test]
fn the_factory_sounds_are_within_3_db_of_each_other() {
    assert_eq!(LOUDNESS_SPREAD_DB, 3.0);
    let loudness: Vec<(&str, f64)> = FactorySound::all()
        .iter()
        .map(|sound| (sound.key, measure(sound).loudness_db))
        .collect();
    let quietest = loudness.iter().min_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    let loudest = loudness.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    assert!(
        loudest.1 - quietest.1 <= LOUDNESS_SPREAD_DB,
        "{} at {:.2} dB is {:.2} dB louder than {} at {:.2} dB",
        loudest.0,
        loudest.1,
        loudest.1 - quietest.1,
        quietest.0,
        quietest.1
    );
}
