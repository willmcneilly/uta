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
    Audition, Command, CommandList, FactorySound, LOUDNESS_SPREAD_DB, NoteId, Project, ProjectId,
    SynthSettings, Targets,
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

/// A 16th note, in ticks.
const SIXTEENTH: u64 = TICKS_PER_QUARTER / 4;

/// A factory sound's listening render: a 2-bar loop at 120 BPM of the kind
/// of part it's for. Each note is its start and length in 16ths, and its
/// pitch.
struct Phrase {
    key: &'static str,
    notes: &'static [(u64, u8, u64)],
}

/// Each factory sound's phrase, in the Sound menu's order. The 1-bar
/// chords and lines repeat in the second bar where a part would.
const PHRASES: [Phrase; 8] = [
    // Root, octave and fifth on the off-beats, as a house or synth-pop line.
    Phrase {
        key: "bass",
        notes: &[
            (0, 36, 3),
            (4, 36, 1),
            (6, 48, 2),
            (8, 36, 3),
            (12, 39, 2),
            (14, 41, 2),
            (16, 34, 3),
            (20, 34, 1),
            (22, 46, 2),
            (24, 34, 3),
            (28, 41, 2),
            (30, 43, 2),
        ],
    },
    // Overlapping notes slide; a note after a gap starts fresh.
    Phrase {
        key: "glide_bass",
        notes: &[
            (0, 36, 4),
            (3, 48, 3),
            (6, 36, 2),
            (10, 39, 3),
            (12, 43, 3),
            (14, 36, 2),
            (16, 36, 2),
            (20, 36, 4),
            (23, 51, 3),
            (25, 48, 3),
            (27, 43, 3),
            (29, 36, 3),
        ],
    },
    // 8th-note arpeggios over Cmaj7, then Am7.
    Phrase {
        key: "pluck",
        notes: &[
            (0, 60, 1),
            (2, 64, 1),
            (4, 67, 1),
            (6, 71, 1),
            (8, 72, 1),
            (10, 71, 1),
            (12, 67, 1),
            (14, 64, 1),
            (16, 57, 1),
            (18, 60, 1),
            (20, 64, 1),
            (22, 67, 1),
            (24, 69, 1),
            (26, 67, 1),
            (28, 64, 1),
            (30, 60, 1),
        ],
    },
    // Cmaj7, then Am7, a bar each.
    Phrase {
        key: "pad",
        notes: &[
            (0, 48, 16),
            (0, 52, 16),
            (0, 55, 16),
            (0, 59, 16),
            (16, 45, 16),
            (16, 52, 16),
            (16, 55, 16),
            (16, 60, 16),
        ],
    },
    // The Pad's chords, so the two compare.
    Phrase {
        key: "strings",
        notes: &[
            (0, 48, 16),
            (0, 52, 16),
            (0, 55, 16),
            (0, 59, 16),
            (16, 45, 16),
            (16, 52, 16),
            (16, 55, 16),
            (16, 60, 16),
        ],
    },
    // A held note for the vibrato, overlapping notes that glide, and fresh
    // notes after rests.
    Phrase {
        key: "lead",
        notes: &[
            (0, 67, 6),
            (5, 72, 3),
            (8, 71, 2),
            (10, 67, 2),
            (12, 64, 4),
            (16, 67, 3),
            (20, 69, 3),
            (22, 71, 3),
            (24, 72, 8),
        ],
    },
    // Each fresh note tears in; the held ones settle.
    Phrase {
        key: "sync_lead",
        notes: &[
            (0, 60, 6),
            (8, 63, 2),
            (12, 65, 4),
            (16, 67, 6),
            (24, 65, 2),
            (26, 63, 2),
            (28, 60, 4),
        ],
    },
    // A sparse tune with room to ring.
    Phrase {
        key: "bell",
        notes: &[
            (0, 72, 2),
            (4, 79, 2),
            (8, 76, 2),
            (14, 74, 2),
            (16, 72, 2),
            (20, 67, 2),
            (24, 69, 4),
        ],
    },
];

/// Where `phrase`'s listening render is, from the repository's root: its
/// place in the Sound menu, then its key.
fn list_path(index: usize, phrase: &Phrase) -> String {
    format!(
        "examples/listen/factory/{:02}-{}.json",
        index + 1,
        phrase.key.replace('_', "-")
    )
}

