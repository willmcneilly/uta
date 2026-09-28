//! Commands: each one is a single, saveable change to a project.

use serde::{Deserialize, Serialize};

use crate::{ClipId, Note, NoteId, SynthParam, TrackId};

/// The command format written by this version of Uta. Bump it when a saved
/// command's shape changes, and teach [`Command`]'s deserialisation to read
/// the older formats.
///
/// - Format 1: `set_master_volume`.
/// - Format 2 adds the notes, tempo, loop and synth commands (RFC-002).
pub const COMMAND_FORMAT: u32 = 2;

/// One change to a project.
///
/// Commands serialise with their format version, as
/// `{"format":2,"command":{"type":"set_master_volume","volume_db":-6.0}}`.
/// They refer to things by permanent IDs, so replaying the same commands
/// always rebuilds the same project. A command that adds something carries
/// the new thing's ID, chosen before the command is applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "wire::Envelope", try_from = "wire::Envelope")]
pub enum Command {
    /// Set the project's master volume, in dB.
    SetMasterVolume { volume_db: f32 },
    /// Add notes to a clip. The inverse of [`Command::RemoveNotes`].
    AddNotes { clip: ClipId, notes: Vec<Note> },
    /// Remove notes from a clip. The inverse of [`Command::AddNotes`].
    RemoveNotes { clip: ClipId, notes: Vec<NoteId> },
    /// Set every value of existing notes in a clip: pitch, velocity, start
    /// and length. Each note is found by its ID.
    SetNotes { clip: ClipId, notes: Vec<Note> },
    /// Set the tempo, in quarter notes per minute.
    SetTempo { bpm: f32 },
    /// Set the loop's length in bars. In project 1 this sets the length of
    /// the project's one clip too, and both undo together.
    SetLoopLength { bars: u32 },
    /// Set one of a track's synth settings.
    SetSynthParam { track: TrackId, param: SynthParam },
}

impl Command {
    /// Whether `other` sets the same thing as this command, so a run of them
    /// can be undone as one (see [`crate::Session::amend`]). Only commands
    /// that set an absolute value qualify: the first one's inverse then
    /// undoes the whole run. For [`Command::SetNotes`], that means the same
    /// notes in the same clip.
    pub fn sets_same_as(&self, other: &Command) -> bool {
        match (self, other) {
            (Self::SetMasterVolume { .. }, Self::SetMasterVolume { .. })
            | (Self::SetTempo { .. }, Self::SetTempo { .. })
            | (Self::SetLoopLength { .. }, Self::SetLoopLength { .. }) => true,
            (
                Self::SetNotes { clip, notes },
                Self::SetNotes {
                    clip: other_clip,
                    notes: other_notes,
                },
            ) => clip == other_clip && sorted_ids(notes) == sorted_ids(other_notes),
            (
                Self::SetSynthParam { track, param },
                Self::SetSynthParam {
                    track: other_track,
                    param: other_param,
                },
            ) => track == other_track && param.same_setting(other_param),
            _ => false,
        }
    }
}

impl Command {
    /// What the undo history keeps when `next` continues this command as
    /// part of the same run (see [`crate::Session::amend`]), or `None` if it
    /// doesn't continue it. A command that sets the same thing replaces this
    /// one. A [`Command::SetNotes`] of exactly the notes this
    /// [`Command::AddNotes`] added folds into it, as the notes' new values,
    /// so drawing a note and dragging out its length undoes as one step.
    pub fn continued_by(&self, next: &Command) -> Option<Command> {
        if self.sets_same_as(next) {
            return Some(next.clone());
        }
        match (self, next) {
            (
                Self::AddNotes { clip, notes },
                Self::SetNotes {
                    clip: next_clip,
                    notes: next_notes,
                },
            ) if clip == next_clip && sorted_ids(notes) == sorted_ids(next_notes) => {
                Some(Self::AddNotes {
                    clip: *clip,
                    notes: next_notes.clone(),
                })
            }
            _ => None,
        }
    }
}

fn sorted_ids(notes: &[Note]) -> Vec<NoteId> {
    let mut ids: Vec<_> = notes.iter().map(|note| note.id).collect();
    ids.sort_unstable();
    ids
}

