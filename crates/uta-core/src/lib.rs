//! Uta's project core: owns the project, applies commands and keeps undo
//! history. See RFC-001, "The project core owns the project".
//!
//! - [`Project`] is the data. It only changes through [`Command`]s, and
//!   applying one returns its inverse.
//! - [`Command`]s are serde-serialisable and carry [`COMMAND_FORMAT`], so
//!   saved commands keep loading as the format changes.
//! - [`Session`] owns a project with its undo and redo history, and numbers
//!   every change.
//!
//! This crate has no audio or UI dependencies. The engine builds its "what to
//! play" snapshot from a [`Project`].

mod command;
mod project;
mod session;

pub use command::{COMMAND_FORMAT, Command, CommandError};
pub use project::{Project, ProjectId};
pub use session::{Applied, Session};

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
