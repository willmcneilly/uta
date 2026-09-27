//! Commands: each one is a single, saveable change to a project.

use serde::{Deserialize, Serialize};

/// The command format written by this version of Uta. Bump it when a saved
/// command's shape changes, and teach [`Command`]'s deserialisation to read
/// the older formats.
pub const COMMAND_FORMAT: u32 = 1;

/// One change to a project.
///
/// Commands serialise with their format version, as
/// `{"format":1,"command":{"type":"set_master_volume","volume_db":-6.0}}`.
/// They refer to things by permanent IDs, so replaying the same commands
/// always rebuilds the same project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "wire::Envelope", try_from = "wire::Envelope")]
pub enum Command {
    /// Set the project's master volume, in dB.
    SetMasterVolume { volume_db: f32 },
}

/// Why a command couldn't be applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandError {
    /// The volume was outside the project's limits, or not a number.
    VolumeOutOfRange(f32),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VolumeOutOfRange(volume_db) => {
                write!(f, "volume {volume_db} dB is out of range")
            }
        }
    }
}

impl std::error::Error for CommandError {}

/// The saved form of a command. It's kept apart from [`Command`] so the
/// in-memory type can change while older saved formats still load: a new
/// format gets its own body type here and a conversion to [`Command`].
mod wire {
    use serde::{Deserialize, Serialize};

    use super::{COMMAND_FORMAT, Command};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct Envelope {
        format: u32,
        command: Body,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
    enum Body {
        SetMasterVolume { volume_db: f32 },
    }

    impl From<Command> for Envelope {
        fn from(command: Command) -> Self {
            let command = match command {
                Command::SetMasterVolume { volume_db } => Body::SetMasterVolume { volume_db },
            };
            Self {
                format: COMMAND_FORMAT,
                command,
            }
        }
    }

    impl TryFrom<Envelope> for Command {
        type Error = String;

        fn try_from(envelope: Envelope) -> Result<Self, Self::Error> {
            if envelope.format != COMMAND_FORMAT {
                return Err(format!(
                    "command format {} isn't supported (this version reads format {COMMAND_FORMAT})",
                    envelope.format
                ));
            }
            Ok(match envelope.command {
                Body::SetMasterVolume { volume_db } => Command::SetMasterVolume { volume_db },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn serialises_with_its_format_version() {
        let json = serde_json::to_string(&Command::SetMasterVolume { volume_db: -6.0 }).unwrap();
        assert_eq!(
            json,
            r#"{"format":1,"command":{"type":"set_master_volume","volume_db":-6.0}}"#
        );
    }

    #[test]
    fn reads_the_saved_format() {
        let json = r#"{"format":1,"command":{"type":"set_master_volume","volume_db":-6.0}}"#;
        let command: Command = serde_json::from_str(json).unwrap();
        assert_eq!(command, Command::SetMasterVolume { volume_db: -6.0 });
    }

    #[test]
    fn rejects_an_unknown_format() {
        let json = r#"{"format":99,"command":{"type":"set_master_volume","volume_db":-6.0}}"#;
        let error = serde_json::from_str::<Command>(json).unwrap_err();
        assert!(error.to_string().contains("format 99"), "{error}");
    }

    #[test]
    fn rejects_a_missing_format() {
        let json = r#"{"command":{"type":"set_master_volume","volume_db":-6.0}}"#;
        assert!(serde_json::from_str::<Command>(json).is_err());
    }

    #[test]
    fn rejects_an_unknown_command() {
        let json = r#"{"format":1,"command":{"type":"launch_rocket"}}"#;
        assert!(serde_json::from_str::<Command>(json).is_err());
    }

    proptest! {
        #[test]
        fn commands_round_trip_through_serialisation(
            volume_db in any::<f32>().prop_filter("JSON has no NaN or infinity", |v| v.is_finite())
        ) {
            let command = Command::SetMasterVolume { volume_db };
            let json = serde_json::to_string(&command).unwrap();
            let back: Command = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, command);
        }
    }
}
