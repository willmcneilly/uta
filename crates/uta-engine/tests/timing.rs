//! Block timing against the real-time deadline. Reported, never gating: shared
//! CI machines are too noisy to fail on. Ignored by default because debug
//! timings mean nothing. Run it with
//! `cargo test -p uta-engine --release --test timing -- --ignored --nocapture`.
//! In CI it also writes the table to the job summary.

mod common;

use std::io::Write;
use std::time::{Duration, Instant};

use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{
    Clip, ClipId, Command, DrumParam, DrumSound, KIT, Note, NoteId, PlacedClip, PlacedTrack,
    Project, ProjectId, SynthParam, TrackId,
};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, NoteKey, Snapshot, SynthSettings, TrackSound, VOICES};
use uuid::Uuid;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const BLOCKS: usize = 20_000;
/// How many times Play is pressed to time the first block after it.
const PLAYS: usize = 200;
/// A bar of 4/4, in ticks.
const BAR: Ticks = 4 * TICKS_PER_QUARTER;

/// What the engine is doing while it's timed.
#[derive(Clone, Copy)]
enum Load {
    /// The demo loop playing, with the volume gliding.
    Loop,
    /// All 16 synth voices sounding saws, with the filter cutoff gliding, on
    /// top of an empty loop.
    Voices,
    /// The demo song's three tracks playing, with the volume gliding.
    Song,
    /// 32 tracks each holding an 8-note saw chord: 256 voices, with the
    /// volume gliding. Far more than a realistic song, to see where the
    /// limit is. See RFC-003, "Risks & unknowns" (CPU with many tracks).
    Tracks,
    /// Make a song's manual check 7, the stress ceiling: 7 tracks × 28
    /// one-bar clips × 3,000 stress notes, 588,000 notes in all, with the
    /// volume gliding. See RFC-004, "What we measured".
    Check7,
    /// 32 drum tracks, each playing every kit note at once on every 16th,
    /// with every sound at its longest Decay so they never fall silent, and
    /// the kick's and snare's Tune and the clap's Tone gliding. It grows as
    /// the sounds arrive. See RFC-006,
    /// "Risks & unknowns" (CPU).
    Drums,
    /// The same song, timing only the first block after Play from bar
    /// 10.5, where the notes already sounding there are started. Play is
    /// pressed [`PLAYS`] times, each once the last Play's notes are silent.
    Check7FirstBlock,
}

impl Load {
    fn name(self) -> &'static str {
        match self {
            Self::Loop => "Demo loop",
            Self::Voices => "16 voices",
            Self::Song => "Demo song",
            Self::Tracks => "32 tracks × 8 voices",
            Self::Check7 => "Check 7 (588k notes)",
            Self::Drums => "32 drum tracks × every sound",
            Self::Check7FirstBlock => "Check 7, first block after Play from bar 10.5",
        }
    }
}

/// Check 7's song: 7 tracks, each with 28 one-bar clips end to end, each
/// clip holding the same 3,000 stress notes. The loop stays on its first 4
/// bars, which play the same notes as any other 4.
fn check_7() -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let track = project.tracks()[0].id();
    let first = project.tracks()[0].clips()[0].id();
    project
        .apply(&Command::RemoveClips { clips: vec![first] })
        .unwrap();
    // Only the pattern: each bar's copy gets new IDs below.
    let pattern = common::stress_notes(0);
    let clips = (0..28)
        .map(|bar| PlacedClip {
            track,
            clip: Clip::new(ClipId::random(), bar * BAR, BAR).with_notes(pattern.iter().map(
                |note| Note {
                    id: NoteId::random(),
                    ..*note
                },
            )),
        })
        .collect();
    project.apply(&Command::AddClips { clips }).unwrap();
    let first = project.tracks()[0].clone();
    let copies = (1..7)
        .map(|index| PlacedTrack {
            index,
            track: first.copy(
                TrackId::random(),
                format!("Synth {}", index + 1),
                ClipId::random,
                NoteId::random,
            ),
        })
        .collect();
    project
        .apply(&Command::AddTracks { tracks: copies })
        .unwrap();
    project
}

