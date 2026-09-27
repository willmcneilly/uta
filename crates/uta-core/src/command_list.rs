//! A saved list of commands, and the project it builds.

use serde::{Deserialize, Serialize};

use crate::{Command, CommandError, Project, ProjectId};

/// Commands to replay onto a new project, as `uta render --commands` reads
/// them:
///
/// ```json
/// {"project": "<uuid>", "commands": [{"format": 2, "command": {...}}, ...]}
/// ```
///
/// Commands find the project's track and clip by IDs worked out from the
/// project's ID (see [`Project::with_id`]), so the list names the project it
/// was written for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandList {
    pub project: ProjectId,
    pub commands: Vec<Command>,
}

impl CommandList {
    /// A new project with the list's ID, with every command applied in
    /// order. Fails at the first invalid command, saying which one it was.
    pub fn build(&self) -> Result<Project, (usize, CommandError)> {
        let mut project = Project::with_id(self.project);
        for (index, command) in self.commands.iter().enumerate() {
            project.apply(command).map_err(|error| (index, error))?;
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::note;

    #[test]
    fn builds_the_project_the_commands_describe() {
        let id = ProjectId::random();
        let clip = Project::with_id(id).tracks()[0].clips()[0].id();
        let list = CommandList {
            project: id,
            commands: vec![
                Command::SetTempo { bpm: 90.0 },
                Command::AddNotes {
                    clip,
                    notes: vec![note(0, 60, 0)],
                },
            ],
        };
        let project = list.build().unwrap();
        assert_eq!(project.id(), id);
        assert_eq!(project.transport().tempo_map().bpm(), 90.0);
        assert_eq!(project.clip(clip).unwrap().notes().len(), 1);
    }

    #[test]
    fn says_which_command_failed() {
        let list = CommandList {
            project: ProjectId::random(),
            commands: vec![
                Command::SetTempo { bpm: 90.0 },
                Command::SetTempo { bpm: 1000.0 },
            ],
        };
        assert!(matches!(list.build(), Err((1, _))));
    }

    #[test]
    fn round_trips_through_json() {
        let json = r#"{"project":"6f1c2c1e-8a47-4a8e-9d57-3f2b2f0c9a10","commands":[{"format":2,"command":{"type":"set_tempo","bpm":100.0}}]}"#;
        let list: CommandList = serde_json::from_str(json).unwrap();
        assert_eq!(list.commands, [Command::SetTempo { bpm: 100.0 }]);
        assert_eq!(serde_json::to_string(&list).unwrap(), json);
    }
}
