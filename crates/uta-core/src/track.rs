//! Tracks, clips and notes. See RFC-002, "The shared model", point 2, and
//! RFC-003, "The shared model, extended".

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::time::{MAX_TICKS, Ticks};
use crate::{ClipId, CommandError, NoteId, SynthSettings, TrackId};

/// A chain that makes sound: a source, then effects, then a mixer strip.
/// It holds the clips that play through it.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub(crate) id: TrackId,
    pub(crate) name: String,
    pub(crate) source: Source,
    pub(crate) effects: Vec<Effect>,
    pub(crate) mixer: MixerStrip,
    /// In order of start, then ID, so the order they were added in doesn't
    /// matter: removing a clip and adding it back gives exactly the same
    /// track.
    pub(crate) clips: Vec<Clip>,
}

impl Track {
    /// A track with no effects, the default mixer strip and no clips.
    pub fn new(id: TrackId, name: impl Into<String>, source: Source) -> Self {
        Self {
            id,
            name: name.into(),
            source,
            effects: Vec::new(),
            mixer: MixerStrip::default(),
            clips: Vec::new(),
        }
    }

    /// This track with the given mixer strip.
    pub fn with_mixer(self, mixer: MixerStrip) -> Self {
        Self { mixer, ..self }
    }

    /// This track with `clips` added to it.
    pub fn with_clips(mut self, clips: impl IntoIterator<Item = Clip>) -> Self {
        for clip in clips {
            self.insert_clip(clip);
        }
        self
    }

    pub fn id(&self) -> TrackId {
        self.id
    }

    /// The track's name, such as "Synth 2". It's chosen when the track is
    /// added (see [`crate::Project::next_track_name`]) and stays the same
    /// when tracks are reordered or deleted.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    /// The effects after the source, in order. Always empty for now.
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    pub fn mixer(&self) -> &MixerStrip {
        &self.mixer
    }

    /// The track's clips, in order of start, then ID. They may overlap.
    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|clip| clip.id == id)
    }

    /// A copy of this track with new IDs for it, its clips and every note,
    /// for duplicating it. `name` is the copy's name, and `new_clip_id` and
    /// `new_note_id` give the copy's clips and notes their IDs.
    pub fn copy(
        &self,
        id: TrackId,
        name: impl Into<String>,
        mut new_clip_id: impl FnMut() -> ClipId,
        mut new_note_id: impl FnMut() -> NoteId,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            source: self.source.clone(),
            effects: self.effects.clone(),
            mixer: self.mixer,
            clips: Vec::new(),
        }
        .with_clips(
            self.clips
                .iter()
                .map(|clip| clip.copy(new_clip_id(), &mut new_note_id)),
        )
    }

    /// Adds `clip` in its place in the order.
    pub(crate) fn insert_clip(&mut self, clip: Clip) {
        let index = self
            .clips
            .partition_point(|other| (other.start, other.id) < (clip.start, clip.id));
        self.clips.insert(index, clip);
    }

    /// Removes the clip with this ID, if it's on the track.
    pub(crate) fn remove_clip(&mut self, id: ClipId) -> Option<Clip> {
        let index = self.clips.iter().position(|clip| clip.id == id)?;
        Some(self.clips.remove(index))
    }
}

/// What makes a track's sound.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// The built-in synth. Plugins come later.
    Synth(SynthSettings),
}

/// An effect on a track. There are none yet, so an effects list is always
/// empty: this type has no values.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {}

/// A track's volume, pan, mute and solo, applied after its effects.
///
/// A track plays if it isn't muted, and either no track is soloed or it is.
/// So mute always wins, even on a soloed track. See RFC-003, "The mixer in
/// the track headers".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixerStrip {
    /// From [`Self::MIN_VOLUME_DB`] to [`Self::MAX_VOLUME_DB`].
    pub volume_db: f32,
    /// From -1 (left) through 0 (centre) to 1 (right).
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
}

impl MixerStrip {
    pub const MIN_VOLUME_DB: f32 = -60.0;
    pub const MAX_VOLUME_DB: f32 = 6.0;
    pub const MIN_PAN: f32 = -1.0;
    pub const MAX_PAN: f32 = 1.0;

    /// Checks the volume and pan are in range.
    pub(crate) fn validate(&self) -> Result<(), CommandError> {
        if !(Self::MIN_VOLUME_DB..=Self::MAX_VOLUME_DB).contains(&self.volume_db) {
            return Err(CommandError::VolumeOutOfRange(self.volume_db));
        }
        if !(Self::MIN_PAN..=Self::MAX_PAN).contains(&self.pan) {
            return Err(CommandError::PanOutOfRange(self.pan));
        }
        Ok(())
    }
}

impl Default for MixerStrip {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
        }
    }
}

