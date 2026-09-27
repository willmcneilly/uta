//! A project with its undo history.

use crate::{Command, CommandError, Project};

/// A change that was made to the project: a new command, an undo or a redo.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// Counts up by one with every change, starting at 1.
    pub sequence: u64,
    /// The command that was applied. For an undo, that's the inverse.
    ///
    /// Replaying every `Applied::command` in sequence order, starting from the
    /// session's first project, rebuilds the current project. That's what a
    /// command journal will save.
    pub command: Command,
}

/// A command and its inverse, as kept on the undo and redo stacks.
#[derive(Debug, Clone)]
struct Entry {
    command: Command,
    inverse: Command,
}

/// Owns a project, applies commands to it and keeps the undo history.
#[derive(Debug, Clone)]
pub struct Session {
    project: Project,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    last_sequence: u64,
}

impl Session {
    /// A session with no history, starting from `project`.
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            last_sequence: 0,
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// The sequence number of the latest change, or 0 before any.
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Applies a new command. It can be undone, and it clears the redo
    /// history. If it's invalid, nothing changes and no sequence number is
    /// used.
    pub fn apply(&mut self, command: Command) -> Result<Applied, CommandError> {
        let inverse = self.project.apply(&command)?;
        self.redo.clear();
        self.undo.push(Entry {
            command: command.clone(),
            inverse,
        });
        Ok(self.record(command))
    }

    /// Applies a command that continues the latest one, such as the next step
    /// of a volume drag, so a single undo reverts the whole run. If the
    /// latest command sets something else, or there's nothing to undo, it's
    /// the same as [`Self::apply`]. The change still gets its own sequence
    /// number, so replaying the journal is unaffected.
    pub fn amend(&mut self, command: Command) -> Result<Applied, CommandError> {
        let continues = self
            .undo
            .last()
            .is_some_and(|entry| entry.command.sets_same_as(&command));
        if !continues {
            return self.apply(command);
        }
        self.project.apply(&command)?;
        self.redo.clear();
        let entry = self.undo.last_mut().expect("checked above");
        entry.command = command.clone();
        Ok(self.record(command))
    }

    /// Undoes the latest command. Returns `None` if there's nothing to undo.
    pub fn undo(&mut self) -> Option<Applied> {
        let entry = self.undo.pop()?;
        let command = self.apply_recorded(&entry.inverse);
        self.redo.push(entry);
        Some(self.record(command))
    }

    /// Redoes the latest undone command. Returns `None` if there's nothing to
    /// redo.
    pub fn redo(&mut self) -> Option<Applied> {
        let entry = self.redo.pop()?;
        let command = self.apply_recorded(&entry.command);
        self.undo.push(entry);
        Some(self.record(command))
    }

    /// Applies a command from the history. The history only holds commands
    /// that applied to exactly this state before, so they always apply again.
    fn apply_recorded(&mut self, command: &Command) -> Command {
        self.project
            .apply(command)
            .expect("a command from the history applies to the state it was recorded against");
        command.clone()
    }

    fn record(&mut self, command: Command) -> Applied {
        self.last_sequence += 1;
        Applied {
            sequence: self.last_sequence,
            command,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn volume(volume_db: f32) -> Command {
        Command::SetMasterVolume { volume_db }
    }

    #[test]
    fn apply_then_undo_gives_the_exact_previous_state() {
        let mut session = Session::new(Project::new());
        let before = session.project().clone();
        session.apply(volume(-3.0)).unwrap();
        session.undo().unwrap();
        assert_eq!(session.project(), &before);
    }

    #[test]
    fn undo_and_redo_step_through_the_history() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        session.apply(volume(0.0)).unwrap();

        session.undo().unwrap();
        assert_eq!(session.project().master_volume_db(), -6.0);
        session.undo().unwrap();
        assert_eq!(session.project().master_volume_db(), -12.0);
        assert!(session.undo().is_none());

        session.redo().unwrap();
        assert_eq!(session.project().master_volume_db(), -6.0);
        session.redo().unwrap();
        assert_eq!(session.project().master_volume_db(), 0.0);
        assert!(session.redo().is_none());
    }

    #[test]
    fn amending_makes_one_undo_step_of_a_run() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        for volume_db in [-7.0, -8.0, -9.0] {
            session.amend(volume(volume_db)).unwrap();
        }
        assert_eq!(session.project().master_volume_db(), -9.0);

        assert_eq!(session.undo().unwrap().command, volume(-12.0));
        assert!(!session.can_undo());
        assert_eq!(session.redo().unwrap().command, volume(-9.0));
    }

    #[test]
    fn amending_with_nothing_to_undo_applies() {
        let mut session = Session::new(Project::new());
        session.amend(volume(-6.0)).unwrap();
        assert_eq!(session.undo().unwrap().command, volume(-12.0));
    }

