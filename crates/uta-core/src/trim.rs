//! Trimming the notes an edit covers, so a note never hides another of the
//! same pitch. The edited notes win, and the covered part of any other note
//! goes.

use std::collections::{BTreeMap, HashSet};

use crate::time::Ticks;
use crate::{Clip, Command, Note, NoteId};

impl Clip {
    /// The commands that trim the notes `edited` now cover: a
    /// [`Command::SetNotes`] of the notes that get shorter, a
    /// [`Command::RemoveNotes`] of those with nothing left, and a
    /// [`Command::AddNotes`] of the pieces split off, whichever are needed,
    /// in that order. Empty if nothing overlaps. `edited` is the notes a drag
    /// moved or resized, or a paste added; IDs not in the clip are skipped.
    /// `new_id` gives each split-off piece its permanent ID.
    ///
    /// For each edited note, any other note of the same pitch that overlaps
    /// it is trimmed:
    /// - one that starts before it and ends underneath is cut at its start;
    /// - one that starts underneath and ends after it is shortened to start
    ///   where it ends;
    /// - one that starts before it and ends after it is split in two around
    ///   it: the note keeps the part before, and a new note with the same
    ///   pitch and velocity takes the part after;
    /// - one that starts and ends underneath it is removed.
    ///
    /// The edited notes always win over the others. Among themselves, the
    /// later start wins, so of two edited notes that overlap, the earlier
    /// one is trimmed. For the same start, the one later in `edited` wins.
    pub fn trims_under(
        &self,
        edited: &[NoteId],
        mut new_id: impl FnMut() -> NoteId,
    ) -> Vec<Command> {
        let mut winners: Vec<Note> = Vec::new();
        for id in edited {
            if let Some(note) = self.notes.get(id)
                && !winners.iter().any(|winner| winner.id == *id)
            {
                winners.push(*note);
            }
        }
        // Strongest first: the latest start, then the latest listed. Reversed
        // first because the sort is stable: notes with the same start keep
        // the latest listed first.
        winners.reverse();
        winners.sort_by_key(|note| std::cmp::Reverse(note.start));

        let pitches: HashSet<u8> = winners.iter().map(|note| note.pitch).collect();
        // Every note that could be trimmed, as it stands so far, including
        // pieces split off. `None` once it's removed.
        let mut notes: BTreeMap<NoteId, Option<Note>> = self
            .notes
            .values()
            .filter(|note| pitches.contains(&note.pitch))
            .map(|note| (note.id, Some(*note)))
            .collect();
        // The pieces split off, in the order they were made.
        let mut pieces = Vec::new();
        // Notes that have trimmed the others, which nothing weaker trims.
        let mut kept = HashSet::new();

        for winner in &winners {
            // A stronger edited note may already have trimmed or removed it.
            let Some(winner) = notes[&winner.id] else {
                continue;
            };
            kept.insert(winner.id);
            let mut split_off = Vec::new();
            for (id, note) in &mut notes {
                if let Some(covered) = note
                    && covered.pitch == winner.pitch
                    && !kept.contains(id)
                {
                    let (rest, after) = trimmed(*covered, &winner);
                    *note = rest;
                    split_off.extend(after);
                }
            }
            // Weaker edited notes may trim the new pieces too.
            for piece in split_off {
                let piece = Note {
                    id: new_id(),
                    ..piece
                };
                pieces.push(piece.id);
                notes.insert(piece.id, Some(piece));
            }
        }

        let mut set = Vec::new();
        let mut remove = Vec::new();
        for (id, note) in &notes {
            let Some(was) = self.notes.get(id) else {
                continue;
            };
            match note {
                None => remove.push(*id),
                Some(note) if note != was => set.push(*note),
                Some(_) => {}
            }
        }
        let add: Vec<Note> = pieces.iter().filter_map(|id| notes[id]).collect();
        let clip = self.id;
        let mut commands = Vec::new();
        if !set.is_empty() {
            commands.push(Command::SetNotes { clip, notes: set });
        }
        if !remove.is_empty() {
            commands.push(Command::RemoveNotes {
                clip,
                notes: remove,
            });
        }
        if !add.is_empty() {
            commands.push(Command::AddNotes { clip, notes: add });
        }
        commands
    }
}

