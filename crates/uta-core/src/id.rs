//! Permanent IDs for the things in a project.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Defines a permanent ID type. They're all UUIDs, but each is its own type,
/// so a note's ID can't be passed where a clip's is expected.
macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// A new, random ID.
            pub fn random() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(
    /// A project's permanent ID. It never changes, so saved commands and files
    /// can always find the project they belong to.
    ProjectId
);

id_type!(
    /// A track's permanent ID.
    TrackId
);

id_type!(
    /// A clip's permanent ID.
    ClipId
);

id_type!(
    /// A note's permanent ID. It's chosen when the command that adds the note
    /// is built, before the project applies it, so replaying the command
    /// always gives the note the same ID.
    NoteId
);