    #[test]
    fn an_invalid_amend_changes_nothing() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        assert!(session.amend(volume(f32::NAN)).is_err());
        assert_eq!(session.project().master_volume_db(), -6.0);
        assert_eq!(session.last_sequence(), 1);
        assert_eq!(session.undo().unwrap().command, volume(-12.0));
        assert_eq!(session.redo().unwrap().command, volume(-6.0));
    }

    #[test]
    fn a_new_command_clears_redo() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        session.undo().unwrap();
        assert!(session.can_redo());
        session.apply(volume(-1.0)).unwrap();
        assert!(!session.can_redo());
        assert!(session.redo().is_none());
    }

    #[test]
    fn every_change_gets_the_next_sequence_number() {
        let mut session = Session::new(Project::new());
        assert_eq!(session.last_sequence(), 0);
        assert_eq!(session.apply(volume(-6.0)).unwrap().sequence, 1);
        assert_eq!(session.undo().unwrap().sequence, 2);
        assert_eq!(session.redo().unwrap().sequence, 3);
        assert_eq!(session.last_sequence(), 3);
    }

    #[test]
    fn undo_reports_the_inverse_it_applied() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        assert_eq!(session.undo().unwrap().command, volume(-12.0));
        assert_eq!(session.redo().unwrap().command, volume(-6.0));
    }

    #[test]
    fn an_invalid_command_changes_nothing() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        session.undo().unwrap();
        let before = session.project().clone();

        assert!(session.apply(volume(f32::NAN)).is_err());
        assert_eq!(session.project(), &before);
        assert_eq!(session.last_sequence(), 2);
        assert!(session.can_redo(), "a rejected command must not clear redo");
    }

    /// One step a user might take.
    #[derive(Debug, Clone)]
    enum Step {
        Apply(Command),
        Amend(Command),
        Undo,
        Redo,
    }

    fn step() -> impl Strategy<Value = Step> {
        // Mostly valid volumes, with some that must be rejected.
        let volume_db = prop_oneof![
            8 => Project::MIN_VOLUME_DB..=Project::MAX_VOLUME_DB,
            1 => prop_oneof![
                Just(f32::NAN),
                Just(f32::INFINITY),
                Just(f32::NEG_INFINITY),
                7.0f32..1000.0,
                -1000.0f32..-121.0,
            ],
        ];
        prop_oneof![
            4 => volume_db.clone().prop_map(|volume_db| Step::Apply(volume(volume_db))),
            2 => volume_db.prop_map(|volume_db| Step::Amend(volume(volume_db))),
            2 => Just(Step::Undo),
            1 => Just(Step::Redo),
        ]
    }

    /// Runs `steps` and returns the session and the journal of changes.
    fn run(steps: &[Step]) -> (Project, Session, Vec<Applied>) {
        let start = Project::new();
        let mut session = Session::new(start.clone());
        let mut journal = Vec::new();
        for step in steps {
            let applied = match step {
                Step::Apply(command) => session.apply(command.clone()).ok(),
                Step::Amend(command) => session.amend(command.clone()).ok(),
                Step::Undo => session.undo(),
                Step::Redo => session.redo(),
            };
            journal.extend(applied);
        }
        (start, session, journal)
    }

    proptest! {
        #[test]
        fn replaying_the_journal_always_gives_the_same_project(
            steps in prop::collection::vec(step(), 0..200)
        ) {
            let (start, session, journal) = run(&steps);

            // Through serialisation, as a saved journal would go.
            let json = serde_json::to_string(
                &journal.iter().map(|a| &a.command).collect::<Vec<_>>()
            ).unwrap();
            let saved: Vec<Command> = serde_json::from_str(&json).unwrap();

            let replayed = start.clone().replay(&saved).unwrap();
            prop_assert_eq!(&replayed, session.project());
            // And again: replay is deterministic.
            prop_assert_eq!(start.replay(&saved).unwrap(), replayed);
        }

        #[test]
        fn sequence_numbers_count_up_by_one(
            steps in prop::collection::vec(step(), 0..200)
        ) {
            let (_, session, journal) = run(&steps);
            for (index, applied) in journal.iter().enumerate() {
                prop_assert_eq!(applied.sequence, index as u64 + 1);
            }
            prop_assert_eq!(session.last_sequence(), journal.len() as u64);
        }

        #[test]
        fn undoing_everything_gives_the_first_project_and_redo_the_last(
            steps in prop::collection::vec(step(), 0..200)
        ) {
            let (start, mut session, _) = run(&steps);
            // The latest state in the history, past any pending redos.
            while session.redo().is_some() {}
            let end = session.project().clone();
            while session.undo().is_some() {}
            prop_assert_eq!(session.project(), &start);
            while session.redo().is_some() {}
            prop_assert_eq!(session.project(), &end);
        }

        #[test]
        fn apply_then_undo_restores_any_state(
            steps in prop::collection::vec(step(), 0..50),
            volume_db in Project::MIN_VOLUME_DB..=Project::MAX_VOLUME_DB,
        ) {
            let (_, mut session, _) = run(&steps);
            let before = session.project().clone();
            session.apply(volume(volume_db)).unwrap();
            session.undo().unwrap();
            prop_assert_eq!(session.project(), &before);
        }
    }
}
