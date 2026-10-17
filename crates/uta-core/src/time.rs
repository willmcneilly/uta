//! Musical time. See RFC-002, "The shared model", point 1.
//!
//! The project stores every position and length as a whole number of
//! [`Ticks`], 960 to a quarter note. The [`TempoMap`] says how fast they go,
//! and turns them into sample positions for playback. Nothing in the project
//! depends on the sample rate.

use serde::{Deserialize, Serialize};

/// A position or length in ticks: whole fractions of a beat.
pub type Ticks = u64;

/// Ticks per quarter note. 960 divides evenly by 3 and 5, so triplets and
/// quintuplets land exactly on a tick.
pub const TICKS_PER_QUARTER: Ticks = 960;

/// The latest a note may end, in ticks from its clip's start. It's about 248
/// hours at 300 BPM, and keeps sums of positions far from overflowing.
pub const MAX_TICKS: Ticks = u32::MAX as Ticks;

/// How beats group into bars. Fixed at 4/4 for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSignature {
    /// Beats in a bar: the 3 in 3/4.
    pub beats_per_bar: u32,
    /// The note value of one beat: the 4 in 3/4 (a quarter note).
    pub beat_unit: u32,
}

impl TimeSignature {
    pub const FOUR_FOUR: Self = Self {
        beats_per_bar: 4,
        beat_unit: 4,
    };

    /// The length of one bar, in ticks.
    pub fn ticks_per_bar(&self) -> Ticks {
        Ticks::from(self.beats_per_bar) * TICKS_PER_QUARTER * 4 / Ticks::from(self.beat_unit)
    }
}

/// A stretch of the song at a steady tempo, from `start` until the next
/// section starts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoSection {
    /// Where the section starts, in ticks from the start of the song.
    pub start: Ticks,
    /// Quarter notes per minute.
    pub bpm: f32,
}

/// How fast beats go over the song: a list of [`TempoSection`]s, the first
/// starting at 0. Project 1 only ever has one section, but the shape allows
/// tempo changes later.
#[derive(Debug, Clone, PartialEq)]
pub struct TempoMap {
    /// Sorted by start, the first at 0, never empty.
    sections: Vec<TempoSection>,
}

impl TempoMap {
    /// A map with one tempo for the whole song.
    pub fn new(bpm: f32) -> Self {
        Self {
            sections: vec![TempoSection { start: 0, bpm }],
        }
    }

    /// Several sections. The first must start at 0 and the rest must be in
    /// order. Only tests need this until tempo changes arrive.
    #[cfg(test)]
    pub(crate) fn with_sections(sections: Vec<TempoSection>) -> Self {
        assert_eq!(sections.first().map(|s| s.start), Some(0));
        assert!(sections.windows(2).all(|w| w[0].start < w[1].start));
        Self { sections }
    }

    pub fn sections(&self) -> &[TempoSection] {
        &self.sections
    }

    /// The tempo at the start of the song. In project 1, the only tempo.
    pub fn bpm(&self) -> f32 {
        self.sections[0].bpm
    }

    /// Sets the tempo at the start of the song, returning the old one.
    pub(crate) fn set_bpm(&mut self, bpm: f32) -> f32 {
        std::mem::replace(&mut self.sections[0].bpm, bpm)
    }

    /// The sample a position in ticks falls on, at `sample_rate`, rounded to
    /// the nearest. It's worked out directly from the start of the position's
    /// tempo section, never by adding up small steps, so nothing drifts.
    pub fn ticks_to_samples(&self, ticks: Ticks, sample_rate: u32) -> u64 {
        let mut section_start = 0;
        for (index, section) in self.sections.iter().enumerate() {
            match self.sections.get(index + 1) {
                Some(next) if ticks >= next.start => {
                    section_start += section_ticks_to_samples(
                        next.start - section.start,
                        section.bpm,
                        sample_rate,
                    );
                }
                _ => {
                    return section_start
                        + section_ticks_to_samples(
                            ticks - section.start,
                            section.bpm,
                            sample_rate,
                        );
                }
            }
        }
        unreachable!("the last section runs to the end of the song")
    }