/// Why a command couldn't be applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandError {
    /// The volume was outside the project's limits, or not a number.
    VolumeOutOfRange(f32),
    /// The tempo was outside 20 to 300 BPM, or not a number.
    TempoOutOfRange(f32),
    /// The loop length was outside 1 to 16 bars.
    LoopLengthOutOfRange(u32),
    /// A synth setting was outside its range, or not a number.
    SynthParamOutOfRange(SynthParam),
    UnknownTrack(TrackId),
    UnknownClip(ClipId),
    /// A note to remove or set isn't in the clip.
    UnknownNote(NoteId),
    /// A note to add has the same ID as one already in the clip.
    NoteAlreadyExists(NoteId),
    /// The same note appears more than once in one command.
    NoteListedTwice(NoteId),
    /// A notes command with no notes in it.
    NoNotes,
    PitchOutOfRange {
        note: NoteId,
        pitch: u8,
    },
    VelocityOutOfRange {
        note: NoteId,
        velocity: u8,
    },
    /// A note with a length of 0.
    EmptyNote(NoteId),
    /// A note that ends after [`crate::time::MAX_TICKS`].
    NoteTooLate(NoteId),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::{Note, Project};
        match self {
            Self::VolumeOutOfRange(volume_db) => {
                write!(f, "volume {volume_db} dB is out of range")
            }
            Self::TempoOutOfRange(bpm) => write!(
                f,
                "tempo {bpm} BPM is out of range ({} to {})",
                Project::MIN_BPM,
                Project::MAX_BPM
            ),
            Self::LoopLengthOutOfRange(bars) => write!(
                f,
                "a loop of {bars} bars is out of range ({} to {})",
                Project::MIN_LOOP_BARS,
                Project::MAX_LOOP_BARS
            ),
            Self::SynthParamOutOfRange(param) => match param.range() {
                Some((value, min, max)) => {
                    write!(f, "{param:?}: {value} is out of range ({min} to {max})")
                }
                None => write!(f, "{param:?} is out of range"),
            },
            Self::UnknownTrack(id) => write!(f, "there's no track {id}"),
            Self::UnknownClip(id) => write!(f, "there's no clip {id}"),
            Self::UnknownNote(id) => write!(f, "there's no note {id} in the clip"),
            Self::NoteAlreadyExists(id) => write!(f, "note {id} is already in the clip"),
            Self::NoteListedTwice(id) => write!(f, "note {id} is listed more than once"),
            Self::NoNotes => write!(f, "the command has no notes"),
            Self::PitchOutOfRange { note, pitch } => write!(
                f,
                "note {note}: pitch {pitch} is out of range (0 to {})",
                Note::MAX_PITCH
            ),
            Self::VelocityOutOfRange { note, velocity } => write!(
                f,
                "note {note}: velocity {velocity} is out of range ({} to {})",
                Note::MIN_VELOCITY,
                Note::MAX_VELOCITY
            ),
            Self::EmptyNote(id) => write!(f, "note {id} has no length"),
            Self::NoteTooLate(id) => write!(f, "note {id} ends too late"),
        }
    }
}

impl std::error::Error for CommandError {}

/// The saved form of a command. It's kept apart from [`Command`] so the
/// in-memory type can change while older saved formats still load: a new
/// format gets its own body type here and a conversion to [`Command`].
///
/// Format 2 only added commands, so one body type reads both formats, and a
/// format 1 envelope may only hold the commands format 1 had.
mod wire {
    use serde::{Deserialize, Serialize};

