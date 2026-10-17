//! Commands: each one is a single, saveable change to a project.

use serde::{Deserialize, Serialize};

use crate::time::Ticks;
use crate::{
    Chain, Clip, ClipId, DelayParam, DrumParam, DrumSound, Effect, EffectId, MixerStrip, Note,
    NoteId, SynthParam, SynthSettings, Track, TrackId,
};

/// The command format written by this version of Uta. Bump it when a saved
/// command's shape changes, and teach [`Command`]'s deserialisation to read
/// the older formats.
///
/// - Format 1: `set_master_volume`.
/// - Format 2 adds the notes, tempo, loop and synth commands (RFC-002).
/// - Format 3 adds the track, clip and loop region commands (RFC-003).
/// - Format 4 adds drum tracks and `set_drum_param` (RFC-006).
/// - Format 5 adds effect chains: `add_effects`, `remove_effects`,
///   `set_effect_mix`, `set_delay_param`, and a track's effects in
///   `add_tracks` (RFC-007).
/// - Format 6 adds the synth's second oscillator, sub, noise, mixer levels
///   and Osc retrigger, and renames `waveform` to `osc1_wave` and `square`
///   to `pulse` (RFC-008). Older synth settings still read: those they
///   didn't have take their defaults, and the old names read as the new.
/// - Format 7 adds `move_effect`, `set_effect_enabled` and the master's
///   chain (RFC-007).
/// - Format 8 adds the synth's filter mode, filter envelope and key
///   tracking (RFC-008). Older synth settings still read, with those they
///   didn't have at their defaults.
/// - Format 9 adds the Delay's tape loop: its Sync, synced time (a note
///   division), Mode, Low cut, High cut, Drive and Wobble, and Feedback up
///   to 110% (RFC-007). A Delay from an older format reads as the plain
///   Delay it was: unsynced, with its cuts wide open and no Drive or
///   Wobble.
/// - Format 10 adds oscillator 1's Width and oscillator 2's Sync
///   (RFC-008).
/// - Format 11 adds the synth's LFO: its Shape, Rate, Sync, synced rate (a
///   note division), Retrigger, Vibrato and Width, and the filter's LFO
///   amount (RFC-008). Older synth settings still read, with the LFO
///   moving nothing.
/// - Format 12 adds the synth's Voice mode (Poly or Mono), Legato and Glide
///   (RFC-008).
/// - Format 13 adds the synth's three mod slots: each a Source, a
///   Destination and an Amount (RFC-008). Older synth settings still read,
///   with the slots empty.
/// - Format 14 adds `set_synth_settings`, which sets every synth setting at
///   once, as choosing a factory sound does (RFC-008).
pub const COMMAND_FORMAT: u32 = 14;

/// One change to a project.
///
/// Commands serialise with their format version, as
/// `{"format":14,"command":{"type":"set_master_volume","volume_db":-6.0}}`.
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
    /// Set every one of a synth track's settings at once, as choosing a
    /// factory sound does. Its inverse puts them all back.
    SetSynthSettings {
        track: TrackId,
        settings: SynthSettings,
    },
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
    /// Add effects to chains, each at its place in its chain. The inverse
    /// of [`Command::RemoveEffects`]. A chain holds at most
    /// [`crate::MAX_EFFECTS`].
    AddEffects { effects: Vec<PlacedEffect> },
    /// Remove effects from whichever chains they're in, with their
    /// settings. The inverse of [`Command::AddEffects`].
    RemoveEffects { effects: Vec<EffectId> },
    /// Move an effect to `index` in its own chain, counting from 0.
    MoveEffect { effect: EffectId, index: usize },
    /// Switch an effect on or off. Off, the sound passes through it
    /// unchanged.
    SetEffectEnabled { effect: EffectId, enabled: bool },
    /// Set how much of an effect you hear, from 0 to 1.
    SetEffectMix { effect: EffectId, mix: f32 },
    /// Set one of a Delay's settings.
    SetDelayParam { effect: EffectId, param: DelayParam },
}

/// A track to add, and where it goes: its index in the order once it's
/// added, counting from 0.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedTrack {
    pub index: usize,
    pub track: Track,
}

