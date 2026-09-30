//! Block timing against the real-time deadline. Reported, never gating: shared
//! CI machines are too noisy to fail on. Ignored by default because debug
//! timings mean nothing. Run it with
//! `cargo test -p uta-engine --release --test timing -- --ignored --nocapture`.
//! In CI it also writes the table to the job summary.

mod common;

use std::io::Write;
use std::time::{Duration, Instant};

use uta_core::{
    ClipId, Command, Note, NoteId, PlacedTrack, Project, ProjectId, SynthParam, TrackId,
};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, NoteKey, Snapshot, SynthSettings, VOICES};
use uuid::Uuid;

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const BLOCKS: usize = 20_000;

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
}

impl Load {
    fn name(self) -> &'static str {
        match self {
            Self::Loop => "Demo loop",
            Self::Voices => "16 voices",
            Self::Song => "Demo song",
            Self::Tracks => "32 tracks × 8 voices",
        }
    }
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

struct Report {
    load: Load,
    block_size: usize,
    deadline: Duration,
    p50: Duration,
    p99: Duration,
    max: Duration,
}

fn measure(load: Load, block_size: usize) -> Report {
    let config = EngineConfig {
        sample_rate: SAMPLE_RATE,
        channels: CHANNELS,
    };
    let snapshot = match load {
        Load::Loop => Snapshot::from(&common::demo_loop()),
        Load::Voices => Snapshot::default(),
        Load::Song => Snapshot::from(&common::demo_song()),
        Load::Tracks => Snapshot::from(&many_tracks()),
    };
    let mut renderer = Renderer::new(config, snapshot, block_size);
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
                Load::Loop | Load::Song | Load::Tracks => renderer
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
            }
        }
        let start = Instant::now();
        renderer.processor().process(&mut buffer);
        times.push(start.elapsed());
        renderer.controller.poll();
    }
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
    for (load, block_size) in [Load::Loop, Load::Voices, Load::Song, Load::Tracks]
        .into_iter()
        .flat_map(|load| [32, 128, 1024].map(|block_size| (load, block_size)))
    {
        let r = measure(load, block_size);
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
