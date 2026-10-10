//! The `uta` command. `render` writes a project to a WAV without a sound
//! device; `render-all` does the same for every command list in a folder;
//! `play` plays it through the default output, round the loop until Ctrl-C
//! or to the song's end. They build the project from a command list
//! (`--commands`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use clap::{Parser, Subcommand};
use uta_core::time::{TICKS_PER_QUARTER, Ticks, TimeSignature};
use uta_core::{CommandList, Project};
use uta_engine::live::{self, DeviceState, DeviceStatus, LiveOutput};
use uta_engine::{EngineConfig, Snapshot, Status, offline};

#[derive(Parser)]
#[command(name = "uta", version = version(), about = "Uta, from the terminal")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render the song to a stereo WAV file, offline (no sound device
    /// needed). It plays from the top, round the loop if it's on, then stops
    /// and lets the notes ring out.
    Render {
        /// Where to write the WAV.
        file: PathBuf,
        /// A JSON command list to build the project from, such as
        /// `examples/demo-loop.json`. Without one, the project is empty and
        /// the render is silent.
        #[arg(long)]
        commands: Option<PathBuf>,
        /// How long to play, in seconds. If not given: with the loop on,
        /// into the loop and twice round it; with the loop off, to the end
        /// of the song.
        #[arg(long, value_parser = positive_seconds)]
        seconds: Option<f64>,
        /// Switch the loop off, so it plays to the end of the song.
        #[arg(long)]
        no_loop: bool,
    },
    /// Render every command list in a folder (each `.json` file in it), to a
    /// WAV of the same name in another, as `render` would with its defaults.
    /// For the listening renders in `examples/listen/`.
    RenderAll {
        /// The folder of JSON command lists, such as `examples/listen/kick`.
        commands: PathBuf,
        /// The folder to write the WAVs to. It's created if it isn't there.
        out: PathBuf,
    },
    /// Play the project through the default output: round the loop until
    /// Ctrl-C, or with the loop off (or starting after it) to the end of the
    /// song. Follows the output when you switch, unplug or replug it.
    Play {
        /// A JSON command list to build the project from, such as
        /// `examples/demo-loop.json`. Without one, the loop is empty and
        /// silent.
        #[arg(long)]
        commands: Option<PathBuf>,
        /// The buffer size in frames: 32, 64 or 128. If the device can't do
        /// it, it gets the nearest size it can.
        #[arg(long, default_value_t = live::DEFAULT_BUFFER_SIZE, value_parser = buffer_size)]
        buffer: u32,
        /// Where to start playing: a bar, or a bar and beat such as `3.3`,
        /// counting from 1. Notes already under way there start at once.
        #[arg(long, default_value = "1", value_parser = position)]
        from: Position,
        /// Switch the loop off, so it plays to the end of the song.
        #[arg(long)]
        no_loop: bool,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Render {
            file,
            commands,
            seconds,
            no_loop,
        } => {
            load(commands.as_deref(), no_loop).and_then(|project| render(&project, &file, seconds))
        }
        Command::RenderAll { commands, out } => render_all(&commands, &out),
        Command::Play {
            commands,
            buffer,
            from,
            no_loop,
        } => load(commands.as_deref(), no_loop).and_then(|project| play(&project, buffer, from)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("uta: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The project a command list builds, or a new, empty one, with the loop
/// switched off if `no_loop`.
fn load(commands: Option<&Path>, no_loop: bool) -> Result<Project, String> {
    let mut project = match commands {
        None => Project::new(),
        Some(path) => {
            let json = std::fs::read_to_string(path)
                .map_err(|error| format!("couldn't read {}: {error}", path.display()))?;
            build(&json).map_err(|error| format!("{}: {error}", path.display()))?
        }
    };
    if no_loop {
        project
            .apply(&uta_core::Command::SetLoopEnabled { enabled: false })
            .expect("switching the loop off always works");
    }
    Ok(project)
}

/// Builds the project a JSON command list describes.
fn build(json: &str) -> Result<Project, String> {
    let list: CommandList =
        serde_json::from_str(json).map_err(|error| format!("isn't a command list: {error}"))?;
    list.build()
        .map_err(|(index, error)| format!("command {} failed: {error}", index + 1))
}

/// Seconds from the start of the song to `ticks`.
fn seconds_at(project: &Project, ticks: Ticks) -> f64 {
    let rate = EngineConfig::default().sample_rate;
    let samples = project
        .transport()
        .tempo_map()
        .ticks_to_samples(ticks, rate);
    samples as f64 / f64::from(rate)
}

/// How long `render` plays by default: with the loop on, from the top into
/// the loop and twice round it; with it off, to the end of the song.
fn default_seconds(project: &Project) -> f64 {
    let transport = project.transport();
    if transport.loop_enabled() {
        seconds_at(
            project,
            transport.loop_start() + 2 * transport.loop_length(),
        )
    } else {
        seconds_at(project, project.song_end())
    }
}

fn render(project: &Project, file: &Path, seconds: Option<f64>) -> Result<(), String> {
    // Stereo, so tracks panned apart stay apart.
    let config = EngineConfig {
        channels: 2,
        ..EngineConfig::default()
    };
    let seconds = seconds.unwrap_or_else(|| default_seconds(project));
    let samples = offline::render_song(config, Snapshot::from(project), seconds);
    offline::write_wav(file, config, &samples)
        .map_err(|error| format!("couldn't write {}: {error}", file.display()))?;
    println!(
        "Wrote {seconds:.2} s of {} (plus the release) at {} Hz to {}",
        describe_project(project),
        config.sample_rate,
        file.display()
    );
    Ok(())
}

/// Renders every `.json` command list in `commands` to a WAV of the same
/// name in `out`, in name order. Stops at the first that fails.
fn render_all(commands: &Path, out: &Path) -> Result<(), String> {
    let mut lists = command_lists(commands)?;
    lists.sort();
    if lists.is_empty() {
        return Err(format!("no .json command lists in {}", commands.display()));
    }
    std::fs::create_dir_all(out)
        .map_err(|error| format!("couldn't create {}: {error}", out.display()))?;
    for list in &lists {
        let project = load(Some(list), false)?;
        let name = list.file_stem().expect("a .json file has a name");
        render(&project, &out.join(name).with_extension("wav"), None)?;
    }
    Ok(())
}

/// The `.json` files in `folder`.
fn command_lists(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(folder)
        .map_err(|error| format!("couldn't read {}: {error}", folder.display()))?;
    let mut lists = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("couldn't read {}: {error}", folder.display()))?
            .path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            lists.push(path);
        }
    }
    Ok(lists)
}

