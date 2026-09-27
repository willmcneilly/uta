//! The project: everything Uta saves about a piece of music.

use std::collections::HashSet;

use uuid::Uuid;

use crate::time::{TempoMap, Ticks, TimeSignature};
use crate::track::{Clip, MixerStrip, Source, Track};
use crate::{ClipId, Command, CommandError, NoteId, ProjectId, SynthSettings, TrackId};

/// The song's tempo, time signature and loop.
#[derive(Debug, Clone, PartialEq)]
pub struct Transport {
    tempo_map: TempoMap,
    time_signature: TimeSignature,
    loop_start: Ticks,
    loop_length: Ticks,
}

impl Transport {
    pub fn tempo_map(&self) -> &TempoMap {
        &self.tempo_map
    }

    /// Always 4/4 for now.
    pub fn time_signature(&self) -> TimeSignature {
        self.time_signature
    }

    /// Where the loop starts, in ticks. Always 0 for now.
    pub fn loop_start(&self) -> Ticks {
        self.loop_start
    }

    /// How long the loop is, in ticks. Always a whole number of bars.
    pub fn loop_length(&self) -> Ticks {
        self.loop_length
    }
}

/// A project: a master volume, a transport, and tracks holding clips of
/// notes. See RFC-002, "The shared model".
///
/// Its data only changes through [`Command`]s, so every change can be undone
/// and replayed. In project 1 it always has exactly one track with one clip,
/// created with the project.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    id: ProjectId,
    master_volume_db: f32,
    transport: Transport,
    tracks: Vec<Track>,
}

impl Project {
    /// A new project's master volume, in dB.
    pub const DEFAULT_MASTER_VOLUME_DB: f32 = -12.0;
    /// The quietest master volume, in dB. The engine treats it as silence.
    pub const MIN_VOLUME_DB: f32 = -120.0;
    /// The loudest master volume, in dB.
    pub const MAX_VOLUME_DB: f32 = 6.0;

    /// A new project's tempo, in quarter notes per minute.
    pub const DEFAULT_BPM: f32 = 120.0;
    pub const MIN_BPM: f32 = 20.0;
    pub const MAX_BPM: f32 = 300.0;

    /// A new project's loop length, in bars.
    pub const DEFAULT_LOOP_BARS: u32 = 4;
    pub const MIN_LOOP_BARS: u32 = 1;
    pub const MAX_LOOP_BARS: u32 = 16;

    /// A new project with a random ID.
    pub fn new() -> Self {
        Self::with_id(ProjectId::random())
    }

    /// A new project with the given ID. Its track's and clip's IDs are
    /// worked out from the project's ID, so the same ID always gives exactly
    /// the same project, and commands saved against it replay.
    pub fn with_id(id: ProjectId) -> Self {
        let time_signature = TimeSignature::FOUR_FOUR;
        let loop_length = Ticks::from(Self::DEFAULT_LOOP_BARS) * time_signature.ticks_per_bar();
        let derived = |name: &str| Uuid::new_v5(&id.as_uuid(), name.as_bytes());
        Self {
            id,
            master_volume_db: Self::DEFAULT_MASTER_VOLUME_DB,
            transport: Transport {
                tempo_map: TempoMap::new(Self::DEFAULT_BPM),
                time_signature,
                loop_start: 0,
                loop_length,
            },
            tracks: vec![Track {
                id: TrackId::from_uuid(derived("track 1")),
                source: Source::Synth(SynthSettings::default()),
                effects: Vec::new(),
                mixer: MixerStrip::default(),
                clips: vec![Clip {
                    id: ClipId::from_uuid(derived("clip 1")),
                    start: 0,
                    length: loop_length,
                    notes: Default::default(),
                }],
            }],
        }
    }

    pub fn id(&self) -> ProjectId {
        self.id
    }

