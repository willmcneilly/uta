//! Block timing against the real-time deadline. Reported, never gating: shared
//! CI machines are too noisy to fail on. Ignored by default because debug
//! timings mean nothing. Run it with
//! `cargo test -p uta-engine --release --test timing -- --ignored --nocapture`.
//! Locally it compares each p99 with the committed baseline (see
//! [`BASELINE`]). In CI it also writes the table to the job summary.

mod common;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::synth_project;

use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{
    Clip, ClipId, Command, DrumParam, DrumSound, KIT, Note, NoteId, PlacedClip, PlacedTrack,
    Project, SynthParam, TrackId,
};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, NoteKey, Snapshot, SynthSettings, TrackSound, VOICES};

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
    /// the kick's, snare's, hats' and toms' Tune and the clap's, hats' and
    /// cymbal's Tone gliding. It grows as the sounds arrive. See RFC-006, "Risks &
    /// unknowns" (CPU).
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
    let mut project = synth_project();
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
    let mut project = synth_project();
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
    let mut project = synth_project();
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
        (
            DrumSound::ClosedHat,
            DrumParam::DecaySeconds(uta_core::ClosedHatSettings::MAX_DECAY_SECONDS),
        ),
        (
            DrumSound::OpenHat,
            DrumParam::DecaySeconds(uta_core::OpenHatSettings::MAX_DECAY_SECONDS),
        ),
        (
            DrumSound::LowTom,
            DrumParam::DecaySeconds(uta_core::LowTomSettings::MAX_DECAY_SECONDS),
        ),
        (
            DrumSound::HighTom,
            DrumParam::DecaySeconds(uta_core::HighTomSettings::MAX_DECAY_SECONDS),
        ),
        (
            DrumSound::Cymbal,
            DrumParam::DecaySeconds(uta_core::CymbalSettings::MAX_DECAY_SECONDS),
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
                // Every kick's, snare's, hats' and toms' Tune, and every
                // clap's, hats' and cymbal's Tone, gliding.
                Load::Drums => {
                    let mut snapshot = renderer.controller.snapshot().clone();
                    for track in snapshot.tracks_mut() {
                        if let TrackSound::Drums(kit) = &mut track.sound {
                            let step = ((i / 64) % 40) as f32;
                            kit.kick.tune_hz = 40.0 + step;
                            kit.snare.tune_hz = 140.0 + 3.0 * step;
                            kit.clap.tone_hz = 700.0 + 30.0 * step;
                            kit.closed_hat.tune_hz = 102.65 + 7.0 * step;
                            kit.closed_hat.tone_hz = 4000.0 + 200.0 * step;
                            kit.low_tom.tune_hz = 80.0 + 0.5 * step;
                            kit.high_tom.tune_hz = 165.0 + 1.375 * step;
                            kit.cymbal.tone = step / 40.0;
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

/// The committed baseline the report compares against, so a change can be
/// judged without building `main` for numbers to compare. Regenerate it with
/// `UTA_TIMING_BASELINE=1` on a quiet machine when a change makes the engine
/// faster or slower on purpose, or adds a load, and say so in the PR.
const BASELINE: &str = "tests/timing-baseline.tsv";

/// The baseline's p50 and p99 in µs, by load name and block size, and the
/// line saying where it was recorded.
struct Baseline {
    recorded: String,
    rows: HashMap<(String, usize), (f64, f64)>,
}

fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(BASELINE)
}

fn read_baseline() -> Option<Baseline> {
    let text = std::fs::read_to_string(baseline_path()).ok()?;
    let mut recorded = String::new();
    let mut rows = HashMap::new();
    for line in text.lines() {
        if let Some(comment) = line.strip_prefix("# ") {
            if comment.starts_with("Recorded") {
                recorded = comment.to_string();
            }
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if let [load, block, p50, p99] = fields[..]
            && let (Ok(block), Ok(p50), Ok(p99)) = (block.parse(), p50.parse(), p99.parse())
        {
            rows.insert((load.to_string(), block), (p50, p99));
        }
    }
    Some(Baseline { recorded, rows })
}

fn write_baseline(reports: &[Report]) {
    let output = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string())
    };
    // The date as well as the commit: a branch's commit is gone once it's
    // squashed into main.
    let date = output("date", &["+%-d %b %Y"]);
    let commit = output("git", &["rev-parse", "--short", "HEAD"]);
    let mut text = format!(
        "# The block timing report's baseline: p50 and p99 in µs. See BASELINE in timing.rs.\n\
         # Recorded on {date} at {commit}, on Will's Mac.\n\
         load\tblock\tp50_us\tp99_us\n"
    );
    for r in reports {
        text += &format!(
            "{}\t{}\t{:.2}\t{:.2}\n",
            r.load.name(),
            r.block_size,
            r.p50.as_secs_f64() * 1e6,
            r.p99.as_secs_f64() * 1e6,
        );
    }
    std::fs::write(baseline_path(), text).unwrap();
}

/// How this run's time compares with the baseline's, e.g. "+4%".
fn change(now: Duration, baseline: f64) -> String {
    format!(
        "{:+.0}%",
        (now.as_secs_f64() * 1e6 / baseline - 1.0) * 100.0
    )
}

#[test]
#[ignore = "timing report: run in release with --ignored --nocapture"]
fn report_block_timing() {
    let loads = [
        Load::Loop,
        Load::Voices,
        Load::Song,
        Load::Tracks,
        Load::Check7,
        Load::Check7FirstBlock,
        Load::Drums,
    ];
    let mut reports = Vec::new();
    for load in loads {
        let snapshot = snapshot(load);
        for block_size in [32, 128, 1024] {
            reports.push(measure(load, block_size, snapshot.clone()));
        }
    }

    let writing = std::env::var_os("UTA_TIMING_BASELINE").is_some();
    // CI's machines aren't the one the baseline was recorded on, so it isn't
    // compared there.
    let baseline = if writing || std::env::var_os("GITHUB_ACTIONS").is_some() {
        None
    } else {
        read_baseline()
    };
    let mut table =
        format!("### Block timing ({SAMPLE_RATE} Hz, {CHANNELS} channels, {BLOCKS} blocks)\n\n");
    if let Some(baseline) = &baseline {
        // Measured when the baseline was recorded: two runs of the same code.
        table += &format!(
            "Compared with {BASELINE}. {} Two runs of the same code moved p50 \
             by under 5% and p99 by up to 90%, so read p50 first, and run it \
             again before trusting a change in p99.\n\n\
             | Load | Block | Deadline | p50 | p99 | Max | p99 of deadline | p50 change | p99 change |\n\
             |---|---|---|---|---|---|---|---|---|\n",
            baseline.recorded,
        );
    } else {
        table += "| Load | Block | Deadline | p50 | p99 | Max | p99 of deadline |\n\
                  |---|---|---|---|---|---|---|\n";
    }
    for r in &reports {
        table += &format!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            r.load.name(),
            r.block_size,
            micros(r.deadline),
            micros(r.p50),
            micros(r.p99),
            micros(r.max),
            share(r.p99, r.deadline),
        );
        if let Some(baseline) = &baseline {
            table += &match baseline
                .rows
                .get(&(r.load.name().to_string(), r.block_size))
            {
                Some(&(_, p99)) => format!(" {p99:.2} µs | {} |", change(r.p99, p99)),
                None => " – | new |".to_string(),
            };
        }
        table += "\n";
    }
    if cfg!(debug_assertions) {
        table += "\nDebug build: these numbers are not representative.\n";
    }
    if writing {
        write_baseline(&reports);
        table += &format!("\nWrote this run to {BASELINE}.\n");
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