/// A one-line summary: the tracks, notes, tempo, and the loop or the song's
/// length.
fn describe_project(project: &Project) -> String {
    let tracks = project.tracks().len();
    let notes: usize = project
        .tracks()
        .iter()
        .flat_map(|track| track.clips())
        .map(|clip| clip.notes().len())
        .sum();
    let transport = project.transport();
    let bar = transport.time_signature().ticks_per_bar();
    let length = if transport.loop_enabled() {
        format!("a {}-bar loop", transport.loop_length() / bar)
    } else {
        format!("a {}-bar song", project.song_end().div_ceil(bar))
    };
    format!(
        "{notes} notes on {tracks} track{} in {length} at {} BPM",
        if tracks == 1 { "" } else { "s" },
        transport.tempo_map().bpm()
    )
}

/// How often the status line updates.
const STATUS_INTERVAL: Duration = Duration::from_millis(100);

fn play(project: &Project, buffer: u32, from: Position) -> Result<(), String> {
    check_from(project, from)?;
    let interrupted = Arc::new(AtomicBool::new(false));
    ctrlc::set_handler({
        let interrupted = interrupted.clone();
        move || interrupted.store(true, Ordering::Relaxed)
    })
    .map_err(|error| format!("couldn't catch Ctrl-C: {error}"))?;

    // The supervisor moves the processor to the device's own rate before
    // the first block, and the controller then retimes the notes for it, so
    // the rate here doesn't matter.
    let (mut controller, processor) =
        uta_engine::engine(EngineConfig::default(), Snapshot::from(project));
    let output = LiveOutput::start(processor, buffer)
        .map_err(|error| format!("couldn't start playback: {error}"))?;
    controller
        .locate(from.ticks())
        .expect("fresh queue has room");
    controller.play().expect("fresh queue has room");
    let from = if from.ticks() > 0 {
        format!(" from {from}")
    } else {
        String::new()
    };
    println!(
        "Playing {}{from}. Press Ctrl-C to stop.",
        describe_project(project)
    );

    let mut last: Option<DeviceStatus> = None;
    // Whether the engine has reported playing yet, so it stopping means it
    // reached the end of the song.
    let mut started = false;
    while !interrupted.load(Ordering::Relaxed) {
        let status = output.status();
        let engine = controller.poll();
        if engine.playing {
            started = true;
        } else if started {
            println!("\r\x1b[2KReached the end of the song.");
            break;
        }
        if last
            .as_ref()
            .is_none_or(|last| describe(last, buffer) != describe(&status, buffer))
        {
            println!("\r\x1b[2K{}", describe(&status, buffer));
        }
        print!("\r{}", progress(&status, &engine));
        let _ = std::io::stdout().flush();
        last = Some(status);
        std::thread::sleep(STATUS_INTERVAL);
    }

    // Release the notes; closing the output fades it out first, so stopping
    // doesn't click.
    let _ = controller.stop();
    let dropouts = output.status().dropouts;
    output.stop();
    println!("\nStopped. {dropouts} dropouts.");
    Ok(())
}

