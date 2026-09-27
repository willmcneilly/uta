//! Stress notes: thousands of made-up notes that fill the loop, to learn
//! whether the piano roll stays smooth with a lot to draw (RFC-002, "Risks &
//! unknowns", web view performance).

use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{Note, NoteId};

/// How many notes one press of the menu item adds.
pub const NOTE_COUNT: usize = 3_000;

/// The lowest and highest pitches used: C1 to C8, most of a piano.
const PITCHES: (u8, u8) = (24, 108);

/// [`NOTE_COUNT`] notes spread over `loop_length` ticks, on a sixteenth-note
/// grid, with varied pitches, lengths and velocities. The same `seed` gives
/// the same pattern; each note gets a new random ID.
pub fn notes(loop_length: Ticks, seed: u64) -> Vec<Note> {
    let sixteenth = TICKS_PER_QUARTER / 4;
    let steps = (loop_length / sixteenth).max(1);
    let mut random = XorShift::new(seed);
    (0..NOTE_COUNT)
        .map(|_| Note {
            id: NoteId::random(),
            pitch: PITCHES.0 + (random.next() % u64::from(PITCHES.1 - PITCHES.0 + 1)) as u8,
            velocity: Note::MIN_VELOCITY
                + (random.next() % u64::from(Note::MAX_VELOCITY - Note::MIN_VELOCITY + 1)) as u8,
            start: (random.next() % steps) * sixteenth,
            // A sixteenth to a bar.
            length: (1 + random.next() % 16) * sixteenth,
        })
        .collect()
}

/// A small, fast pseudo-random generator. Good enough for made-up notes, and
/// needs no dependency.
struct XorShift(u64);

impl XorShift {
    fn new(seed: u64) -> Self {
        // Zero would stay zero forever.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAR: Ticks = TICKS_PER_QUARTER * 4;

    #[test]
    fn fills_the_loop_with_valid_notes() {
        let notes = notes(4 * BAR, 0);
        assert_eq!(notes.len(), NOTE_COUNT);
        for note in &notes {
            assert!((PITCHES.0..=PITCHES.1).contains(&note.pitch), "{note:?}");
            assert!((Note::MIN_VELOCITY..=Note::MAX_VELOCITY).contains(&note.velocity));
            assert!(note.start < 4 * BAR);
            assert!(note.length >= TICKS_PER_QUARTER / 4);
        }
        // Spread across the whole loop, not bunched at the start.
        assert!(notes.iter().any(|note| note.start >= 3 * BAR));
    }

    #[test]
    fn the_seed_picks_the_pattern() {
        let shape = |notes: Vec<Note>| -> Vec<_> {
            notes
                .iter()
                .map(|n| (n.pitch, n.velocity, n.start, n.length))
                .collect()
        };
        assert_eq!(shape(notes(BAR, 7)), shape(notes(BAR, 7)));
        assert_ne!(shape(notes(BAR, 7)), shape(notes(BAR, 8)));
    }
}
