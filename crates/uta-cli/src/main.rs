//! The `uta` command. `render` writes the tone to a WAV without a sound
//! device; `play` arrives with live playback.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Render { file, seconds } => render(&file, seconds),
    }
}

fn render(file: &std::path::Path, seconds: f64) -> ExitCode {
    let config = EngineConfig::default();
    let samples = offline::render_tone(config, Snapshot::default(), seconds);
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
    fn seconds_must_be_positive() {
        assert_eq!(positive_seconds("2.5"), Ok(2.5));
        assert!(positive_seconds("0").is_err());
        assert!(positive_seconds("-1").is_err());
        assert!(positive_seconds("abc").is_err());
    }
}
