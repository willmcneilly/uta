//! The `uta` command. `render` writes a project's loop to a WAV without a
//! sound device; `play` loops it through the default output until Ctrl-C.
//! Both build the project from a command list (`--commands`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use clap::{Parser, Subcommand};
use uta_core::time::{TICKS_PER_QUARTER, TimeSignature};
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
    /// Render the loop to a WAV file, offline (no sound device needed). It
    /// plays from the loop's start, then stops and lets the notes ring out.
    Render {
        /// Where to write the WAV.
        file: PathBuf,
        /// A JSON command list to build the project from, such as
        /// `examples/demo-loop.json`. Without one, the project is empty and
        /// the render is silent.
        #[arg(long)]
        commands: Option<PathBuf>,
        /// How long to play, in seconds. Twice round the loop if not given.
        #[arg(long, value_parser = positive_seconds)]
        seconds: Option<f64>,
    },
    /// Loop the project through the default output until Ctrl-C. Follows the
    /// output when you switch, unplug or replug it.
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
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Render {
            file,
            commands,
            seconds,
        } => load(commands.as_deref()).and_then(|project| render(&project, &file, seconds)),
        Command::Play { commands, buffer } => {
            load(commands.as_deref()).and_then(|project| play(&project, buffer))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("uta: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The project a command list builds, or a new, empty one.
fn load(commands: Option<&Path>) -> Result<Project, String> {
    let Some(path) = commands else {
        return Ok(Project::new());
    };
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("couldn't read {}: {error}", path.display()))?;
    build(&json).map_err(|error| format!("{}: {error}", path.display()))
}

/// Builds the project a JSON command list describes.
fn build(json: &str) -> Result<Project, String> {
    let list: CommandList =
        serde_json::from_str(json).map_err(|error| format!("isn't a command list: {error}"))?;
    list.build()
        .map_err(|(index, error)| format!("command {} failed: {error}", index + 1))
}

/// The loop's length in seconds.
fn loop_seconds(project: &Project) -> f64 {
    let transport = project.transport();
    let rate = EngineConfig::default().sample_rate;
    let samples = transport
        .tempo_map()
        .ticks_to_samples(transport.loop_start() + transport.loop_length(), rate)
        - transport
            .tempo_map()
            .ticks_to_samples(transport.loop_start(), rate);
    samples as f64 / f64::from(rate)
}

fn render(project: &Project, file: &Path, seconds: Option<f64>) -> Result<(), String> {
    let config = EngineConfig::default();
    let seconds = seconds.unwrap_or_else(|| 2.0 * loop_seconds(project));
    let samples = offline::render_loop(config, Snapshot::from(project), seconds);
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

/// A one-line summary: the notes, tempo and loop.
fn describe_project(project: &Project) -> String {
    let notes: usize = project
        .tracks()
        .iter()
        .flat_map(|track| track.clips())
        .map(|clip| clip.notes().len())
        .sum();
    let transport = project.transport();
    let bars = transport.loop_length() / transport.time_signature().ticks_per_bar();
    format!(
        "{notes} notes in a {bars}-bar loop at {} BPM",
        transport.tempo_map().bpm()
    )
}

/// How often the status line updates.
const STATUS_INTERVAL: Duration = Duration::from_millis(100);

fn play(project: &Project, buffer: u32) -> Result<(), String> {
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
    controller.play().expect("fresh queue has room");
    println!(
        "Playing {}. Press Ctrl-C to stop.",
        describe_project(project)
    );

    let mut last: Option<DeviceStatus> = None;
    while !interrupted.load(Ordering::Relaxed) {
        let status = output.status();
        let engine = controller.poll();
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
            "28 notes in a 2-bar loop at 112 BPM"
        );
        // 2 bars of 4/4 at 112 BPM.
        assert!((loop_seconds(&project) - 8.0 * 60.0 / 112.0).abs() < 1e-4);
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