/// Refuses a start past the song's end unless the loop will catch it, since
/// playback would stop before a single sample.
fn check_from(project: &Project, from: Position) -> Result<(), String> {
    let transport = project.transport();
    let loop_end = transport.loop_start() + transport.loop_length();
    let into_the_loop = transport.loop_enabled() && from.ticks() < loop_end;
    if !into_the_loop && from.ticks() >= project.song_end() {
        let bars = project
            .song_end()
            .div_ceil(transport.time_signature().ticks_per_bar());
        return Err(format!(
            "--from {from} is past the end of the song, which is {bars} bars long"
        ));
    }
    Ok(())
}

/// A line about the device, printed whenever it changes.
fn describe(status: &DeviceStatus, requested_buffer: u32) -> String {
    let device = |status: &DeviceStatus| {
        status
            .device
            .as_ref()
            .map_or("the last device".to_owned(), |d| d.name.clone())
    };
    match status.state {
        DeviceState::Running => {
            let rate = status.device.as_ref().map_or(0, |d| d.sample_rate);
            let mut line = format!(
                "Output: {} at {rate} Hz, buffer {}",
                device(status),
                status.buffer_size
            );
            if status.buffer_size != requested_buffer {
                line.push_str(&format!(" ({requested_buffer} isn't supported)"));
            }
            line
        }
        DeviceState::Waiting => {
            let mut line = format!(
                "No output device. Checking every {:.1} s.",
                live::DEVICE_POLL_INTERVAL.as_secs_f64()
            );
            if let Some(error) = &status.last_error {
                line.push_str(&format!(" Last error: {error}"));
            }
            line
        }
        DeviceState::Failed => format!("Playback on {} failed; restart uta.", device(status)),
    }
}

/// The running status line: the playhead in bars and beats, dropouts, and
/// any note events skipped for being too many in one block.
fn progress(status: &DeviceStatus, engine: &Status) -> String {
    let ticks_per_bar = TimeSignature::FOUR_FOUR.ticks_per_bar();
    let bar = engine.playhead / ticks_per_bar + 1;
    let beat = engine.playhead % ticks_per_bar / TICKS_PER_QUARTER + 1;
    let mut line = format!("  bar {bar:3} beat {beat}  dropouts: {}", status.dropouts);
    if engine.dropped_note_events > 0 {
        line.push_str(&format!(
            "  skipped note events: {}",
            engine.dropped_note_events
        ));
    }
    line.push_str("  ");
    line
}

/// A place in the song, as bars and beats count it: both from 1, in 4/4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position {
    bar: u32,
    beat: u32,
}

impl Position {
    fn ticks(self) -> Ticks {
        let signature = TimeSignature::FOUR_FOUR;
        Ticks::from(self.bar - 1) * signature.ticks_per_bar()
            + Ticks::from(self.beat - 1) * TICKS_PER_QUARTER
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bar {}", self.bar)?;
        if self.beat > 1 {
            write!(f, " beat {}", self.beat)?;
        }
        Ok(())
    }
}

/// Parses a bar (`3`) or a bar and beat (`3.2`), both from 1.
fn position(value: &str) -> Result<Position, String> {
    let (bar, beat) = value.split_once('.').unwrap_or((value, "1"));
    let bar: u32 = bar.parse().map_err(|_| format!("{bar:?} isn't a bar"))?;
    let beat: u32 = beat.parse().map_err(|_| format!("{beat:?} isn't a beat"))?;
    if bar == 0 || bar > 10_000 {
        return Err("the bar must be from 1 to 10000".into());
    }
    if !(1..=4).contains(&beat) {
        return Err("the beat must be from 1 to 4".into());
    }
    Ok(Position { bar, beat })
}

