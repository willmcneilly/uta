//! The project: everything Uta saves about a piece of music.

use std::collections::HashSet;

use uuid::Uuid;

use crate::command::{ClipPosition, PlacedClip, PlacedTrack};
use crate::time::{MAX_TICKS, TempoMap, Ticks, TimeSignature};
use crate::track::{Clip, Source, SourceKind, Track, check_span};
use crate::{
    ClipId, Command, CommandError, DrumParamError, KitSettings, NoteId, ProjectId, SynthSettings,
    TrackId,
};

/// The song's tempo, time signature and loop region.
#[derive(Debug, Clone, PartialEq)]
pub struct Transport {
    tempo_map: TempoMap,
    time_signature: TimeSignature,
    loop_start: Ticks,
    loop_length: Ticks,
    loop_enabled: bool,
}

impl Transport {
    pub fn tempo_map(&self) -> &TempoMap {
        &self.tempo_map
    }

    /// Always 4/4 for now.
    pub fn time_signature(&self) -> TimeSignature {
        self.time_signature
    }

    /// Where the loop starts, in ticks. Always on a bar.
    pub fn loop_start(&self) -> Ticks {
        self.loop_start
    }

    /// How long the loop is, in ticks. Always a whole number of bars.
    pub fn loop_length(&self) -> Ticks {
        self.loop_length
    }

    /// Whether playback goes round the loop, rather than on to the end of
    /// the song.
    pub fn loop_enabled(&self) -> bool {
        self.loop_enabled
    }
}