    /// The tick nearest to a sample position, at `sample_rate`. It exactly
    /// undoes [`Self::ticks_to_samples`] at any tempo from 20 to 300 BPM and
    /// any sample rate from 44.1 kHz, where a tick is at least 9 samples long.
    pub fn samples_to_ticks(&self, samples: u64, sample_rate: u32) -> Ticks {
        let mut section_start = 0;
        for (index, section) in self.sections.iter().enumerate() {
            if let Some(next) = self.sections.get(index + 1) {
                let length =
                    section_ticks_to_samples(next.start - section.start, section.bpm, sample_rate);
                if samples >= section_start + length {
                    section_start += length;
                    continue;
                }
            }
            return section.start
                + section_samples_to_ticks(samples - section_start, section.bpm, sample_rate);
        }
        unreachable!("the last section runs to the end of the song")
    }
}

/// Ticks from a section's start to samples, rounded to the nearest. The
/// numerator and denominator are both whole numbers that f64 holds exactly
/// (for an hour or more at 96 kHz and a whole-number tempo), so the one
/// division rounds exactly as whole-number maths would.
fn section_ticks_to_samples(ticks: Ticks, bpm: f32, sample_rate: u32) -> u64 {
    let numerator = ticks as f64 * 60.0 * f64::from(sample_rate);
    let denominator = f64::from(bpm) * TICKS_PER_QUARTER as f64;
    (numerator / denominator).round() as u64
}

/// Samples from a section's start to ticks, rounded to the nearest.
fn section_samples_to_ticks(samples: u64, bpm: f32, sample_rate: u32) -> Ticks {
    let numerator = samples as f64 * f64::from(bpm) * TICKS_PER_QUARTER as f64;
    let denominator = 60.0 * f64::from(sample_rate);
    (numerator / denominator).round() as Ticks
}

/// A note length that a setting syncs to the tempo with: 1/32 to 1/1, each
/// straight, dotted or triplet, eighteen in all. Defined once here for
/// every setting that syncs: the Delay's time, and the synth's LFO rate to
/// come. See RFC-007, "Shared with RFC-008", and open question 1.
///
/// Serialises as its [`value`](Self::value): `"1/8"`, `"1/8d"` (dotted) or
/// `"1/8t"` (triplet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteDivision {
    /// The note: how many go to a whole note, from 1 to 32.
    note: u8,
    feel: Feel,
}

/// Whether a division is played straight, dotted (half as long again) or
/// as a triplet (three in the time of two).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Feel {
    Straight,
    Dotted,
    Triplet,
}

impl NoteDivision {
    pub const DOTTED_EIGHTH: Self = Self::new(8, Feel::Dotted);

    /// Every division, in a menu's order: from the shortest note to the
    /// longest, each as a triplet, straight, then dotted.
    pub const ALL: [Self; 18] = {
        const NOTES: [u8; 6] = [32, 16, 8, 4, 2, 1];
        const FEELS: [Feel; 3] = [Feel::Triplet, Feel::Straight, Feel::Dotted];
        let mut all = [Self::new(1, Feel::Straight); 18];
        let mut index = 0;
        while index < 18 {
            all[index] = Self::new(NOTES[index / 3], FEELS[index % 3]);
            index += 1;
        }
        all
    };

    const fn new(note: u8, feel: Feel) -> Self {
        Self { note, feel }
    }

    /// Its length in ticks. Every division lands exactly on a tick: a
    /// triplet 1/32 is 80.
    pub const fn ticks(self) -> Ticks {
        let straight = TICKS_PER_QUARTER * 4 / self.note as Ticks;
        match self.feel {
            Feel::Straight => straight,
            Feel::Dotted => straight * 3 / 2,
            Feel::Triplet => straight * 2 / 3,
        }
    }

    /// Its length in seconds at `bpm` quarter notes a minute.
    pub fn seconds_at(self, bpm: f32) -> f64 {
        self.ticks() as f64 / TICKS_PER_QUARTER as f64 * 60.0 / f64::from(bpm)
    }