fn buffer_size(value: &str) -> Result<u32, String> {
    let sizes = live::BUFFER_SIZES;
    match value.parse::<u32>() {
        Ok(size) if sizes.contains(&size) => Ok(size),
        Ok(_) => Err(format!("must be one of {sizes:?}")),
        Err(error) => Err(error.to_string()),
    }
}

fn positive_seconds(value: &str) -> Result<f64, String> {
    match value.parse::<f64>() {
        Ok(seconds) if seconds > 0.0 && seconds <= 3600.0 => Ok(seconds),
        Ok(_) => Err("must be more than 0 and at most 3600".into()),
        Err(error) => Err(error.to_string()),
    }
}

fn version() -> String {
    format!(
        "{} (core {}, engine {})",
        env!("CARGO_PKG_VERSION"),
        uta_core::VERSION,
        uta_engine::VERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn version_names_every_crate() {
        let version = version();
        assert!(version.contains("core "));
        assert!(version.contains("engine "));
    }

    #[test]
    fn from_is_a_bar_or_a_bar_and_beat() {
        let from = |value: &str| position(value).map(Position::ticks);
        assert_eq!(from("1"), Ok(0));
        assert_eq!(from("3"), Ok(2 * 3840));
        assert_eq!(from("3.1"), Ok(2 * 3840));
        assert_eq!(from("3.3"), Ok(2 * 3840 + 2 * 960));
        for bad in ["0", "-1", "3.0", "3.5", "x", "3.x", "", "3."] {
            assert!(from(bad).is_err(), "{bad:?}");
        }
        assert_eq!(position("3.3").unwrap().to_string(), "bar 3 beat 3");
        assert_eq!(position("3").unwrap().to_string(), "bar 3");
        let Command::Play { from, .. } = Cli::try_parse_from(["uta", "play"]).unwrap().command
        else {
            panic!("play");
        };
        assert_eq!(from.ticks(), 0);
    }

    /// Past the song's end nothing would play, so it's refused, unless the
    /// loop is on and the start is before its end.
    #[test]
    fn from_past_the_songs_end_is_refused() {
        // A 4-bar clip and loop: the song is 5 bars.
        let mut project = Project::new();
        let at = |value: &str| position(value).unwrap();
        assert!(check_from(&project, at("5.4")).is_ok());
        assert!(
            check_from(&project, at("6")).is_err(),
            "the loop ends first"
        );
        project
            .apply(&uta_core::Command::SetLoop {
                start_bar: 8,
                bars: 2,
            })
            .unwrap();
        assert!(check_from(&project, at("6")).is_ok(), "plays into the loop");
        assert!(check_from(&project, at("10")).is_ok());
        assert!(check_from(&project, at("11")).is_err());
        project
            .apply(&uta_core::Command::SetLoopEnabled { enabled: false })
            .unwrap();
        assert_eq!(
            check_from(&project, at("50")),
            Err("--from bar 50 is past the end of the song, which is 5 bars long".into())
        );
    }

    #[test]
    fn no_loop_switches_the_loop_off() {
        assert!(load(None, false).unwrap().transport().loop_enabled());
        assert!(!load(None, true).unwrap().transport().loop_enabled());
    }

    #[test]
    fn buffer_must_be_32_64_or_128() {
        assert_eq!(buffer_size("32"), Ok(32));
        assert_eq!(buffer_size("64"), Ok(64));
        assert_eq!(buffer_size("128"), Ok(128));
        assert!(buffer_size("256").is_err());
        assert!(buffer_size("0").is_err());
        assert!(buffer_size("abc").is_err());
    }

    fn running_on(buffer_size: u32) -> DeviceStatus {
        DeviceStatus {
            state: DeviceState::Running,
            device: Some(live::DeviceInfo {
                name: "Speakers".into(),
                sample_rate: 48_000,
                channels: 2,
                buffer_range: Some((64, 4096)),
            }),
            buffer_size,
            dropouts: 2,
            rebuilds: 0,
            last_error: None,
        }
    }

    #[test]
    fn status_names_the_device_buffer_and_dropouts() {
        let status = running_on(128);
        assert_eq!(
            describe(&status, 128),
            "Output: Speakers at 48000 Hz, buffer 128"
        );
        let engine = Status {
            // Bar 2, beat 3.
            playhead: 3840 + 2 * 960 + 100,
            ..Status::default()
        };
        assert_eq!(
            progress(&status, &engine).trim(),
            "bar   2 beat 3  dropouts: 2"
        );
        let engine = Status {
            dropped_note_events: 7,
            ..engine
        };
        assert!(progress(&status, &engine).ends_with("skipped note events: 7  "));
    }

    #[test]
    fn status_says_when_the_buffer_fell_back() {
        assert_eq!(
            describe(&running_on(64), 32),
            "Output: Speakers at 48000 Hz, buffer 64 (32 isn't supported)"
        );
    }

    #[test]
    fn status_says_when_there_is_no_device() {
        let status = DeviceStatus {
            state: DeviceState::Waiting,
            ..running_on(128)
        };
        assert_eq!(
            describe(&status, 128),
            "No output device. Checking every 1.5 s."
        );
    }

    #[test]
    fn the_demo_loop_builds() {
        let json = include_str!("../../../examples/demo-loop.json");
        let project = build(json).unwrap();
        assert_eq!(
            describe_project(&project),
            "28 notes on 2 tracks in a 2-bar loop at 112 BPM"
        );
        // Twice round the loop, which starts at the top: 4 bars of 4/4 at
        // 112 BPM.
        assert!((default_seconds(&project) - 16.0 * 60.0 / 112.0).abs() < 1e-4);
    }

    /// With the loop on, a render goes from the top into the loop and twice
    /// round it; with it off, to the end of the song.
    #[test]
    fn the_default_render_length_follows_the_loop_switch() {
        let mut project = Project::new();
        let seconds_per_bar = 2.0;
        project
            .apply(&uta_core::Command::SetLoop {
                start_bar: 3,
                bars: 2,
            })
            .unwrap();
        // Bars 1 to 3, then bars 4 and 5 twice.
        assert!((default_seconds(&project) - 7.0 * seconds_per_bar).abs() < 1e-4);
        project
            .apply(&uta_core::Command::SetLoopEnabled { enabled: false })
            .unwrap();
        // The 4-bar clip, then a bar.
        assert!((default_seconds(&project) - 5.0 * seconds_per_bar).abs() < 1e-4);
        assert_eq!(
            describe_project(&project),
            "0 notes on 2 tracks in a 5-bar song at 120 BPM"
        );
    }

    #[test]
    fn the_demo_song_builds() {
        let json = include_str!("../../../examples/demo-song.json");
        let project = build(json).unwrap();
        assert_eq!(
            describe_project(&project),
            "60 notes on 4 tracks in a 4-bar loop at 112 BPM"
        );
    }

    /// Every listening render builds, and `render-all` finds them all.
    #[test]
    fn the_listening_lists_build() {
        for sound in ["kick", "snare", "clap", "hats"] {
            let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/listen")
                .join(sound);
            let lists = command_lists(&folder).unwrap();
            assert!(lists.len() >= 8, "{sound}: {lists:?}");
            for list in lists {
                let project = load(Some(&list), false).unwrap();
                assert!(default_seconds(&project) > 1.0, "{}", list.display());
            }
        }
    }

    #[test]
    fn render_all_needs_a_folder_of_lists() {
        let empty = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let error = render_all(&empty, Path::new("unused")).unwrap_err();
        assert!(error.starts_with("no .json command lists"), "{error}");
        assert!(render_all(Path::new("no/such/folder"), Path::new("unused")).is_err());
    }

    #[test]
    fn bad_command_lists_say_what_went_wrong() {
        assert!(build("[]").unwrap_err().starts_with("isn't a command list"));
        let id = uta_core::ProjectId::random();
        let json = format!(
            r#"{{"project":"{id}","commands":[{{"format":2,"command":{{"type":"set_tempo","bpm":1000.0}}}}]}}"#
        );
        assert!(build(&json).unwrap_err().starts_with("command 1 failed"));
    }

    #[test]
    fn seconds_must_be_positive() {
        assert_eq!(positive_seconds("2.5"), Ok(2.5));
        assert!(positive_seconds("0").is_err());
        assert!(positive_seconds("-1").is_err());
        assert!(positive_seconds("abc").is_err());
    }
}