    /// The master volume, in dB.
    pub fn master_volume_db(&self) -> f32 {
        self.master_volume_db
    }

    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.tracks
            .iter()
            .flat_map(|track| &track.clips)
            .find(|clip| clip.id == id)
    }

    /// Applies `command` and returns its inverse: the command that puts the
    /// project back exactly as it was. If the command is invalid, the project
    /// is left unchanged.
    pub fn apply(&mut self, command: &Command) -> Result<Command, CommandError> {
        match command {
            &Command::SetMasterVolume { volume_db } => {
                if !(Self::MIN_VOLUME_DB..=Self::MAX_VOLUME_DB).contains(&volume_db) {
                    return Err(CommandError::VolumeOutOfRange(volume_db));
                }
                let previous = std::mem::replace(&mut self.master_volume_db, volume_db);
                Ok(Command::SetMasterVolume {
                    volume_db: previous,
                })
            }
            Command::AddNotes { clip, notes } => {
                let clip = self.clip_mut(*clip)?;
                check_listed_once(notes.iter().map(|note| note.id))?;
                for note in notes {
                    note.validate()?;
                    if clip.notes.contains_key(&note.id) {
                        return Err(CommandError::NoteAlreadyExists(note.id));
                    }
                }
                clip.notes.extend(notes.iter().map(|note| (note.id, *note)));
                Ok(Command::RemoveNotes {
                    clip: clip.id,
                    notes: notes.iter().map(|note| note.id).collect(),
                })
            }
            Command::RemoveNotes { clip, notes } => {
                let clip = self.clip_mut(*clip)?;
                check_listed_once(notes.iter().copied())?;
                check_in_clip(clip, notes.iter().copied())?;
                let removed = notes
                    .iter()
                    .map(|id| clip.notes.remove(id).expect("checked above"))
                    .collect();
                Ok(Command::AddNotes {
                    clip: clip.id,
                    notes: removed,
                })
            }
            Command::SetNotes { clip, notes } => {
                let clip = self.clip_mut(*clip)?;
                check_listed_once(notes.iter().map(|note| note.id))?;
                check_in_clip(clip, notes.iter().map(|note| note.id))?;
                for note in notes {
                    note.validate()?;
                }
                let previous = notes
                    .iter()
                    .map(|note| clip.notes.insert(note.id, *note).expect("checked above"))
                    .collect();
                Ok(Command::SetNotes {
                    clip: clip.id,
                    notes: previous,
                })
            }
            &Command::SetTempo { bpm } => {
                if !(Self::MIN_BPM..=Self::MAX_BPM).contains(&bpm) {
                    return Err(CommandError::TempoOutOfRange(bpm));
                }
                let previous = self.transport.tempo_map.set_bpm(bpm);
                Ok(Command::SetTempo { bpm: previous })
            }
            &Command::SetLoopLength { bars } => {
                if !(Self::MIN_LOOP_BARS..=Self::MAX_LOOP_BARS).contains(&bars) {
                    return Err(CommandError::LoopLengthOutOfRange(bars));
                }
                let bar = self.transport.time_signature.ticks_per_bar();
                let length = Ticks::from(bars) * bar;
                let previous = std::mem::replace(&mut self.transport.loop_length, length);
                // In project 1 the loop and the one clip are the same length.
                // Make a song gives clips their own lengths.
                self.tracks[0].clips[0].length = length;
                Ok(Command::SetLoopLength {
                    bars: u32::try_from(previous / bar).expect("at most 16 bars"),
                })
            }
            &Command::SetSynthParam { track, param } => {
                let track = self
                    .tracks
                    .iter_mut()
                    .find(|candidate| candidate.id == track)
                    .ok_or(CommandError::UnknownTrack(track))?;
                let Source::Synth(settings) = &mut track.source;
                let previous = settings
                    .set(param)
                    .map_err(CommandError::SynthParamOutOfRange)?;
                Ok(Command::SetSynthParam {
                    track: track.id,
                    param: previous,
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

    fn clip_mut(&mut self, id: ClipId) -> Result<&mut Clip, CommandError> {
        self.tracks
            .iter_mut()
            .flat_map(|track| &mut track.clips)
            .find(|clip| clip.id == id)
            .ok_or(CommandError::UnknownClip(id))
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

/// Checks a notes command names at least one note, and each only once.
fn check_listed_once(ids: impl Iterator<Item = NoteId>) -> Result<(), CommandError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(CommandError::NoteListedTwice(id));
        }
    }
    if seen.is_empty() {
        return Err(CommandError::NoNotes);
    }
    Ok(())
}

/// Checks every note is in `clip`.
fn check_in_clip(clip: &Clip, mut ids: impl Iterator<Item = NoteId>) -> Result<(), CommandError> {
    match ids.find(|id| !clip.notes.contains_key(id)) {
        Some(missing) => Err(CommandError::UnknownNote(missing)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, note, note_id};
    use crate::{Effect, Note, SynthParam, Waveform};
    use proptest::prelude::*;

    fn clip_id(project: &Project) -> ClipId {
        project.tracks()[0].clips()[0].id()
    }

    fn track_id(project: &Project) -> TrackId {
        project.tracks()[0].id()
    }

    /// Applies `command`, checks its inverse puts the project back exactly,
    /// and returns the project with the command applied.
    fn apply_and_check_undo(project: &Project, command: Command) -> Project {
        let mut changed = project.clone();
        let inverse = changed.apply(&command).unwrap();
        assert_ne!(&changed, project, "{command:?} changed nothing");
        let mut undone = changed.clone();
        undone.apply(&inverse).unwrap();
        assert_eq!(&undone, project, "undoing {command:?}");
        changed
    }

    /// Applies an invalid `command` and checks it's rejected with `error`
    /// and changes nothing.
    fn assert_rejected(project: &Project, command: Command, error: CommandError) {
        let mut attempt = project.clone();
        assert_eq!(attempt.apply(&command), Err(error), "{command:?}");
        assert_eq!(&attempt, project);
    }

    /// The project with notes 0 and 1 in its clip.
    fn with_two_notes() -> Project {
        let mut project = testing::project();
        let clip = clip_id(&project);
        project
            .apply(&Command::AddNotes {
                clip,
                notes: vec![note(0, 60, 0), note(1, 64, 960)],
            })
            .unwrap();
        project
    }

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

    #[test]
    fn new_project_has_one_track_with_one_empty_clip() {
        let project = Project::new();
        let transport = project.transport();
        assert_eq!(transport.tempo_map().bpm(), 120.0);
        assert_eq!(transport.tempo_map().sections().len(), 1);
        assert_eq!(transport.time_signature(), TimeSignature::FOUR_FOUR);
        assert_eq!(transport.loop_start(), 0);
        assert_eq!(transport.loop_length(), 4 * 3840, "4 bars");

        let [track] = project.tracks() else {
            panic!("expected one track");
        };
        assert_eq!(track.source(), &Source::Synth(SynthSettings::default()));
        assert_eq!(track.effects(), &[] as &[Effect]);
        assert_eq!(track.mixer(), &MixerStrip::default());
        let [clip] = track.clips() else {
            panic!("expected one clip");
        };
        assert_eq!(clip.start(), 0);
        assert_eq!(clip.length(), transport.loop_length());
        assert_eq!(clip.notes().len(), 0);
    }

    #[test]
    fn the_same_project_id_gives_the_same_project() {
        let id = ProjectId::random();
        assert_eq!(Project::with_id(id), Project::with_id(id));
        let (a, b) = (Project::new(), Project::new());
        assert_ne!(track_id(&a), track_id(&b));
        assert_ne!(clip_id(&a), clip_id(&b));
        assert_ne!(track_id(&a).as_uuid(), clip_id(&a).as_uuid());
    }

    #[test]
    fn add_notes_adds_them_and_undoes_by_removing_them() {
        let project = testing::project();
        let clip = clip_id(&project);
        let notes = vec![note(0, 60, 0), note(1, 64, 960)];
        let mut changed = project.clone();
        let inverse = changed
            .apply(&Command::AddNotes {
                clip,
                notes: notes.clone(),
            })
            .unwrap();
        assert_eq!(
            inverse,
            Command::RemoveNotes {
                clip,
                notes: vec![note_id(0), note_id(1)],
            }
        );
        let stored = changed.clip(clip).unwrap();
        assert_eq!(stored.note(note_id(0)), Some(&notes[0]));
        assert_eq!(stored.note(note_id(1)), Some(&notes[1]));
        apply_and_check_undo(&project, Command::AddNotes { clip, notes });
    }

    #[test]
    fn remove_notes_undoes_by_adding_them_back() {
        let project = with_two_notes();
        let clip = clip_id(&project);
        // Removed in the opposite order to how they were added.
        let command = Command::RemoveNotes {
            clip,
            notes: vec![note_id(1), note_id(0)],
        };
        let mut changed = project.clone();
        let inverse = changed.apply(&command).unwrap();
        assert_eq!(
            inverse,
            Command::AddNotes {
                clip,
                notes: vec![note(1, 64, 960), note(0, 60, 0)],
            }
        );
        assert_eq!(changed.clip(clip).unwrap().notes().len(), 0);
        apply_and_check_undo(&project, command);
    }

    #[test]
    fn set_notes_sets_every_value_and_undoes_to_the_old_ones() {
        let project = with_two_notes();
        let clip = clip_id(&project);
        let moved = Note {
            id: note_id(1),
            pitch: 72,
            velocity: 30,
            start: 1920,
            length: 240,
        };
        let mut changed = project.clone();
        let inverse = changed
            .apply(&Command::SetNotes {
                clip,
                notes: vec![moved],
            })
            .unwrap();
        assert_eq!(
            inverse,
            Command::SetNotes {
                clip,
                notes: vec![note(1, 64, 960)],
            }
        );
        assert_eq!(changed.clip(clip).unwrap().note(note_id(1)), Some(&moved));
        assert_eq!(
            changed.clip(clip).unwrap().note(note_id(0)),
            Some(&note(0, 60, 0)),
            "other notes are untouched"
        );
        apply_and_check_undo(
            &project,
            Command::SetNotes {
                clip,
                notes: vec![moved],
            },
        );
    }

    #[test]
    fn notes_outside_the_clip_are_kept() {
        let project = testing::project();
        let clip = clip_id(&project);
        let past_the_end = note(0, 60, 4 * 3840 + 960);
        let mut changed = project.clone();
        changed
            .apply(&Command::AddNotes {
                clip,
                notes: vec![past_the_end],
            })
            .unwrap();
        // Shortening the loop and lengthening it again keeps the note too.
        changed.apply(&Command::SetLoopLength { bars: 1 }).unwrap();
        changed.apply(&Command::SetLoopLength { bars: 8 }).unwrap();
        assert_eq!(
            changed.clip(clip).unwrap().note(note_id(0)),
            Some(&past_the_end)
        );
    }

    #[test]
    fn note_limits_are_inclusive() {
        let project = testing::project();
        let clip = clip_id(&project);
        let notes = vec![
            Note {
                id: note_id(0),
                pitch: 0,
                velocity: 1,
                start: 0,
                length: 1,
            },
            Note {
                id: note_id(1),
                pitch: 127,
                velocity: 127,
                start: 0,
                length: crate::time::MAX_TICKS,
            },
        ];
        apply_and_check_undo(&project, Command::AddNotes { clip, notes });
    }

    #[test]
    fn invalid_notes_are_rejected_and_change_nothing() {
        let project = with_two_notes();
        let clip = clip_id(&project);
        let fresh = note(5, 60, 0);
        let invalid = [
            (
                Note {
                    pitch: 128,
                    ..fresh
                },
                CommandError::PitchOutOfRange {
                    note: fresh.id,
                    pitch: 128,
                },
            ),
            (
                Note {
                    velocity: 0,
                    ..fresh
                },
                CommandError::VelocityOutOfRange {
                    note: fresh.id,
                    velocity: 0,
                },
            ),
            (
                Note {
                    velocity: 128,
                    ..fresh
                },
                CommandError::VelocityOutOfRange {
                    note: fresh.id,
                    velocity: 128,
                },
            ),
            (
                Note { length: 0, ..fresh },
                CommandError::EmptyNote(fresh.id),
            ),
            (
                Note {
                    start: crate::time::MAX_TICKS,
                    ..fresh
                },
                CommandError::NoteTooLate(fresh.id),
            ),
            (
                Note {
                    start: u64::MAX,
                    ..fresh
                },
                CommandError::NoteTooLate(fresh.id),
            ),
        ];
        for (bad, error) in invalid {
            // One valid note first, so a half-applied command would show.
            assert_rejected(
                &project,
                Command::AddNotes {
                    clip,
                    notes: vec![note(6, 60, 0), bad],
                },
                error,
            );
            assert_rejected(
                &project,
                Command::SetNotes {
                    clip,
                    notes: vec![
                        note(0, 61, 0),
                        Note {
                            id: note_id(1),
                            ..bad
                        },
                    ],
                },
                match error {
                    CommandError::PitchOutOfRange { pitch, .. } => CommandError::PitchOutOfRange {
                        note: note_id(1),
                        pitch,
                    },
                    CommandError::VelocityOutOfRange { velocity, .. } => {
                        CommandError::VelocityOutOfRange {
                            note: note_id(1),
                            velocity,
                        }
                    }
                    CommandError::EmptyNote(_) => CommandError::EmptyNote(note_id(1)),
                    CommandError::NoteTooLate(_) => CommandError::NoteTooLate(note_id(1)),
                    other => other,
                },
            );
        }
    }

    #[test]
    fn notes_commands_check_which_notes_they_name() {
        let project = with_two_notes();
        let clip = clip_id(&project);
        let unknown_clip = ClipId::random();
        let cases = [
            (
                Command::AddNotes {
                    clip: unknown_clip,
                    notes: vec![note(5, 60, 0)],
                },
                CommandError::UnknownClip(unknown_clip),
            ),
            (
                Command::AddNotes {
                    clip,
                    notes: vec![note(5, 60, 0), note(0, 60, 0)],
                },
                CommandError::NoteAlreadyExists(note_id(0)),
            ),
            (
                Command::AddNotes {
                    clip,
                    notes: vec![note(5, 60, 0), note(5, 62, 0)],
                },
                CommandError::NoteListedTwice(note_id(5)),
            ),
            (
                Command::AddNotes {
                    clip,
                    notes: vec![],
                },
                CommandError::NoNotes,
            ),
            (
                Command::RemoveNotes {
                    clip,
                    notes: vec![note_id(0), note_id(5)],
                },
                CommandError::UnknownNote(note_id(5)),
            ),
            (
                Command::RemoveNotes {
                    clip,
                    notes: vec![note_id(0), note_id(0)],
                },
                CommandError::NoteListedTwice(note_id(0)),
            ),
            (
                Command::RemoveNotes {
                    clip: unknown_clip,
                    notes: vec![note_id(0)],
                },
                CommandError::UnknownClip(unknown_clip),
            ),
            (
                Command::RemoveNotes {
                    clip,
                    notes: vec![],
                },
                CommandError::NoNotes,
            ),
            (
                Command::SetNotes {
                    clip,
                    notes: vec![note(0, 62, 0), note(5, 60, 0)],
                },
                CommandError::UnknownNote(note_id(5)),
            ),
            (
                Command::SetNotes {
                    clip,
                    notes: vec![note(0, 62, 0), note(0, 63, 0)],
                },
                CommandError::NoteListedTwice(note_id(0)),
            ),
            (
                Command::SetNotes {
                    clip,
                    notes: vec![],
                },
                CommandError::NoNotes,
            ),
        ];
        for (command, error) in cases {
            assert_rejected(&project, command, error);
        }
    }

    #[test]
    fn set_tempo_sets_it_and_undoes() {
        let project = testing::project();
        let changed = apply_and_check_undo(&project, Command::SetTempo { bpm: 90.5 });
        assert_eq!(changed.transport().tempo_map().bpm(), 90.5);
        for bpm in [Project::MIN_BPM, Project::MAX_BPM] {
            apply_and_check_undo(&project, Command::SetTempo { bpm });
        }
        for bpm in [19.9, 300.1, 0.0, -120.0, f32::INFINITY] {
            assert_rejected(
                &project,
                Command::SetTempo { bpm },
                CommandError::TempoOutOfRange(bpm),
            );
        }
        // NaN never equals itself, so check this one by shape.
        let mut attempt = project.clone();
        let result = attempt.apply(&Command::SetTempo { bpm: f32::NAN });
        assert!(matches!(result, Err(CommandError::TempoOutOfRange(bpm)) if bpm.is_nan()));
        assert_eq!(attempt, project);
    }

    #[test]
    fn set_loop_length_sets_the_loop_and_the_clip_and_undoes_both() {
        let project = testing::project();
        let mut changed = project.clone();
        let inverse = changed.apply(&Command::SetLoopLength { bars: 7 }).unwrap();
        assert_eq!(inverse, Command::SetLoopLength { bars: 4 });
        assert_eq!(changed.transport().loop_length(), 7 * 3840);
        assert_eq!(changed.clip(clip_id(&changed)).unwrap().length(), 7 * 3840);
        changed.apply(&inverse).unwrap();
        assert_eq!(changed, project);

        for bars in [Project::MIN_LOOP_BARS, Project::MAX_LOOP_BARS] {
            apply_and_check_undo(&project, Command::SetLoopLength { bars });
        }
        for bars in [0, 17, u32::MAX] {
            assert_rejected(
                &project,
                Command::SetLoopLength { bars },
                CommandError::LoopLengthOutOfRange(bars),
            );
        }
    }

    #[test]
    fn set_synth_param_sets_it_and_undoes() {
        let project = testing::project();
        let track = track_id(&project);
        let changed = apply_and_check_undo(
            &project,
            Command::SetSynthParam {
                track,
                param: SynthParam::CutoffHz(800.0),
            },
        );
        let Source::Synth(settings) = changed.track(track).unwrap().source();
        assert_eq!(settings.cutoff_hz, 800.0);

        for param in [
            SynthParam::Waveform(Waveform::Sine),
            SynthParam::Resonance(0.5),
            SynthParam::AttackSeconds(1.0),
            SynthParam::DecaySeconds(0.001),
            SynthParam::Sustain(0.2),
            SynthParam::ReleaseSeconds(10.0),
        ] {
            apply_and_check_undo(&project, Command::SetSynthParam { track, param });
        }

        let param = SynthParam::Resonance(2.0);
        assert_rejected(
            &project,
            Command::SetSynthParam { track, param },
            CommandError::SynthParamOutOfRange(param),
        );
        let unknown = TrackId::random();
        assert_rejected(
            &project,
            Command::SetSynthParam {
                track: unknown,
                param: SynthParam::Sustain(0.5),
            },
            CommandError::UnknownTrack(unknown),
        );
    }

    #[test]
    fn errors_say_what_was_wrong() {
        let messages = [
            CommandError::TempoOutOfRange(400.0).to_string(),
            CommandError::LoopLengthOutOfRange(17).to_string(),
            CommandError::SynthParamOutOfRange(SynthParam::CutoffHz(5.0)).to_string(),
            CommandError::PitchOutOfRange {
                note: note_id(0),
                pitch: 200,
            }
            .to_string(),
        ];
        assert_eq!(messages[0], "tempo 400 BPM is out of range (20 to 300)");
        assert_eq!(messages[1], "a loop of 17 bars is out of range (1 to 16)");
        assert_eq!(
            messages[2],
            "CutoffHz(5.0): 5 is out of range (20 to 20000)"
        );
        assert!(messages[3].contains("pitch 200 is out of range (0 to 127)"));
    }

    proptest! {
        #[test]
        fn any_command_that_applies_undoes_to_the_exact_previous_state(
            setup in testing::any_commands(30),
            command in testing::any_command(),
        ) {
            let mut project = testing::project();
            for command in &setup {
                let _ = project.apply(command);
            }
            let before = project.clone();
            match project.apply(&command) {
                Ok(inverse) => {
                    project.apply(&inverse).unwrap();
                    prop_assert_eq!(&project, &before);
                }
                Err(_) => prop_assert_eq!(&project, &before, "a rejected command changed the project"),
            }
        }
    }
}
