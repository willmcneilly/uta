//! Tracks, clips and notes. See RFC-002, "The shared model", point 2.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::time::{MAX_TICKS, Ticks};
use crate::{ClipId, CommandError, NoteId, SynthSettings, TrackId};

/// A chain that makes sound: a source, then effects, then a mixer strip.
/// It holds the clips that play through it.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub(crate) id: TrackId,
    pub(crate) source: Source,
    pub(crate) effects: Vec<Effect>,
    pub(crate) mixer: MixerStrip,
    pub(crate) clips: Vec<Clip>,
}

impl Track {
    pub fn id(&self) -> TrackId {
        self.id
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

    pub fn clips(&self) -> &[Clip] {
        &self.clips
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

/// A track's volume, pan and mute, applied after its effects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerStrip {
    pub volume_db: f32,
    /// From -1 (left) through 0 (centre) to 1 (right).
    pub pan: f32,
    pub mute: bool,
}

impl Default for MixerStrip {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
        }
    }
}

/// A container placed on a track: a start, a length and its notes.
///
/// Notes that fall outside the clip's length are kept but not played, so
/// shortening the clip and lengthening it again brings them back.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub(crate) id: ClipId,
    pub(crate) start: Ticks,
    pub(crate) length: Ticks,
    /// Keyed by ID, so the order notes were added in doesn't matter: removing
    /// a note and adding it back gives exactly the same clip.
    pub(crate) notes: BTreeMap<NoteId, Note>,
}

impl Clip {
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

    /// The clip's notes, in order of ID.
    pub fn notes(&self) -> impl ExactSizeIterator<Item = &Note> {
        self.notes.values()
    }

    pub fn note(&self, id: NoteId) -> Option<&Note> {
        self.notes.get(&id)
    }
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