/// An effect to add, the chain it goes in, and its place there: its index
/// in the chain once it's added, counting from 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedEffect {
    pub chain: Chain,
    pub index: usize,
    pub effect: Effect,
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
            (
                Self::SetEffectMix { effect, .. },
                Self::SetEffectMix {
                    effect: other_effect,
                    ..
                },
            ) => effect == other_effect,
            (
                Self::SetDelayParam { effect, param },
                Self::SetDelayParam {
                    effect: other_effect,
                    param: other_param,
                },
            ) => effect == other_effect && param.same_setting(other_param),
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
    /// The limits are the ones it has on that sound (on the kick, its chosen
    /// model's).
    DrumParamOutOfRange {
        sound: DrumSound,
        param: DrumParam,
        min: f32,
        max: f32,
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
    UnknownEffect(EffectId),
    /// An effect to add has the same ID as one already in the project, in
    /// any chain.
    EffectAlreadyExists(EffectId),
    /// The same effect appears more than once in one command.
    EffectListedTwice(EffectId),
    /// An effects command with no effects in it.
    NoEffects,
    /// Adding the effects would take a chain past [`crate::MAX_EFFECTS`].
    ChainFull(Chain),
    /// A place in a chain past its end.
    EffectIndexOutOfRange(usize),
    /// Two effects to add at the same place in one chain.
    EffectIndexListedTwice(usize),
    /// An effect's Mix was outside 0 to 1, or not a number.
    EffectMixOutOfRange(f32),
    /// A Delay setting was outside its range, or not a number.
    DelayParamOutOfRange(DelayParam),
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
            Self::DrumParamOutOfRange {
                sound,
                param,
                min,
                max,
            } => write!(f, "{sound:?} {param:?} is out of range ({min} to {max})"),
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
            Self::UnknownEffect(id) => write!(f, "there's no effect {id}"),
            Self::EffectAlreadyExists(id) => write!(f, "effect {id} is already in the project"),
            Self::EffectListedTwice(id) => write!(f, "effect {id} is listed more than once"),
            Self::NoEffects => write!(f, "the command has no effects"),
            Self::ChainFull(Chain::Track(id)) => write!(
                f,
                "track {id}'s chain can hold at most {} effects",
                crate::MAX_EFFECTS
            ),
            Self::ChainFull(Chain::Master) => write!(
                f,
                "the master's chain can hold at most {} effects",
                crate::MAX_EFFECTS
            ),
            Self::EffectIndexOutOfRange(index) => {
                write!(f, "effect position {index} is past the end of its chain")
            }
            Self::EffectIndexListedTwice(index) => {
                write!(f, "two effects are added at position {index} in one chain")
            }
            Self::EffectMixOutOfRange(mix) => write!(
                f,
                "mix {mix} is out of range ({} to {})",
                Effect::MIN_MIX,
                Effect::MAX_MIX
            ),
            Self::DelayParamOutOfRange(param) => match param.range() {
                Some((value, min, max)) => {
                    write!(f, "{param:?}: {value} is out of range ({min} to {max})")
                }
                None => write!(f, "{param:?} is out of range"),
            },
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
/// Formats 2 to 6 only added commands, a kind of track, a track's effects
/// and the master's chain, so one body type reads every format, and an
/// envelope may only hold the commands, tracks and chains its format had.
mod wire {
    use std::collections::HashSet;

    use serde::{Deserialize, Serialize};

    use super::{COMMAND_FORMAT, ClipPosition, Command, PlacedClip, PlacedEffect, PlacedTrack};
    use crate::time::Ticks;
    use crate::{
        Chain, Clip, ClipId, DelayParam, DrumParam, DrumSound, Effect, EffectId, EffectSettings,
        KitSettings, MixerStrip, Note, NoteId, Source, SynthParam, SynthSettings, Track, TrackId,
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
        // Format 5.
        AddEffects {
            effects: Vec<EffectAt>,
        },
        RemoveEffects {
            effects: Vec<EffectId>,
        },
        SetEffectMix {
            effect: EffectId,
            mix: f32,
        },
        SetDelayParam {
            effect: EffectId,
            param: DelayParam,
        },
        // Format 6.
        MoveEffect {
            effect: EffectId,
            index: usize,
        },
        SetEffectEnabled {
            effect: EffectId,
            enabled: bool,
        },
        // Format 14.
        SetSynthSettings {
            track: TrackId,
            settings: SynthSettings,
        },
    }

    impl Body {
        /// The first format that has this command, and every kind of track
        /// it adds.
        fn since_format(&self) -> u32 {
            match self {
                Self::SetMasterVolume { .. } => 1,
                Self::SetDelayParam { param, .. } => param.since_format(),
                Self::SetSynthSettings { .. } => 14,
                Self::MoveEffect { .. } | Self::SetEffectEnabled { .. } => 7,
                Self::AddEffects { effects }
                    if effects.iter().any(|at| at.chain == Chain::Master) =>
                {
                    7
                }
                Self::AddNotes { .. }
                | Self::RemoveNotes { .. }
                | Self::SetNotes { .. }
                | Self::SetTempo { .. }
                | Self::SetLoopLength { .. } => 2,
                Self::SetSynthParam { param, .. } => param.since_format(),
                Self::AddTracks { tracks }
                    if tracks.iter().any(|at| !at.track.effects.is_empty()) =>
                {
                    5
                }
                Self::AddEffects { .. }
                | Self::RemoveEffects { .. }
                | Self::SetEffectMix { .. } => 5,
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

    /// A track, with its chain, its clips and their notes. A track with no
    /// effects leaves them out, as it was before format 5.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TrackBody {
        id: TrackId,
        name: String,
        source: SourceBody,
        // Format 5.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        effects: Vec<EffectBody>,
        mixer: MixerStrip,
        clips: Vec<ClipBody>,
    }

    /// An effect with its chain and place, as `add_effects` carries it.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EffectAt {
        chain: Chain,
        index: usize,
        effect: EffectBody,
    }

    /// An effect: its ID, its settings (which say its kind), whether it's
    /// on, and its Mix.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EffectBody {
        id: EffectId,
        settings: EffectSettings,
        enabled: bool,
        mix: f32,
    }

    impl From<Effect> for EffectBody {
        fn from(effect: Effect) -> Self {
            Self {
                id: effect.id,
                settings: effect.settings,
                enabled: effect.enabled,
                mix: effect.mix,
            }
        }
    }

    impl From<EffectBody> for Effect {
        fn from(body: EffectBody) -> Self {
            Effect::new(body.id, body.settings)
                .with_enabled(body.enabled)
                .with_mix(body.mix)
        }
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
                effects: track.effects.into_iter().map(EffectBody::from).collect(),
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
                .with_effects(body.effects.into_iter().map(Effect::from))
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
                Command::SetSynthSettings { track, settings } => {
                    Body::SetSynthSettings { track, settings }
                }
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
                Command::AddEffects { effects } => Body::AddEffects {
                    effects: effects
                        .into_iter()
                        .map(|placed| EffectAt {
                            chain: placed.chain,
                            index: placed.index,
                            effect: placed.effect.into(),
                        })
                        .collect(),
                },
                Command::RemoveEffects { effects } => Body::RemoveEffects { effects },
                Command::MoveEffect { effect, index } => Body::MoveEffect { effect, index },
                Command::SetEffectEnabled { effect, enabled } => {
                    Body::SetEffectEnabled { effect, enabled }
                }
                Command::SetEffectMix { effect, mix } => Body::SetEffectMix { effect, mix },
                Command::SetDelayParam { effect, param } => Body::SetDelayParam { effect, param },
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
                Body::SetSynthSettings { track, settings } => {
                    Command::SetSynthSettings { track, settings }
                }
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
                Body::AddEffects { effects } => Command::AddEffects {
                    effects: effects
                        .into_iter()
                        .map(|at| PlacedEffect {
                            chain: at.chain,
                            index: at.index,
                            effect: at.effect.into(),
                        })
                        .collect(),
                },
                Body::RemoveEffects { effects } => Command::RemoveEffects { effects },
                Body::MoveEffect { effect, index } => Command::MoveEffect { effect, index },
                Body::SetEffectEnabled { effect, enabled } => {
                    Command::SetEffectEnabled { effect, enabled }
                }
                Body::SetEffectMix { effect, mix } => Command::SetEffectMix { effect, mix },
                Body::SetDelayParam { effect, param } => Command::SetDelayParam { effect, param },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, note, note_id};
    use crate::time::NoteDivision;
    use crate::{
        DrumParam, DrumSound, KickModel, KitSettings, Source, SynthSettings, VoiceMode, Waveform,
        time::Ticks,
    };
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
        let effect = |effect: &Effect| {
            !effect.mix().is_finite()
                || match effect.settings() {
                    crate::EffectSettings::Delay(settings) => {
                        settings.params().iter().any(|param| {
                            param
                                .range()
                                .is_some_and(|(value, _, _)| !value.is_finite())
                        })
                    }
                }
        };
        match command {
            Command::SetMasterVolume { volume_db } => !volume_db.is_finite(),
            Command::SetTempo { bpm } => !bpm.is_finite(),
            Command::SetSynthParam { param: p, .. } => param(p),
            Command::SetSynthSettings { settings, .. } => settings.params().iter().any(param),
            Command::SetTrackMixer { mixer: m, .. } => mixer(m),
            Command::SetDrumParam { param, .. } => param.value().is_some_and(|v| !v.is_finite()),
            Command::SetEffectMix { mix, .. } => !mix.is_finite(),
            Command::SetDelayParam { param, .. } => param
                .range()
                .is_some_and(|(value, _, _)| !value.is_finite()),
            Command::AddEffects { effects } => effects.iter().any(|placed| effect(&placed.effect)),
            Command::AddTracks { tracks } => tracks.iter().any(|placed| {
                mixer(placed.track.mixer())
                    || placed.track.effects().iter().any(effect)
                    || match placed.track.source() {
                        Source::Synth(settings) => settings.params().iter().any(param),
                        Source::Drums(kit) => KickModel::ALL.iter().any(|&model| {
                            kit.with_kick_model(model)
                                .params()
                                .iter()
                                .any(|(_, param)| param.value().is_some_and(|v| !v.is_finite()))
                        }),
                    }
            }),
            _ => false,
        }
    }

    /// The plain Delay's settings, as today's format writes them.
    const PLAIN_DELAY_JSON: &str = r#"{"sync":false,"division":"1/8d","time_seconds":0.375,"mode":"stereo","feedback":0.4,"low_cut_hz":20.0,"high_cut_hz":20000.0,"drive":0.0,"wobble":0.0}"#;
    /// A new Delay's settings.
    const DEFAULT_DELAY_JSON: &str = r#"{"sync":true,"division":"1/8d","time_seconds":0.375,"mode":"stereo","feedback":0.4,"low_cut_hz":100.0,"high_cut_hz":8000.0,"drive":0.2,"wobble":0.1}"#;
    /// A new track's synth settings, as `add_tracks` carries them.
    const DEFAULT_SYNTH_JSON: &str = concat!(
        r#"{"osc1_wave":"saw","osc1_octave":0,"osc1_width":0.5,"osc2_wave":"saw","osc2_octave":0,"#,
        r#""osc2_pitch_semitones":0.0,"osc2_sync":false,"sub_octave":-1,"osc1_level":1.0,"#,
        r#""osc2_level":0.0,"sub_level":0.0,"noise_level":0.0,"filter_mode":"low_pass","#,
        r#""cutoff_hz":20000.0,"resonance":0.0,"filter_env_octaves":0.0,"filter_key":0.0,"#,
        r#""filter_lfo_octaves":0.0,"#,
        r#""filter_attack_seconds":0.001,"filter_decay_seconds":0.3,"filter_sustain":0.0,"#,
        r#""filter_release_seconds":0.2,"attack_seconds":0.005,"decay_seconds":0.2,"sustain":0.7,"release_seconds":0.2,"#,
        r#""lfo_shape":"sine","lfo_rate_hz":5.0,"lfo_sync":false,"lfo_division":"1/4","#,
        r#""lfo_retrigger":false,"lfo_vibrato_semitones":0.0,"lfo_width":0.0,"#,
        r#""voice_mode":"poly","legato":false,"glide_seconds":0.001,"osc_retrigger":true,"#,
        r#""mod1_source":"velocity","mod1_destination":"off","mod1_amount":0.0,"#,
        r#""mod2_source":"filter_env","mod2_destination":"off","mod2_amount":0.0,"#,
        r#""mod3_source":"lfo","mod3_destination":"off","mod3_amount":0.0}"#,
    );

    #[test]
    fn serialises_with_its_format_version() {
        let json = serde_json::to_string(&Command::SetMasterVolume { volume_db: -6.0 }).unwrap();
        assert_eq!(
            json,
            r#"{"format":14,"command":{"type":"set_master_volume","volume_db":-6.0}}"#
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
                    param: SynthParam::Osc1Wave(Waveform::Pulse),
                },
                format!(
                    r#"{{"type":"set_synth_param","track":"{track}","param":{{"name":"osc1_wave","value":"pulse"}}}}"#
                ),
            ),
        ];
        for (command, body) in cases {
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // Format 2 to 6 lists still load.
            for format in [2, 3, 4, 5, 6] {
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
        let settings_json = DEFAULT_SYNTH_JSON;
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
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // Format 3 to 6 lists still load.
            for format in [3, 4, 5, 6] {
                let json = format!(r#"{{"format":{format},"command":{body}}}"#);
                assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            }
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
            r#"{"kick":{"model":"808","tune_hz":49.0,"tone":0.2,"decay_seconds":0.3,"level_db":0.0,"#,
            r#""909":{"tune_hz":55.0,"sweep":0.4,"attack":0.5,"decay_seconds":0.5,"level_db":-2.0}},"#,
            r#""snare":{"tune_hz":180.0,"tone_seconds":0.16,"snappy":0.5,"level_db":-2.0},"#,
            r#""clap":{"tone_hz":1000.0,"decay_seconds":0.2,"level_db":2.5},"#,
            r#""closed_hat":{"tune_hz":205.3,"tone_hz":7100.0,"decay_seconds":0.05,"level_db":-1.0},"#,
            r#""open_hat":{"decay_seconds":0.35,"level_db":-9.5},"#,
            r#""low_tom":{"tune_hz":90.0,"decay_seconds":0.2,"level_db":-4.5},"#,
            r#""high_tom":{"tune_hz":185.0,"decay_seconds":0.1,"level_db":-3.0},"#,
            r#""cymbal":{"tone":0.5,"decay_seconds":0.8,"level_db":-12.0}}"#,
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
                Command::SetDrumParam {
                    track,
                    sound: DrumSound::Kick,
                    param: DrumParam::Model(KickModel::Tr909),
                },
                format!(
                    r#"{{"type":"set_drum_param","track":"{track}","sound":"kick","param":{{"name":"model","value":"909"}}}}"#
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
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // Format 4 to 6 lists still load.
            for format in [4, 5, 6] {
                let json = format!(r#"{{"format":{format},"command":{body}}}"#);
                assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            }
            // Drums are new in format 4, even in a format 3 command.
            let json = format!(r#"{{"format":3,"command":{body}}}"#);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(error.to_string().contains("arrived in format 4"), "{error}");
        }
    }

    /// Format 4's and 5's synth commands convert: `waveform` is oscillator
    /// 1's wave, `square` is a pulse, and a track's settings that they didn't
    /// have take their defaults, which play today's sound. A setting new in
    /// format 6 is refused in an older `set_synth_param`.
    #[test]
    fn older_synth_commands_convert() {
        let track = track();
        let new_track = testing::track_id(0);
        let settings_json = r#"{"waveform":"square","cutoff_hz":800.0,"resonance":0.0,"attack_seconds":0.005,"decay_seconds":0.2,"sustain":0.7,"release_seconds":0.2}"#;
        for format in [4, 5] {
            let json = format!(
                r#"{{"format":{format},"command":{{"type":"set_synth_param","track":"{track}","param":{{"name":"waveform","value":"square"}}}}}}"#
            );
            assert_eq!(
                serde_json::from_str::<Command>(&json).unwrap(),
                Command::SetSynthParam {
                    track,
                    param: SynthParam::Osc1Wave(Waveform::Pulse),
                }
            );

            let json = format!(
                r#"{{"format":{format},"command":{{"type":"add_tracks","tracks":[{{"index":1,"track":{{"id":"{new_track}","name":"Synth 2","source":{{"synth":{settings_json}}},"mixer":{{"volume_db":0.0,"pan":0.0,"mute":false,"solo":false}},"clips":[]}}}}]}}}}"#
            );
            let Command::AddTracks { tracks } = serde_json::from_str::<Command>(&json).unwrap()
            else {
                panic!("an add_tracks command");
            };
            assert_eq!(
                tracks[0].track.source(),
                &Source::Synth(SynthSettings {
                    osc1_wave: Waveform::Pulse,
                    cutoff_hz: 800.0,
                    ..SynthSettings::default()
                })
            );

            for param in [
                r#"{"name":"osc2_level","value":0.5}"#,
                r#"{"name":"osc_retrigger","value":false}"#,
            ] {
                let json = format!(
                    r#"{{"format":{format},"command":{{"type":"set_synth_param","track":"{track}","param":{param}}}}}"#
                );
                let error = serde_json::from_str::<Command>(&json).unwrap_err();
                assert!(error.to_string().contains("arrived in format 6"), "{error}");
                let json = json.replace(&format!(r#""format":{format}"#), r#""format":6"#);
                assert!(serde_json::from_str::<Command>(&json).is_ok());
            }
        }
    }

    /// Format 7's synth settings, which have no filter mode, filter
    /// envelope or key tracking, read with those at their defaults: an
    /// unmoved low-pass, today's sound. The filter's new settings are
    /// refused in an older `set_synth_param`.
    #[test]
    fn format_7_synth_settings_read_with_the_filter_at_its_defaults() {
        let track = track();
        let new_track = testing::track_id(0);
        let settings_json = DEFAULT_SYNTH_JSON
            .split(',')
            .filter(|field| !field.contains("filter_"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(!settings_json.contains("filter"), "{settings_json}");
        let json = format!(
            r#"{{"format":7,"command":{{"type":"add_tracks","tracks":[{{"index":1,"track":{{"id":"{new_track}","name":"Synth 2","source":{{"synth":{settings_json}}},"mixer":{{"volume_db":0.0,"pan":0.0,"mute":false,"solo":false}},"clips":[]}}}}]}}}}"#
        );
        let Command::AddTracks { tracks } = serde_json::from_str::<Command>(&json).unwrap() else {
            panic!("an add_tracks command");
        };
        assert_eq!(
            tracks[0].track.source(),
            &Source::Synth(SynthSettings::default())
        );

        for param in [
            r#"{"name":"filter_mode","value":"high_pass"}"#,
            r#"{"name":"filter_env_octaves","value":3.0}"#,
            r#"{"name":"filter_key","value":1.0}"#,
            r#"{"name":"filter_attack_seconds","value":0.1}"#,
            r#"{"name":"filter_decay_seconds","value":0.1}"#,
            r#"{"name":"filter_sustain","value":0.5}"#,
            r#"{"name":"filter_release_seconds","value":0.1}"#,
        ] {
            let json = format!(
                r#"{{"format":7,"command":{{"type":"set_synth_param","track":"{track}","param":{param}}}}}"#
            );
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(error.to_string().contains("arrived in format 8"), "{error}");
            let json = json.replace(r#""format":7"#, r#""format":8"#);
            assert!(serde_json::from_str::<Command>(&json).is_ok(), "{param}");
        }
    }

    /// Settings new in a format: an older `set_synth_param` that sets one is
    /// refused. Oscillator 1's Width and oscillator 2's Sync are new in
    /// format 10; the Voice mode, Legato and Glide in format 12; the mod
    /// slots in format 13.
    #[test]
    fn the_format_10_12_and_13_synth_settings_serialise_like_this() {
        let track = track();
        let cases = [
            (
                SynthParam::Osc1Width(0.25),
                r#"{"name":"osc1_width","value":0.25}"#,
                10,
            ),
            (
                SynthParam::Osc2Sync(true),
                r#"{"name":"osc2_sync","value":true}"#,
                10,
            ),
            (
                SynthParam::VoiceMode(VoiceMode::Mono),
                r#"{"name":"voice_mode","value":"mono"}"#,
                12,
            ),
            (
                SynthParam::Legato(true),
                r#"{"name":"legato","value":true}"#,
                12,
            ),
            (
                SynthParam::GlideSeconds(0.25),
                r#"{"name":"glide_seconds","value":0.25}"#,
                12,
            ),
            (
                SynthParam::Mod1Source(crate::ModSource::Key),
                r#"{"name":"mod1_source","value":"key"}"#,
                13,
            ),
            (
                SynthParam::Mod2Destination(crate::ModDestination::Osc2Pitch),
                r#"{"name":"mod2_destination","value":"osc2_pitch"}"#,
                13,
            ),
            (
                SynthParam::Mod3Amount(-0.5),
                r#"{"name":"mod3_amount","value":-0.5}"#,
                13,
            ),
        ];
        for (param, param_json, since) in cases {
            let command = Command::SetSynthParam { track, param };
            let body =
                format!(r#"{{"type":"set_synth_param","track":"{track}","param":{param_json}}}"#);
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            let json = format!(r#"{{"format":{since},"command":{body}}}"#);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            let json = format!(r#"{{"format":{},"command":{body}}}"#, since - 1);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&format!("arrived in format {since}")),
                "{error}"
            );
        }
    }

    /// The LFO's settings are new in format 11: an older `set_synth_param`
    /// that sets one is refused.
    #[test]
    fn the_format_11_synth_settings_serialise_like_this() {
        let track = track();
        let cases = [
            (
                SynthParam::FilterLfoOctaves(-1.5),
                r#"{"name":"filter_lfo_octaves","value":-1.5}"#,
            ),
            (
                SynthParam::LfoShape(crate::LfoShape::SampleAndHold),
                r#"{"name":"lfo_shape","value":"sample_and_hold"}"#,
            ),
            (
                SynthParam::LfoRateHz(0.5),
                r#"{"name":"lfo_rate_hz","value":0.5}"#,
            ),
            (
                SynthParam::LfoSync(true),
                r#"{"name":"lfo_sync","value":true}"#,
            ),
            (
                SynthParam::LfoDivision(crate::time::NoteDivision::DOTTED_EIGHTH),
                r#"{"name":"lfo_division","value":"1/8d"}"#,
            ),
            (
                SynthParam::LfoRetrigger(true),
                r#"{"name":"lfo_retrigger","value":true}"#,
            ),
            (
                SynthParam::LfoVibratoSemitones(0.25),
                r#"{"name":"lfo_vibrato_semitones","value":0.25}"#,
            ),
            (
                SynthParam::LfoWidth(0.3),
                r#"{"name":"lfo_width","value":0.3}"#,
            ),
        ];
        for (param, param_json) in cases {
            let command = Command::SetSynthParam { track, param };
            let body =
                format!(r#"{{"type":"set_synth_param","track":"{track}","param":{param_json}}}"#);
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            let json = format!(r#"{{"format":10,"command":{body}}}"#);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(
                error.to_string().contains("arrived in format 11"),
                "{error}"
            );
        }
    }

    #[test]
    fn the_format_5_commands_serialise_like_this() {
        let track = track();
        let effect = testing::effect_id(0);
        // The plain Delay format 5 had, which it wrote with only its time
        // and Feedback, and today's format writes in full.
        let delay = Effect::new(
            effect,
            crate::EffectSettings::Delay(crate::DelaySettings::plain(0.375, 0.4)),
        )
        .with_mix(0.5);
        let delay_json = format!(
            r#"{{"id":"{effect}","settings":{{"delay":{PLAIN_DELAY_JSON}}},"enabled":true,"mix":0.5}}"#
        );
        let new_track = testing::track_id(0);
        let settings_json = DEFAULT_SYNTH_JSON;
        let cases = [
            (
                Command::AddEffects {
                    effects: vec![PlacedEffect {
                        chain: Chain::Track(track),
                        index: 0,
                        effect: delay,
                    }],
                },
                format!(
                    r#"{{"type":"add_effects","effects":[{{"chain":{{"track":"{track}"}},"index":0,"effect":{delay_json}}}]}}"#
                ),
            ),
            (
                Command::RemoveEffects {
                    effects: vec![effect],
                },
                format!(r#"{{"type":"remove_effects","effects":["{effect}"]}}"#),
            ),
            (
                Command::SetEffectMix { effect, mix: 0.25 },
                format!(r#"{{"type":"set_effect_mix","effect":"{effect}","mix":0.25}}"#),
            ),
            (
                Command::SetDelayParam {
                    effect,
                    param: DelayParam::TimeSeconds(0.25),
                },
                format!(
                    r#"{{"type":"set_delay_param","effect":"{effect}","param":{{"name":"time_seconds","value":0.25}}}}"#
                ),
            ),
            (
                Command::AddTracks {
                    tracks: vec![PlacedTrack {
                        index: 1,
                        track: Track::new(
                            new_track,
                            "Synth 2",
                            Source::Synth(SynthSettings::default()),
                        )
                        .with_effects([delay]),
                    }],
                },
                format!(
                    r#"{{"type":"add_tracks","tracks":[{{"index":1,"track":{{"id":"{new_track}","name":"Synth 2","source":{{"synth":{settings_json}}},"effects":[{delay_json}],"mixer":{{"volume_db":0.0,"pan":0.0,"mute":false,"solo":false}},"clips":[]}}}}]}}"#
                ),
            ),
        ];
        for (command, body) in cases {
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // Format 5 to 8 lists still load, with the Delay as they wrote
            // it.
            let old = body.replace(PLAIN_DELAY_JSON, r#"{"time_seconds":0.375,"feedback":0.4}"#);
            for format in [5, 6, 7, 8] {
                let json = format!(r#"{{"format":{format},"command":{old}}}"#);
                assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            }
            // Effects are new in format 5, even in a format 4 command.
            let json = format!(r#"{{"format":4,"command":{body}}}"#);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(error.to_string().contains("arrived in format 5"), "{error}");
        }
    }

    #[test]
    fn the_format_7_commands_serialise_like_this() {
        let effect = testing::effect_id(0);
        let delay = Effect::default_of(effect, crate::EffectKind::Delay);
        let delay_json = format!(
            r#"{{"id":"{effect}","settings":{{"delay":{DEFAULT_DELAY_JSON}}},"enabled":true,"mix":0.3}}"#
        );
        let cases = [
            (
                Command::MoveEffect { effect, index: 2 },
                format!(r#"{{"type":"move_effect","effect":"{effect}","index":2}}"#),
            ),
            (
                Command::SetEffectEnabled {
                    effect,
                    enabled: false,
                },
                format!(r#"{{"type":"set_effect_enabled","effect":"{effect}","enabled":false}}"#),
            ),
            (
                Command::AddEffects {
                    effects: vec![PlacedEffect {
                        chain: Chain::Master,
                        index: 0,
                        effect: delay,
                    }],
                },
                format!(
                    r#"{{"type":"add_effects","effects":[{{"chain":"master","index":0,"effect":{delay_json}}}]}}"#
                ),
            ),
        ];
        for (command, body) in cases {
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // They're new in format 7, even in a format 6 command.
            let json = format!(r#"{{"format":6,"command":{body}}}"#);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(error.to_string().contains("arrived in format 7"), "{error}");
        }
    }

    #[test]
    fn the_format_14_command_serialises_like_this() {
        let track = track();
        let settings = SynthSettings {
            cutoff_hz: 800.0,
            ..SynthSettings::default()
        };
        let command = Command::SetSynthSettings { track, settings };
        let json = serde_json::to_string(&command).unwrap();
        let settings_json = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            json,
            format!(
                r#"{{"format":14,"command":{{"type":"set_synth_settings","track":"{track}","settings":{settings_json}}}}}"#
            )
        );
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        // Settings it leaves out take their defaults, as `add_tracks`' do.
        let partial = format!(
            r#"{{"format":14,"command":{{"type":"set_synth_settings","track":"{track}","settings":{{"cutoff_hz":800.0}}}}}}"#
        );
        assert_eq!(serde_json::from_str::<Command>(&partial).unwrap(), command);
        // It's new in format 14.
        let old = partial.replace(r#""format":14"#, r#""format":13"#);
        let error = serde_json::from_str::<Command>(&old).unwrap_err();
        assert!(
            error.to_string().contains("arrived in format 14"),
            "{error}"
        );
    }

    #[test]
    fn the_format_9_delay_settings_serialise_like_this() {
        let effect = testing::effect_id(0);
        let cases = [
            (DelayParam::Sync(false), r#"{"name":"sync","value":false}"#),
            (
                DelayParam::Division(NoteDivision::from_value("1/4t").unwrap()),
                r#"{"name":"division","value":"1/4t"}"#,
            ),
            (
                DelayParam::Mode(crate::DelayMode::PingPong),
                r#"{"name":"mode","value":"ping_pong"}"#,
            ),
            (
                DelayParam::LowCutHz(250.0),
                r#"{"name":"low_cut_hz","value":250.0}"#,
            ),
            (
                DelayParam::HighCutHz(3000.0),
                r#"{"name":"high_cut_hz","value":3000.0}"#,
            ),
            (DelayParam::Drive(0.5), r#"{"name":"drive","value":0.5}"#),
            (
                DelayParam::Wobble(0.25),
                r#"{"name":"wobble","value":0.25}"#,
            ),
        ];
        for (param, param_json) in cases {
            let command = Command::SetDelayParam { effect, param };
            let body =
                format!(r#"{{"type":"set_delay_param","effect":"{effect}","param":{param_json}}}"#);
            let json = format!(r#"{{"format":14,"command":{body}}}"#);
            assert_eq!(serde_json::to_string(&command).unwrap(), json);
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
            // New in format 9, even in a format 8 command.
            let json = format!(r#"{{"format":8,"command":{body}}}"#);
            let error = serde_json::from_str::<Command>(&json).unwrap_err();
            assert!(error.to_string().contains("arrived in format 9"), "{error}");
        }
        // Feedback past 100% is a format 5 setting with a wider range.
        let json = format!(
            r#"{{"format":5,"command":{{"type":"set_delay_param","effect":"{effect}","param":{{"name":"feedback","value":1.1}}}}}}"#
        );
        assert!(serde_json::from_str::<Command>(&json).is_ok());
    }

    #[test]
    fn effect_commands_continue_only_with_the_same_effect_and_setting() {
        let (a, b) = (testing::effect_id(0), testing::effect_id(1));
        let mix = |effect, mix| Command::SetEffectMix { effect, mix };
        assert!(mix(a, 0.2).sets_same_as(&mix(a, 0.8)));
        assert!(!mix(a, 0.2).sets_same_as(&mix(b, 0.8)));
        let set = |effect, param| Command::SetDelayParam { effect, param };
        let time = set(a, DelayParam::TimeSeconds(0.25));
        assert!(time.sets_same_as(&set(a, DelayParam::TimeSeconds(0.5))));
        assert!(!time.sets_same_as(&set(a, DelayParam::Feedback(0.5))));
        assert!(!time.sets_same_as(&set(b, DelayParam::TimeSeconds(0.5))));
        assert!(!time.sets_same_as(&mix(a, 0.25)));
        // Switching and moving are steps of their own: switching off and on
        // again is two undos, as is moving twice.
        let switch = |enabled| Command::SetEffectEnabled { effect: a, enabled };
        assert!(!switch(false).sets_same_as(&switch(true)));
        let move_to = |index| Command::MoveEffect { effect: a, index };
        assert!(!move_to(0).sets_same_as(&move_to(1)));
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
        assert!(!tune.sets_same_as(&set(DrumSound::Kick, DrumParam::Model(KickModel::Tr909))));
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
        for format in [0, 15, 99] {
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
