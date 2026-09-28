//! Trimming the notes an edit covers, so a note never hides another of the
//! same pitch. It works like Ableton: the edited notes win, and the covered
//! part of any other note goes.

use std::collections::{BTreeMap, HashSet};

use crate::time::Ticks;
use crate::{Clip, Command, Note, NoteId};

impl Clip {
    /// The commands that trim the notes `edited` now cover: a
    /// [`Command::SetNotes`] of the notes that get shorter and a
    /// [`Command::RemoveNotes`] of those with nothing left, whichever are
    /// needed. Empty if nothing overlaps. `edited` is the notes a drag moved
    /// or resized, or a paste added; IDs not in the clip are skipped.
    ///
    /// For each edited note, any other note of the same pitch that overlaps
    /// it is trimmed:
    /// - one that starts before it is cut at its start (losing any part that
    ///   ran past its end);
    /// - one that starts underneath it and ends after it is shortened to
    ///   start where it ends;
    /// - one that starts and ends underneath it is removed.
    ///
    /// The edited notes always win over the others. Among themselves, the
    /// later start wins, so of two edited notes that overlap, the earlier
    /// one is trimmed. For the same start, the one later in `edited` wins.
    pub fn trims_under(&self, edited: &[NoteId]) -> Vec<Command> {
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
        // Every note that could be trimmed, as it stands so far. `None` once
        // it's removed.
        let mut notes: BTreeMap<NoteId, Option<Note>> = self
            .notes
            .values()
            .filter(|note| pitches.contains(&note.pitch))
            .map(|note| (note.id, Some(*note)))
            .collect();
        // Notes that have trimmed the others, which nothing weaker trims.
        let mut kept = HashSet::new();

        for winner in &winners {
            // A stronger edited note may already have trimmed or removed it.
            let Some(winner) = notes[&winner.id] else {
                continue;
            };
            kept.insert(winner.id);
            for (id, note) in &mut notes {
                if let Some(covered) = note
                    && covered.pitch == winner.pitch
                    && !kept.contains(id)
                {
                    *note = trimmed(*covered, &winner);
                }
            }
        }

        let mut set = Vec::new();
        let mut remove = Vec::new();
        for (id, note) in notes {
            match note {
                None => remove.push(id),
                Some(note) if note != self.notes[&id] => set.push(note),
                Some(_) => {}
            }
        }
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
        commands
    }
}

/// `note` with the part `winner` covers taken out, or `None` if nothing is
/// left. Unchanged if they don't overlap.
fn trimmed(note: Note, winner: &Note) -> Option<Note> {
    let (note_end, winner_end) = (end(&note), end(winner));
    if note_end <= winner.start || note.start >= winner_end {
        return Some(note);
    }
    if note.start < winner.start {
        Some(Note {
            length: winner.start - note.start,
            ..note
        })
    } else if note_end > winner_end {
        Some(Note {
            start: winner_end,
            length: note_end - winner_end,
            ..note
        })
    } else {
        None
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
        for command in clip(&session).trims_under(&edited) {
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
    fn a_note_that_covers_the_edited_note_keeps_only_what_comes_before_it() {
        let after = trim(&[note(0, 60, 0, 1920), note(1, 60, 480, 480)], &[1]);
        assert_eq!(after, [Some((0, 480)), Some((480, 480))]);
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
                .trims_under(&[note_id(0)])
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
        assert!(clip(&session_with(&notes)).trims_under(&[]).is_empty());
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
        for command in clip(&session).trims_under(&[note_id(0)]) {
            session.join(command).unwrap();
        }
        assert_eq!(clip(&session).note(note_id(1)), None);
        let edited = session.project().clone();

        assert_eq!(session.undo().len(), 2, "the drag and the trim");
        assert_eq!(session.project(), &before, "one undo brings note 1 back");
        session.redo();
        assert_eq!(session.project(), &edited);
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
            for command in clip(&session).trims_under(&edited) {
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
            // Nothing is ever lengthened or moved outside where it was.
            for note in clip.notes() {
                let was = before.clip(clip.id()).unwrap().note(note.id).unwrap();
                prop_assert!(note.start >= was.start && end(note) <= end(was));
            }
            // Trimming again changes nothing.
            prop_assert!(clip.trims_under(&edited).is_empty());
        }
    }
}
