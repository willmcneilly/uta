//! A pattern's line of steps, read into hits: notes for a synth, hits for
//! one drum.

use uta_core::Note;

/// What a line plays: each hit with the step it starts on and how many
/// steps it's held, and how many steps the line has, rests included.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub hits: Vec<Hit>,
    pub steps: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hit {
    pub step: u64,
    pub steps: u64,
    pub pitch: u8,
    pub velocity: u8,
}

/// A mistake in a line, and where in it, in bytes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepError {
    pub offset: usize,
    pub message: String,
}

/// The velocity an accent plays at: `X` in a drum line.
const ACCENT: u8 = Note::MAX_VELOCITY;

/// Reads a synth line: notes separated by spaces, each one step. `C4` is
/// middle C; `A3+C4+E4` is a chord; `@90` after a note or chord is its
/// velocity, `velocity` if it has none; `-` holds the note or chord before
/// it one more step; `.` is a rest; `|` is a bar line, for reading, and
/// isn't a step.
pub(crate) fn notes(text: &str, velocity: u8) -> Result<Line, StepError> {
    let mut hits: Vec<Hit> = Vec::new();
    // The hits the last step started, which `-` holds.
    let mut held = 0..0;
    let mut step = 0;
    for (offset, token) in tokens(text) {
        let error = |message: String| StepError { offset, message };
        match token {
            "|" => continue,
            "." => held = hits.len()..hits.len(),
            "-" => {
                if held.is_empty() {
                    return Err(error(
                        "`-` holds the note before it, and there isn't one".into(),
                    ));
                }
                for hit in &mut hits[held.clone()] {
                    hit.steps += 1;
                }
            }
            _ => {
                let (chord, velocity) = match token.rsplit_once('@') {
                    Some((chord, number)) => (chord, parse_velocity(number).map_err(error)?),
                    None => (token, velocity),
                };
                let start = hits.len();
                for name in chord.split('+') {
                    hits.push(Hit {
                        step,
                        steps: 1,
                        pitch: pitch(name).map_err(error)?,
                        velocity,
                    });
                }
                held = start..hits.len();
            }
        }
        step += 1;
    }
    Ok(Line { hits, steps: step })
}

/// Reads one drum's line: a character a step. `x` is a hit at `velocity`,
/// `X` an accent, and either followed by a number is a hit at that
/// velocity (`x90`); `.` is a rest. Spaces and `|` are for reading, and
/// aren't steps. Each hit lasts its step.
pub(crate) fn hits(text: &str, pitch: u8, velocity: u8) -> Result<Line, StepError> {
    let mut hits = Vec::new();
    let mut step = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        match c {
            c if c.is_whitespace() || c == '|' => continue,
            '.' => {}
            'x' | 'X' => {
                let mut number = String::new();
                while let Some(&(_, digit)) = chars.peek().filter(|(_, d)| d.is_ascii_digit()) {
                    number.push(digit);
                    chars.next();
                }
                let velocity = match (number.is_empty(), c) {
                    (false, _) => {
                        parse_velocity(&number).map_err(|message| StepError { offset, message })?
                    }
                    (true, 'X') => ACCENT,
                    (true, _) => velocity,
                };
                hits.push(Hit {
                    step,
                    steps: 1,
                    pitch,
                    velocity,
                });
            }
            _ => {
                return Err(StepError {
                    offset,
                    message: format!(
                        "`{c}` isn't a step: x is a hit (X an accent, x90 one at velocity 90) and . a rest"
                    ),
                });
            }
        }
        step += 1;
    }
    Ok(Line { hits, steps: step })
}

/// The words of `text`, split at whitespace, each with its offset.
fn tokens(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_whitespace()
        .map(move |token| (token.as_ptr() as usize - text.as_ptr() as usize, token))
}