/// A container placed on a track: a start, a length and its notes.
///
/// Notes that fall outside the clip's length are kept but not played, so
/// shortening the clip and lengthening it again brings them back.
///
/// A clip also has a content offset and a content length: which part of its
/// notes it plays. They're fixed for now at no offset and the clip's own
/// length. They leave room for clips that repeat their contents, and for
/// trimming a clip's left edge, without changing a clip's shape. See
/// RFC-003, "The shared model, extended", point 2.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub(crate) id: ClipId,
    pub(crate) start: Ticks,
    pub(crate) length: Ticks,
    pub(crate) content_offset: Ticks,
    pub(crate) content_length: Ticks,
    /// Keyed by ID, so the order notes were added in doesn't matter: removing
    /// a note and adding it back gives exactly the same clip.
    pub(crate) notes: BTreeMap<NoteId, Note>,
}

impl Clip {
    /// An empty clip.
    pub fn new(id: ClipId, start: Ticks, length: Ticks) -> Self {
        Self {
            id,
            start,
            length,
            content_offset: 0,
            content_length: length,
            notes: BTreeMap::new(),
        }
    }

    /// This clip with `notes` added to it. A note with the same ID as one
    /// already in it replaces it.
    pub fn with_notes(mut self, notes: impl IntoIterator<Item = Note>) -> Self {
        self.notes
            .extend(notes.into_iter().map(|note| (note.id, note)));
        self
    }

    pub fn id(&self) -> ClipId {
        self.id
    }

    /// Where the clip starts, in ticks from the start of the song.
    pub fn start(&self) -> Ticks {
        self.start
    }

    pub fn length(&self) -> Ticks {
        self.length
    }

    /// Where the clip ends, in ticks from the start of the song.
    pub fn end(&self) -> Ticks {
        self.start + self.length
    }

    /// Where the part of its notes the clip plays begins, in ticks from the
    /// start of its notes. Always 0 for now.
    pub fn content_offset(&self) -> Ticks {
        self.content_offset
    }

    /// How long the part of its notes the clip plays is. Always the clip's
    /// length for now.
    pub fn content_length(&self) -> Ticks {
        self.content_length
    }

    /// The clip's notes, in order of ID.
    pub fn notes(&self) -> impl ExactSizeIterator<Item = &Note> {
        self.notes.values()
    }

    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.notes.get(&id)
    }

    /// A copy of this clip with new IDs for it and every note, for pasting
    /// or duplicating it. `new_note_id` gives each note its ID.
    pub fn copy(&self, id: ClipId, mut new_note_id: impl FnMut() -> NoteId) -> Self {
        Self {
            id,
            start: self.start,
            length: self.length,
            content_offset: self.content_offset,
            content_length: self.content_length,
            notes: BTreeMap::new(),
        }
        .with_notes(self.notes.values().map(|note| Note {
            id: new_note_id(),
            ..*note
        }))
    }

    /// Sets where the clip is and how long it is. Its contents follow its
    /// length.
    pub(crate) fn set_span(&mut self, start: Ticks, length: Ticks) {
        self.start = start;
        self.length = length;
        self.content_length = length;
    }

    /// Checks the clip has a length and ends in time, and its notes are
    /// valid.
    pub(crate) fn validate(&self) -> Result<(), CommandError> {
        check_span(self.id, self.start, self.length)?;
        self.notes.values().try_for_each(Note::validate)
    }
}

/// Checks a clip at `start` lasting `length` has a length and ends in time.
pub(crate) fn check_span(id: ClipId, start: Ticks, length: Ticks) -> Result<(), CommandError> {
    if length == 0 {
        return Err(CommandError::EmptyClip(id));
    }
    if start.checked_add(length).is_none_or(|end| end > MAX_TICKS) {
        return Err(CommandError::ClipTooLate(id));
    }
    Ok(())
}

/// A note in a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: NoteId,
    /// The MIDI note number, from 0 to 127. Middle C is 60.
    pub pitch: u8,
    /// How hard it's played, from 1 to 127.
    pub velocity: u8,
    /// Where it starts, in ticks from the start of its clip.
    pub start: Ticks,
    /// How long it lasts, in ticks. At least 1.
    pub length: Ticks,
}

impl Note {
    pub const MAX_PITCH: u8 = 127;
    pub const MIN_VELOCITY: u8 = 1;
    pub const MAX_VELOCITY: u8 = 127;

    /// Checks the note's values are in range.
    pub(crate) fn validate(&self) -> Result<(), CommandError> {
        if self.pitch > Self::MAX_PITCH {
            return Err(CommandError::PitchOutOfRange {
                note: self.id,
                pitch: self.pitch,
            });
        }
        if !(Self::MIN_VELOCITY..=Self::MAX_VELOCITY).contains(&self.velocity) {
            return Err(CommandError::VelocityOutOfRange {
                note: self.id,
                velocity: self.velocity,
            });
        }
        if self.length == 0 {
            return Err(CommandError::EmptyNote(self.id));
        }
        if self
            .start
            .checked_add(self.length)
            .is_none_or(|end| end > MAX_TICKS)
        {
            return Err(CommandError::NoteTooLate(self.id));
        }
        Ok(())
    }
}