/// A project: a master volume, a transport, and tracks holding clips of
/// notes. See RFC-002, "The shared model", and RFC-003, "The shared model,
/// extended".
///
/// Its data only changes through [`Command`]s, so every change can be undone
/// and replayed. Every track, clip and note has an ID that's unique across
/// the whole project.
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
    /// The limits [`Command::SetLoopLength`] allows.
    pub const MIN_LOOP_BARS: u32 = 1;
    pub const MAX_LOOP_BARS: u32 = 16;

    /// The most tracks a project can have. The audio thread sets aside room
    /// for this many when the stream starts.
    pub const MAX_TRACKS: usize = 32;

    /// A new project with a random ID.
    pub fn new() -> Self {
        Self::with_id(ProjectId::random())
    }

    /// A new project with the given ID: a synth track, "Synth 1", and a drum
    /// track, "Drums 1", each with an empty 4-bar clip, and the loop switched
    /// on over those 4 bars. Its tracks' and clips' IDs are worked out from
    /// the project's ID, so the same ID always gives exactly the same
    /// project, and commands saved against it replay. Synth 1 and its clip
    /// keep the IDs they had before Drums 1 was added, so older command
    /// lists still find them.
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
                loop_enabled: true,
            },
            tracks: vec![
                Track::new(
                    TrackId::from_uuid(derived("track 1")),
                    "Synth 1",
                    Source::Synth(SynthSettings::default()),
                )
                .with_clips([Clip::new(
                    ClipId::from_uuid(derived("clip 1")),
                    0,
                    loop_length,
                )]),
                Track::new(
                    TrackId::from_uuid(derived("track 2")),
                    "Drums 1",
                    Source::Drums(KitSettings::default()),
                )
                .with_clips([Clip::new(
                    ClipId::from_uuid(derived("clip 2")),
                    0,
                    loop_length,
                )]),
            ],
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

    /// Where the song ends, in ticks: one bar after the last clip ends, or
    /// one bar in if there are no clips.
    pub fn song_end(&self) -> Ticks {
        let last = self
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .map(Clip::end)
            .max()
            .unwrap_or(0);
        last + self.transport.time_signature.ticks_per_bar()
    }

    /// The name for a new track of `kind`, including a duplicate: "Synth n"
    /// or "Drums n", where n is one more than the highest number any track's
    /// name of that kind uses.
    pub fn next_track_name(&self, kind: SourceKind) -> String {
        let prefix = kind.name();
        let highest = self
            .tracks
            .iter()
            .filter_map(|track| {
                let number = track.name.strip_prefix(prefix)?.strip_prefix(' ')?;
                number.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0);
        format!("{prefix} {}", highest.saturating_add(1))
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
            &Command::AddNotes { clip, ref notes } => {
                let source = &self.track_of(clip)?.source;
                check_listed_once(notes.iter().map(|note| note.id))?;
                for note in notes {
                    note.validate()?;
                    source.check_note(note)?;
                    if self.note_in_use(note.id) {
                        return Err(CommandError::NoteAlreadyExists(note.id));
                    }
                }
                let clip = self.clip_mut(clip)?;
                clip.notes_mut()
                    .extend(notes.iter().map(|note| (note.id, *note)));
                Ok(Command::RemoveNotes {
                    clip: clip.id,
                    notes: notes.iter().map(|note| note.id).collect(),
                })
            }
            Command::RemoveNotes { clip, notes } => {
                let clip = self.clip_mut(*clip)?;
                check_listed_once(notes.iter().copied())?;
                check_in_clip(clip, notes.iter().copied())?;
                let clip_notes = clip.notes_mut();
                let removed = notes
                    .iter()
                    .map(|id| clip_notes.remove(id).expect("checked above"))
                    .collect();
                Ok(Command::AddNotes {
                    clip: clip.id,
                    notes: removed,
                })
            }
            Command::SetNotes { clip, notes } => {
                // Cheap: a source is a few settings.
                let source = self.track_of(*clip)?.source.clone();
                let clip = self.clip_mut(*clip)?;
                check_listed_once(notes.iter().map(|note| note.id))?;
                check_in_clip(clip, notes.iter().map(|note| note.id))?;
                for note in notes {
                    note.validate()?;
                    source.check_note(note)?;
                }
                let clip_notes = clip.notes_mut();
                let previous = notes
                    .iter()
                    .map(|note| clip_notes.insert(note.id, *note).expect("checked above"))
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
                // In Make a loop, the loop was always 1 to 16 bars and the
                // one clip was as long as it. Once that's no longer so, this
                // command's inverse couldn't put the project back exactly,
                // so it's refused.
                let loop_length = self.transport.loop_length;
                let loop_bars = loop_length / bar;
                let first_clip = self
                    .tracks
                    .first_mut()
                    .and_then(|track| track.clips.first_mut());
                if !(Ticks::from(Self::MIN_LOOP_BARS)..=Ticks::from(Self::MAX_LOOP_BARS))
                    .contains(&loop_bars)
                    || first_clip
                        .as_ref()
                        .is_some_and(|clip| clip.length != loop_length)
                {
                    return Err(CommandError::LoopLengthUnavailable);
                }
                if let Some(clip) = &first_clip {
                    check_span(clip.id, clip.start, length)?;
                }
                if let Some(clip) = first_clip {
                    // A new length doesn't change the clip's place in the
                    // order, so it stays the first clip.
                    clip.set_span(clip.start, length);
                }
                let previous = std::mem::replace(&mut self.transport.loop_length, length);
                Ok(Command::SetLoopLength {
                    bars: self.bars(previous),
                })
            }
            &Command::SetSynthParam { track, param } => {
                let track = self
                    .tracks
                    .iter_mut()
                    .find(|candidate| candidate.id == track)
                    .ok_or(CommandError::UnknownTrack(track))?;
                let Source::Synth(settings) = &mut track.source else {
                    return Err(CommandError::NotASynthTrack(track.id));
                };
                let previous = settings
                    .set(param)
                    .map_err(CommandError::SynthParamOutOfRange)?;
                Ok(Command::SetSynthParam {
                    track: track.id,
                    param: previous,
                })
            }
            &Command::SetDrumParam {
                track,
                sound,
                param,
            } => {
                let index = self.track_index(track)?;
                let Source::Drums(kit) = &mut self.tracks[index].source else {
                    return Err(CommandError::NotADrumTrack(track));
                };
                let previous = kit.set(sound, param).map_err(|error| match error {
                    DrumParamError::NoSuchSetting => CommandError::NoSuchDrumParam { sound, param },
                    DrumParamError::OutOfRange { .. } => {
                        CommandError::DrumParamOutOfRange { sound, param }
                    }
                })?;
                Ok(Command::SetDrumParam {
                    track,
                    sound,
                    param: previous,
                })
            }
            Command::AddTracks { tracks } => self.add_tracks(tracks),
            Command::RemoveTracks { tracks } => self.remove_tracks(tracks),
            &Command::MoveTrack { track, index } => {
                let from = self.track_index(track)?;
                if index >= self.tracks.len() {
                    return Err(CommandError::TrackIndexOutOfRange(index));
                }
                let moved = self.tracks.remove(from);
                self.tracks.insert(index, moved);
                Ok(Command::MoveTrack { track, index: from })
            }
            &Command::SetTrackMixer { track, mixer } => {
                mixer.validate()?;
                let index = self.track_index(track)?;
                let previous = std::mem::replace(&mut self.tracks[index].mixer, mixer);
                Ok(Command::SetTrackMixer {
                    track,
                    mixer: previous,
                })
            }
            Command::AddClips { clips } => self.add_clips(clips),
            Command::RemoveClips { clips } => self.remove_clips(clips),
            Command::SetClips { clips } => self.set_clips(clips),
            &Command::SetLoop { start_bar, bars } => {
                let bar = self.transport.time_signature.ticks_per_bar();
                let end = (Ticks::from(start_bar) + Ticks::from(bars)) * bar;
                if bars == 0 || end > MAX_TICKS {
                    return Err(CommandError::LoopOutOfRange { start_bar, bars });
                }
                let start =
                    std::mem::replace(&mut self.transport.loop_start, Ticks::from(start_bar) * bar);
                let length =
                    std::mem::replace(&mut self.transport.loop_length, Ticks::from(bars) * bar);
                Ok(Command::SetLoop {
                    start_bar: self.bars(start),
                    bars: self.bars(length),
                })
            }
            &Command::SetLoopEnabled { enabled } => {
                let previous = std::mem::replace(&mut self.transport.loop_enabled, enabled);
                Ok(Command::SetLoopEnabled { enabled: previous })
            }
        }
    }

    fn add_tracks(&mut self, tracks: &[PlacedTrack]) -> Result<Command, CommandError> {
        if tracks.is_empty() {
            return Err(CommandError::NoTracks);
        }
        if self.tracks.len() + tracks.len() > Self::MAX_TRACKS {
            return Err(CommandError::TooManyTracks);
        }
        let mut new = NewIds::new(self);
        for placed in tracks {
            new.track(&placed.track)?;
        }
        // Added in order of index, each track lands at its own: those before
        // it are already in place.
        let mut placed: Vec<&PlacedTrack> = tracks.iter().collect();
        placed.sort_by_key(|placed| placed.index);
        for (count, track) in placed.iter().enumerate() {
            if count > 0 && placed[count - 1].index == track.index {
                return Err(CommandError::TrackIndexListedTwice(track.index));
            }
            if track.index > self.tracks.len() + count {
                return Err(CommandError::TrackIndexOutOfRange(track.index));
            }
        }
        for placed in placed {
            self.tracks.insert(placed.index, placed.track.clone());
        }
        Ok(Command::RemoveTracks {
            tracks: tracks.iter().map(|placed| placed.track.id).collect(),
        })
    }

    fn remove_tracks(&mut self, ids: &[TrackId]) -> Result<Command, CommandError> {
        if ids.is_empty() {
            return Err(CommandError::NoTracks);
        }
        let mut indices = Vec::with_capacity(ids.len());
        let mut seen = HashSet::new();
        for &id in ids {
            if !seen.insert(id) {
                return Err(CommandError::TrackListedTwice(id));
            }
            indices.push(self.track_index(id)?);
        }
        // Removed from the back, so the indices before stay right.
        indices.sort_unstable();
        let mut removed: Vec<PlacedTrack> = indices
            .into_iter()
            .rev()
            .map(|index| PlacedTrack {
                index,
                track: self.tracks.remove(index),
            })
            .collect();
        removed.reverse();
        Ok(Command::AddTracks { tracks: removed })
    }

    fn add_clips(&mut self, clips: &[PlacedClip]) -> Result<Command, CommandError> {
        if clips.is_empty() {
            return Err(CommandError::NoClips);
        }
        let mut new = NewIds::new(self);
        for placed in clips {
            let track = self
                .track(placed.track)
                .ok_or(CommandError::UnknownTrack(placed.track))?;
            new.clip(&placed.clip, &track.source)?;
        }
        for placed in clips {
            let index = self.track_index(placed.track).expect("checked above");
            self.tracks[index].insert_clip(placed.clip.clone());
        }
        Ok(Command::RemoveClips {
            clips: clips.iter().map(|placed| placed.clip.id).collect(),
        })
    }

    fn remove_clips(&mut self, ids: &[ClipId]) -> Result<Command, CommandError> {
        check_clips_listed_once(ids.iter().copied())?;
        if let Some(&missing) = ids.iter().find(|&&id| self.clip(id).is_none()) {
            return Err(CommandError::UnknownClip(missing));
        }
        let removed = ids
            .iter()
            .map(|&id| {
                let track = self
                    .tracks
                    .iter_mut()
                    .find(|track| track.clip(id).is_some())
                    .expect("checked above");
                PlacedClip {
                    track: track.id,
                    clip: track.remove_clip(id).expect("checked above"),
                }
            })
            .collect();
        Ok(Command::AddClips { clips: removed })
    }

    fn set_clips(&mut self, positions: &[ClipPosition]) -> Result<Command, CommandError> {
        check_clips_listed_once(positions.iter().map(|position| position.id))?;
        for position in positions {
            if self.clip(position.id).is_none() {
                return Err(CommandError::UnknownClip(position.id));
            }
            let track = self
                .track(position.track)
                .ok_or(CommandError::UnknownTrack(position.track))?;
            check_span(position.id, position.start, position.length)?;
            // A clip moves only between tracks of the same kind, so it never
            // takes notes off the kit onto a drum track either.
            let from = self.track_of(position.id).expect("checked above");
            if from.source.kind() != track.source.kind() {
                return Err(CommandError::ClipToOtherKind {
                    clip: position.id,
                    track: position.track,
                });
            }
        }
        let previous = positions
            .iter()
            .map(|position| {
                let track = self
                    .tracks
                    .iter_mut()
                    .find(|track| track.clip(position.id).is_some())
                    .expect("checked above");
                let mut clip = track.remove_clip(position.id).expect("checked above");
                let was = ClipPosition {
                    id: clip.id,
                    track: track.id,
                    start: clip.start,
                    length: clip.length,
                };
                clip.set_span(position.start, position.length);
                let index = self.track_index(position.track).expect("checked above");
                self.tracks[index].insert_clip(clip);
                was
            })
            .collect();
        Ok(Command::SetClips { clips: previous })
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

    /// The track the clip with this ID is on.
    fn track_of(&self, clip: ClipId) -> Result<&Track, CommandError> {
        self.tracks
            .iter()
            .find(|track| track.clip(clip).is_some())
            .ok_or(CommandError::UnknownClip(clip))
    }

    fn track_index(&self, id: TrackId) -> Result<usize, CommandError> {
        self.tracks
            .iter()
            .position(|track| track.id == id)
            .ok_or(CommandError::UnknownTrack(id))
    }

    /// Whether any clip in the project has a note with this ID.
    fn note_in_use(&self, id: NoteId) -> bool {
        self.tracks
            .iter()
            .flat_map(|track| &track.clips)
            .any(|clip| clip.notes.contains_key(&id))
    }

    /// `ticks` as a whole number of bars.
    fn bars(&self, ticks: Ticks) -> u32 {
        let bars = ticks / self.transport.time_signature.ticks_per_bar();
        u32::try_from(bars).expect("loops end by MAX_TICKS, so their bars fit")
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

/// Checks a clips command names at least one clip, and each only once.
fn check_clips_listed_once(ids: impl Iterator<Item = ClipId>) -> Result<(), CommandError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(CommandError::ClipListedTwice(id));
        }
    }
    if seen.is_empty() {
        return Err(CommandError::NoClips);
    }
    Ok(())
}