fn parse_velocity(number: &str) -> Result<u8, String> {
    number
        .parse::<u8>()
        .ok()
        .filter(|velocity| (Note::MIN_VELOCITY..=Note::MAX_VELOCITY).contains(velocity))
        .ok_or_else(|| {
            format!(
                "velocity `{number}` isn't {} to {}",
                Note::MIN_VELOCITY,
                Note::MAX_VELOCITY
            )
        })
}

/// The MIDI note a name is: a letter from A to G, then `#` or `b`, then an
/// octave, where `C4` is middle C (60), as `uta analyse --pitch` has it.
pub(crate) fn pitch(name: &str) -> Result<u8, String> {
    let wrong = || {
        format!(
            "`{name}` isn't a note: a letter A to G, then # or b, then an octave (C4 is middle C)"
        )
    };
    let mut chars = name.chars();
    let semitone: i32 = match chars.next() {
        Some('C') => 0,
        Some('D') => 2,
        Some('E') => 4,
        Some('F') => 5,
        Some('G') => 7,
        Some('A') => 9,
        Some('B') => 11,
        _ => return Err(wrong()),
    };
    let rest = chars.as_str();
    let (semitone, octave) = match rest.strip_prefix('#') {
        Some(octave) => (semitone + 1, octave),
        None => match rest.strip_prefix('b') {
            Some(octave) => (semitone - 1, octave),
            None => (semitone, rest),
        },
    };
    let octave: i32 = octave.parse().map_err(|_| wrong())?;
    let pitch = (octave + 1) * 12 + semitone;
    u8::try_from(pitch)
        .ok()
        .filter(|&pitch| pitch <= Note::MAX_PITCH)
        .ok_or_else(|| format!("`{name}` is outside the notes MIDI has (C-1 to G9)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names_are_midi_notes_with_middle_c_as_c4() {
        assert_eq!(pitch("C4"), Ok(60));
        assert_eq!(pitch("A4"), Ok(69));
        assert_eq!(pitch("C#4"), Ok(61));
        assert_eq!(pitch("Db4"), Ok(61));
        assert_eq!(pitch("C-1"), Ok(0));
        assert_eq!(pitch("G9"), Ok(127));
        assert!(pitch("G#9").is_err());
        assert!(pitch("H2").is_err());
        assert!(pitch("c4").is_err());
        assert!(pitch("C").is_err());
    }

    #[test]
    fn a_synth_line_reads_notes_chords_holds_and_rests() {
        let line = notes("A2@105 - . A3+C4 | C4 -", 80).unwrap();
        assert_eq!(line.steps, 6);
        let hit = |step, steps, pitch, velocity| Hit {
            step,
            steps,
            pitch,
            velocity,
        };
        assert_eq!(
            line.hits,
            [
                hit(0, 2, 45, 105),
                hit(3, 1, 57, 80),
                hit(3, 1, 60, 80),
                hit(4, 2, 60, 80),
            ]
        );
    }

    #[test]
    fn a_synth_line_says_where_its_mistake_is() {
        assert_eq!(notes("C4 . Q4", 100).unwrap_err().offset, 5);
        assert_eq!(notes("- C4", 100).unwrap_err().offset, 0);
        assert_eq!(notes("C4 . -", 100).unwrap_err().offset, 5);
        assert_eq!(notes("C4@0", 100).unwrap_err().offset, 0);
    }

    #[test]
    fn a_drum_line_reads_hits_accents_and_velocities() {
        let line = hits("x... X.x90. | x", 36, 100).unwrap();
        assert_eq!(line.steps, 9);
        let velocities: Vec<_> = line
            .hits
            .iter()
            .map(|hit| (hit.step, hit.velocity))
            .collect();
        assert_eq!(velocities, [(0, 100), (4, 127), (6, 90), (8, 100)]);
        assert!(
            line.hits
                .iter()
                .all(|hit| hit.pitch == 36 && hit.steps == 1)
        );
        assert_eq!(hits("x.o", 36, 100).unwrap_err().offset, 2);
        assert_eq!(hits("x.x200", 36, 100).unwrap_err().offset, 2);
    }
}
