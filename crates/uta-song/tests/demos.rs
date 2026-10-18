//! The demo songs in `examples/demos/`: each builds, plays without
//! clipping, and has no silent track. Not golden WAVs: the songs are meant
//! to change. See RFC-009, "The demo song".
//!
//! Building is quick, so every `cargo test` does it. Rendering every song,
//! and every track on its own, takes minutes in a debug build, so that test
//! is ignored, and CI runs it with `--release` in its "Timing reports and demo renders" job:
//! `cargo test -p uta-song --release --test demos -- --ignored`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use uta_core::time::Ticks;
use uta_core::{Command, CommandList, MixerStrip, Project, Track};
use uta_engine::{EngineConfig, Snapshot, offline};

fn demos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demos")
}

/// Every song file in `examples/demos/`, in name order.
fn songs() -> Vec<PathBuf> {
    let mut songs: Vec<_> = std::fs::read_dir(demos())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    songs.sort();
    songs
}

fn build(path: &Path) -> Project {
    let text = std::fs::read_to_string(path).unwrap();
    let list =
        uta_song::compile(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    list.build()
        .unwrap_or_else(|(index, error)| panic!("{}: command {index}: {error}", path.display()))
}

/// The song from the top to its end, with the loop off, and its tail.
fn render(project: &Project) -> Vec<f32> {
    let mut project = project.clone();
    project
        .apply(&Command::SetLoopEnabled { enabled: false })
        .unwrap();
    let config = EngineConfig {
        channels: 2,
        ..EngineConfig::default()
    };
    let samples = project
        .transport()
        .tempo_map()
        .ticks_to_samples(project.song_end(), config.sample_rate);
    let seconds = samples as f64 / f64::from(config.sample_rate);
    offline::render_song(config, Snapshot::from(&project), seconds).samples
}

/// The quietest a track may be on its own, at its loudest, in dBFS: well
/// above silence, and far below any part meant to be heard.
const SILENT_DB: f64 = -50.0;

fn db(level: f32) -> f64 {
    20.0 * f64::from(level).log10()
}

#[test]
fn every_demo_song_compiles_and_builds() {
    let songs = songs();
    assert!(
        songs.iter().any(|path| path.ends_with("loop.toml")),
        "the demo songs are in {}",
        demos().display()
    );
    for path in &songs {
        build(path);
    }
}

/// What one render plays: a song, or one of its tracks on its own.
struct Render {
    song: String,
    /// The track soloed, or `None` for the whole song.
    track: Option<String>,
    project: Project,
}

/// Every song, then every track of it on its own.
fn renders() -> Vec<Render> {
    let mut renders = Vec::new();
    for path in songs() {
        let song = path.file_name().unwrap().to_string_lossy().into_owned();
        let project = build(&path);
        for track in project.tracks() {
            let mut solo = project.clone();
            solo.apply(&Command::SetTrackMixer {
                track: track.id(),
                mixer: MixerStrip {
                    solo: true,
                    ..*track.mixer()
                },
            })
            .unwrap();
            renders.push(Render {
                song: song.clone(),
                track: Some(track.name().to_owned()),
                project: solo,
            });
        }
        renders.push(Render {
            song,
            track: None,
            project,
        });
    }
    renders
}

/// Each render's peak, in the order given, rendered on every core at once:
/// each thread takes the next render not yet taken, so the time grows with
/// the total, divided by the cores, not one song after another.
fn peaks(renders: &[Render]) -> Vec<f32> {
    let next = AtomicUsize::new(0);
    let threads = thread::available_parallelism().map_or(1, usize::from);
    let mut peaks = vec![0.0; renders.len()];
    thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = renders.get(index) else {
                            return done;
                        };
                        done.push((index, uta_analysis::peak(&render(&job.project))));
                    }
                })
            })
            .collect();
        for worker in workers {
            for (index, peak) in worker.join().unwrap() {
                peaks[index] = peak;
            }
        }
    });
    peaks
}

