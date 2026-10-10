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

/// A command and its inverse.
#[derive(Debug, Clone)]
struct Entry {
    command: Command,
    inverse: Command,
}

/// One undo step, as kept on the undo and redo stacks: the commands it
/// applied, in order. Usually one; more when a change joins it (see
/// [`Session::join`]).
type Step = Vec<Entry>;

/// Owns a project, applies commands to it and keeps the undo history.
#[derive(Debug, Clone)]
pub struct Session {
    project: Project,
    undo: Vec<Step>,
    redo: Vec<Step>,
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
        self.undo.push(vec![Entry {
            command: command.clone(),
            inverse,
        }]);
        Ok(self.record(command))
    }

    /// Applies a command that continues the latest one, such as the next step
    /// of a volume drag, so a single undo reverts the whole run. If it
    /// doesn't continue the latest command (see [`Command::continued_by`]),
    /// or there's nothing to undo, it's the same as [`Self::apply`]. The
    /// change still gets its own sequence number, so replaying the journal is
    /// unaffected.
    pub fn amend(&mut self, command: Command) -> Result<Applied, CommandError> {
        let Some(continued) = self
            .undo
            .last()
            .and_then(|step| step.last())
            .and_then(|entry| entry.command.continued_by(&command))
        else {
            return self.apply(command);
        };
        self.project.apply(&command)?;
        self.redo.clear();
        let entry = self
            .undo
            .last_mut()
            .and_then(|step| step.last_mut())
            .expect("checked above");
        entry.command = continued;
        Ok(self.record(command))
    }

    /// Applies a command as part of the latest undo step, so one undo
    /// reverts both: trimming the notes an edit covers, say. With nothing to
    /// undo, it's the same as [`Self::apply`].
    pub fn join(&mut self, command: Command) -> Result<Applied, CommandError> {
        if self.undo.is_empty() {
            return self.apply(command);
        }
        let inverse = self.project.apply(&command)?;
        self.redo.clear();
        self.undo.last_mut().expect("checked above").push(Entry {
            command: command.clone(),
            inverse,
        });
        Ok(self.record(command))
    }

    /// Undoes the latest step and forgets it, as if it had never been made:
    /// it can't be redone. For cancelling a drag with Esc. Like a new change,
    /// it clears the redo history, whose commands were recorded against a
    /// state that's now gone. Returns the inverses it applied, or nothing if
    /// there's nothing to undo. They still get sequence numbers, so replaying
    /// the journal gives the same project.
    pub fn withdraw(&mut self) -> Vec<Applied> {
        let Some(step) = self.undo.pop() else {
            return Vec::new();
        };
        self.redo.clear();
        self.apply_inverses(&step)
    }

    /// Undoes the latest step. Returns the inverses it applied, in order, or
    /// nothing if there's nothing to undo.
    pub fn undo(&mut self) -> Vec<Applied> {
        let Some(step) = self.undo.pop() else {
            return Vec::new();
        };
        let applied = self.apply_inverses(&step);
        self.redo.push(step);
        applied
    }

    /// Redoes the latest undone step. Returns the commands it applied, in
    /// order, or nothing if there's nothing to redo.
    pub fn redo(&mut self) -> Vec<Applied> {
        let Some(step) = self.redo.pop() else {
            return Vec::new();
        };
        let applied = step
            .iter()
            .map(|entry| self.apply_recorded(&entry.command))
            .collect();
        self.undo.push(step);
        applied
    }

    /// Applies a step's inverses, latest first.
    fn apply_inverses(&mut self, step: &Step) -> Vec<Applied> {
        step.iter()
            .rev()
            .map(|entry| self.apply_recorded(&entry.inverse))
            .collect()
    }

    /// Applies a command from the history. The history only holds commands
    /// that applied to exactly this state before, so they always apply again.
    fn apply_recorded(&mut self, command: &Command) -> Applied {
        self.project
            .apply(command)
            .expect("a command from the history applies to the state it was recorded against");
        self.record(command.clone())
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
    use crate::testing::{self, note, note_id};
    use crate::{
        Clip, ClipId, ClipPosition, MixerStrip, Note, PlacedTrack, Source, SourceKind, SynthParam,
        SynthSettings, Track, TrackId, Waveform,
    };
    use proptest::prelude::*;

    fn volume(volume_db: f32) -> Command {
        Command::SetMasterVolume { volume_db }
    }

    fn clip_id(session: &Session) -> ClipId {
        session.project().tracks()[0].clips()[0].id()
    }

    fn track_id(session: &Session) -> TrackId {
        session.project().tracks()[0].id()
    }

    /// The one command an undo, redo or withdraw applied.
    fn only(applied: Vec<Applied>) -> Applied {
        let [applied] = <[Applied; 1]>::try_from(applied).expect("exactly one command");
        applied
    }

    #[test]
    fn a_joined_command_undoes_and_redoes_with_the_step_before_it() {
        let mut session = Session::new(testing::project());
        let clip = clip_id(&session);
        session.apply(volume(-6.0)).unwrap();
        let before = session.project().clone();
        session
            .apply(Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0), note(1, 60, 960)],
            })
            .unwrap();
        session
            .join(Command::SetNotes {
                clip,
                notes: vec![note(0, 62, 0)],
            })
            .unwrap();
        session
            .join(Command::RemoveNotes {
                clip,
                notes: vec![note_id(1)],
            })
            .unwrap();
        let after = session.project().clone();

        let undone = session.undo();
        let commands: Vec<_> = undone.iter().map(|applied| &applied.command).collect();
        assert_eq!(
            commands,
            [
                &Command::AddNotes {
                    clip,
                    notes: vec![note(1, 60, 960)],
                },
                &Command::SetNotes {
                    clip,
                    notes: vec![note(0, 60, 0)],
                },
                &Command::RemoveNotes {
                    clip,
                    notes: vec![note_id(0), note_id(1)],
                },
            ],
            "the inverses, latest first"
        );
        assert_eq!(session.project(), &before);
        assert_eq!(session.redo().len(), 3);
        assert_eq!(session.project(), &after);
        session.undo();
        assert_eq!(
            only(session.undo()).command,
            volume(-12.0),
            "the step before is its own"
        );
    }

    #[test]
    fn withdrawing_takes_back_a_joined_step_whole() {
        let mut session = Session::new(testing::project());
        let clip = clip_id(&session);
        let before = session.project().clone();
        session
            .apply(Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0)],
            })
            .unwrap();
        session.join(volume(-3.0)).unwrap();
        assert_eq!(session.withdraw().len(), 2);
        assert_eq!(session.project(), &before);
        assert!(!session.can_undo() && !session.can_redo());
    }

    #[test]
    fn joining_with_nothing_to_undo_applies() {
        let mut session = Session::new(Project::new());
        session.join(volume(-6.0)).unwrap();
        assert_eq!(only(session.undo()).command, volume(-12.0));
    }

    #[test]
    fn an_invalid_join_changes_nothing() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        session.apply(volume(-3.0)).unwrap();
        session.undo();
        assert!(session.join(volume(f32::NAN)).is_err());
        assert!(session.can_redo(), "a rejected command must not clear redo");
        assert_eq!(session.last_sequence(), 3);
        assert_eq!(only(session.undo()).command, volume(-12.0));
    }

    #[test]
    fn apply_then_undo_gives_the_exact_previous_state() {
        let mut session = Session::new(Project::new());
        let before = session.project().clone();
        session.apply(volume(-3.0)).unwrap();
        only(session.undo());
        assert_eq!(session.project(), &before);
    }

    #[test]
    fn undo_and_redo_step_through_the_history() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        session.apply(volume(0.0)).unwrap();

        only(session.undo());
        assert_eq!(session.project().master_volume_db(), -6.0);
        only(session.undo());
        assert_eq!(session.project().master_volume_db(), -12.0);
        assert!(session.undo().is_empty());

        only(session.redo());
        assert_eq!(session.project().master_volume_db(), -6.0);
        only(session.redo());
        assert_eq!(session.project().master_volume_db(), 0.0);
        assert!(session.redo().is_empty());
    }

    #[test]
    fn amending_makes_one_undo_step_of_a_run() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        for volume_db in [-7.0, -8.0, -9.0] {
            session.amend(volume(volume_db)).unwrap();
        }
        assert_eq!(session.project().master_volume_db(), -9.0);

        assert_eq!(only(session.undo()).command, volume(-12.0));
        assert!(!session.can_undo());
        assert_eq!(only(session.redo()).command, volume(-9.0));
    }

    #[test]
    fn amending_with_nothing_to_undo_applies() {
        let mut session = Session::new(Project::new());
        session.amend(volume(-6.0)).unwrap();
        assert_eq!(only(session.undo()).command, volume(-12.0));
    }

    #[test]
    fn an_invalid_amend_changes_nothing() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        assert!(session.amend(volume(f32::NAN)).is_err());
        assert_eq!(session.project().master_volume_db(), -6.0);
        assert_eq!(session.last_sequence(), 1);
        assert_eq!(only(session.undo()).command, volume(-12.0));
        assert_eq!(only(session.redo()).command, volume(-6.0));
    }

    #[test]
    fn a_new_command_clears_redo() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        only(session.undo());
        assert!(session.can_redo());
        session.apply(volume(-1.0)).unwrap();
        assert!(!session.can_redo());
        assert!(session.redo().is_empty());
    }

    #[test]
    fn every_change_gets_the_next_sequence_number() {
        let mut session = Session::new(Project::new());
        assert_eq!(session.last_sequence(), 0);
        assert_eq!(session.apply(volume(-6.0)).unwrap().sequence, 1);
        assert_eq!(only(session.undo()).sequence, 2);
        assert_eq!(only(session.redo()).sequence, 3);
        assert_eq!(session.last_sequence(), 3);
    }

    #[test]
    fn undo_reports_the_inverse_it_applied() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        assert_eq!(only(session.undo()).command, volume(-12.0));
        assert_eq!(only(session.redo()).command, volume(-6.0));
    }

    #[test]
    fn an_invalid_command_changes_nothing() {
        let mut session = Session::new(Project::new());
        session.apply(volume(-6.0)).unwrap();
        only(session.undo());
        let before = session.project().clone();

        assert!(session.apply(volume(f32::NAN)).is_err());
        assert_eq!(session.project(), &before);
        assert_eq!(session.last_sequence(), 2);
        assert!(session.can_redo(), "a rejected command must not clear redo");
    }

    #[test]
    fn a_drag_of_set_notes_undoes_as_one_step() {
        let mut session = Session::new(testing::project());
        let clip = clip_id(&session);
        session
            .apply(Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0), note(1, 64, 0), note(2, 67, 0)],
            })
            .unwrap();
        let before = session.project().clone();

        // Drag notes 0 and 1 right and up, a step at a time.
        let dragged = |step: u64| Command::SetNotes {
            clip,
            notes: vec![
                Note {
                    start: step * 120,
                    ..note(0, 60 + step as u8, 0)
                },
                Note {
                    start: step * 120,
                    ..note(1, 64 + step as u8, 0)
                },
            ],
        };
        session.apply(dragged(1)).unwrap();
        for step in 2..=24 {
            session.amend(dragged(step)).unwrap();
        }
        let clip_now = session.project().clip(clip).unwrap();
        assert_eq!(clip_now.note(note_id(0)).unwrap().start, 24 * 120);
        assert_eq!(clip_now.note(note_id(1)).unwrap().pitch, 88);

        only(session.undo());
        assert_eq!(
            session.project(),
            &before,
            "one undo reverts the whole drag"
        );
        only(session.undo());
        assert_eq!(session.project().clip(clip).unwrap().notes().len(), 0);

        only(session.redo());
        only(session.redo());
        assert_eq!(
            session
                .project()
                .clip(clip)
                .unwrap()
                .note(note_id(0))
                .unwrap()
                .start,
            24 * 120,
            "redo replays the drag's last step"
        );
    }

    #[test]
    fn set_notes_on_other_notes_is_a_new_step() {
        let mut session = Session::new(testing::project());
        let clip = clip_id(&session);
        session
            .apply(Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0), note(1, 64, 0)],
            })
            .unwrap();
        session
            .apply(Command::SetNotes {
                clip,
                notes: vec![note(0, 61, 0)],
            })
            .unwrap();
        session
            .amend(Command::SetNotes {
                clip,
                notes: vec![note(1, 65, 0)],
            })
            .unwrap();
        only(session.undo());
        let clip_now = session.project().clip(clip).unwrap();
        assert_eq!(clip_now.note(note_id(0)).unwrap().pitch, 61);
        assert_eq!(clip_now.note(note_id(1)).unwrap().pitch, 64);
    }

    #[test]
    fn drawing_a_note_and_dragging_its_length_undoes_as_one_step() {
        let mut session = Session::new(testing::project());
        let clip = clip_id(&session);
        let before = session.project().clone();
        session
            .apply(Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0)],
            })
            .unwrap();
        for length in [720, 960, 1200] {
            session
                .amend(Command::SetNotes {
                    clip,
                    notes: vec![Note {
                        length,
                        ..note(0, 60, 0)
                    }],
                })
                .unwrap();
        }
        let drawn = session.project().clone();
        assert_eq!(
            drawn.clip(clip).unwrap().note(note_id(0)).unwrap().length,
            1200
        );

        only(session.undo());
        assert_eq!(session.project(), &before, "one undo removes the note");
        assert!(!session.can_undo());
        only(session.redo());
        assert_eq!(
            session.project(),
            &drawn,
            "redo adds it at its final length"
        );
    }

    #[test]
    fn set_notes_on_other_notes_after_adding_is_a_new_step() {
        let mut session = Session::new(testing::project());
        let clip = clip_id(&session);
        session
            .apply(Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0), note(1, 64, 0)],
            })
            .unwrap();
        // Only one of the two added notes: not a continuation.
        session
            .amend(Command::SetNotes {
                clip,
                notes: vec![note(0, 61, 0)],
            })
            .unwrap();
        only(session.undo());
        assert_eq!(session.project().clip(clip).unwrap().notes().len(), 2);
    }

    #[test]
    fn withdrawing_puts_the_project_and_history_back() {
        let mut session = Session::new(testing::project());
        let clip = clip_id(&session);
        session
            .apply(Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0)],
            })
            .unwrap();
        let before = session.project().clone();
        session
            .apply(Command::SetNotes {
                clip,
                notes: vec![note(0, 72, 960)],
            })
            .unwrap();
        session
            .amend(Command::SetNotes {
                clip,
                notes: vec![note(0, 74, 1920)],
            })
            .unwrap();

        let withdrawn = only(session.withdraw());
        assert_eq!(session.project(), &before);
        assert_eq!(
            withdrawn.command,
            Command::SetNotes {
                clip,
                notes: vec![note(0, 60, 0)],
            },
            "the journal records the inverse"
        );
        assert!(!session.can_redo(), "a withdrawn change can't be redone");
        assert!(session.can_undo(), "earlier changes are still there");
        only(session.undo());
        assert_eq!(session.project().clip(clip).unwrap().notes().len(), 0);
    }

    #[test]
    fn withdrawing_with_nothing_to_undo_does_nothing() {
        let mut session = Session::new(Project::new());
        assert!(session.withdraw().is_empty());
        assert_eq!(session.last_sequence(), 0);
    }

    #[test]
    fn tempo_loop_and_synth_drags_undo_as_one_step_each() {
        let mut session = Session::new(testing::project());
        let before = session.project().clone();
        let track = track_id(&session);

        session.apply(Command::SetTempo { bpm: 121.0 }).unwrap();
        for bpm in [125.0, 140.0, 90.0] {
            session.amend(Command::SetTempo { bpm }).unwrap();
        }
        session.apply(Command::SetLoopLength { bars: 5 }).unwrap();
        for bars in [8, 2] {
            session.amend(Command::SetLoopLength { bars }).unwrap();
        }
        let cutoff = |hz| Command::SetSynthParam {
            track,
            param: SynthParam::CutoffHz(hz),
        };
        session.apply(cutoff(10_000.0)).unwrap();
        for hz in [5_000.0, 800.0] {
            session.amend(cutoff(hz)).unwrap();
        }
        // A different setting is a new step, even when amended.
        session
            .amend(Command::SetSynthParam {
                track,
                param: SynthParam::Resonance(0.4),
            })
            .unwrap();

        let Source::Synth(settings) = session.project().tracks()[0].source() else {
            panic!("a synth track")
        };
        assert_eq!(settings.cutoff_hz, 800.0);
        assert_eq!(settings.resonance, 0.4);

        only(session.undo());
        let Source::Synth(settings) = session.project().tracks()[0].source() else {
            panic!("a synth track")
        };
        assert_eq!((settings.cutoff_hz, settings.resonance), (800.0, 0.0));
        only(session.undo());
        let Source::Synth(settings) = session.project().tracks()[0].source() else {
            panic!("a synth track")
        };
        assert_eq!(settings.cutoff_hz, 20_000.0);
        only(session.undo());
        assert_eq!(session.project().transport().loop_length(), 4 * 3840);
        assert_eq!(session.project().transport().tempo_map().bpm(), 90.0);
        only(session.undo());
        assert_eq!(session.project(), &before);
        assert!(!session.can_undo());
    }

    #[test]
    fn a_drum_knob_drag_undoes_as_one_step() {
        let mut session = Session::new(testing::project());
        let track = testing::track_id(0);
        session
            .apply(Command::AddTracks {
                tracks: vec![crate::PlacedTrack {
                    index: 1,
                    track: crate::Track::new(
                        track,
                        "Drums 1",
                        Source::Drums(crate::KitSettings::default()),
                    ),
                }],
            })
            .unwrap();
        let before = session.project().clone();
        let set = |param| Command::SetDrumParam {
            track,
            sound: crate::DrumSound::Kick,
            param,
        };
        let tune = |hz| set(crate::DrumParam::TuneHz(hz));
        session.apply(tune(50.0)).unwrap();
        for hz in [55.0, 62.0, 58.5] {
            session.amend(tune(hz)).unwrap();
        }
        // A different setting is a new step, even when amended.
        session.amend(set(crate::DrumParam::Tone(0.8))).unwrap();
        let kick = |session: &Session| match session.project().tracks()[1].source() {
            Source::Drums(kit) => kit.kick,
            Source::Synth(_) => panic!("a drum track"),
        };
        assert_eq!((kick(&session).tune_hz, kick(&session).tone), (58.5, 0.8));

        only(session.undo());
        assert_eq!(
            (kick(&session).tune_hz, kick(&session).tone),
            (58.5, crate::KickSettings::default().tone)
        );
        only(session.undo());
        assert_eq!(session.project(), &before);
    }

    #[test]
    fn clip_and_track_mixer_drags_undo_as_one_step_each() {
        let mut session = Session::new(testing::project());
        let before = session.project().clone();
        let track = track_id(&session);
        let clip = clip_id(&session);

        let position = |start| Command::SetClips {
            clips: vec![ClipPosition {
                id: clip,
                track,
                start,
                length: 2 * 3840,
            }],
        };
        session.apply(position(960)).unwrap();
        for start in [1920, 3840, 7680] {
            session.amend(position(start)).unwrap();
        }
        let mixer = |volume_db, pan| Command::SetTrackMixer {
            track,
            mixer: MixerStrip {
                volume_db,
                pan,
                ..MixerStrip::default()
            },
        };
        session.apply(mixer(-1.0, 0.0)).unwrap();
        for (volume_db, pan) in [(-3.0, 0.0), (-3.0, -0.5), (4.5, -0.5)] {
            session.amend(mixer(volume_db, pan)).unwrap();
        }
        let project = session.project();
        assert_eq!(project.clip(clip).unwrap().start(), 7680);
        assert_eq!(project.track(track).unwrap().mixer().volume_db, 4.5);

        only(session.undo());
        let project = session.project();
        assert_eq!(
            project.track(track).unwrap().mixer(),
            &MixerStrip::default()
        );
        assert_eq!(project.clip(clip).unwrap().start(), 7680);
        only(session.undo());
        assert_eq!(session.project(), &before);
        assert!(!session.can_undo());
    }

    #[test]
    fn undoing_a_track_delete_brings_it_back_whole() {
        let mut session = Session::new(testing::project());
        let first = track_id(&session);
        let second = testing::track_id(0);
        let track = Track::new(
            second,
            session.project().next_track_name(SourceKind::Synth),
            Source::Synth(SynthSettings {
                waveform: Waveform::Sine,
                ..SynthSettings::default()
            }),
        )
        .with_mixer(MixerStrip {
            volume_db: -9.0,
            pan: 0.3,
            mute: false,
            solo: true,
        })
        .with_clips([
            Clip::new(testing::clip_id(0), 3840, 3840).with_notes([note(0, 40, 0)]),
            Clip::new(testing::clip_id(1), 0, 7680).with_notes([note(1, 43, 960)]),
        ]);
        session
            .apply(Command::AddTracks {
                tracks: vec![PlacedTrack { index: 0, track }],
            })
            .unwrap();
        session
            .apply(Command::MoveTrack {
                track: first,
                index: 0,
            })
            .unwrap();
        let before = session.project().clone();

        session
            .apply(Command::RemoveTracks {
                tracks: vec![second],
            })
            .unwrap();
        assert_eq!(session.project().tracks().len(), before.tracks().len() - 1);
        only(session.undo());
        assert_eq!(session.project(), &before);
        assert_eq!(session.project().tracks()[1].id(), second, "in its place");
        only(session.redo());
        assert!(session.project().track(second).is_none());
    }

    /// One step a user might take.
    #[derive(Debug, Clone)]
    enum Step {
        Apply(Command),
        Amend(Command),
        Join(Command),
        Undo,
        Redo,
        Withdraw,
    }

    /// Any step, with commands of every kind: mostly valid, some rejected.
    fn step() -> impl Strategy<Value = Step> {
        prop_oneof![
            4 => testing::any_command().prop_map(Step::Apply),
            2 => testing::any_command().prop_map(Step::Amend),
            1 => testing::any_command().prop_map(Step::Join),
            2 => Just(Step::Undo),
            1 => Just(Step::Redo),
            1 => Just(Step::Withdraw),
        ]
    }

    /// Runs `steps` and returns the session and the journal of changes.
    fn run(steps: &[Step]) -> (Project, Session, Vec<Applied>) {
        let start = testing::project();
        let mut session = Session::new(start.clone());
        let mut journal = Vec::new();
        for step in steps {
            journal.extend(run_step(&mut session, step));
        }
        (start, session, journal)
    }

    /// Takes one step and returns the changes it made.
    fn run_step(session: &mut Session, step: &Step) -> Vec<Applied> {
        match step {
            Step::Apply(command) => session.apply(command.clone()).into_iter().collect(),
            Step::Amend(command) => session.amend(command.clone()).into_iter().collect(),
            Step::Join(command) => session.join(command.clone()).into_iter().collect(),
            Step::Undo => session.undo(),
            Step::Redo => session.redo(),
            Step::Withdraw => session.withdraw(),
        }
    }

    /// The clips whose notes `command` sets: the clip a notes command
    /// names, and every clip it adds.
    fn sets_notes_of(command: &Command) -> Vec<ClipId> {
        match command {
            Command::AddNotes { clip, .. }
            | Command::RemoveNotes { clip, .. }
            | Command::SetNotes { clip, .. } => vec![*clip],
            Command::AddClips { clips } => clips.iter().map(|placed| placed.clip.id()).collect(),
            Command::AddTracks { tracks } => tracks
                .iter()
                .flat_map(|placed| placed.track.clips())
                .map(Clip::id)
                .collect(),
            _ => Vec::new(),
        }
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

        /// After any step, including an undo or a redo, every clip whose
        /// notes it didn't set still shares them with the project before,
        /// by pointer. See RFC-004, "How changes are spotted".
        #[test]
        fn a_step_shares_the_notes_of_every_clip_it_doesn_t_change(
            steps in prop::collection::vec(step(), 0..200)
        ) {
            let mut session = Session::new(testing::project());
            for step in &steps {
                let before = session.project().clone();
                let applied = run_step(&mut session, step);
                let set: Vec<ClipId> = applied.iter().flat_map(|a| sets_notes_of(&a.command)).collect();
                for clip in session.project().tracks().iter().flat_map(Track::clips) {
                    if let Some(was) = before.clip(clip.id())
                        && !set.contains(&clip.id())
                    {
                        prop_assert!(clip.shares_notes(was), "{:?} copied {}'s notes", step, clip.id());
                    }
                }
            }
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
            while !session.redo().is_empty() {}
            let end = session.project().clone();
            while !session.undo().is_empty() {}
            prop_assert_eq!(session.project(), &start);
            while !session.redo().is_empty() {}
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
            only(session.undo());
            prop_assert_eq!(session.project(), &before);
        }
    }
}
