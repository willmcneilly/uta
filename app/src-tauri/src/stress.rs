//! Stress notes: thousands of made-up notes that fill the loop, to learn
//! whether the piano roll stays smooth with a lot to draw (RFC-002, "Risks &
//! unknowns", web view performance). And the benchmark's test songs, made of
//! them (RFC-004, "The benchmark stays").

use serde::Deserialize;
use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{Clip, ClipId, Note, NoteId, Source, SynthSettings, Track, TrackId};

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

/// The songs the benchmark builds, named after RFC-004's loads ("What we
/// measured"). The UI only says which; Rust knows the recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestSong {
    /// 12 tracks × 64 bars, 64 notes a bar: 49,152 notes.
    Heavy,
    /// 32 tracks × 200 bars, 8 notes a bar: 51,200 notes in 6,400 clips.
    Wide,
    /// Make a song's manual check 7: 7 tracks × 28 bars, 3,000 notes a bar.
    #[serde(rename = "check-7")]
    Check7,
}

/// How a test song is made: `tracks` tracks, each with `bars` one-bar clips
/// end to end from the start of the song, each holding `notes_per_clip`
/// stress notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recipe {
    pub tracks: usize,
    pub bars: u64,
    pub notes_per_clip: usize,
}

impl TestSong {
    pub fn recipe(self) -> Recipe {
        let (tracks, bars, notes_per_clip) = match self {
            Self::Heavy => (12, 64, 64),
            Self::Wide => (32, 200, 8),
            Self::Check7 => (7, 28, NOTE_COUNT),
        };
        Recipe {
            tracks,
            bars,
            notes_per_clip,
        }
    }
}

impl Recipe {
    /// The song's tracks, named "Synth 1" onwards, with the default sound
    /// and mixer. Every clip holds the same pattern, as if it had been
    /// duplicated, and every track, clip and note has a new random ID.
    pub fn tracks(self, bar: Ticks) -> Vec<Track> {
        let pattern: Vec<Note> = notes(bar, 0)
            .into_iter()
            .take(self.notes_per_clip)
            .collect();
        (0..self.tracks)
            .map(|index| {
                let clips = (0..self.bars).map(|at| {
                    Clip::new(ClipId::random(), at * bar, bar).with_notes(pattern.iter().map(
                        |note| Note {
                            id: NoteId::random(),
                            ..*note
                        },
                    ))
                });
                Track::new(
                    TrackId::random(),
                    format!("Synth {}", index + 1),
                    Source::Synth(SynthSettings::default()),
                )
                .with_clips(clips)
            })
            .collect()
    }
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
    fn each_test_song_has_the_rfcs_load() {
        let notes = |song: TestSong| {
            let recipe = song.recipe();
            recipe.tracks as u64 * recipe.bars * recipe.notes_per_clip as u64
        };
        assert_eq!(notes(TestSong::Heavy), 49_152);
        assert_eq!(notes(TestSong::Wide), 51_200);
        assert_eq!(notes(TestSong::Check7), 588_000);
    }

    #[test]
    fn a_recipe_builds_its_tracks_clips_and_notes() {
        let recipe = Recipe {
            tracks: 3,
            bars: 5,
            notes_per_clip: 7,
        };
        let tracks = recipe.tracks(BAR);
        let names: Vec<_> = tracks.iter().map(Track::name).collect();
        assert_eq!(names, ["Synth 1", "Synth 2", "Synth 3"]);
        for track in &tracks {
            let clips = track.clips();
            assert_eq!(clips.len(), 5);
            for (bar, clip) in (0..).zip(clips) {
                assert_eq!((clip.start(), clip.length()), (bar * BAR, BAR));
                assert_eq!(clip.notes().len(), 7);
                assert!(clip.notes().all(|note| note.start < BAR));
            }
        }
        // Every clip holds the same pattern, under its own IDs.
        let shape = |clip: &Clip| -> Vec<_> {
            let mut notes: Vec<_> = clip
                .notes()
                .map(|n| (n.pitch, n.velocity, n.start, n.length))
                .collect();
            notes.sort_unstable();
            notes
        };
        assert_eq!(shape(&tracks[0].clips()[0]), shape(&tracks[2].clips()[4]));
        let ids: std::collections::HashSet<_> = tracks
            .iter()
            .flat_map(Track::clips)
            .flat_map(Clip::notes)
            .map(|note| note.id)
            .collect();
        assert_eq!(ids.len(), 3 * 5 * 7);
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
