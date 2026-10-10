//! Commands: each one is a single, saveable change to a project.

use serde::{Deserialize, Serialize};

use crate::time::Ticks;
use crate::{
    Clip, ClipId, DrumParam, DrumSound, KitSettings, MixerStrip, Note, NoteId, SynthParam, Track,
    TrackId,
};

/// The command format written by this version of Uta. Bump it when a saved
/// command's shape changes, and teach [`Command`]'s deserialisation to read
/// the older formats.
///
/// - Format 1: `set_master_volume`.
/// - Format 2 adds the notes, tempo, loop and synth commands (RFC-002).
/// - Format 3 adds the track, clip and loop region commands (RFC-003).
/// - Format 4 adds drum tracks and `set_drum_param` (RFC-006).
pub const COMMAND_FORMAT: u32 = 4;

/// One change to a project.
///
/// Commands serialise with their format version, as
/// `{"format":4,"command":{"type":"set_master_volume","volume_db":-6.0}}`.
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
    /// Set the loop's length in bars, and the length of the first track's
    /// first clip to match, as in Make a loop, where they were always the
    /// same. It's refused once they differ, or the loop is outside 1 to 16
    /// bars: use [`Command::SetLoop`] and [`Command::SetClips`] instead.
    SetLoopLength { bars: u32 },
    /// Set one of a synth track's settings.
    SetSynthParam { track: TrackId, param: SynthParam },
    /// Set one setting of one sound on a drum track.
    SetDrumParam {
        track: TrackId,
        sound: DrumSound,
        param: DrumParam,
    },
    /// Add tracks, each at its place in the order. The inverse of
    /// [`Command::RemoveTracks`].
    AddTracks { tracks: Vec<PlacedTrack> },
    /// Remove tracks, with their clips. The inverse of
    /// [`Command::AddTracks`].
    RemoveTracks { tracks: Vec<TrackId> },
    /// Move a track to `index` in the order, counting from 0.
    MoveTrack { track: TrackId, index: usize },
    /// Set a track's volume, pan, mute and solo.
    SetTrackMixer { track: TrackId, mixer: MixerStrip },
    /// Add clips to tracks. The inverse of [`Command::RemoveClips`].
    AddClips { clips: Vec<PlacedClip> },
    /// Remove clips, with their notes. The inverse of
    /// [`Command::AddClips`].
    RemoveClips { clips: Vec<ClipId> },
    /// Set existing clips' track, start and length. Each clip is found by
    /// its ID, so moving a clip to another track is one command.
    SetClips { clips: Vec<ClipPosition> },
    /// Set the loop region: where it starts and how long it is, in bars.
    SetLoop { start_bar: u32, bars: u32 },
    /// Switch the loop on or off.
    SetLoopEnabled { enabled: bool },
}

/// A track to add, and where it goes: its index in the order once it's
/// added, counting from 0.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedTrack {
    pub index: usize,
    pub track: Track,
}

/// A clip to add, and the track it goes on.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedClip {
    pub track: TrackId,
    pub clip: Clip,
}

/// Where a clip is: its track, start and length, as
/// [`Command::SetClips`] carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipPosition {
    pub id: ClipId,
    pub track: TrackId,
    /// In ticks from the start of the song.
    pub start: Ticks,
    pub length: Ticks,
}