/// Checks the tracks, clips and notes a command adds: that each is valid,
/// and has an ID that isn't used in the project or listed twice in the
/// command.
struct NewIds<'a> {
    project: &'a Project,
    tracks: HashSet<TrackId>,
    clips: HashSet<ClipId>,
    notes: HashSet<NoteId>,
}

impl<'a> NewIds<'a> {
    fn new(project: &'a Project) -> Self {
        Self {
            project,
            tracks: HashSet::new(),
            clips: HashSet::new(),
            notes: HashSet::new(),
        }
    }

    fn track(&mut self, track: &Track) -> Result<(), CommandError> {
        if self.project.track(track.id).is_some() {
            return Err(CommandError::TrackAlreadyExists(track.id));
        }
        if !self.tracks.insert(track.id) {
            return Err(CommandError::TrackListedTwice(track.id));
        }
        if track.name.trim().is_empty() {
            return Err(CommandError::UnnamedTrack(track.id));
        }
        track.source.validate()?;
        track.mixer.validate()?;
        track
            .clips
            .iter()
            .try_for_each(|clip| self.clip(clip, &track.source))
    }

    /// Checks a clip to add to a track with this source.
    fn clip(&mut self, clip: &Clip, source: &Source) -> Result<(), CommandError> {
        if self.project.clip(clip.id).is_some() {
            return Err(CommandError::ClipAlreadyExists(clip.id));
        }
        if !self.clips.insert(clip.id) {
            return Err(CommandError::ClipListedTwice(clip.id));
        }
        clip.validate()?;
        for note in clip.notes() {
            if self.project.note_in_use(note.id) {
                return Err(CommandError::NoteAlreadyExists(note.id));
            }
            if !self.notes.insert(note.id) {
                return Err(CommandError::NoteListedTwice(note.id));
            }
            source.check_note(note)?;
        }
        Ok(())
    }
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
    use crate::{
        DrumParam, DrumSound, Effect, KitSettings, MixerStrip, Note, SynthParam, Waveform,
    };
    use proptest::prelude::*;

    const BAR: Ticks = 3840;

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
    fn new_project_has_a_synth_and_a_drum_track_each_with_one_empty_clip() {
        let project = Project::new();
        let transport = project.transport();
        assert_eq!(transport.tempo_map().bpm(), 120.0);
        assert_eq!(transport.tempo_map().sections().len(), 1);
        assert_eq!(transport.time_signature(), TimeSignature::FOUR_FOUR);
        assert_eq!(transport.loop_start(), 0);
        assert_eq!(transport.loop_length(), 4 * 3840, "4 bars");
        assert!(transport.loop_enabled());

        let [synth, drums] = project.tracks() else {
            panic!("expected two tracks");
        };
        assert_eq!(synth.name(), "Synth 1");
        assert_eq!(synth.source(), &Source::Synth(SynthSettings::default()));
        assert_eq!(drums.name(), "Drums 1");
        assert_eq!(drums.source(), &Source::Drums(KitSettings::default()));
        for track in [synth, drums] {
            assert_eq!(track.effects(), &[] as &[Effect]);
            assert_eq!(track.mixer(), &MixerStrip::default());
            let [clip] = track.clips() else {
                panic!("expected one clip");
            };
            assert_eq!(clip.start(), 0);
            assert_eq!(clip.length(), transport.loop_length());
            assert_eq!(clip.content_offset(), 0);
            assert_eq!(clip.content_length(), clip.length());
            assert_eq!(clip.notes().len(), 0);
        }
        assert_eq!(project.song_end(), 5 * BAR, "one bar after the clips");
    }

