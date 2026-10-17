//! Mono mode's held notes: every note started and not let go, in the order
//! they started, so the latest can play and letting go of it can return to
//! the one before. See RFC-008, "In the engine" (Mono mode).
//!
//! They're plain values (key, pitch, velocity) in a fixed array, so holding
//! and letting go of notes on the audio thread never allocates or frees.

use super::{NoteKey, NoteOn};

/// How many notes Mono remembers as held. Past it, the oldest is forgotten,
/// as if let go.
pub(crate) const HELD_NOTES: usize = 16;

/// The held notes, oldest first.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HeldNotes {
    notes: [NoteOn; HELD_NOTES],
    len: usize,
}

impl Default for HeldNotes {
    fn default() -> Self {
        let none = NoteOn {
            key: NoteKey(0),
            pitch: 0,
            velocity: 0,
            sequenced: false,
        };
        Self {
            notes: [none; HELD_NOTES],
            len: 0,
        }
    }
}

impl HeldNotes {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The held notes, oldest first.
    pub(crate) fn notes(&self) -> &[NoteOn] {
        &self.notes[..self.len]
    }

    /// The note that started last: the one Mono plays.
    pub(crate) fn latest(&self) -> Option<NoteOn> {
        self.notes().last().copied()
    }

    pub(crate) fn contains(&self, key: NoteKey) -> bool {
        self.notes().iter().any(|note| note.key == key)
    }

    /// Holds `note` as the latest. A note already held with its key is let
    /// go first, and if the list is full the oldest is forgotten.
    pub(crate) fn push(&mut self, note: NoteOn) {
        self.remove(note.key);
        if self.len == HELD_NOTES {
            self.notes.copy_within(1.., 0);
            self.len -= 1;
        }
        self.notes[self.len] = note;
        self.len += 1;
    }

    /// Holds `note` as the oldest, under the rest: for a note chased on
    /// Play, as the notes already under way come latest start first. Does
    /// nothing if its key is already held, or the list is full, when it
    /// would be the one forgotten.
    pub(crate) fn push_oldest(&mut self, note: NoteOn) {
        if self.len == HELD_NOTES || self.contains(note.key) {
            return;
        }
        self.notes.copy_within(0..self.len, 1);
        self.notes[0] = note;
        self.len += 1;
    }

    /// Lets go of the note with this key. Whether it was held.
    pub(crate) fn remove(&mut self, key: NoteKey) -> bool {
        let Some(index) = self.notes().iter().position(|note| note.key == key) else {
            return false;
        };
        self.notes.copy_within(index + 1..self.len, index);
        self.len -= 1;
        true
    }

    /// Lets go of every note `keep` doesn't accept. Bounded by
    /// [`HELD_NOTES`].
    pub(crate) fn retain(&mut self, keep: impl Fn(&NoteOn) -> bool) {
        let mut kept = 0;
        for index in 0..self.len {
            let note = self.notes[index];
            if keep(&note) {
                self.notes[kept] = note;
                kept += 1;
            }
        }
        self.len = kept;
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(key: u128) -> NoteOn {
        NoteOn {
            key: NoteKey(key),
            pitch: key as u8,
            velocity: 100,
            sequenced: key.is_multiple_of(2),
        }
    }

    fn keys(held: &HeldNotes) -> Vec<u128> {
        held.notes().iter().map(|note| note.key.0).collect()
    }

    #[test]
    fn the_latest_note_is_last_and_letting_go_returns_to_the_one_before() {
        let mut held = HeldNotes::default();
        assert_eq!(held.latest(), None);
        for key in [1, 2, 3] {
            held.push(note(key));
        }
        assert_eq!(held.latest(), Some(note(3)));
        assert!(held.remove(NoteKey(3)));
        assert_eq!(held.latest(), Some(note(2)));
        // Letting go of one under the latest leaves the latest.
        assert!(held.remove(NoteKey(1)));
        assert_eq!(keys(&held), [2]);
        assert!(!held.remove(NoteKey(1)), "already let go");
    }

    #[test]
    fn the_same_key_again_becomes_the_latest_once() {
        let mut held = HeldNotes::default();
        for key in [1, 2, 3, 1] {
            held.push(note(key));
        }
        assert_eq!(keys(&held), [2, 3, 1]);
    }

    #[test]
    fn past_sixteen_the_oldest_is_forgotten() {
        let mut held = HeldNotes::default();
        for key in 0..HELD_NOTES as u128 + 2 {
            held.push(note(key));
        }
        assert_eq!(held.len(), HELD_NOTES);
        assert_eq!(keys(&held), (2..HELD_NOTES as u128 + 2).collect::<Vec<_>>());
    }

    #[test]
    fn a_chased_note_goes_under_the_rest() {
        let mut held = HeldNotes::default();
        held.push(note(5));
        held.push_oldest(note(4));
        held.push_oldest(note(3));
        held.push_oldest(note(5));
        assert_eq!(keys(&held), [3, 4, 5]);
        for key in 10..30 {
            held.push(note(key));
        }
        held.push_oldest(note(99));
        assert!(!held.contains(NoteKey(99)), "full: it would be forgotten");
    }

    #[test]
    fn retain_keeps_the_order() {
        let mut held = HeldNotes::default();
        for key in 1..=6 {
            held.push(note(key));
        }
        held.retain(|note| !note.sequenced);
        assert_eq!(keys(&held), [1, 3, 5]);
        held.clear();
        assert!(held.is_empty());
    }
}