/// The command list that plays `sound` in `phrase`: a new project with a
/// 2-bar loop at 120 BPM, its first track's sound chosen as the Sound menu
/// chooses it, and the phrase in its clip.
fn listening_list(index: usize, sound: &FactorySound, phrase: &Phrase) -> CommandList {
    let id = ProjectId::from_uuid(Uuid::from_u128(0xfac7_0000 + index as u128));
    let project = Project::with_id(id);
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let notes = phrase
        .notes
        .iter()
        .enumerate()
        .map(|(i, &(start, pitch, length))| uta_core::Note {
            id: NoteId::from_uuid(Uuid::from_u128(0xfac7_1000 + i as u128)),
            pitch,
            velocity: 100,
            start: start * SIXTEENTH,
            length: length * SIXTEENTH,
        })
        .collect();
    CommandList {
        project: id,
        commands: vec![
            Command::SetTempo { bpm: BPM },
            Command::SetLoopLength { bars: 2 },
            Command::SetSynthSettings {
                track,
                settings: sound.settings,
            },
            Command::AddNotes { clip, notes },
        ],
    }
}

/// Each factory sound's listening render in `examples/listen/factory/`
/// plays it with its settings as they are here, so the renders can't drift
/// from the sounds. `UTA_GOLDEN=1` rewrites them, as it does the golden
/// WAVs, three of which are rendered from these lists.
#[test]
fn the_listening_renders_play_the_factory_sounds() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (index, (sound, phrase)) in FactorySound::all().iter().zip(&PHRASES).enumerate() {
        assert_eq!(sound.key, phrase.key, "the phrases are in the menu's order");
        let list = listening_list(index, sound, phrase);
        list.build().expect("the listening render builds");
        let path = root.join(list_path(index, phrase));
        let json = serde_json::to_string_pretty(&list).unwrap() + "\n";
        if std::env::var_os("UTA_GOLDEN").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, json).unwrap();
            eprintln!("Wrote {}", path.display());
            continue;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "can't read {} ({e}); write it with UTA_GOLDEN=1",
                path.display()
            )
        });
        assert!(
            committed == json,
            "{} doesn't play the {} as it is; rewrite it with UTA_GOLDEN=1",
            path.display(),
            sound.key
        );
    }
}

/// How often [`switching`] chooses the next sound: every 38 blocks of 128,
/// about a tenth of a second.
const SWITCH_SAMPLES: usize = 38 * 128;

/// How long after a switch [`assert_switches_are_smooth`] looks for a
/// click: longer than the synth's smoothing and crossfades.
const SWITCH_WINDOW: usize = 2_400;

/// A C major chord at 120 BPM, struck at the top of each bar and held
/// almost to the next, as the sounds are switched over it: the Mono sounds
/// play its top note.
fn struck_chords() -> Project {
    let mut project = synth_project();
    let clip = project.tracks()[0].clips()[0].id();
    let notes = (0..4u64)
        .flat_map(|bar| [48u8, 52, 55].map(move |pitch| (bar, pitch)))
        .enumerate()
        .map(|(i, (bar, pitch))| uta_core::Note {
            id: NoteId::from_uuid(Uuid::from_u128(1000 + i as u128)),
            pitch,
            velocity: 100,
            start: bar * 4 * TICKS_PER_QUARTER,
            length: 4 * TICKS_PER_QUARTER - SIXTEENTH,
        })
        .collect();
    for command in [
        Command::SetTempo { bpm: BPM },
        Command::AddNotes { clip, notes },
    ] {
        project.apply(&command).unwrap();
    }
    project
}