/// `note` with the part `winner` covers taken out: what's left of it (or
/// `None` if nothing is), and the part after `winner` if it had to be split
/// off. Unchanged if they don't overlap.
fn trimmed(note: Note, winner: &Note) -> (Option<Note>, Option<Note>) {
    let (note_end, winner_end) = (end(&note), end(winner));
    if note_end <= winner.start || note.start >= winner_end {
        return (Some(note), None);
    }
    let before = (note.start < winner.start).then(|| Note {
        length: winner.start - note.start,
        ..note
    });
    let after = (note_end > winner_end).then(|| Note {
        start: winner_end,
        length: note_end - winner_end,
        ..note
    });
    match before {
        Some(before) => (Some(before), after),
        None => (after, None),
    }
}

fn end(note: &Note) -> Ticks {
    note.start + note.length
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, note_id};
    use crate::{Project, Session};
    use proptest::prelude::*;

    fn note(index: u128, pitch: u8, start: Ticks, length: Ticks) -> Note {
        Note {
            length,
            ..testing::note(index, pitch, start)
        }
    }

    /// IDs for split-off pieces: note IDs from 100 up.
    fn piece_ids() -> impl FnMut() -> NoteId {
        let mut next = 100;
        move || {
            next += 1;
            note_id(next - 1)
        }
    }

    /// A session whose clip holds `notes`.
    fn session_with(notes: &[Note]) -> Session {
        let mut session = Session::new(testing::project());
        let clip = clip_id(session.project());
        session
            .apply(Command::AddNotes {
                clip,
                notes: notes.to_vec(),
            })
            .unwrap();
        session
    }

    fn clip_id(project: &Project) -> crate::ClipId {
        project.tracks()[0].clips()[0].id()
    }

    fn clip(session: &Session) -> &Clip {
        session.project().clip(clip_id(session.project())).unwrap()
    }

    /// The clip's notes after trimming under `edited`, by index, as
    /// `(start, length)`, or `None` for a removed note.
    fn trim(notes: &[Note], edited: &[u128]) -> Vec<Option<(Ticks, Ticks)>> {
        let mut session = session_with(notes);
        let edited: Vec<_> = edited.iter().map(|&index| note_id(index)).collect();
        for command in clip(&session).trims_under(&edited, piece_ids()) {
            session.apply(command).unwrap();
        }
        notes
            .iter()
            .map(|note| {
                clip(&session)
                    .note(note.id)
                    .map(|note| (note.start, note.length))
            })
            .collect()
    }

    #[test]
    fn a_note_that_ends_underneath_is_cut_at_the_edited_notes_start() {
        // Note 0 from 0 to 960, edited note 1 from 480 to 1440.
        let after = trim(&[note(0, 60, 0, 960), note(1, 60, 480, 960)], &[1]);
        assert_eq!(after, [Some((0, 480)), Some((480, 960))]);
    }

    #[test]
    fn a_note_that_starts_underneath_starts_where_the_edited_note_ends() {
        let after = trim(&[note(0, 60, 0, 960), note(1, 60, 480, 960)], &[0]);
        assert_eq!(after, [Some((0, 960)), Some((960, 480))]);
    }

    #[test]
    fn a_note_covered_completely_is_removed() {
        let after = trim(&[note(0, 60, 0, 1920), note(1, 60, 480, 480)], &[0]);
        assert_eq!(after, [Some((0, 1920)), None]);
        let exactly = trim(&[note(0, 60, 480, 480), note(1, 60, 480, 480)], &[0]);
        assert_eq!(exactly, [Some((480, 480)), None]);
    }

    #[test]
    fn a_note_that_spans_the_edited_note_is_split_around_it() {
        let long = Note {
            velocity: 90,
            ..note(0, 60, 0, 1920)
        };
        let mut session = session_with(&[long, note(1, 60, 480, 480)]);
        let commands = clip(&session).trims_under(&[note_id(1)], piece_ids());
        let clip_id = clip_id(session.project());
        let tail = Note {
            id: note_id(100),
            start: 960,
            length: 960,
            ..long
        };
        assert_eq!(
            commands,
            [
                Command::SetNotes {
                    clip: clip_id,
                    notes: vec![Note {
                        length: 480,
                        ..long
                    }],
                },
                Command::AddNotes {
                    clip: clip_id,
                    notes: vec![tail],
                },
            ]
        );
        for command in commands {
            session.apply(command).unwrap();
        }
        assert_eq!(clip(&session).note(tail.id), Some(&tail));
    }

    #[test]
    fn a_note_that_spans_several_edited_notes_is_split_around_each() {
        // Note 0 runs from 0 to 3840; edited notes at 960 and 2880.
        let notes = [
            note(0, 60, 0, 3840),
            note(1, 60, 960, 480),
            note(2, 60, 2880, 480),
        ];
        let mut session = session_with(&notes);
        for command in clip(&session).trims_under(&[note_id(1), note_id(2)], piece_ids()) {
            session.apply(command).unwrap();
        }
        let mut after: Vec<_> = clip(&session)
            .notes()
            .map(|note| (note.start, note.length))
            .collect();
        after.sort_unstable();
        assert_eq!(
            after,
            [(0, 960), (960, 480), (1440, 1440), (2880, 480), (3360, 480)]
        );
    }

    #[test]
    fn several_notes_are_trimmed_at_once() {
        let notes = [
            note(0, 60, 0, 3840),  // edited: covers the rest
            note(1, 60, 0, 480),   // same start: removed
            note(2, 60, 960, 480), // inside: removed
            note(3, 60, 3360, 960),
            note(4, 60, 3840, 480), // just touching: kept
        ];
        let after = trim(&notes, &[0]);
        assert_eq!(
            after,
            [
                Some((0, 3840)),
                None,
                None,
                Some((3840, 480)),
                Some((3840, 480)),
            ]
        );
    }

    #[test]
    fn several_edited_notes_trim_one_note_from_both_sides() {
        // Note 0 runs from 480 to 2400; edited notes cover its start and end.
        let notes = [
            note(0, 60, 480, 1920),
            note(1, 60, 0, 960),
            note(2, 60, 1920, 960),
        ];
        let after = trim(&notes, &[1, 2]);
        assert_eq!(after, [Some((960, 960)), Some((0, 960)), Some((1920, 960))]);
    }

    #[test]
    fn other_pitches_are_left_alone() {
        let after = trim(&[note(0, 60, 0, 960), note(1, 64, 0, 960)], &[0]);
        assert_eq!(after, [Some((0, 960)), Some((0, 960))]);
        let clip_notes = [note(0, 60, 0, 960), note(1, 64, 0, 960)];
        assert!(
            clip(&session_with(&clip_notes))
                .trims_under(&[note_id(0)], piece_ids())
                .is_empty()
        );
    }

    #[test]
    fn edited_notes_that_overlap_keep_the_later_one_whole() {
        // A duplicate onto itself: both notes are edited.
        let after = trim(&[note(0, 60, 0, 960), note(1, 60, 480, 960)], &[0, 1]);
        assert_eq!(after, [Some((0, 480)), Some((480, 960))]);
        let listed_the_other_way = trim(&[note(0, 60, 0, 960), note(1, 60, 480, 960)], &[1, 0]);
        assert_eq!(listed_the_other_way, after);
        // With the same start, the one listed later wins.
        let same_start = trim(&[note(0, 60, 0, 960), note(1, 60, 0, 480)], &[0, 1]);
        assert_eq!(same_start, [Some((480, 480)), Some((0, 480))]);
    }

    #[test]
    fn a_removed_edited_note_trims_nothing() {
        // Note 1 is removed by note 2, so it doesn't cut note 0.
        let notes = [
            note(0, 60, 0, 960),
            note(1, 60, 480, 480),
            note(2, 60, 480, 960),
        ];
        let after = trim(&notes, &[1, 2]);
        assert_eq!(after, [Some((0, 480)), None, Some((480, 960))]);
    }

    #[test]
    fn unknown_and_repeated_ids_are_skipped() {
        let notes = [note(0, 60, 0, 960), note(1, 60, 480, 960)];
        assert_eq!(trim(&notes, &[5, 1, 1]), trim(&notes, &[1]));
        assert!(
            clip(&session_with(&notes))
                .trims_under(&[], piece_ids())
                .is_empty()
        );
    }

    #[test]
    fn the_trim_undoes_with_the_edit_as_one_step() {
        let mut session = session_with(&[note(0, 60, 0, 960), note(1, 60, 1920, 480)]);
        let before = session.project().clone();
        let clip_id = clip_id(session.project());

        // Lengthen note 0 over note 1, one drag step at a time, then trim.
        session
            .apply(Command::SetNotes {
                clip: clip_id,
                notes: vec![note(0, 60, 0, 1440)],
            })
            .unwrap();
        session
            .amend(Command::SetNotes {
                clip: clip_id,
                notes: vec![note(0, 60, 0, 2880)],
            })
            .unwrap();
        for command in clip(&session).trims_under(&[note_id(0)], piece_ids()) {
            session.join(command).unwrap();
        }
        assert_eq!(clip(&session).note(note_id(1)), None);
        let edited = session.project().clone();

        assert_eq!(session.undo().len(), 2, "the drag and the trim");
        assert_eq!(session.project(), &before, "one undo brings note 1 back");
        session.redo();
        assert_eq!(session.project(), &edited);
    }

    #[test]
    fn trimming_leaves_an_overlapping_clips_notes_alone() {
        let covered = note(0, 60, 0, 1920);
        let mut session = session_with(&[covered]);
        let track = session.project().tracks()[0].id();
        let edited_clip = clip_id(session.project());
        // Another clip on the same track, over the first, with a note of
        // the same pitch at the same place in the song.
        let other = crate::testing::clip_id(0);
        session
            .apply(Command::AddClips {
                clips: vec![crate::PlacedClip {
                    track,
                    clip: Clip::new(other, 0, 3840).with_notes([note(1, 60, 480, 480)]),
                }],
            })
            .unwrap();
        let before = session.project().clone();
        let edited = note(2, 60, 960, 480);
        session
            .apply(Command::AddNotes {
                clip: edited_clip,
                notes: vec![edited],
            })
            .unwrap();
        let trims = session
            .project()
            .clip(edited_clip)
            .unwrap()
            .trims_under(&[edited.id], piece_ids());
        assert!(!trims.is_empty(), "the note in the same clip is trimmed");
        for command in trims {
            session.join(command).unwrap();
        }
        assert_eq!(
            session.project().clip(other),
            before.clip(other),
            "the other clip is untouched"
        );
    }

    proptest! {
        #[test]
        fn no_edited_note_is_left_overlapping_one_of_the_same_pitch(
            notes in prop::collection::vec((0u8..3, 0u64..4000, 1u64..2000), 1..12),
            edited in prop::collection::vec(0u128..12, 0..4),
        ) {
            let notes: Vec<Note> = notes
                .into_iter()
                .enumerate()
                .map(|(index, (pitch, start, length))| note(index as u128, 60 + pitch, start, length))
                .collect();
            let mut session = session_with(&notes);
            let edited: Vec<_> = edited.into_iter().map(note_id).collect();
            let before = session.project().clone();
            for command in clip(&session).trims_under(&edited, piece_ids()) {
                session.join(command).unwrap();
            }

            let clip = clip(&session);
            for winner in edited.iter().filter_map(|id| clip.note(*id)) {
                for other in clip.notes() {
                    let overlaps = other.start < end(winner) && end(other) > winner.start;
                    prop_assert!(
                        other.id == winner.id || other.pitch != winner.pitch || !overlaps,
                        "{other:?} overlaps {winner:?}"
                    );
                }
            }
            // Nothing is ever lengthened or moved outside where it was, and
            // each split-off piece lies within a note it came from.
            let was = before.clip(clip.id()).unwrap();
            for note in clip.notes() {
                let within = |was: &Note| {
                    was.pitch == note.pitch
                        && was.velocity == note.velocity
                        && note.start >= was.start
                        && end(note) <= end(was)
                };
                match was.note(note.id) {
                    Some(was) => prop_assert!(within(was)),
                    None => prop_assert!(was.notes().any(within), "{note:?}"),
                }
            }
            // Trimming again changes nothing.
            prop_assert!(clip.trims_under(&edited, piece_ids()).is_empty());
        }
    }
}