    /// The division as a command carries it: `"1/8d"`.
    pub const fn value(self) -> &'static str {
        DIVISION_NAMES[self.index()].0
    }

    /// What a menu calls it: "1/8 dotted".
    pub const fn label(self) -> &'static str {
        DIVISION_NAMES[self.index()].1
    }

    /// Its place in [`Self::ALL`].
    const fn index(self) -> usize {
        let note = match self.note {
            32 => 0,
            16 => 1,
            8 => 2,
            4 => 3,
            2 => 4,
            _ => 5,
        };
        let feel = match self.feel {
            Feel::Triplet => 0,
            Feel::Straight => 1,
            Feel::Dotted => 2,
        };
        note * 3 + feel
    }

    /// The division a command's value names, if any.
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|division| division.value() == value)
    }
}

/// Each division's value and label, in the order of [`NoteDivision::ALL`].
const DIVISION_NAMES: [(&str, &str); 18] = [
    ("1/32t", "1/32 triplet"),
    ("1/32", "1/32"),
    ("1/32d", "1/32 dotted"),
    ("1/16t", "1/16 triplet"),
    ("1/16", "1/16"),
    ("1/16d", "1/16 dotted"),
    ("1/8t", "1/8 triplet"),
    ("1/8", "1/8"),
    ("1/8d", "1/8 dotted"),
    ("1/4t", "1/4 triplet"),
    ("1/4", "1/4"),
    ("1/4d", "1/4 dotted"),
    ("1/2t", "1/2 triplet"),
    ("1/2", "1/2"),
    ("1/2d", "1/2 dotted"),
    ("1/1t", "1/1 triplet"),
    ("1/1", "1/1"),
    ("1/1d", "1/1 dotted"),
];

impl Serialize for NoteDivision {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.value())
    }
}

impl<'de> Deserialize<'de> for NoteDivision {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_value(&value).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "{value:?} isn't a note division (1/32 to 1/1, with d for dotted or t for triplet)"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATES: [u32; 3] = [44_100, 48_000, 96_000];
    /// Whole-number tempos, checked against exact whole-number maths.
    const WHOLE_TEMPOS: [u32; 6] = [20, 60, 90, 120, 137, 300];
    /// Tempos with a fraction, checked by the round trip.
    const FRACTIONAL_TEMPOS: [f32; 3] = [33.3, 128.75, 174.2];

    /// The exact sample for `ticks` at a whole-number tempo, rounded to the
    /// nearest with halves rounded up, in whole-number maths.
    fn exact_samples(ticks: Ticks, bpm: u32, sample_rate: u32) -> u64 {
        let numerator = u128::from(ticks) * 60 * u128::from(sample_rate);
        let denominator = u128::from(bpm) * u128::from(TICKS_PER_QUARTER);
        ((2 * numerator + denominator) / (2 * denominator)) as u64
    }

    /// An hour of positions at `bpm`: every tick of the first and last
    /// 20,000, and every 97th tick in between.
    fn an_hours_ticks(bpm: f32) -> impl Iterator<Item = Ticks> {
        let hour = (f64::from(bpm) * 60.0 * TICKS_PER_QUARTER as f64) as Ticks;
        (0..20_000)
            .chain((20_000..hour - 20_000).step_by(97))
            .chain(hour - 20_000..=hour)
    }

    #[test]
    fn a_bar_of_four_four_is_four_quarter_notes() {
        assert_eq!(TimeSignature::FOUR_FOUR.ticks_per_bar(), 3840);
    }

    #[test]
    fn a_quarter_note_at_120_bpm_is_half_a_second() {
        let map = TempoMap::new(120.0);
        assert_eq!(map.ticks_to_samples(960, 48_000), 24_000);
        assert_eq!(map.ticks_to_samples(960, 44_100), 22_050);
        assert_eq!(map.samples_to_ticks(24_000, 48_000), 960);
    }

    #[test]
    fn ticks_to_samples_is_exact_over_an_hour() {
        for sample_rate in SAMPLE_RATES {
            for bpm in WHOLE_TEMPOS {
                let map = TempoMap::new(bpm as f32);
                for ticks in an_hours_ticks(bpm as f32) {
                    let samples = map.ticks_to_samples(ticks, sample_rate);
                    assert_eq!(
                        samples,
                        exact_samples(ticks, bpm, sample_rate),
                        "{ticks} ticks at {bpm} BPM, {sample_rate} Hz"
                    );
                    assert_eq!(
                        map.samples_to_ticks(samples, sample_rate),
                        ticks,
                        "{ticks} ticks at {bpm} BPM, {sample_rate} Hz"
                    );
                }
            }
        }
    }