impl Command {
    /// Whether `other` sets the same thing as this command, so a run of them
    /// can be undone as one (see [`crate::Session::amend`]). Only commands
    /// that set an absolute value qualify: the first one's inverse then
    /// undoes the whole run. For [`Command::SetNotes`] and
    /// [`Command::SetClips`], that means the same notes or clips.
    pub fn sets_same_as(&self, other: &Command) -> bool {
        match (self, other) {
            (Self::SetMasterVolume { .. }, Self::SetMasterVolume { .. })
            | (Self::SetTempo { .. }, Self::SetTempo { .. })
            | (Self::SetLoopLength { .. }, Self::SetLoopLength { .. })
            | (Self::SetLoop { .. }, Self::SetLoop { .. }) => true,
            (
                Self::SetTrackMixer { track, .. },
                Self::SetTrackMixer {
                    track: other_track, ..
                },
            ) => track == other_track,
            (Self::SetClips { clips }, Self::SetClips { clips: other_clips }) => {
                sorted_clip_ids(clips) == sorted_clip_ids(other_clips)
            }
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
            (
                Self::SetDrumParam {
                    track,
                    sound,
                    param,
                },
                Self::SetDrumParam {
                    track: other_track,
                    sound: other_sound,
                    param: other_param,
                },
            ) => track == other_track && sound == other_sound && param.same_setting(other_param),
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

fn sorted_clip_ids(clips: &[ClipPosition]) -> Vec<ClipId> {
    let mut ids: Vec<_> = clips.iter().map(|clip| clip.id).collect();
    ids.sort_unstable();
    ids
}

/// Why a command couldn't be applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandError {
    /// The master or a track's volume was outside its limits, or not a
    /// number.
    VolumeOutOfRange(f32),
    /// A track's pan was outside -1 to 1, or not a number.
    PanOutOfRange(f32),
    /// The tempo was outside 20 to 300 BPM, or not a number.
    TempoOutOfRange(f32),
    /// The loop length was outside 1 to 16 bars.
    LoopLengthOutOfRange(u32),
    /// [`Command::SetLoopLength`] once the loop is outside 1 to 16 bars, or
    /// the first track's first clip is no longer as long as it.
    LoopLengthUnavailable,
    /// A loop region with no length, or that ends too late.
    LoopOutOfRange {
        start_bar: u32,
        bars: u32,
    },
    /// A synth setting was outside its range, or not a number.
    SynthParamOutOfRange(SynthParam),
    /// A drum setting was outside its range on that sound, or not a number.
    DrumParamOutOfRange {
        sound: DrumSound,
        param: DrumParam,
    },
    /// A drum setting the sound doesn't have.
    NoSuchDrumParam {
        sound: DrumSound,
        param: DrumParam,
    },
    /// A synth command for a track that isn't a synth.
    NotASynthTrack(TrackId),
    /// A drum command for a track that isn't a drum track.
    NotADrumTrack(TrackId),
    UnknownTrack(TrackId),
    UnknownClip(ClipId),
    /// A track to add has the same ID as one already in the project.
    TrackAlreadyExists(TrackId),
    /// The same track appears more than once in one command.
    TrackListedTwice(TrackId),
    /// A tracks command with no tracks in it.
    NoTracks,
    /// Adding the tracks would take the project past
    /// [`crate::Project::MAX_TRACKS`].
    TooManyTracks,
    /// A place in the track order past the end.
    TrackIndexOutOfRange(usize),
    /// Two tracks to add at the same place in the order.
    TrackIndexListedTwice(usize),
    /// A track to add with no name.
    UnnamedTrack(TrackId),
    /// A clip to add has the same ID as one already in the project.
    ClipAlreadyExists(ClipId),
    /// The same clip appears more than once in one command.
    ClipListedTwice(ClipId),
    /// A clips command with no clips in it.
    NoClips,
    /// A clip with a length of 0.
    EmptyClip(ClipId),
    /// A clip that ends after [`crate::time::MAX_TICKS`].
    ClipTooLate(ClipId),
    /// A clip moved to a track of the other kind: from a synth track to a
    /// drum track, or back.
    ClipToOtherKind {
        clip: ClipId,
        track: TrackId,
    },
    /// A note to remove or set isn't in the clip.
    UnknownNote(NoteId),
    /// A note to add has the same ID as one already in the project, in any
    /// clip.
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
    /// A note on a drum track at a pitch that isn't one of the kit's notes.
    NoteOffKit {
        note: NoteId,
        pitch: u8,
    },
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::{Note, Project};
        match self {
            Self::VolumeOutOfRange(volume_db) => {
                write!(f, "volume {volume_db} dB is out of range")
            }
            Self::PanOutOfRange(pan) => write!(
                f,
                "pan {pan} is out of range ({} to {})",
                MixerStrip::MIN_PAN,
                MixerStrip::MAX_PAN
            ),
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
            Self::LoopLengthUnavailable => write!(
                f,
                "set_loop_length only works while the loop is {} to {} bars and the first clip is as long as it; set the loop region and the clips instead",
                Project::MIN_LOOP_BARS,
                Project::MAX_LOOP_BARS
            ),
            Self::LoopOutOfRange { start_bar, bars } => write!(
                f,
                "a loop of {bars} bars from bar {start_bar} is out of range: it needs at least 1 bar and must end in time"
            ),
            Self::SynthParamOutOfRange(param) => match param.range() {
                Some((value, min, max)) => {
                    write!(f, "{param:?}: {value} is out of range ({min} to {max})")
                }
                None => write!(f, "{param:?} is out of range"),
            },
            Self::DrumParamOutOfRange { sound, param } => match KitSettings::range(*sound, param) {
                Some((min, max)) => write!(
                    f,
                    "{sound:?} {param:?}: {} is out of range ({min} to {max})",
                    param.value()
                ),
                None => write!(f, "{sound:?} {param:?} is out of range"),
            },
            Self::NoSuchDrumParam { sound, param } => {
                write!(f, "the {sound:?} has no setting {param:?}")
            }
            Self::NotASynthTrack(id) => write!(f, "track {id} isn't a synth track"),
            Self::NotADrumTrack(id) => write!(f, "track {id} isn't a drum track"),
            Self::UnknownTrack(id) => write!(f, "there's no track {id}"),
            Self::UnknownClip(id) => write!(f, "there's no clip {id}"),
            Self::TrackAlreadyExists(id) => write!(f, "track {id} is already in the project"),
            Self::TrackListedTwice(id) => write!(f, "track {id} is listed more than once"),
            Self::NoTracks => write!(f, "the command has no tracks"),
            Self::TooManyTracks => write!(
                f,
                "a project can have at most {} tracks",
                Project::MAX_TRACKS
            ),
            Self::TrackIndexOutOfRange(index) => {
                write!(f, "track position {index} is past the end")
            }
            Self::TrackIndexListedTwice(index) => {
                write!(f, "two tracks are added at position {index}")
            }
            Self::UnnamedTrack(id) => write!(f, "track {id} has no name"),
            Self::ClipAlreadyExists(id) => write!(f, "clip {id} is already in the project"),
            Self::ClipListedTwice(id) => write!(f, "clip {id} is listed more than once"),
            Self::NoClips => write!(f, "the command has no clips"),
            Self::EmptyClip(id) => write!(f, "clip {id} has no length"),
            Self::ClipTooLate(id) => write!(f, "clip {id} ends too late"),
            Self::UnknownNote(id) => write!(f, "there's no note {id} in the clip"),
            Self::NoteAlreadyExists(id) => write!(f, "note {id} is already in the project"),
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
            Self::ClipToOtherKind { clip, track } => write!(
                f,
                "clip {clip} can't move to track {track}: it's a different kind of track"
            ),
            Self::NoteOffKit { note, pitch } => write!(
                f,
                "note {note}: pitch {pitch} isn't one of the drum kit's notes ({})",
                crate::KIT
                    .iter()
                    .map(|row| format!("{} {}", row.name, row.pitch))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl std::error::Error for CommandError {}

/// The saved form of a command. It's kept apart from [`Command`] so the
/// in-memory type can change while older saved formats still load: a new
/// format gets its own body type here and a conversion to [`Command`].
///
/// Formats 2 to 4 only added commands and a kind of track, so one body type
/// reads every format, and an envelope may only hold the commands and
/// tracks its format had.
mod wire {
    use std::collections::HashSet;

    use serde::{Deserialize, Serialize};

    use super::{COMMAND_FORMAT, ClipPosition, Command, PlacedClip, PlacedTrack};
    use crate::time::Ticks;
    use crate::{
        Clip, ClipId, DrumParam, DrumSound, KitSettings, MixerStrip, Note, NoteId, Source,
        SynthParam, SynthSettings, Track, TrackId,
    };

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
        SetMasterVolume {
            volume_db: f32,
        },
        // Format 2.
        AddNotes {
            clip: ClipId,
            notes: Vec<Note>,
        },
        RemoveNotes {
            clip: ClipId,
            notes: Vec<NoteId>,
        },
        SetNotes {
            clip: ClipId,
            notes: Vec<Note>,
        },
        SetTempo {
            bpm: f32,
        },
        SetLoopLength {
            bars: u32,
        },
        SetSynthParam {
            track: TrackId,
            param: SynthParam,
        },
        // Format 3.
        AddTracks {
            tracks: Vec<TrackAt>,
        },
        RemoveTracks {
            tracks: Vec<TrackId>,
        },
        MoveTrack {
            track: TrackId,
            index: usize,
        },
        SetTrackMixer {
            track: TrackId,
            mixer: MixerStrip,
        },
        AddClips {
            clips: Vec<ClipOn>,
        },
        RemoveClips {
            clips: Vec<ClipId>,
        },
        SetClips {
            clips: Vec<ClipPosition>,
        },
        SetLoop {
            start_bar: u32,
            bars: u32,
        },
        SetLoopEnabled {
            enabled: bool,
        },
        // Format 4.
        SetDrumParam {
            track: TrackId,
            sound: DrumSound,
            param: DrumParam,
        },
    }

    impl Body {
        /// The first format that has this command, and every kind of track
        /// it adds.
        fn since_format(&self) -> u32 {
            match self {
                Self::SetMasterVolume { .. } => 1,
                Self::AddNotes { .. }
                | Self::RemoveNotes { .. }
                | Self::SetNotes { .. }
                | Self::SetTempo { .. }
                | Self::SetLoopLength { .. }
                | Self::SetSynthParam { .. } => 2,
                Self::AddTracks { tracks }
                    if tracks
                        .iter()
                        .any(|at| matches!(at.track.source, SourceBody::Drums(_))) =>
                {
                    4
                }
                Self::SetDrumParam { .. } => 4,
                _ => 3,
            }
        }
    }

    /// A track with its place in the order, as `add_tracks` carries it.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TrackAt {
        index: usize,
        track: TrackBody,
    }

    /// A track, with its clips and their notes. It has no effects yet, so
    /// they aren't saved.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TrackBody {
        id: TrackId,
        name: String,
        source: SourceBody,
        mixer: MixerStrip,
        clips: Vec<ClipBody>,
    }

    /// Serialises as `{"synth":{...}}` or `{"drums":{...}}`.
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case", deny_unknown_fields)]
    enum SourceBody {
        Synth(SynthSettings),
        // Format 4.
        Drums(KitSettings),
    }

    impl From<Source> for SourceBody {
        fn from(source: Source) -> Self {
            match source {
                Source::Synth(settings) => Self::Synth(settings),
                Source::Drums(kit) => Self::Drums(kit),
            }
        }
    }

    impl From<SourceBody> for Source {
        fn from(body: SourceBody) -> Self {
            match body {
                SourceBody::Synth(settings) => Self::Synth(settings),
                SourceBody::Drums(kit) => Self::Drums(kit),
            }
        }
    }

    /// A clip with the track it goes on, as `add_clips` carries it.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ClipOn {
        track: TrackId,
        clip: ClipBody,
    }

    /// A clip with its notes. Its content offset and length are fixed for
    /// now, so they aren't saved: a format that lets them change will add
    /// them.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ClipBody {
        id: ClipId,
        start: Ticks,
        length: Ticks,
        notes: Vec<Note>,
    }

