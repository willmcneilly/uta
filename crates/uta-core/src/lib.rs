//! Uta's project core: owns the project, applies commands and keeps undo
//! history. See RFC-001, "The project core owns the project".
//!
//! - [`Project`] is the data. It only changes through [`Command`]s, and
//!   applying one returns its inverse.
//! - [`Command`]s are serde-serialisable and carry [`COMMAND_FORMAT`], so
//!   saved commands keep loading as the format changes.
//! - [`Session`] owns a project with its undo and redo history, and numbers
//!   every change.
//! - Positions and lengths are musical: whole ticks, turned into samples by
//!   the [`time::TempoMap`]. See RFC-002, "The shared model".
//!
//! This crate has no audio or UI dependencies. The engine builds its "what to
//! play" snapshot from a [`Project`].

mod command;
mod command_list;
mod id;
mod project;
mod session;
mod synth;
#[cfg(test)]
mod testing;
pub mod time;
mod track;

pub use command::{COMMAND_FORMAT, Command, CommandError};
pub use command_list::CommandList;
pub use id::{ClipId, NoteId, ProjectId, TrackId};
pub use project::{Project, Transport};
pub use session::{Applied, Session};
pub use synth::{SynthParam, SynthSettings, Waveform};
pub use track::{Clip, Effect, MixerStrip, Note, Source, Track};

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
