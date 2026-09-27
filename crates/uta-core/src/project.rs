//! The project: everything Uta saves about a piece of music.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Command, CommandError};

/// A project's permanent ID. It never changes, so saved commands and files can
/// always find the project they belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(Uuid);

impl ProjectId {
    /// A new, random ID.
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

/// A project. For milestone 0 that's just the master volume.
///
/// Its data only changes through [`Command`]s, so every change can be undone
/// and replayed.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    id: ProjectId,
    master_volume_db: f32,
}

impl Project {
    /// A new project's master volume, in dB.
    pub const DEFAULT_MASTER_VOLUME_DB: f32 = -12.0;
    /// The quietest master volume, in dB. The engine treats it as silence.
    pub const MIN_VOLUME_DB: f32 = -120.0;
    /// The loudest master volume, in dB.
    pub const MAX_VOLUME_DB: f32 = 6.0;

    /// A new project with a random ID.
    pub fn new() -> Self {
        Self::with_id(ProjectId::random())
    }

    /// A new project with the given ID.
    pub fn with_id(id: ProjectId) -> Self {
        Self {
            id,
            master_volume_db: Self::DEFAULT_MASTER_VOLUME_DB,
        }
    }

    pub fn id(&self) -> ProjectId {
        self.id
    }

    /// The master volume, in dB.
    pub fn master_volume_db(&self) -> f32 {
        self.master_volume_db
    }

    /// Applies `command` and returns its inverse: the command that puts the
    /// project back exactly as it was. If the command is invalid, the project
    /// is left unchanged.
    pub fn apply(&mut self, command: &Command) -> Result<Command, CommandError> {
        match *command {
            Command::SetMasterVolume { volume_db } => {
                if !(Self::MIN_VOLUME_DB..=Self::MAX_VOLUME_DB).contains(&volume_db) {
                    return Err(CommandError::VolumeOutOfRange(volume_db));
                }
                let previous = std::mem::replace(&mut self.master_volume_db, volume_db);
                Ok(Command::SetMasterVolume {
                    volume_db: previous,
                })
            }
        }
    }

    /// Applies `commands` in order, stopping at the first invalid one.
    pub fn replay<'a>(
        mut self,
        commands: impl IntoIterator<Item = &'a Command>,
    ) -> Result<Self, CommandError> {
        for command in commands {
            self.apply(command)?;
        }
        Ok(self)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_has_default_volume() {
        let project = Project::new();
        assert_eq!(
            project.master_volume_db(),
            Project::DEFAULT_MASTER_VOLUME_DB
        );
    }

    #[test]
    fn new_projects_get_different_ids() {
        assert_ne!(Project::new().id(), Project::new().id());
    }

    #[test]
    fn apply_returns_the_inverse() {
        let mut project = Project::new();
        let inverse = project
            .apply(&Command::SetMasterVolume { volume_db: -6.0 })
            .unwrap();
        assert_eq!(project.master_volume_db(), -6.0);
        assert_eq!(inverse, Command::SetMasterVolume { volume_db: -12.0 });
    }

    #[test]
    fn apply_then_inverse_gives_the_exact_previous_state() {
        let mut project = Project::new();
        let before = project.clone();
        let inverse = project
            .apply(&Command::SetMasterVolume { volume_db: 3.5 })
            .unwrap();
        project.apply(&inverse).unwrap();
        assert_eq!(project, before);
    }

    #[test]
    fn volume_limits_are_inclusive() {
        let mut project = Project::new();
        for volume_db in [Project::MIN_VOLUME_DB, Project::MAX_VOLUME_DB] {
            project
                .apply(&Command::SetMasterVolume { volume_db })
                .unwrap();
            assert_eq!(project.master_volume_db(), volume_db);
        }
    }

    #[test]
    fn invalid_volumes_are_rejected_and_change_nothing() {
        let mut project = Project::new();
        let before = project.clone();
        for volume_db in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 6.01, -120.5] {
            let result = project.apply(&Command::SetMasterVolume { volume_db });
            assert!(
                matches!(result, Err(CommandError::VolumeOutOfRange(_))),
                "{volume_db} was accepted"
            );
            assert_eq!(project, before);
        }
    }

    #[test]
    fn replay_stops_at_the_first_invalid_command() {
        let commands = [
            Command::SetMasterVolume { volume_db: -3.0 },
            Command::SetMasterVolume { volume_db: 60.0 },
        ];
        let result = Project::new().replay(&commands);
        assert_eq!(result, Err(CommandError::VolumeOutOfRange(60.0)));
    }
}