    use super::{COMMAND_FORMAT, Command};
    use crate::{ClipId, Note, NoteId, SynthParam, TrackId};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct Envelope {
        format: u32,
        command: Body,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
    enum Body {
        // Format 1.
        SetMasterVolume { volume_db: f32 },
        // Format 2.
        AddNotes { clip: ClipId, notes: Vec<Note> },
        RemoveNotes { clip: ClipId, notes: Vec<NoteId> },
        SetNotes { clip: ClipId, notes: Vec<Note> },
        SetTempo { bpm: f32 },
        SetLoopLength { bars: u32 },
        SetSynthParam { track: TrackId, param: SynthParam },
    }

    impl Body {
        /// The first format that has this command.
        fn since_format(&self) -> u32 {
            match self {
                Self::SetMasterVolume { .. } => 1,
                _ => 2,
            }
        }
    }

    impl From<Command> for Envelope {
        fn from(command: Command) -> Self {
            let command = match command {
                Command::SetMasterVolume { volume_db } => Body::SetMasterVolume { volume_db },
                Command::AddNotes { clip, notes } => Body::AddNotes { clip, notes },
                Command::RemoveNotes { clip, notes } => Body::RemoveNotes { clip, notes },
                Command::SetNotes { clip, notes } => Body::SetNotes { clip, notes },
                Command::SetTempo { bpm } => Body::SetTempo { bpm },
                Command::SetLoopLength { bars } => Body::SetLoopLength { bars },
                Command::SetSynthParam { track, param } => Body::SetSynthParam { track, param },
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
            if !(1..=COMMAND_FORMAT).contains(&envelope.format) {
                return Err(format!(
                    "command format {} isn't supported (this version reads formats 1 to {COMMAND_FORMAT})",
                    envelope.format
                ));
            }
            let since = envelope.command.since_format();
            if envelope.format < since {
                return Err(format!(
                    "command format {} doesn't have this command (it arrived in format {since})",
                    envelope.format
                ));
            }
            Ok(match envelope.command {
                Body::SetMasterVolume { volume_db } => Command::SetMasterVolume { volume_db },
                Body::AddNotes { clip, notes } => Command::AddNotes { clip, notes },
                Body::RemoveNotes { clip, notes } => Command::RemoveNotes { clip, notes },
                Body::SetNotes { clip, notes } => Command::SetNotes { clip, notes },
                Body::SetTempo { bpm } => Command::SetTempo { bpm },
                Body::SetLoopLength { bars } => Command::SetLoopLength { bars },
                Body::SetSynthParam { track, param } => Command::SetSynthParam { track, param },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, note, note_id};
    use crate::{Waveform, time::Ticks};
    use proptest::prelude::*;

    fn clip() -> ClipId {
        testing::project().tracks()[0].clips()[0].id()
    }

    fn track() -> TrackId {
        testing::project().tracks()[0].id()
    }

    /// Whether a command holds a NaN or infinity, which JSON can't carry.
    fn has_non_finite(command: &Command) -> bool {
        match command {
            Command::SetMasterVolume { volume_db } => !volume_db.is_finite(),
            Command::SetTempo { bpm } => !bpm.is_finite(),
            Command::SetSynthParam { param, .. } => param
                .range()
                .is_some_and(|(value, _, _)| !value.is_finite()),
            _ => false,
        }
    }

    #[test]
    fn serialises_with_its_format_version() {
        let json = serde_json::to_string(&Command::SetMasterVolume { volume_db: -6.0 }).unwrap();
        assert_eq!(
            json,
            r#"{"format":2,"command":{"type":"set_master_volume","volume_db":-6.0}}"#
        );
    }

    #[test]
    fn the_new_commands_serialise_like_this() {
        let clip = clip();
        let track = track();
        let note = note(0, 60, 960);
        let note_json = format!(
            r#"{{"id":"{}","pitch":60,"velocity":100,"start":960,"length":480}}"#,
            note.id
        );
        let cases = [
            (
                Command::AddNotes {
                    clip,
                    notes: vec![note],
                },
                format!(r#"{{"type":"add_notes","clip":"{clip}","notes":[{note_json}]}}"#),
            ),
            (
                Command::RemoveNotes {
                    clip,
                    notes: vec![note.id],
                },
                format!(
                    r#"{{"type":"remove_notes","clip":"{clip}","notes":["{}"]}}"#,
                    note.id
                ),
            ),
            (
                Command::SetNotes {
                    clip,
                    notes: vec![note],
                },
                format!(r#"{{"type":"set_notes","clip":"{clip}","notes":[{note_json}]}}"#),
            ),
            (
                Command::SetTempo { bpm: 96.5 },
                r#"{"type":"set_tempo","bpm":96.5}"#.to_string(),
            ),
            (
                Command::SetLoopLength { bars: 8 },
                r#"{"type":"set_loop_length","bars":8}"#.to_string(),
            ),
            (
                Command::SetSynthParam {
                    track,
                    param: SynthParam::CutoffHz(1000.0),
                },
                format!(
                    r#"{{"type":"set_synth_param","track":"{track}","param":{{"name":"cutoff_hz","value":1000.0}}}}"#
                ),
            ),
            (
                Command::SetSynthParam {
                    track,
                    param: SynthParam::Waveform(Waveform::Square),
                },
                format!(
                    r#"{{"type":"set_synth_param","track":"{track}","param":{{"name":"waveform","value":"square"}}}}"#
                ),
            ),
        ];
        for (command, body) in cases {
            let json = format!(r#"{{"format":2,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        }
    }

    #[test]
    fn reads_format_1() {
        let json = r#"{"format":1,"command":{"type":"set_master_volume","volume_db":-6.0}}"#;
        let command: Command = serde_json::from_str(json).unwrap();
        assert_eq!(command, Command::SetMasterVolume { volume_db: -6.0 });
    }

    #[test]
    fn format_1_only_has_format_1_commands() {
        let json = r#"{"format":1,"command":{"type":"set_tempo","bpm":90.0}}"#;
        let error = serde_json::from_str::<Command>(json).unwrap_err();
        assert!(error.to_string().contains("arrived in format 2"), "{error}");
    }

    #[test]
    fn rejects_an_unknown_format() {
        for format in [0, 3, 99] {
            let json = format!(
                r#"{{"format":{format},"command":{{"type":"set_master_volume","volume_db":-6.0}}}}"#
            );
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(
                error.to_string().contains(&format!("format {format}")),
                "{error}"
            );
        }
    }

    #[test]
    fn rejects_a_missing_format() {
        let json = r#"{"command":{"type":"set_master_volume","volume_db":-6.0}}"#;
        assert!(serde_json::from_str::<Command>(json).is_err());
    }

    #[test]
    fn rejects_an_unknown_command() {
        let json = r#"{"format":2,"command":{"type":"launch_rocket"}}"#;
        assert!(serde_json::from_str::<Command>(json).is_err());
    }

    #[test]
    fn rejects_a_note_with_an_unknown_field() {
        let json = format!(
            r#"{{"format":2,"command":{{"type":"add_notes","clip":"{}","notes":[{{"id":"{}","pitch":60,"velocity":100,"start":0,"length":480,"colour":"red"}}]}}}}"#,
            clip(),
            note_id(0)
        );
        assert!(serde_json::from_str::<Command>(&json).is_err());
    }

    #[test]
    fn set_notes_continues_only_with_the_same_notes() {
        let clip = clip();
        let set = |notes: Vec<Note>| Command::SetNotes { clip, notes };
        let first = set(vec![note(0, 60, 0), note(1, 62, 0)]);
        assert!(first.sets_same_as(&set(vec![note(0, 72, 480), note(1, 74, 480)])));
        assert!(
            first.sets_same_as(&set(vec![note(1, 74, 480), note(0, 72, 480)])),
            "order doesn't matter"
        );
        assert!(!first.sets_same_as(&set(vec![note(0, 72, 480)])));
        assert!(!first.sets_same_as(&set(vec![note(0, 72, 480), note(2, 74, 480)])));
        assert!(!first.sets_same_as(&Command::SetNotes {
            clip: ClipId::random(),
            notes: vec![note(0, 60, 0), note(1, 62, 0)],
        }));
    }

    #[test]
    fn set_synth_param_continues_only_with_the_same_setting() {
        let track = track();
        let set = |param| Command::SetSynthParam { track, param };
        let cutoff = set(SynthParam::CutoffHz(1000.0));
        assert!(cutoff.sets_same_as(&set(SynthParam::CutoffHz(900.0))));
        assert!(!cutoff.sets_same_as(&set(SynthParam::Resonance(0.5))));
        assert!(!cutoff.sets_same_as(&Command::SetSynthParam {
            track: TrackId::random(),
            param: SynthParam::CutoffHz(900.0),
        }));
    }

    #[test]
    fn only_absolute_commands_continue() {
        let tempo = Command::SetTempo { bpm: 100.0 };
        let loop_length = Command::SetLoopLength { bars: 2 };
        assert!(tempo.sets_same_as(&Command::SetTempo { bpm: 101.0 }));
        assert!(loop_length.sets_same_as(&Command::SetLoopLength { bars: 3 }));
        assert!(!tempo.sets_same_as(&loop_length));
        let add = Command::AddNotes {
            clip: clip(),
            notes: vec![note(0, 60, 0)],
        };
        assert!(
            !add.sets_same_as(&add.clone()),
            "adding twice isn't one change"
        );
        let remove = Command::RemoveNotes {
            clip: clip(),
            notes: vec![note_id(0)],
        };
        assert!(!remove.sets_same_as(&remove.clone()));
    }

    proptest! {
        #[test]
        fn master_volume_round_trips_through_serialisation(
            volume_db in any::<f32>().prop_filter("JSON has no NaN or infinity", |v| v.is_finite())
        ) {
            let command = Command::SetMasterVolume { volume_db };
            let json = serde_json::to_string(&command).unwrap();
            let back: Command = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, command);
        }

        #[test]
        fn commands_round_trip_through_serialisation(
            command in testing::any_command().prop_filter("JSON has no NaN or infinity", |c| !has_non_finite(c)),
            far in any::<Ticks>(),
        ) {
            let json = serde_json::to_string(&command).unwrap();
            let back: Command = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, command);

            // Any tick value survives, even ones the project would reject.
            let far_note = Command::SetNotes { clip: clip(), notes: vec![Note { start: far, length: far, ..note(0, 60, 0) }] };
            let json = serde_json::to_string(&far_note).unwrap();
            prop_assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), far_note);
        }
    }
}