    impl From<Track> for TrackBody {
        fn from(track: Track) -> Self {
            Self {
                id: track.id,
                name: track.name,
                source: track.source.into(),
                mixer: track.mixer,
                clips: track.clips.into_iter().map(ClipBody::from).collect(),
            }
        }
    }

    impl TryFrom<TrackBody> for Track {
        type Error = String;

        fn try_from(body: TrackBody) -> Result<Self, Self::Error> {
            let clips = body
                .clips
                .into_iter()
                .map(Clip::try_from)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Track::new(body.id, body.name, body.source.into())
                .with_mixer(body.mixer)
                .with_clips(clips))
        }
    }

    impl From<Clip> for ClipBody {
        fn from(clip: Clip) -> Self {
            Self {
                id: clip.id,
                start: clip.start,
                length: clip.length,
                notes: clip.notes.values().copied().collect(),
            }
        }
    }

    impl TryFrom<ClipBody> for Clip {
        type Error = String;

        /// Fails if a note is listed twice, which a [`Clip`] can't hold.
        fn try_from(body: ClipBody) -> Result<Self, Self::Error> {
            let mut seen = HashSet::new();
            if let Some(note) = body.notes.iter().find(|note| !seen.insert(note.id)) {
                return Err(format!(
                    "note {} is listed more than once in clip {}",
                    note.id, body.id
                ));
            }
            Ok(Clip::new(body.id, body.start, body.length).with_notes(body.notes))
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
                Command::SetDrumParam {
                    track,
                    sound,
                    param,
                } => Body::SetDrumParam {
                    track,
                    sound,
                    param,
                },
                Command::AddTracks { tracks } => Body::AddTracks {
                    tracks: tracks
                        .into_iter()
                        .map(|placed| TrackAt {
                            index: placed.index,
                            track: placed.track.into(),
                        })
                        .collect(),
                },
                Command::RemoveTracks { tracks } => Body::RemoveTracks { tracks },
                Command::MoveTrack { track, index } => Body::MoveTrack { track, index },
                Command::SetTrackMixer { track, mixer } => Body::SetTrackMixer { track, mixer },
                Command::AddClips { clips } => Body::AddClips {
                    clips: clips
                        .into_iter()
                        .map(|placed| ClipOn {
                            track: placed.track,
                            clip: placed.clip.into(),
                        })
                        .collect(),
                },
                Command::RemoveClips { clips } => Body::RemoveClips { clips },
                Command::SetClips { clips } => Body::SetClips { clips },
                Command::SetLoop { start_bar, bars } => Body::SetLoop { start_bar, bars },
                Command::SetLoopEnabled { enabled } => Body::SetLoopEnabled { enabled },
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
                Body::SetDrumParam {
                    track,
                    sound,
                    param,
                } => Command::SetDrumParam {
                    track,
                    sound,
                    param,
                },
                Body::AddTracks { tracks } => Command::AddTracks {
                    tracks: tracks
                        .into_iter()
                        .map(|at| {
                            Ok(PlacedTrack {
                                index: at.index,
                                track: at.track.try_into()?,
                            })
                        })
                        .collect::<Result<_, String>>()?,
                },
                Body::RemoveTracks { tracks } => Command::RemoveTracks { tracks },
                Body::MoveTrack { track, index } => Command::MoveTrack { track, index },
                Body::SetTrackMixer { track, mixer } => Command::SetTrackMixer { track, mixer },
                Body::AddClips { clips } => Command::AddClips {
                    clips: clips
                        .into_iter()
                        .map(|on| {
                            Ok(PlacedClip {
                                track: on.track,
                                clip: on.clip.try_into()?,
                            })
                        })
                        .collect::<Result<_, String>>()?,
                },
                Body::RemoveClips { clips } => Command::RemoveClips { clips },
                Body::SetClips { clips } => Command::SetClips { clips },
                Body::SetLoop { start_bar, bars } => Command::SetLoop { start_bar, bars },
                Body::SetLoopEnabled { enabled } => Command::SetLoopEnabled { enabled },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, note, note_id};
    use crate::{DrumParam, DrumSound, KitSettings, Source, SynthSettings, Waveform, time::Ticks};
    use proptest::prelude::*;

    fn clip() -> ClipId {
        testing::project().tracks()[0].clips()[0].id()
    }

    fn track() -> TrackId {
        testing::project().tracks()[0].id()
    }

    /// Whether a command holds a NaN or infinity, which JSON can't carry.
    fn has_non_finite(command: &Command) -> bool {
        let param = |param: &SynthParam| {
            param
                .range()
                .is_some_and(|(value, _, _)| !value.is_finite())
        };
        let mixer = |mixer: &MixerStrip| !mixer.volume_db.is_finite() || !mixer.pan.is_finite();
        match command {
            Command::SetMasterVolume { volume_db } => !volume_db.is_finite(),
            Command::SetTempo { bpm } => !bpm.is_finite(),
            Command::SetSynthParam { param: p, .. } => param(p),
            Command::SetTrackMixer { mixer: m, .. } => mixer(m),
            Command::SetDrumParam { param, .. } => !param.value().is_finite(),
            Command::AddTracks { tracks } => tracks.iter().any(|placed| {
                mixer(placed.track.mixer())
                    || match placed.track.source() {
                        Source::Synth(settings) => settings.params().iter().any(param),
                        Source::Drums(kit) => kit
                            .params()
                            .iter()
                            .any(|(_, param)| !param.value().is_finite()),
                    }
            }),
            _ => false,
        }
    }

    #[test]
    fn serialises_with_its_format_version() {
        let json = serde_json::to_string(&Command::SetMasterVolume { volume_db: -6.0 }).unwrap();
        assert_eq!(
            json,
            r#"{"format":4,"command":{"type":"set_master_volume","volume_db":-6.0}}"#
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
            let json = format!(r#"{{"format":4,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // Format 2 and 3 lists still load.
            for format in [2, 3] {
                let json = format!(r#"{{"format":{format},"command":{body}}}"#);
                assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            }
        }
    }

    #[test]
    fn the_format_3_commands_serialise_like_this() {
        let track = track();
        let clip = clip();
        let note = note(0, 60, 960);
        let note_json = format!(
            r#"{{"id":"{}","pitch":60,"velocity":100,"start":960,"length":480}}"#,
            note.id
        );
        let new_track = testing::track_id(0);
        let new_clip = testing::clip_id(0);
        let mixer = MixerStrip {
            volume_db: -3.5,
            pan: 0.25,
            mute: false,
            solo: true,
        };
        let mixer_json = r#"{"volume_db":-3.5,"pan":0.25,"mute":false,"solo":true}"#;
        let clip_json =
            format!(r#"{{"id":"{new_clip}","start":7680,"length":3840,"notes":[{note_json}]}}"#);
        let settings_json = r#"{"waveform":"saw","cutoff_hz":20000.0,"resonance":0.0,"attack_seconds":0.005,"decay_seconds":0.2,"sustain":0.7,"release_seconds":0.2}"#;
        let cases = [
            (
                Command::AddTracks {
                    tracks: vec![PlacedTrack {
                        index: 1,
                        track: Track::new(
                            new_track,
                            "Synth 2",
                            Source::Synth(SynthSettings::default()),
                        )
                        .with_mixer(mixer)
                        .with_clips([Clip::new(new_clip, 7680, 3840).with_notes([note])]),
                    }],
                },
                format!(
                    r#"{{"type":"add_tracks","tracks":[{{"index":1,"track":{{"id":"{new_track}","name":"Synth 2","source":{{"synth":{settings_json}}},"mixer":{mixer_json},"clips":[{clip_json}]}}}}]}}"#
                ),
            ),
            (
                Command::RemoveTracks {
                    tracks: vec![track],
                },
                format!(r#"{{"type":"remove_tracks","tracks":["{track}"]}}"#),
            ),
            (
                Command::MoveTrack { track, index: 2 },
                format!(r#"{{"type":"move_track","track":"{track}","index":2}}"#),
            ),
            (
                Command::SetTrackMixer { track, mixer },
                format!(r#"{{"type":"set_track_mixer","track":"{track}","mixer":{mixer_json}}}"#),
            ),
            (
                Command::AddClips {
                    clips: vec![PlacedClip {
                        track,
                        clip: Clip::new(new_clip, 7680, 3840).with_notes([note]),
                    }],
                },
                format!(
                    r#"{{"type":"add_clips","clips":[{{"track":"{track}","clip":{clip_json}}}]}}"#
                ),
            ),
            (
                Command::RemoveClips { clips: vec![clip] },
                format!(r#"{{"type":"remove_clips","clips":["{clip}"]}}"#),
            ),
            (
                Command::SetClips {
                    clips: vec![ClipPosition {
                        id: clip,
                        track,
                        start: 3840,
                        length: 960,
                    }],
                },
                format!(
                    r#"{{"type":"set_clips","clips":[{{"id":"{clip}","track":"{track}","start":3840,"length":960}}]}}"#
                ),
            ),
            (
                Command::SetLoop {
                    start_bar: 2,
                    bars: 8,
                },
                r#"{"type":"set_loop","start_bar":2,"bars":8}"#.to_string(),
            ),
            (
                Command::SetLoopEnabled { enabled: false },
                r#"{"type":"set_loop_enabled","enabled":false}"#.to_string(),
            ),
        ];
        for (command, body) in cases {
            let json = format!(r#"{{"format":4,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // Format 3 lists still load.
            let json = format!(r#"{{"format":3,"command":{body}}}"#);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // They're new in format 3.
            let json = format!(r#"{{"format":2,"command":{body}}}"#);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(error.to_string().contains("arrived in format 3"), "{error}");
        }
    }

    #[test]
    fn the_format_4_commands_serialise_like_this() {
        let track = track();
        let new_track = testing::track_id(0);
        let new_clip = testing::clip_id(0);
        let kick = Note {
            pitch: 36,
            ..note(0, 36, 0)
        };
        let kit_json = concat!(
            r#"{"kick":{"tune_hz":49.0,"tone":0.2,"decay_seconds":0.3,"level_db":0.0},"#,
            r#""snare":{"tune_hz":180.0,"tone_seconds":0.16,"snappy":0.5,"level_db":0.0},"#,
            r#""clap":{"tone_hz":1000.0,"decay_seconds":0.2,"level_db":0.0},"#,
            r#""closed_hat":{"tune_hz":205.3,"tone_hz":7100.0,"decay_seconds":0.05,"level_db":0.0},"#,
            r#""open_hat":{"decay_seconds":0.35,"level_db":0.0}}"#,
        );
        let cases = [
            (
                Command::SetDrumParam {
                    track,
                    sound: DrumSound::Kick,
                    param: DrumParam::TuneHz(55.0),
                },
                format!(
                    r#"{{"type":"set_drum_param","track":"{track}","sound":"kick","param":{{"name":"tune_hz","value":55.0}}}}"#
                ),
            ),
            (
                Command::AddTracks {
                    tracks: vec![PlacedTrack {
                        index: 1,
                        track: Track::new(
                            new_track,
                            "Drums 1",
                            Source::Drums(KitSettings::default()),
                        )
                        .with_clips([Clip::new(new_clip, 0, 3840).with_notes([kick])]),
                    }],
                },
                format!(
                    r#"{{"type":"add_tracks","tracks":[{{"index":1,"track":{{"id":"{new_track}","name":"Drums 1","source":{{"drums":{kit_json}}},"mixer":{{"volume_db":0.0,"pan":0.0,"mute":false,"solo":false}},"clips":[{{"id":"{new_clip}","start":0,"length":3840,"notes":[{{"id":"{}","pitch":36,"velocity":100,"start":0,"length":480}}]}}]}}}}]}}"#,
                    kick.id
                ),
            ),
        ];
        for (command, body) in cases {
            let json = format!(r#"{{"format":4,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // Drums are new in format 4, even in a format 3 command.
            let json = format!(r#"{{"format":3,"command":{body}}}"#);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(error.to_string().contains("arrived in format 4"), "{error}");
        }
    }

    #[test]
    fn set_drum_param_continues_only_with_the_same_sound_and_setting() {
        let track = track();
        let set = |sound, param| Command::SetDrumParam {
            track,
            sound,
            param,
        };
        let tune = set(DrumSound::Kick, DrumParam::TuneHz(50.0));
        assert!(tune.sets_same_as(&set(DrumSound::Kick, DrumParam::TuneHz(60.0))));
        assert!(!tune.sets_same_as(&set(DrumSound::Kick, DrumParam::Tone(0.2))));
        assert!(!tune.sets_same_as(&set(DrumSound::LowTom, DrumParam::TuneHz(90.0))));
        assert!(!tune.sets_same_as(&Command::SetDrumParam {
            track: TrackId::random(),
            sound: DrumSound::Kick,
            param: DrumParam::TuneHz(60.0),
        }));
    }

    #[test]
    fn rejects_a_clip_that_lists_a_note_twice() {
        let track = track();
        let clip = testing::clip_id(0);
        let note_json = format!(
            r#"{{"id":"{}","pitch":60,"velocity":100,"start":0,"length":480}}"#,
            note_id(0)
        );
        let json = format!(
            r#"{{"format":3,"command":{{"type":"add_clips","clips":[{{"track":"{track}","clip":{{"id":"{clip}","start":0,"length":3840,"notes":[{note_json},{note_json}]}}}}]}}}}"#
        );
        let error = serde_json::from_str::<Command>(&json).unwrap_err();
        assert!(
            error.to_string().contains("listed more than once"),
            "{error}"
        );
    }

    #[test]
    fn set_clips_and_set_track_mixer_continue_only_with_the_same_things() {
        let track = track();
        let position = |id, start| ClipPosition {
            id,
            track,
            start,
            length: 3840,
        };
        let (a, b) = (testing::clip_id(0), testing::clip_id(1));
        let set = |clips: Vec<ClipPosition>| Command::SetClips { clips };
        let first = set(vec![position(a, 0), position(b, 0)]);
        assert!(first.sets_same_as(&set(vec![position(b, 960), position(a, 960)])));
        assert!(!first.sets_same_as(&set(vec![position(a, 960)])));

        let mixer = |track, volume_db| Command::SetTrackMixer {
            track,
            mixer: MixerStrip {
                volume_db,
                ..MixerStrip::default()
            },
        };
        assert!(mixer(track, 0.0).sets_same_as(&mixer(track, -6.0)));
        assert!(!mixer(track, 0.0).sets_same_as(&mixer(TrackId::random(), -6.0)));

        let region = |start_bar| Command::SetLoop { start_bar, bars: 4 };
        assert!(region(0).sets_same_as(&region(2)));
        let enabled = Command::SetLoopEnabled { enabled: true };
        assert!(
            !enabled.sets_same_as(&enabled.clone()),
            "a switch isn't a drag"
        );
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
        for format in [0, 5, 99] {
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