    #[test]
    fn synth_1_keeps_the_ids_it_had_before_drums_1() {
        // Older command lists, such as examples/demo-loop.json, name Synth 1
        // and its clip by these IDs, worked out from the project's.
        let project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(7)));
        let derived = |name: &str| Uuid::new_v5(&project.id().as_uuid(), name.as_bytes());
        let ids = |track: &Track| (track.id().as_uuid(), track.clips()[0].id().as_uuid());
        assert_eq!(
            ids(&project.tracks()[0]),
            (derived("track 1"), derived("clip 1"))
        );
        assert_eq!(
            ids(&project.tracks()[1]),
            (derived("track 2"), derived("clip 2"))
        );
    }

    #[test]
    fn a_new_tracks_mixer_is_centred_at_0_db() {
        assert_eq!(
            MixerStrip::default(),
            MixerStrip {
                volume_db: 0.0,
                pan: 0.0,
                mute: false,
                solo: false,
            }
        );
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
        let Source::Synth(settings) = changed.track(track).unwrap().source() else {
            panic!("a synth track")
        };
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

    /// The test project, with its drum track, "Drums 1", and that track's
    /// empty 4-bar clip.
    fn with_drums() -> (Project, TrackId, ClipId) {
        let project = testing::project();
        let drums = &project.tracks()[1];
        let (track, clip) = (drums.id(), drums.clips()[0].id());
        (project, track, clip)
    }

    #[test]
    fn set_drum_param_sets_it_and_undoes() {
        let (project, track, _) = with_drums();
        let changed = apply_and_check_undo(
            &project,
            Command::SetDrumParam {
                track,
                sound: DrumSound::Kick,
                param: DrumParam::TuneHz(60.0),
            },
        );
        let Source::Drums(kit) = changed.track(track).unwrap().source() else {
            panic!("a drum track")
        };
        assert_eq!(kit.kick.tune_hz, 60.0);
        for param in [
            DrumParam::Tone(0.0),
            DrumParam::DecaySeconds(0.8),
            DrumParam::LevelDb(-6.0),
        ] {
            apply_and_check_undo(
                &project,
                Command::SetDrumParam {
                    track,
                    sound: DrumSound::Kick,
                    param,
                },
            );
        }

        let set = |track, sound, param| Command::SetDrumParam {
            track,
            sound,
            param,
        };
        let param = DrumParam::DecaySeconds(2.0);
        assert_rejected(
            &project,
            set(track, DrumSound::Kick, param),
            CommandError::DrumParamOutOfRange {
                sound: DrumSound::Kick,
                param,
            },
        );
        // The snare's, clap's, hats' and toms' settings set and undo too.
        for (sound, param) in [
            (DrumSound::Snare, DrumParam::TuneHz(200.0)),
            (DrumSound::Snare, DrumParam::Tone(0.3)),
            (DrumSound::Snare, DrumParam::Snappy(0.9)),
            (DrumSound::Clap, DrumParam::Tone(1500.0)),
            (DrumSound::Clap, DrumParam::DecaySeconds(0.4)),
            (DrumSound::ClosedHat, DrumParam::TuneHz(300.0)),
            (DrumSound::ClosedHat, DrumParam::Tone(9000.0)),
            (DrumSound::ClosedHat, DrumParam::DecaySeconds(0.1)),
            (DrumSound::OpenHat, DrumParam::DecaySeconds(0.5)),
            (DrumSound::OpenHat, DrumParam::LevelDb(-6.0)),
            (DrumSound::LowTom, DrumParam::TuneHz(95.0)),
            (DrumSound::LowTom, DrumParam::DecaySeconds(0.4)),
            (DrumSound::HighTom, DrumParam::TuneHz(200.0)),
            (DrumSound::HighTom, DrumParam::LevelDb(-6.0)),
        ] {
            apply_and_check_undo(&project, set(track, sound, param));
        }
        let param = DrumParam::TuneHz(200.0);
        assert_rejected(
            &project,
            set(track, DrumSound::Clap, param),
            CommandError::NoSuchDrumParam {
                sound: DrumSound::Clap,
                param,
            },
        );
        // Each kind of track takes only its own settings.
        let synth = track_id(&project);
        assert_rejected(
            &project,
            set(synth, DrumSound::Kick, DrumParam::TuneHz(60.0)),
            CommandError::NotADrumTrack(synth),
        );
        assert_rejected(
            &project,
            Command::SetSynthParam {
                track,
                param: SynthParam::Sustain(0.5),
            },
            CommandError::NotASynthTrack(track),
        );
        let unknown = TrackId::random();
        assert_rejected(
            &project,
            set(unknown, DrumSound::Kick, DrumParam::TuneHz(60.0)),
            CommandError::UnknownTrack(unknown),
        );
    }

    #[test]
    fn a_drum_track_takes_only_the_kits_notes() {
        let (project, track, clip) = with_drums();
        let kick = note(0, 36, 0);
        let off_kit = note(1, 37, 0);
        let refused = CommandError::NoteOffKit {
            note: off_kit.id,
            pitch: 37,
        };
        // Every row's note goes on.
        let rows: Vec<Note> = crate::KIT
            .iter()
            .enumerate()
            .map(|(i, row)| note(10 + i as u128, row.pitch, 0))
            .collect();
        apply_and_check_undo(&project, Command::AddNotes { clip, notes: rows });

        // AddNotes.
        assert_rejected(
            &project,
            Command::AddNotes {
                clip,
                notes: vec![kick, off_kit],
            },
            refused,
        );
        // SetNotes: moving a kick off the kit.
        let mut with_kick = project.clone();
        with_kick
            .apply(&Command::AddNotes {
                clip,
                notes: vec![kick],
            })
            .unwrap();
        assert_rejected(
            &with_kick,
            Command::SetNotes {
                clip,
                notes: vec![Note { pitch: 35, ..kick }],
            },
            CommandError::NoteOffKit {
                note: kick.id,
                pitch: 35,
            },
        );
        apply_and_check_undo(
            &with_kick,
            Command::SetNotes {
                clip,
                notes: vec![Note { pitch: 38, ..kick }],
            },
        );
        // AddTracks: a drum track that arrives with a note off the kit.
        let drums = |id, clip_id| {
            Track::new(id, "Drums 2", Source::Drums(KitSettings::default()))
                .with_clips([Clip::new(clip_id, 0, BAR).with_notes([off_kit])])
        };
        assert_rejected(
            &project,
            Command::AddTracks {
                tracks: vec![PlacedTrack {
                    index: 0,
                    track: drums(testing::track_id(8), testing::clip_id(8)),
                }],
            },
            refused,
        );
        // AddClips: a clip with a note off the kit, onto a drum track.
        assert_rejected(
            &project,
            Command::AddClips {
                clips: vec![PlacedClip {
                    track,
                    clip: Clip::new(testing::clip_id(8), BAR, BAR).with_notes([off_kit]),
                }],
            },
            refused,
        );
        // SetClips: a synth clip with a note off the kit can't move onto it,
        // because no clip moves to a track of the other kind.
        let mut with_off_kit = project.clone();
        let synth_clip = clip_id(&project);
        with_off_kit
            .apply(&Command::AddNotes {
                clip: synth_clip,
                notes: vec![off_kit],
            })
            .unwrap();
        assert_rejected(
            &with_off_kit,
            Command::SetClips {
                clips: vec![ClipPosition {
                    id: synth_clip,
                    track,
                    start: 0,
                    length: 4 * BAR,
                }],
            },
            CommandError::ClipToOtherKind {
                clip: synth_clip,
                track,
            },
        );
        // A synth track still takes any pitch.
        apply_and_check_undo(
            &project,
            Command::AddNotes {
                clip: clip_id(&project),
                notes: vec![off_kit],
            },
        );
        assert!(refused.to_string().contains("Kick 36"), "{refused}");
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

    /// A synth track from the pool, named "Synth n", with default settings
    /// and no clips.
    fn synth_track(index: u128, n: u32) -> Track {
        Track::new(
            testing::track_id(index),
            format!("Synth {n}"),
            Source::Synth(SynthSettings::default()),
        )
    }

    fn placed(index: usize, track: Track) -> PlacedTrack {
        PlacedTrack { index, track }
    }

    fn names(project: &Project) -> Vec<&str> {
        project.tracks().iter().map(Track::name).collect()
    }

    /// The test project with its drum track taken out, and tracks "Synth 2"
    /// and "Synth 3" after the first. "Synth 2" has a clip with note 0 in
    /// it, and "Synth 3" a mixer and sound of its own.
    fn with_three_tracks() -> Project {
        let mut project = testing::project();
        let drums = project.tracks()[1].id();
        project
            .apply(&Command::RemoveTracks {
                tracks: vec![drums],
            })
            .unwrap();
        project
            .apply(&Command::AddTracks {
                tracks: vec![
                    placed(
                        1,
                        synth_track(0, 2)
                            .with_clips([Clip::new(testing::clip_id(0), BAR, BAR)
                                .with_notes([note(0, 48, 0)])]),
                    ),
                    placed(
                        2,
                        Track::new(
                            testing::track_id(1),
                            "Synth 3",
                            Source::Synth(SynthSettings {
                                waveform: Waveform::Square,
                                ..SynthSettings::default()
                            }),
                        )
                        .with_mixer(MixerStrip {
                            volume_db: -6.0,
                            pan: -0.5,
                            mute: true,
                            solo: true,
                        }),
                    ),
                ],
            })
            .unwrap();
        project
    }

    #[test]
    fn add_tracks_puts_each_at_its_place_and_undoes_by_removing_them() {
        let project = testing::project();
        let first = track_id(&project);
        let command = Command::AddTracks {
            // Listed out of order: each still lands at its index.
            tracks: vec![placed(2, synth_track(1, 3)), placed(0, synth_track(0, 2))],
        };
        let mut changed = project.clone();
        let inverse = changed.apply(&command).unwrap();
        assert_eq!(
            inverse,
            Command::RemoveTracks {
                tracks: vec![testing::track_id(1), testing::track_id(0)],
            }
        );
        let ids: Vec<_> = changed.tracks().iter().map(Track::id).collect();
        let drums = project.tracks()[1].id();
        assert_eq!(
            ids,
            [testing::track_id(0), first, testing::track_id(1), drums]
        );
        apply_and_check_undo(&project, command);
    }

    #[test]
    fn undoing_a_track_delete_restores_its_clips_sound_mixer_and_place() {
        let project = with_three_tracks();
        // The first and third, which aren't next to each other.
        let command = Command::RemoveTracks {
            tracks: vec![testing::track_id(1), track_id(&project)],
        };
        let mut changed = project.clone();
        let inverse = changed.apply(&command).unwrap();
        assert_eq!(names(&changed), ["Synth 2"]);
        let Command::AddTracks { tracks } = &inverse else {
            panic!("expected AddTracks, got {inverse:?}");
        };
        let places: Vec<_> = tracks
            .iter()
            .map(|placed| (placed.index, placed.track.name()))
            .collect();
        assert_eq!(places, [(0, "Synth 1"), (2, "Synth 3")]);
        assert_eq!(&tracks[1].track, &project.tracks()[2], "the whole track");
        changed.apply(&inverse).unwrap();
        assert_eq!(changed, project);

        // A track with clips and notes comes back with them.
        apply_and_check_undo(
            &project,
            Command::RemoveTracks {
                tracks: vec![testing::track_id(0)],
            },
        );
    }

    #[test]
    fn a_project_can_have_32_tracks_and_no_more() {
        let project = testing::project();
        let room = Project::MAX_TRACKS - project.tracks().len();
        let tracks = (0..room)
            .map(|index| placed(index + 2, synth_track(index as u128, index as u32 + 2)))
            .collect();
        let full = apply_and_check_undo(&project, Command::AddTracks { tracks });
        assert_eq!(full.tracks().len(), Project::MAX_TRACKS);
        assert_rejected(
            &full,
            Command::AddTracks {
                tracks: vec![placed(0, synth_track(99, 99))],
            },
            CommandError::TooManyTracks,
        );
        assert_eq!(
            CommandError::TooManyTracks.to_string(),
            "a project can have at most 32 tracks"
        );
    }

    #[test]
    fn tracks_commands_check_what_they_add_and_name() {
        let project = with_three_tracks();
        let fresh = || synth_track(5, 4);
        let first = track_id(&project);
        let unknown = TrackId::random();
        let cases = [
            (
                Command::AddTracks { tracks: vec![] },
                CommandError::NoTracks,
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(0, synth_track(0, 4))],
                },
                CommandError::TrackAlreadyExists(testing::track_id(0)),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(0, fresh()), placed(1, fresh())],
                },
                CommandError::TrackListedTwice(testing::track_id(5)),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(0, synth_track(5, 4)), placed(0, synth_track(6, 5))],
                },
                CommandError::TrackIndexListedTwice(0),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(4, fresh())],
                },
                CommandError::TrackIndexOutOfRange(4),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(
                        0,
                        Track {
                            name: " ".into(),
                            ..fresh()
                        },
                    )],
                },
                CommandError::UnnamedTrack(testing::track_id(5)),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(
                        0,
                        Track {
                            source: Source::Synth(SynthSettings {
                                sustain: 2.0,
                                ..SynthSettings::default()
                            }),
                            ..fresh()
                        },
                    )],
                },
                CommandError::SynthParamOutOfRange(SynthParam::Sustain(2.0)),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(
                        0,
                        fresh().with_mixer(MixerStrip {
                            pan: 1.5,
                            ..MixerStrip::default()
                        }),
                    )],
                },
                CommandError::PanOutOfRange(1.5),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(
                        0,
                        fresh().with_clips([Clip::new(testing::clip_id(0), 0, BAR)]),
                    )],
                },
                CommandError::ClipAlreadyExists(testing::clip_id(0)),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(
                        0,
                        fresh().with_clips([Clip::new(testing::clip_id(5), 0, 0)]),
                    )],
                },
                CommandError::EmptyClip(testing::clip_id(5)),
            ),
            (
                Command::AddTracks {
                    tracks: vec![placed(
                        0,
                        fresh().with_clips([
                            Clip::new(testing::clip_id(5), 0, BAR).with_notes([note(0, 60, 0)])
                        ]),
                    )],
                },
                CommandError::NoteAlreadyExists(note_id(0)),
            ),
            (
                Command::RemoveTracks { tracks: vec![] },
                CommandError::NoTracks,
            ),
            (
                Command::RemoveTracks {
                    tracks: vec![first, unknown],
                },
                CommandError::UnknownTrack(unknown),
            ),
            (
                Command::RemoveTracks {
                    tracks: vec![first, first],
                },
                CommandError::TrackListedTwice(first),
            ),
            (
                Command::MoveTrack {
                    track: first,
                    index: 3,
                },
                CommandError::TrackIndexOutOfRange(3),
            ),
            (
                Command::MoveTrack {
                    track: unknown,
                    index: 0,
                },
                CommandError::UnknownTrack(unknown),
            ),
        ];
        for (command, error) in cases {
            assert_rejected(&project, command, error);
        }
    }

    #[test]
    fn move_track_moves_it_and_undoes() {
        let project = with_three_tracks();
        let first = track_id(&project);
        let mut changed = project.clone();
        let inverse = changed
            .apply(&Command::MoveTrack {
                track: first,
                index: 2,
            })
            .unwrap();
        assert_eq!(names(&changed), ["Synth 2", "Synth 3", "Synth 1"]);
        assert_eq!(
            inverse,
            Command::MoveTrack {
                track: first,
                index: 0,
            }
        );
        apply_and_check_undo(
            &project,
            Command::MoveTrack {
                track: testing::track_id(1),
                index: 0,
            },
        );
    }

    #[test]
    fn set_track_mixer_sets_it_and_undoes() {
        let project = testing::project();
        let track = track_id(&project);
        let mixer = MixerStrip {
            volume_db: -12.5,
            pan: 0.75,
            mute: true,
            solo: true,
        };
        let changed = apply_and_check_undo(&project, Command::SetTrackMixer { track, mixer });
        assert_eq!(changed.track(track).unwrap().mixer(), &mixer);

        for (volume_db, pan) in [
            (MixerStrip::MIN_VOLUME_DB, MixerStrip::MIN_PAN),
            (MixerStrip::MAX_VOLUME_DB, MixerStrip::MAX_PAN),
        ] {
            let mixer = MixerStrip {
                volume_db,
                pan,
                ..MixerStrip::default()
            };
            apply_and_check_undo(&project, Command::SetTrackMixer { track, mixer });
        }
        for volume_db in [6.01, -60.5, f32::INFINITY] {
            let mixer = MixerStrip {
                volume_db,
                ..MixerStrip::default()
            };
            assert_rejected(
                &project,
                Command::SetTrackMixer { track, mixer },
                CommandError::VolumeOutOfRange(volume_db),
            );
        }
        for pan in [1.01, -1.01] {
            let mixer = MixerStrip {
                pan,
                ..MixerStrip::default()
            };
            assert_rejected(
                &project,
                Command::SetTrackMixer { track, mixer },
                CommandError::PanOutOfRange(pan),
            );
        }
        let mut attempt = project.clone();
        let nan = MixerStrip {
            pan: f32::NAN,
            ..MixerStrip::default()
        };
        let result = attempt.apply(&Command::SetTrackMixer { track, mixer: nan });
        assert!(matches!(result, Err(CommandError::PanOutOfRange(pan)) if pan.is_nan()));
        assert_eq!(attempt, project);
        let unknown = TrackId::random();
        assert_rejected(
            &project,
            Command::SetTrackMixer {
                track: unknown,
                mixer,
            },
            CommandError::UnknownTrack(unknown),
        );
    }

    #[test]
    fn clips_commands_add_move_and_remove_clips_and_undo() {
        let project = with_three_tracks();
        let first = track_id(&project);
        let third = testing::track_id(1);
        let clip = |index| Clip::new(testing::clip_id(index), 2 * BAR, BAR);
        let add = Command::AddClips {
            clips: vec![
                PlacedClip {
                    track: third,
                    clip: clip(1).with_notes([note(1, 60, 0)]),
                },
                PlacedClip {
                    track: first,
                    clip: clip(2),
                },
            ],
        };
        let mut added = project.clone();
        assert_eq!(
            added.apply(&add).unwrap(),
            Command::RemoveClips {
                clips: vec![testing::clip_id(1), testing::clip_id(2)],
            }
        );
        assert_eq!(
            added.tracks()[2].clips(),
            [clip(1).with_notes([note(1, 60, 0)])]
        );
        apply_and_check_undo(&project, add);

        // Move clip 1 from the third track onto the first, over clip 2, and
        // lengthen it.
        let set = Command::SetClips {
            clips: vec![ClipPosition {
                id: testing::clip_id(1),
                track: first,
                start: 2 * BAR + 960,
                length: 2 * BAR,
            }],
        };
        let mut moved = added.clone();
        assert_eq!(
            moved.apply(&set).unwrap(),
            Command::SetClips {
                clips: vec![ClipPosition {
                    id: testing::clip_id(1),
                    track: third,
                    start: 2 * BAR,
                    length: BAR,
                }],
            }
        );
        assert!(moved.tracks()[2].clips().is_empty());
        let on_first: Vec<_> = moved.tracks()[0]
            .clips()
            .iter()
            .map(|clip| {
                (
                    clip.id(),
                    clip.start(),
                    clip.length(),
                    clip.content_length(),
                )
            })
            .collect();
        assert_eq!(
            on_first,
            [
                (clip_id(&project), 0, 4 * BAR, 4 * BAR),
                (testing::clip_id(2), 2 * BAR, BAR, BAR),
                (testing::clip_id(1), 2 * BAR + 960, 2 * BAR, 2 * BAR),
            ],
            "overlapping, in order of start"
        );
        assert_eq!(
            moved.clip(testing::clip_id(1)).unwrap().notes().len(),
            1,
            "its notes go with it"
        );
        apply_and_check_undo(&added, set);

        let remove = Command::RemoveClips {
            clips: vec![testing::clip_id(1), clip_id(&project)],
        };
        let mut removed = added.clone();
        let inverse = removed.apply(&remove).unwrap();
        assert!(removed.tracks()[0].clips().len() == 1 && removed.tracks()[2].clips().is_empty());
        assert_eq!(
            inverse,
            Command::AddClips {
                clips: vec![
                    PlacedClip {
                        track: third,
                        clip: clip(1).with_notes([note(1, 60, 0)]),
                    },
                    PlacedClip {
                        track: first,
                        clip: project.tracks()[0].clips()[0].clone(),
                    },
                ],
            }
        );
        apply_and_check_undo(&added, remove);
    }

    #[test]
    fn clips_commands_check_what_they_add_and_name() {
        let project = with_three_tracks();
        let first = track_id(&project);
        let existing = testing::clip_id(0);
        let fresh = testing::clip_id(5);
        let unknown_clip = ClipId::random();
        let unknown_track = TrackId::random();
        let add = |track, clip| Command::AddClips {
            clips: vec![PlacedClip { track, clip }],
        };
        let position = |id, track, start, length| Command::SetClips {
            clips: vec![ClipPosition {
                id,
                track,
                start,
                length,
            }],
        };
        let cases = [
            (Command::AddClips { clips: vec![] }, CommandError::NoClips),
            (
                add(unknown_track, Clip::new(fresh, 0, BAR)),
                CommandError::UnknownTrack(unknown_track),
            ),
            (
                add(first, Clip::new(existing, 0, BAR)),
                CommandError::ClipAlreadyExists(existing),
            ),
            (
                Command::AddClips {
                    clips: vec![
                        PlacedClip {
                            track: first,
                            clip: Clip::new(fresh, 0, BAR),
                        },
                        PlacedClip {
                            track: testing::track_id(1),
                            clip: Clip::new(fresh, BAR, BAR),
                        },
                    ],
                },
                CommandError::ClipListedTwice(fresh),
            ),
            (
                add(first, Clip::new(fresh, 0, 0)),
                CommandError::EmptyClip(fresh),
            ),
            (
                add(first, Clip::new(fresh, crate::time::MAX_TICKS, 1)),
                CommandError::ClipTooLate(fresh),
            ),
            (
                add(
                    first,
                    Clip::new(fresh, 0, BAR).with_notes([Note {
                        pitch: 200,
                        ..note(5, 60, 0)
                    }]),
                ),
                CommandError::PitchOutOfRange {
                    note: note_id(5),
                    pitch: 200,
                },
            ),
            (
                // Note 0 is in the second track's clip.
                add(first, Clip::new(fresh, 0, BAR).with_notes([note(0, 60, 0)])),
                CommandError::NoteAlreadyExists(note_id(0)),
            ),
            (
                Command::AddClips {
                    clips: vec![
                        PlacedClip {
                            track: first,
                            clip: Clip::new(fresh, 0, BAR).with_notes([note(5, 60, 0)]),
                        },
                        PlacedClip {
                            track: first,
                            clip: Clip::new(testing::clip_id(6), 0, BAR)
                                .with_notes([note(5, 60, 0)]),
                        },
                    ],
                },
                CommandError::NoteListedTwice(note_id(5)),
            ),
            (
                Command::RemoveClips { clips: vec![] },
                CommandError::NoClips,
            ),
            (
                Command::RemoveClips {
                    clips: vec![existing, unknown_clip],
                },
                CommandError::UnknownClip(unknown_clip),
            ),
            (
                Command::RemoveClips {
                    clips: vec![existing, existing],
                },
                CommandError::ClipListedTwice(existing),
            ),
            (Command::SetClips { clips: vec![] }, CommandError::NoClips),
            (
                position(unknown_clip, first, 0, BAR),
                CommandError::UnknownClip(unknown_clip),
            ),
            (
                position(existing, unknown_track, 0, BAR),
                CommandError::UnknownTrack(unknown_track),
            ),
            (
                position(existing, first, 0, 0),
                CommandError::EmptyClip(existing),
            ),
            (
                position(existing, first, crate::time::MAX_TICKS - 10, BAR),
                CommandError::ClipTooLate(existing),
            ),
            (
                Command::SetClips {
                    clips: vec![
                        ClipPosition {
                            id: existing,
                            track: first,
                            start: 0,
                            length: BAR,
                        };
                        2
                    ],
                },
                CommandError::ClipListedTwice(existing),
            ),
        ];
        for (command, error) in cases {
            assert_rejected(&project, command, error);
        }
    }

    #[test]
    fn note_ids_are_unique_across_the_project() {
        let project = with_three_tracks();
        // Note 0 is in the second track's clip, so the first track's clip
        // can't have a note with its ID.
        assert_rejected(
            &project,
            Command::AddNotes {
                clip: clip_id(&project),
                notes: vec![note(0, 72, 0)],
            },
            CommandError::NoteAlreadyExists(note_id(0)),
        );
        assert_eq!(
            CommandError::NoteAlreadyExists(note_id(0)).to_string(),
            format!("note {} is already in the project", note_id(0))
        );
    }

    #[test]
    fn copies_of_clips_and_tracks_share_no_ids_with_the_originals() {
        let mut project = with_three_tracks();
        project
            .apply(&Command::AddClips {
                clips: vec![PlacedClip {
                    track: testing::track_id(0),
                    clip: Clip::new(testing::clip_id(1), 3 * BAR, BAR)
                        .with_notes([note(1, 50, 0), note(2, 52, 480)]),
                }],
            })
            .unwrap();
        let original = project.track(testing::track_id(0)).unwrap().clone();

        let mut next = 100;
        let mut note_ids = || {
            next += 1;
            note_id(next)
        };
        let mut clip_index = 10;
        let copy = original.copy(
            testing::track_id(10),
            project.next_track_name(SourceKind::Synth),
            || {
                clip_index += 1;
                testing::clip_id(clip_index)
            },
            &mut note_ids,
        );
        assert_eq!(copy.name(), "Synth 4");
        assert_eq!(copy.source(), original.source());
        assert_eq!(copy.mixer(), original.mixer());
        assert_eq!(copy.clips().len(), 2);
        for (copied, was) in copy.clips().iter().zip(original.clips()) {
            assert_ne!(copied.id(), was.id());
            assert_eq!(
                (copied.start(), copied.length()),
                (was.start(), was.length())
            );
            let shape = |clip: &Clip| {
                let mut notes: Vec<_> = clip
                    .notes()
                    .map(|note| (note.pitch, note.velocity, note.start, note.length))
                    .collect();
                notes.sort_unstable();
                notes
            };
            assert_eq!(shape(copied), shape(was));
            for note in copied.notes() {
                assert!(was.note(note.id).is_none());
            }
        }
        // Being independent, the copy can go straight into the project.
        apply_and_check_undo(
            &project,
            Command::AddTracks {
                tracks: vec![placed(3, copy)],
            },
        );

        let clip = project.clip(testing::clip_id(1)).unwrap();
        let clip_copy = clip.copy(testing::clip_id(20), note_ids);
        assert!(clip_copy.notes().all(|note| clip.note(note.id).is_none()));
        apply_and_check_undo(
            &project,
            Command::AddClips {
                clips: vec![PlacedClip {
                    track: track_id(&project),
                    clip: clip_copy,
                }],
            },
        );
    }

    #[test]
    fn new_track_names_count_on_from_the_highest_of_their_kind() {
        use SourceKind::{Drums, Synth};
        let mut project = testing::project();
        assert_eq!(project.next_track_name(Synth), "Synth 2");
        assert_eq!(project.next_track_name(Drums), "Drums 2");
        project
            .apply(&Command::AddTracks {
                tracks: vec![placed(1, synth_track(0, 2)), placed(2, synth_track(1, 3))],
            })
            .unwrap();
        assert_eq!(project.next_track_name(Synth), "Synth 4");
        assert_eq!(
            project.next_track_name(Drums),
            "Drums 2",
            "synths don't count"
        );
        // Deleting "Synth 2" renames nothing, and the next is still 4.
        project
            .apply(&Command::RemoveTracks {
                tracks: vec![testing::track_id(0)],
            })
            .unwrap();
        assert_eq!(names(&project), ["Synth 1", "Synth 3", "Drums 1"]);
        assert_eq!(project.next_track_name(Synth), "Synth 4");
        // Other names don't count, and reordering changes nothing.
        project
            .apply(&Command::AddTracks {
                tracks: vec![
                    placed(
                        0,
                        Track {
                            name: "Bass 9".into(),
                            ..synth_track(2, 0)
                        },
                    ),
                    placed(
                        1,
                        Track {
                            name: "Drumsy 7".into(),
                            ..synth_track(3, 0)
                        },
                    ),
                ],
            })
            .unwrap();
        assert_eq!(project.next_track_name(Synth), "Synth 4");
        assert_eq!(project.next_track_name(Drums), "Drums 2");
        project
            .apply(&Command::RemoveTracks {
                tracks: project.tracks().iter().map(Track::id).collect(),
            })
            .unwrap();
        assert_eq!(project.next_track_name(Synth), "Synth 1");
        assert_eq!(project.next_track_name(Drums), "Drums 1");
    }

    #[test]
    fn a_clip_moves_only_between_tracks_of_the_same_kind() {
        let (project, drums, drum_clip) = with_drums();
        let synth = track_id(&project);
        let synth_clip = clip_id(&project);
        let to = |id, track| Command::SetClips {
            clips: vec![ClipPosition {
                id,
                track,
                start: BAR,
                length: BAR,
            }],
        };
        // Even an empty clip can't change kind, either way.
        assert_rejected(
            &project,
            to(synth_clip, drums),
            CommandError::ClipToOtherKind {
                clip: synth_clip,
                track: drums,
            },
        );
        assert_rejected(
            &project,
            to(drum_clip, synth),
            CommandError::ClipToOtherKind {
                clip: drum_clip,
                track: synth,
            },
        );
        // A move within its own track, or to another of its kind, is fine.
        apply_and_check_undo(&project, to(drum_clip, drums));
        let mut two_kits = project.clone();
        let second = testing::track_id(5);
        two_kits
            .apply(&Command::AddTracks {
                tracks: vec![placed(
                    2,
                    Track::new(second, "Drums 2", Source::Drums(KitSettings::default())),
                )],
            })
            .unwrap();
        apply_and_check_undo(&two_kits, to(drum_clip, second));
        assert!(
            CommandError::ClipToOtherKind {
                clip: drum_clip,
                track: synth
            }
            .to_string()
            .contains("different kind of track")
        );
    }

    #[test]
    fn the_song_ends_a_bar_after_the_last_clip() {
        let mut project = with_three_tracks();
        assert_eq!(project.song_end(), 5 * BAR, "the first clip ends last");
        project
            .apply(&Command::SetClips {
                clips: vec![ClipPosition {
                    id: testing::clip_id(0),
                    track: testing::track_id(1),
                    start: 7 * BAR + 960,
                    length: BAR,
                }],
            })
            .unwrap();
        assert_eq!(project.song_end(), 9 * BAR + 960);
        project
            .apply(&Command::RemoveTracks {
                tracks: project.tracks().iter().map(Track::id).collect(),
            })
            .unwrap();
        assert_eq!(project.song_end(), BAR, "with no clips");
    }

    #[test]
    fn set_loop_sets_the_region_and_undoes() {
        let project = testing::project();
        let mut changed = project.clone();
        let inverse = changed
            .apply(&Command::SetLoop {
                start_bar: 2,
                bars: 20,
            })
            .unwrap();
        assert_eq!(
            inverse,
            Command::SetLoop {
                start_bar: 0,
                bars: 4,
            }
        );
        let transport = changed.transport();
        assert_eq!(
            (transport.loop_start(), transport.loop_length()),
            (2 * BAR, 20 * BAR)
        );
        assert_eq!(
            changed.clip(clip_id(&project)).unwrap().length(),
            4 * BAR,
            "clips keep their lengths"
        );
        apply_and_check_undo(
            &project,
            Command::SetLoop {
                start_bar: 3,
                bars: 1,
            },
        );

        let last_bar = u32::try_from(crate::time::MAX_TICKS / BAR).unwrap();
        apply_and_check_undo(
            &project,
            Command::SetLoop {
                start_bar: last_bar - 1,
                bars: 1,
            },
        );
        for (start_bar, bars) in [(0, 0), (last_bar, 1), (u32::MAX, u32::MAX)] {
            assert_rejected(
                &project,
                Command::SetLoop { start_bar, bars },
                CommandError::LoopOutOfRange { start_bar, bars },
            );
        }
    }

    #[test]
    fn set_loop_enabled_switches_it_and_undoes() {
        let project = testing::project();
        let changed = apply_and_check_undo(&project, Command::SetLoopEnabled { enabled: false });
        assert!(!changed.transport().loop_enabled());
        let back = apply_and_check_undo(&changed, Command::SetLoopEnabled { enabled: true });
        assert_eq!(back, project);
    }

    #[test]
    fn set_loop_length_is_refused_once_the_loop_and_first_clip_differ() {
        let project = testing::project();
        let clip = clip_id(&project);
        let track = track_id(&project);
        let resized = Command::SetClips {
            clips: vec![ClipPosition {
                id: clip,
                track,
                start: 0,
                length: 2 * BAR,
            }],
        };
        let long_loop = Command::SetLoop {
            start_bar: 0,
            bars: 17,
        };
        for different in [resized, long_loop] {
            let changed = project.clone().replay([&different]).unwrap();
            assert_rejected(
                &changed,
                Command::SetLoopLength { bars: 3 },
                CommandError::LoopLengthUnavailable,
            );
        }

        // It still works when they match, from a moved loop region, and
        // with no clips at all.
        let moved = project
            .clone()
            .replay([&Command::SetLoop {
                start_bar: 2,
                bars: 4,
            }])
            .unwrap();
        let changed = apply_and_check_undo(&moved, Command::SetLoopLength { bars: 6 });
        assert_eq!(changed.transport().loop_start(), 2 * BAR);
        assert_eq!(changed.clip(clip).unwrap().length(), 6 * BAR);
        let empty = project
            .clone()
            .replay([&Command::RemoveClips { clips: vec![clip] }])
            .unwrap();
        apply_and_check_undo(&empty, Command::SetLoopLength { bars: 6 });
    }

    #[test]
    fn the_demo_loop_still_loads() {
        let list: crate::CommandList =
            serde_json::from_str(include_str!("../../../examples/demo-loop.json")).unwrap();
        let project = list.build().unwrap();
        let transport = project.transport();
        assert_eq!(transport.loop_length(), 2 * BAR);
        assert_eq!(project.tracks()[0].clips()[0].length(), 2 * BAR);
        assert!(project.tracks()[0].clips()[0].notes().len() > 0);
    }

    /// [`with_two_notes`], with a second clip that has a note of its own.
    fn with_two_clips() -> Project {
        let mut project = with_two_notes();
        let track = track_id(&project);
        project
            .apply(&Command::AddClips {
                clips: vec![PlacedClip {
                    track,
                    clip: Clip::new(testing::clip_id(1), 2 * BAR, BAR).with_notes([note(5, 50, 0)]),
                }],
            })
            .unwrap();
        project
    }

    /// A notes command copies the notes of the clip it changes, and only
    /// that clip's: the other clip still shares its notes with the project
    /// before, and the project before keeps its own notes. See RFC-004,
    /// "How changes are spotted".
    #[test]
    fn a_notes_command_copies_only_its_own_clips_notes() {
        let project = with_two_clips();
        let (clip, other) = (clip_id(&project), testing::clip_id(1));
        let commands = [
            Command::AddNotes {
                clip,
                notes: vec![note(6, 62, 0)],
            },
            Command::RemoveNotes {
                clip,
                notes: vec![note_id(0)],
            },
            Command::SetNotes {
                clip,
                notes: vec![note(0, 61, 0)],
            },
        ];
        for command in commands {
            let changed = apply_and_check_undo(&project, command.clone());
            let shares = |id| {
                changed
                    .clip(id)
                    .unwrap()
                    .shares_notes(project.clip(id).unwrap())
            };
            assert!(!shares(clip), "{command:?} copied its clip's notes");
            assert!(
                shares(other),
                "{command:?} left the other clip's notes shared"
            );
            assert_eq!(
                project,
                with_two_clips(),
                "{command:?} changed only its copy"
            );
        }

        // A rejected one copies nothing.
        let mut attempt = project.clone();
        attempt
            .apply(&Command::RemoveNotes {
                clip,
                notes: vec![note_id(9)],
            })
            .unwrap_err();
        for id in [clip, other] {
            assert!(
                attempt
                    .clip(id)
                    .unwrap()
                    .shares_notes(project.clip(id).unwrap())
            );
        }
    }

    /// Moving a clip, to another place or track, and resizing it, keeps
    /// the same notes, by pointer, and so does undoing it.
    #[test]
    fn moving_or_resizing_a_clip_shares_its_notes() {
        let project = with_three_tracks();
        let clip = testing::clip_id(0);
        let moves = [
            (track_id(&project), BAR, BAR),
            (testing::track_id(0), 3 * BAR, BAR),
            (testing::track_id(0), BAR, 4 * BAR),
            (testing::track_id(1), 0, BAR / 2),
        ];
        for (track, start, length) in moves {
            let command = Command::SetClips {
                clips: vec![ClipPosition {
                    id: clip,
                    track,
                    start,
                    length,
                }],
            };
            let mut moved = project.clone();
            let inverse = moved.apply(&command).unwrap();
            let after = moved.clip(clip).unwrap();
            assert_eq!((after.start(), after.length()), (start, length));
            assert!(
                after.shares_notes(project.clip(clip).unwrap()),
                "{command:?}"
            );
            moved.apply(&inverse).unwrap();
            assert!(
                moved
                    .clip(clip)
                    .unwrap()
                    .shares_notes(project.clip(clip).unwrap())
            );
        }

        // Setting the loop's length resizes the first clip too.
        let project = with_two_notes();
        let mut resized = project.clone();
        resized.apply(&Command::SetLoopLength { bars: 3 }).unwrap();
        let (before, after) = (
            project.clip(clip_id(&project)).unwrap(),
            resized.clip(clip_id(&project)).unwrap(),
        );
        assert_ne!(before.length(), after.length());
        assert!(after.shares_notes(before));
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

        #[test]
        fn no_commands_leave_a_note_off_the_kit_on_a_drum_track(
            commands in testing::any_commands(60),
        ) {
            let mut project = testing::project();
            for command in &commands {
                let _ = project.apply(command);
                for track in project.tracks() {
                    if let Source::Drums(_) = track.source() {
                        let notes = track.clips().iter().flat_map(Clip::notes);
                        for note in notes {
                            prop_assert!(crate::DrumSound::at_pitch(note.pitch).is_some(), "{note:?} after {command:?}");
                        }
                    }
                }
            }
        }

        #[test]
        fn copies_of_any_project_s_tracks_go_straight_back_in(
            setup in testing::any_commands(60),
        ) {
            let mut project = testing::project();
            for command in &setup {
                let _ = project.apply(command);
            }
            let used: HashSet<uuid::Uuid> = project
                .tracks()
                .iter()
                .flat_map(|track| {
                    std::iter::once(track.id().as_uuid()).chain(track.clips().iter().flat_map(|clip| {
                        std::iter::once(clip.id().as_uuid()).chain(clip.notes().map(|note| note.id.as_uuid()))
                    }))
                })
                .collect();
            let copies: Vec<_> = project
                .tracks()
                .iter()
                .take(Project::MAX_TRACKS - project.tracks().len())
                .enumerate()
                .map(|(index, track)| PlacedTrack {
                    index,
                    track: track.copy(TrackId::random(), format!("Synth {index}"), ClipId::random, NoteId::random),
                })
                .collect();
            for copy in &copies {
                let track = &copy.track;
                prop_assert!(!used.contains(&track.id().as_uuid()));
                for clip in track.clips() {
                    prop_assert!(!used.contains(&clip.id().as_uuid()));
                    prop_assert!(clip.notes().all(|note| !used.contains(&note.id.as_uuid())));
                }
            }
            if !copies.is_empty() {
                let mut with_copies = project.clone();
                let result = with_copies.apply(&Command::AddTracks { tracks: copies });
                prop_assert!(result.is_ok(), "{:?}", result);
            }
        }
    }
}
