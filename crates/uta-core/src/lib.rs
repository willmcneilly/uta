//! Uta's project core: owns the project, applies commands and keeps undo
//! history. See RFC-001, "The project core owns the project".
//!
//! - [`Project`] is the data. It only changes through [`Command`]s, and
//!   applying one returns its inverse.
//! - [`Command`]s are serde-serialisable and carry [`COMMAND_FORMAT`], so
//!   saved commands keep loading as the format changes.
//! - [`Session`] owns a project with its undo and redo history, and numbers
//!   every change.
//! - [`Clip::trims_under`] works out how an edit trims the notes it covers,
//!   within one clip. Clip commands never trim notes.
//! - [`Clip::copy`] and [`Track::copy`] copy with new IDs throughout, for
//!   pasting and duplicating.
//! - Each track, and the master, has a chain of up to [`MAX_EFFECTS`]
//!   insert [`Effect`]s: a track's between its source and its mixer strip,
//!   the master's between the sum of the tracks and the master volume. See
//!   RFC-007.
//! - Positions and lengths are musical: whole ticks, turned into samples by
//!   the [`time::TempoMap`]. See RFC-002, "The shared model".
//!
//! This crate has no audio or UI dependencies. The engine builds its "what to
//! play" snapshot from a [`Project`].

mod command;
mod command_list;
mod drums;
mod effect;
mod id;
mod project;
mod session;
mod setting;
mod synth;
#[cfg(test)]
mod testing;
pub mod time;
mod track;
mod trim;

pub use command::{
    COMMAND_FORMAT, ClipPosition, Command, CommandError, PlacedClip, PlacedEffect, PlacedTrack,
};
pub use command_list::CommandList;
pub use drums::{
    ClapSettings, ClosedHatSettings, CymbalSettings, DrumParam, DrumParamError, DrumSound,
    HighTomSettings, KIT, Kick909Settings, KickModel, KickSettings, KitRow, KitSettings,
    LowTomSettings, MAX_LEVEL_DB, MIN_LEVEL_DB, OpenHatSettings, SnareSettings,
};
pub use effect::{
    Chain, DelayParam, DelaySettings, Effect, EffectKind, EffectSettings, MAX_EFFECTS,
};
pub use id::{ClipId, EffectId, NoteId, ProjectId, TrackId};
pub use project::{Project, Transport};
pub use session::{Applied, Session};
pub use setting::{Choice, Description, Kind as SettingKind, Scale, Unit};
pub use synth::{FilterMode, SynthParam, SynthSettings, Waveform};
pub use track::{Clip, MixerStrip, Note, Notes, Source, SourceKind, Track};

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