#[test]
#[ignore = "renders every song: slow in debug, so CI runs it with --release"]
fn every_demo_song_plays_without_clipping_and_has_no_silent_track() {
    let renders = renders();
    let peaks = peaks(&renders);
    let mut failures = Vec::new();
    for (render, peak) in renders.iter().zip(peaks) {
        match &render.track {
            // The engine hard clips at full scale, so a song that clips has
            // samples at full scale exactly.
            None if peak >= 1.0 => failures.push(format!(
                "{} clips: its peak is {:+.2} dBFS",
                render.song,
                db(peak)
            )),
            Some(track) if db(peak) <= SILENT_DB => failures.push(format!(
                "{}'s {track} is silent: its peak on its own is {:.1} dBFS",
                render.song,
                db(peak)
            )),
            _ => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Each note a track plays, in song time: (start, length, pitch, velocity).
fn notes(track: &Track) -> Vec<(Ticks, Ticks, u8, u8)> {
    let mut notes: Vec<_> = track
        .clips()
        .iter()
        .flat_map(|clip| {
            clip.notes().map(|note| {
                (
                    clip.start() + note.start,
                    note.length,
                    note.pitch,
                    note.velocity,
                )
            })
        })
        .collect();
    notes.sort();
    notes
}

/// `loop.toml` and `beat.toml` were rewritten from `demo-loop.json` and
/// `demo-beat.json`, and build the same: the same tempo, master volume and
/// loop, and each track the same sound, mixer strip, effects and notes,
/// where each note is when it plays. The IDs differ, as do the clips' shapes
/// (a clip for each time a section comes round), and the command lists'
/// empty default tracks aren't in the songs.
#[test]
fn loop_and_beat_build_the_same_tracks_and_notes_as_their_command_lists() {
    for (song, list) in [
        ("loop.toml", "demo-loop.json"),
        ("beat.toml", "demo-beat.json"),
    ] {
        let song = build(&demos().join(song));
        let json = std::fs::read_to_string(demos().join("..").join(list)).unwrap();
        let list: CommandList = serde_json::from_str(&json).unwrap();
        let list = list.build().unwrap();

        assert_eq!(
            song.transport().tempo_map().bpm(),
            list.transport().tempo_map().bpm()
        );
        assert_eq!(song.master_volume_db(), list.master_volume_db());
        assert_eq!(song.transport().loop_start(), list.transport().loop_start());
        assert_eq!(
            song.transport().loop_length(),
            list.transport().loop_length()
        );
        assert_eq!(
            song.transport().loop_enabled(),
            list.transport().loop_enabled()
        );

        let playing: Vec<_> = list
            .tracks()
            .iter()
            .filter(|track| !notes(track).is_empty())
            .collect();
        assert_eq!(song.tracks().len(), playing.len());
        for (ours, theirs) in song.tracks().iter().zip(playing) {
            assert_eq!(ours.name(), theirs.name());
            assert_eq!(ours.source(), theirs.source(), "{}", ours.name());
            assert_eq!(ours.mixer(), theirs.mixer(), "{}", ours.name());
            let effects = |track: &Track| {
                track
                    .effects()
                    .iter()
                    .map(|effect| (*effect.settings(), effect.mix(), effect.enabled()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(effects(ours), effects(theirs), "{}", ours.name());
            assert_eq!(notes(ours), notes(theirs), "{}", ours.name());
        }
    }
}

/// The example in `examples/demos/README.md` compiles and builds.
#[test]
fn the_readme_example_builds() {
    let readme = std::fs::read_to_string(demos().join("README.md")).unwrap();
    let example = readme
        .split("```toml\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the README has a TOML example");
    let project = uta_song::compile(example)
        .unwrap_or_else(|error| panic!("{error}"))
        .build()
        .unwrap();
    let names: Vec<_> = project.tracks().iter().map(Track::name).collect();
    assert_eq!(names, ["Bass", "Drums"]);
}