    #[test]
    fn ticks_to_samples_and_back_is_exact_over_an_hour_at_fractional_tempos() {
        for sample_rate in SAMPLE_RATES {
            for bpm in FRACTIONAL_TEMPOS {
                let map = TempoMap::new(bpm);
                let mut previous = None;
                for ticks in an_hours_ticks(bpm) {
                    let samples = map.ticks_to_samples(ticks, sample_rate);
                    assert!(previous < Some(samples), "samples must keep increasing");
                    previous = Some(samples);
                    assert_eq!(
                        map.samples_to_ticks(samples, sample_rate),
                        ticks,
                        "{ticks} ticks at {bpm} BPM, {sample_rate} Hz"
                    );
                }
            }
        }
    }

    #[test]
    fn each_section_counts_from_its_own_start() {
        // A bar at 120 BPM, then 60 BPM.
        let map = TempoMap::with_sections(vec![
            TempoSection {
                start: 0,
                bpm: 120.0,
            },
            TempoSection {
                start: 3840,
                bpm: 60.0,
            },
        ]);
        // The first bar takes 2 s, then each quarter note takes 1 s.
        assert_eq!(map.ticks_to_samples(3840, 48_000), 96_000);
        assert_eq!(map.ticks_to_samples(3840 + 960, 48_000), 144_000);
        assert_eq!(map.samples_to_ticks(95_999, 48_000), 3840);
        assert_eq!(map.samples_to_ticks(144_000, 48_000), 4800);

        for sample_rate in SAMPLE_RATES {
            for ticks in (0..20_000).chain((20_000..1_000_000).step_by(89)) {
                let samples = map.ticks_to_samples(ticks, sample_rate);
                assert_eq!(map.samples_to_ticks(samples, sample_rate), ticks);
            }
        }
    }

    #[test]
    fn there_are_eighteen_divisions_each_exactly_on_a_tick() {
        let ticks: Vec<Ticks> = NoteDivision::ALL.iter().map(|d| d.ticks()).collect();
        assert_eq!(
            ticks,
            [
                80, 120, 180, 160, 240, 360, 320, 480, 720, 640, 960, 1440, 1280, 1920, 2880, 2560,
                3840, 5760
            ]
        );
        let values: Vec<&str> = NoteDivision::ALL.iter().map(|d| d.value()).collect();
        assert_eq!(values.len(), 18);
        for (index, value) in values.iter().enumerate() {
            assert!(!values[..index].contains(value), "{value} twice");
        }
        assert_eq!(NoteDivision::DOTTED_EIGHTH.value(), "1/8d");
        assert_eq!(NoteDivision::DOTTED_EIGHTH.label(), "1/8 dotted");
        assert_eq!(NoteDivision::ALL[0].label(), "1/32 triplet");
    }

    #[test]
    fn a_divisions_length_follows_the_tempo() {
        let quarter = NoteDivision::from_value("1/4").unwrap();
        assert_eq!(quarter.seconds_at(120.0), 0.5);
        assert_eq!(quarter.seconds_at(60.0), 1.0);
        assert_eq!(NoteDivision::DOTTED_EIGHTH.seconds_at(120.0), 0.375);
        // A half note at 40 BPM is 3 s; a whole note at 60 BPM, 4 s.
        assert_eq!(
            NoteDivision::from_value("1/2").unwrap().seconds_at(40.0),
            3.0
        );
        assert_eq!(
            NoteDivision::from_value("1/1").unwrap().seconds_at(60.0),
            4.0
        );
        let triplet = NoteDivision::from_value("1/8t").unwrap();
        assert!((triplet.seconds_at(120.0) - 1.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn a_division_serialises_as_its_value_and_reads_back() {
        for division in NoteDivision::ALL {
            let json = serde_json::to_string(&division).unwrap();
            assert_eq!(json, format!("\"{}\"", division.value()));
            assert_eq!(
                serde_json::from_str::<NoteDivision>(&json).unwrap(),
                division
            );
        }
        for wrong in [r#""1/64""#, r#""3/8""#, r#""1/8x""#, "8"] {
            assert!(
                serde_json::from_str::<NoteDivision>(wrong).is_err(),
                "{wrong}"
            );
        }
    }
}