/// 32 tracks, each holding the same 8-note saw chord through the whole
/// loop at full sustain, centred.
fn many_tracks() -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let loop_length = project.transport().loop_length();
    let notes = (0..8u8)
        .map(|i| Note {
            id: NoteId::random(),
            pitch: 36 + i * 5,
            velocity: 100,
            start: 0,
            length: loop_length,
        })
        .collect();
    project
        .apply(&Command::SetSynthParam {
            track,
            param: SynthParam::Sustain(1.0),
        })
        .unwrap();
    project.apply(&Command::AddNotes { clip, notes }).unwrap();
    let first = project.tracks()[0].clone();
    let copies = (1..Project::MAX_TRACKS)
        .map(|index| PlacedTrack {
            index,
            track: first.copy(
                TrackId::random(),
                format!("Synth {}", index + 1),
                ClipId::random,
                NoteId::random,
            ),
        })
        .collect();
    project
        .apply(&Command::AddTracks { tracks: copies })
        .unwrap();
    project
}

/// 32 drum tracks, each hitting every kit note on every 16th of the loop,
/// with every sound at its longest Decay.
fn drum_tracks() -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let synth = project.tracks()[0].id();
    let loop_length = project.transport().loop_length();
    let sixteenth = TICKS_PER_QUARTER / 4;
    let notes = (0..loop_length / sixteenth).flat_map(|step| {
        KIT.iter().map(move |row| Note {
            id: NoteId::random(),
            pitch: row.pitch,
            velocity: 100,
            start: step * sixteenth,
            length: sixteenth,
        })
    });
    let drums = uta_core::Track::new(
        TrackId::random(),
        "Drums 1",
        uta_core::Source::Drums(uta_core::KitSettings::default()),
    )
    .with_clips([Clip::new(ClipId::random(), 0, loop_length).with_notes(notes)]);
    let id = drums.id();
    project
        .apply(&Command::AddTracks {
            tracks: vec![PlacedTrack {
                index: 1,
                track: drums,
            }],
        })
        .unwrap();
    project
        .apply(&Command::RemoveTracks {
            tracks: vec![synth],
        })
        .unwrap();
    for (sound, param) in [
        (
            DrumSound::Kick,
            DrumParam::DecaySeconds(uta_core::KickSettings::MAX_DECAY_SECONDS),
        ),
        (
            DrumSound::Snare,
            DrumParam::Tone(uta_core::SnareSettings::MAX_TONE_SECONDS),
        ),
        (
            DrumSound::Clap,
            DrumParam::DecaySeconds(uta_core::ClapSettings::MAX_DECAY_SECONDS),
        ),
    ] {
        project
            .apply(&Command::SetDrumParam {
                track: id,
                sound,
                param,
            })
            .unwrap();
    }
    let first = project.tracks()[0].clone();
    let copies = (1..Project::MAX_TRACKS)
        .map(|index| PlacedTrack {
            index,
            track: first.copy(
                TrackId::random(),
                format!("Drums {}", index + 1),
                ClipId::random,
                NoteId::random,
            ),
        })
        .collect();
    project
        .apply(&Command::AddTracks { tracks: copies })
        .unwrap();
    project
}

struct Report {
    load: Load,
    block_size: usize,
    deadline: Duration,
    p50: Duration,
    p99: Duration,
    max: Duration,
}

fn snapshot(load: Load) -> Snapshot {
    match load {
        Load::Loop => Snapshot::from(&common::demo_loop()),
        Load::Voices => Snapshot::default(),
        Load::Song => Snapshot::from(&common::demo_song()),
        Load::Tracks => Snapshot::from(&many_tracks()),
        Load::Check7 | Load::Check7FirstBlock => Snapshot::from(&check_7()),
        Load::Drums => Snapshot::from(&drum_tracks()),
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: SAMPLE_RATE,
        channels: CHANNELS,
    }
}

fn report(load: Load, block_size: usize, mut times: Vec<Duration>) -> Report {
    times.sort_unstable();
    let percentile = |p: f64| times[((times.len() - 1) as f64 * p).round() as usize];
    Report {
        load,
        block_size,
        deadline: Duration::from_secs_f64(block_size as f64 / f64::from(SAMPLE_RATE)),
        p50: percentile(0.5),
        p99: percentile(0.99),
        max: *times.last().unwrap(),
    }
}