/// [`struck_chords`] played with `sounds` in turn, each chosen as the Sound
/// menu chooses it, [`SWITCH_SAMPLES`] apart, the first from the start.
fn switching(sounds: &[FactorySound]) -> Vec<f32> {
    let mut project = struck_chords();
    let track = project.tracks()[0].id();
    let config = uta_engine::EngineConfig {
        sample_rate: 48_000,
        channels: 1,
    };
    let mut renderer =
        uta_engine::offline::Renderer::new(config, uta_engine::Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    for sound in sounds {
        project
            .apply(&Command::SetSynthSettings {
                track,
                settings: sound.settings,
            })
            .unwrap();
        renderer
            .controller
            .set_snapshot(uta_engine::Snapshot::from(&project))
            .unwrap();
        renderer.render(SWITCH_SAMPLES);
        renderer.controller.free_used_snapshots();
    }
    renderer.into_samples()
}

/// Fails if `switched`, which plays `sounds` in turn as [`switching`] does,
/// clicks after a switch: in the window after each, its steepest step is
/// more than either sound's own steepest at that moment, each played alone
/// through the whole render (`alone`, by the sound's place in
/// [`FactorySound::all`]), plus 10%. A saw's edge is a big step by nature,
/// so a switch is held to the sounds' own steps rather than a sine's.
fn assert_switches_are_smooth(sounds: &[FactorySound], switched: &[f32], alone: &[Vec<f32>]) {
    let all = FactorySound::all();
    let index = |sound: &FactorySound| all.iter().position(|s| s.key == sound.key).unwrap();
    for (k, pair) in sounds.windows(2).enumerate() {
        let at = (k + 1) * SWITCH_SAMPLES;
        // From the sample before, so the step into the switch counts.
        let window = at - 1..at + SWITCH_WINDOW;
        let own = |sound: &FactorySound| max_jump(&alone[index(sound)][window.clone()]).0;
        let limit = 1.1 * own(&pair[0]).max(own(&pair[1]));
        let (jump, offset) = max_jump(&switched[window.clone()]);
        assert!(
            jump <= limit,
            "{} to {}: a step of {jump} {offset} samples after the switch, limit {limit}",
            pair[0].key,
            pair[1].key
        );
    }
}

/// Each sound played alone through a whole [`switching`] render of
/// `switches`, in [`FactorySound::all`]'s order.
fn every_sound_alone(switches: usize) -> Vec<Vec<f32>> {
    FactorySound::all()
        .iter()
        .map(|sound| switching(&vec![sound.clone(); switches]))
        .collect()
}

/// An order of `n` things that goes from each to each other exactly once:
/// a walk round every edge of the complete directed graph on `n` (an
/// Euler circuit, by Hierholzer's algorithm).
fn every_pair_once(n: usize) -> Vec<usize> {
    let mut next: Vec<Vec<usize>> = (0..n)
        .map(|from| (0..n).rev().filter(|&to| to != from).collect())
        .collect();
    let (mut stack, mut order) = (vec![0], Vec::new());
    while let Some(&at) = stack.last() {
        match next[at].pop() {
            Some(to) => stack.push(to),
            None => order.push(stack.pop().unwrap()),
        }
    }
    order.reverse();
    order
}

/// Choosing factory sounds while chords sound doesn't click: from every
/// sound to every other, a tenth of a second apart, over the chords'
/// starts, holds and releases, the switch moves no faster than the sounds
/// themselves do.
#[test]
fn switching_factory_sounds_while_notes_sound_stays_under_the_click_limit() {
    let all = FactorySound::all();
    let order: Vec<FactorySound> = every_pair_once(all.len())
        .into_iter()
        .map(|i| all[i].clone())
        .collect();
    let pairs: std::collections::HashSet<_> = order
        .windows(2)
        .map(|pair| (pair[0].key, pair[1].key))
        .collect();
    assert_eq!(pairs.len(), all.len() * (all.len() - 1));
    assert_eq!(order.len(), pairs.len() + 1);
    let switched = switching(&order);
    let alone = every_sound_alone(order.len());
    assert_switches_are_smooth(&order, &switched, &alone);
}

/// Guards the switching test: cutting from the Pad to the Bell at a
/// switch, sample to sample, as a sound that changed at once would, must
/// fail it.
#[test]
fn switching_test_catches_a_hard_cut() {
    let all = FactorySound::all();
    let pad = all.iter().position(|s| s.key == "pad").unwrap();
    let bell = all.iter().position(|s| s.key == "bell").unwrap();
    let alone = every_sound_alone(8);
    let at = 7 * SWITCH_SAMPLES;
    let cut: Vec<f32> = alone[pad][..at]
        .iter()
        .chain(&alone[bell][at..])
        .copied()
        .collect();
    let mut sounds = vec![all[pad].clone(); 7];
    sounds.push(all[bell].clone());
    let result = std::panic::catch_unwind(|| assert_switches_are_smooth(&sounds, &cut, &alone));
    assert!(
        result.is_err(),
        "a hard cut from the Pad to the Bell passed"
    );
}
