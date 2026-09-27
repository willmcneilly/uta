//! The `uta` command. `render` writes the tone to a WAV without a sound
//! device; `play` plays it through the default output until Ctrl-C.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use clap::{Parser, Subcommand};
use uta_core::Project;
use uta_engine::live::{self, DeviceState, DeviceStatus, LiveOutput};
use uta_engine::{EngineConfig, Snapshot, offline};

#[derive(Parser)]
#[command(name = "uta", version = version(), about = "Uta, from the terminal")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render the tone to a WAV file, offline (no sound device needed).
    Render {
        /// Where to write the WAV.
        file: PathBuf,
        /// How long to render, in seconds.
        #[arg(long, default_value_t = 3.0, value_parser = positive_seconds)]
        seconds: f64,
    },
    /// Play the tone through the default output until Ctrl-C. Follows the
    /// output when you switch, unplug or replug it.
    Play {
        /// The buffer size in frames: 32, 64 or 128. If the device can't do
        /// it, it gets the nearest size it can.
        #[arg(long, default_value_t = live::DEFAULT_BUFFER_SIZE, value_parser = buffer_size)]
        buffer: u32,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Render { file, seconds } => render(&file, seconds),
        Command::Play { buffer } => play(buffer),
    }
}

fn render(file: &std::path::Path, seconds: f64) -> ExitCode {
    let config = EngineConfig::default();
    let samples = offline::render_tone(config, Snapshot::from(&Project::new()), seconds);
    match offline::write_wav(file, config, &samples) {
        Ok(()) => {
            println!(
                "Wrote {seconds} s of 440 Hz at {} Hz to {}",
                config.sample_rate,
                file.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("uta: couldn't write {}: {error}", file.display());
            ExitCode::FAILURE
        }
    }
}

/// How often the status line updates.
const STATUS_INTERVAL: Duration = Duration::from_millis(100);

fn play(buffer: u32) -> ExitCode {
    let interrupted = Arc::new(AtomicBool::new(false));
    if let Err(error) = ctrlc::set_handler({
        let interrupted = interrupted.clone();
        move || interrupted.store(true, Ordering::Relaxed)
    }) {
        eprintln!("uta: couldn't catch Ctrl-C: {error}");
        return ExitCode::FAILURE;
    }

    // The supervisor moves the processor to the device's own rate before
    // the first block, so the rate here doesn't matter.
    let (mut controller, processor) =
        uta_engine::engine(EngineConfig::default(), Snapshot::from(&Project::new()));
    let output = match LiveOutput::start(processor, buffer) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("uta: couldn't start playback: {error}");
            return ExitCode::FAILURE;
        }
    };
    controller.play().expect("fresh queue has room");
    println!("Playing 440 Hz. Press Ctrl-C to stop.");

    let mut last: Option<DeviceStatus> = None;
    while !interrupted.load(Ordering::Relaxed) {
        let status = output.status();
        let position = controller.poll().position;
        if last
            .as_ref()
            .is_none_or(|last| describe(last, buffer) != describe(&status, buffer))
        {
            println!("\r\x1b[2K{}", describe(&status, buffer));
        }
        print!("\r{}", progress(&status, position));
        let _ = std::io::stdout().flush();
        last = Some(status);
        std::thread::sleep(STATUS_INTERVAL);
    }

    // Fade out before closing the stream, so stopping doesn't click.
    let _ = controller.stop();
    std::thread::sleep(Duration::from_millis(50));
    let dropouts = output.status().dropouts;
    output.stop();
    println!("\nStopped. {dropouts} dropouts.");
    ExitCode::SUCCESS
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

/// The running status line: time played and dropouts.
fn progress(status: &DeviceStatus, position: u64) -> String {
    let rate = status.device.as_ref().map_or(0, |d| d.sample_rate);
    let seconds = if rate > 0 {
        position as f64 / f64::from(rate)
    } else {
        0.0
    };
    format!("  {seconds:8.1} s  dropouts: {}  ", status.dropouts)
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
        assert_eq!(progress(&status, 96_000).trim(), "2.0 s  dropouts: 2");
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
    fn seconds_must_be_positive() {
        assert_eq!(positive_seconds("2.5"), Ok(2.5));
        assert!(positive_seconds("0").is_err());
        assert!(positive_seconds("-1").is_err());
        assert!(positive_seconds("abc").is_err());
    }
}