fn measure(load: Load, block_size: usize, snapshot: Snapshot) -> Report {
    if let Load::Check7FirstBlock = load {
        return measure_first_blocks(block_size, snapshot);
    }
    let mut renderer = Renderer::new(config(), snapshot, block_size);
    let mut buffer = vec![0.0; block_size * CHANNELS];
    let mut times = Vec::with_capacity(BLOCKS);
    renderer.controller.play().unwrap();
    if let Load::Voices = load {
        for i in 0..VOICES as u8 {
            renderer
                .controller
                .note_on(0, NoteKey(u128::from(i)), 36 + i * 3, 100)
                .unwrap();
        }
    }
    for i in 0..BLOCKS {
        // Keep the smoothing busy, as a user dragging the volume would.
        if i % 64 == 0 {
            match load {
                Load::Loop | Load::Song | Load::Tracks | Load::Check7 => renderer
                    .controller
                    .set_volume_db(-(((i / 64) % 24) as f32))
                    .unwrap(),
                // Sustain 1, so the voices never fade to silence.
                Load::Voices => renderer
                    .controller
                    .set_synth_settings(
                        0,
                        SynthSettings {
                            cutoff_hz: 200.0 * (1 + (i / 64) % 50) as f32,
                            resonance: 0.5,
                            sustain: 1.0,
                            ..SynthSettings::default()
                        },
                    )
                    .unwrap(),
                // Every kick's and snare's Tune, and every clap's Tone,
                // gliding.
                Load::Drums => {
                    let mut snapshot = renderer.controller.snapshot().clone();
                    for track in snapshot.tracks_mut() {
                        if let TrackSound::Drums(kit) = &mut track.sound {
                            let step = ((i / 64) % 40) as f32;
                            kit.kick.tune_hz = 40.0 + step;
                            kit.snare.tune_hz = 140.0 + 3.0 * step;
                            kit.clap.tone_hz = 700.0 + 30.0 * step;
                        }
                    }
                    renderer.controller.set_snapshot(snapshot).unwrap();
                }
                Load::Check7FirstBlock => unreachable!(),
            }
        }
        let start = Instant::now();
        renderer.processor().process(&mut buffer);
        times.push(start.elapsed());
        renderer.controller.poll();
    }
    report(load, block_size, times)
}

/// Times the first block after Play from bar 10.5, [`PLAYS`] times. Before
/// each Play the last one is stopped and its notes are left to fall silent,
/// so every Play starts the same way.
fn measure_first_blocks(block_size: usize, snapshot: Snapshot) -> Report {
    let mut renderer = Renderer::new(config(), snapshot, block_size);
    let mut buffer = vec![0.0; block_size * CHANNELS];
    let mut times = Vec::with_capacity(PLAYS);
    let silent_within = 5 * SAMPLE_RATE as usize / block_size;
    for _ in 0..PLAYS {
        renderer.controller.stop().unwrap();
        renderer.controller.locate(9 * BAR + BAR / 2).unwrap();
        for _ in 0..silent_within {
            renderer.processor().process(&mut buffer);
            if renderer.controller.poll().sounding_slots == 0 {
                break;
            }
        }
        renderer.controller.play().unwrap();
        let start = Instant::now();
        renderer.processor().process(&mut buffer);
        times.push(start.elapsed());
        renderer.controller.poll();
    }
    report(Load::Check7FirstBlock, block_size, times)
}

fn micros(duration: Duration) -> String {
    format!("{:.2} µs", duration.as_secs_f64() * 1e6)
}

fn share(duration: Duration, deadline: Duration) -> String {
    format!(
        "{:.3}%",
        duration.as_secs_f64() / deadline.as_secs_f64() * 100.0
    )
}

#[test]
#[ignore = "timing report: run in release with --ignored --nocapture"]
fn report_block_timing() {
    let mut table = format!(
        "### Block timing ({SAMPLE_RATE} Hz, {CHANNELS} channels, {BLOCKS} blocks)\n\n\
         | Load | Block | Deadline | p50 | p99 | Max | p99 of deadline |\n\
         |---|---|---|---|---|---|---|\n"
    );
    let loads = [
        Load::Loop,
        Load::Voices,
        Load::Song,
        Load::Tracks,
        Load::Check7,
        Load::Check7FirstBlock,
        Load::Drums,
    ];
    for load in loads {
        let snapshot = snapshot(load);
        for block_size in [32, 128, 1024] {
            let r = measure(load, block_size, snapshot.clone());
            table += &format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                r.load.name(),
                r.block_size,
                micros(r.deadline),
                micros(r.p50),
                micros(r.p99),
                micros(r.max),
                share(r.p99, r.deadline),
            );
        }
    }
    if cfg!(debug_assertions) {
        table += "\nDebug build: these numbers are not representative.\n";
    }
    println!("{table}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let mut summary = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();
        writeln!(summary, "{table}").unwrap();
    }
}
