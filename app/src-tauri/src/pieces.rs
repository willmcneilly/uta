//! Revisions for the pieces of the project view: the parts that are big and
//! change on their own, so they're sent to the UI only when they change
//! (RFC-004, part 2). Only a clip's notes are a piece today.
//!
//! The core keeps each piece behind an `Arc` and copies it only when a
//! command changes it, so "has it changed since it was sent?" is "is it a
//! different pointer?". Revisions are the UI's bookkeeping, not project
//! data: they're never saved, and nothing in `uta-core` knows about them.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use uta_core::ClipId;

/// A piece of the project view: its kind and the ID of what it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PieceId {
    /// A clip's notes.
    Notes(ClipId),
}

/// What was last sent of one piece.
struct Sent {
    /// Held so the memory can't be freed and reused for another piece's
    /// data, which would then look unchanged. The extra reference also
    /// means the core's `Arc::make_mut` always copies before an edit, so an
    /// edited piece can never keep its pointer.
    data: Arc<dyn Any + Send + Sync>,
    revision: u64,
    /// The update that last saw the piece in the project.
    seen: u64,
}

/// For each piece, the data last sent to the UI and the revision it was
/// given. A piece gets the next revision whenever its data is a different
/// `Arc`, so revisions only ever go up, even after an undo puts back data
/// that was sent before.
#[derive(Default)]
pub struct Pieces {
    sent: HashMap<PieceId, Sent>,
    last_revision: u64,
    /// The latest update's sequence number.
    sequence: u64,
}

impl Pieces {
    /// Starts the next update and returns its sequence number. Every piece
    /// in the project should then be passed to [`Self::check`], and
    /// [`Self::finish`] called.
    pub fn start(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }

    /// The revision of `id`, whose data is now `data`, and whether that's a
    /// new revision the UI hasn't been sent, so `data` should go in the
    /// update. Records `data` as sent.
    pub fn check<T: Any + Send + Sync>(&mut self, id: PieceId, data: &Arc<T>) -> (u64, bool) {
        let sequence = self.sequence;
        if let Some(sent) = self.sent.get_mut(&id)
            && std::ptr::addr_eq(Arc::as_ptr(&sent.data), Arc::as_ptr(data))
        {
            sent.seen = sequence;
            return (sent.revision, false);
        }
        self.last_revision += 1;
        let sent = Sent {
            data: data.clone(),
            revision: self.last_revision,
            seen: sequence,
        };
        self.sent.insert(id, sent);
        (self.last_revision, true)
    }

    /// Forgets the pieces the update didn't see: they've left the project.
    /// If one comes back (undoing a delete), it gets a new revision.
    pub fn finish(&mut self) {
        let sequence = self.sequence;
        self.sent.retain(|_, sent| sent.seen == sequence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes() -> PieceId {
        PieceId::Notes(ClipId::random())
    }

    #[test]
    fn the_same_data_keeps_its_revision_and_isnt_sent_again() {
        let mut pieces = Pieces::default();
        let (id, data) = (notes(), Arc::new(vec![1]));
        pieces.start();
        let (revision, new) = pieces.check(id, &data);
        assert!(new);
        pieces.finish();
        pieces.start();
        assert_eq!(pieces.check(id, &data.clone()), (revision, false));
    }

    #[test]
    fn different_data_gets_a_higher_revision_even_if_it_was_sent_before() {
        let mut pieces = Pieces::default();
        let id = notes();
        let (old, new) = (Arc::new(vec![1]), Arc::new(vec![1]));
        let mut revisions = Vec::new();
        for data in [&old, &new, &old] {
            pieces.start();
            let (revision, sent) = pieces.check(id, data);
            assert!(sent);
            revisions.push(revision);
            pieces.finish();
        }
        assert!(revisions.is_sorted_by(|a, b| a < b), "{revisions:?}");
    }

    #[test]
    fn new_data_at_the_address_of_data_sent_before_is_still_new() {
        let mut pieces = Pieces::default();
        let id = notes();
        pieces.start();
        let sent = Arc::new(vec![1]);
        let address = Arc::as_ptr(&sent).addr();
        pieces.check(id, &sent);
        pieces.finish();
        // The caller lets go. If the tracker didn't hold its own reference,
        // the allocator would be free to put the next data at the same
        // address, and it would look unchanged.
        drop(sent);
        let next = Arc::new(vec![2]);
        pieces.start();
        let (_, new) = pieces.check(id, &next);
        assert!(new);
        assert_ne!(Arc::as_ptr(&next).addr(), address);
    }

    #[test]
    fn a_piece_that_left_and_came_back_is_sent_again_with_a_higher_revision() {
        let mut pieces = Pieces::default();
        let (id, data) = (notes(), Arc::new(vec![1]));
        pieces.start();
        let (before, _) = pieces.check(id, &data);
        pieces.finish();
        pieces.start();
        pieces.finish();
        pieces.start();
        let (after, sent) = pieces.check(id, &data);
        assert!(sent);
        assert!(after > before);
    }

    #[test]
    fn each_update_has_a_higher_sequence_number() {
        let mut pieces = Pieces::default();
        let first = pieces.start();
        pieces.finish();
        assert!(pieces.start() > first);
    }
}
