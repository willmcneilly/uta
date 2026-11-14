//! The Reverb's factory sounds: the classic reverbs you'd reach for, each
//! made from a written brief with measurable targets, as the synth's are
//! (RFC-010, "Factory sounds"; RFC-008, "Factory sounds, and turning an
//! idea into settings").
//!
//! A reverb's character is mostly things that can be measured on its
//! impulse response, so each brief's targets are a reverb's own: how long
//! each band rings, when the first sound arrives, how the early burst and
//! the tail are balanced, how soon it's a smooth wash, how much top and
//! bottom it has, and how loud it is. They're checked by `uta-engine`'s
//! Reverb tests (`effects/reverb/factory.rs`), on the Reverb's answer to a
//! one-sample click, measured as `uta analyse --impulse-response` does.
//! What making them taught is in `docs/sound-design/reverb.md`.
//!
//! Choosing one sets every Reverb setting, its mode included, but never
//! Mix: Mix is how much reverb the track gets, which is yours (RFC-010,
//! open question 7). Each suggests a Mix, shown in the menu.

use std::ops::RangeInclusive;

use crate::{ReverbMode, ReverbSettings};

/// One of the Reverb's factory sounds: its brief, the targets that say it
/// meets it, the Mix it suggests, and the settings that make it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReverbFactorySound {
    /// Its name as the app's call and a song file carry it:
    /// `"drum_room"`.
    pub key: &'static str,
    /// Its name in the preset menu: "Drum room".
    pub name: &'static str,
    /// What it should sound like, and where the sound comes from: what the
    /// settings were made from.
    pub brief: &'static str,
    /// What its impulse response should measure.
    pub targets: ReverbTargets,
    /// The Mix it suggests, from 0 to 1. Shown in the menu; choosing the
    /// sound leaves the Mix where it is.
    pub suggested_mix: f32,
    /// Every Reverb setting, its mode included.
    pub settings: ReverbSettings,
}

/// What a factory sound's impulse response should measure, each as a range,
/// or `None` where its brief says nothing about it. The impulse response is
/// the Reverb's answer, at Mix 100%, to a full-scale one-sample click on
/// both sides at time 0, so every time is from the click. Each band's decay
/// is the octave's T30 (`uta analyse`'s `decay_*`), on the mid of the two
/// sides.
#[derive(Debug, Clone, PartialEq)]
pub struct ReverbTargets {
    /// How long the middle rings: the 1 kHz octave's decay, in seconds.
    pub mid_decay_seconds: Option<RangeInclusive<f64>>,
    /// How long the lows ring against the middle: the 125 Hz octave's
    /// decay over the 1 kHz octave's.
    pub low_decay_ratio: Option<RangeInclusive<f64>>,
    /// How long the highs ring against the middle: the 4 kHz octave's
    /// decay over the 1 kHz octave's.
    pub high_decay_ratio: Option<RangeInclusive<f64>>,
    /// When the first sound arrives, in ms (`uta analyse`'s `onset_ms`).
    pub onset_ms: Option<RangeInclusive<f64>>,
    /// The early burst against the tail: the energy in the first 80 ms after
    /// the onset over the energy after it, in dB (`early_to_late_db`, the
    /// C80 of room acoustics). Above 0 is mostly early, below is mostly
    /// tail.
    pub early_to_late_db: Option<RangeInclusive<f64>>,
    /// How soon it's a smooth wash: from the onset until the echo density
    /// first reaches near 1, in ms (`dense_after_ms`).
    pub dense_after_ms: Option<RangeInclusive<f64>>,
    /// When it's loudest, in ms: the middle of its loudest 20 ms. A reverb
    /// that blooms is loudest well after the click.
    pub loudest_at_ms: Option<RangeInclusive<f64>>,
    /// How soon it's gone, in ms: when it falls for good to 20 dB under its
    /// loudest, measured in 10 ms windows. A gated burst stops dead.
    pub gone_by_ms: Option<RangeInclusive<f64>>,
    /// How little top it has: the frequency in Hz, and the most of its
    /// energy above it, in percent (`share_above_percent`).
    pub most_above: Option<(f64, f64)>,
    /// How little bottom it has: the frequency in Hz, and the most of its
    /// energy below it, in percent.
    pub most_below: Option<(f64, f64)>,
    /// How far the tail's pitch moves, at the most, in cents: how fast its
    /// lines' lengths drift, at their fastest.
    pub drift_cents: Option<RangeInclusive<f64>>,
    /// How loud it is on a steady sound, against the default Reverb at the
    /// same Mix, in dB: its impulse response's K-weighted energy over the
    /// default's (`steady_loudness_db`). On a steady sound a reverb's output
    /// is as loud as its impulse response's energy says, so this is how
    /// loud the space sounds behind a held chord or a busy loop.
    pub loudness_db: RangeInclusive<f64>,
}

impl ReverbTargets {
    /// No target but loudness, which every sound has: the start of each
    /// brief's targets.
    const LOUD_AS_DEFAULT: ReverbTargets = ReverbTargets {
        mid_decay_seconds: None,
        low_decay_ratio: None,
        high_decay_ratio: None,
        onset_ms: None,
        early_to_late_db: None,
        dense_after_ms: None,
        loudest_at_ms: None,
        gone_by_ms: None,
        most_above: None,
        most_below: None,
        drift_cents: None,
        loudness_db: LOUDNESS,
    };
}

/// How far apart the factory sounds' loudness may be, in dB, from the
/// quietest to the loudest, as [`ReverbTargets::loudness_db`] measures it.
/// Stepping through the menu at the same Mix should change the space, not
/// the amount of reverb (RFC-010, "Manual 6").
pub const REVERB_LOUDNESS_SPREAD_DB: f64 = 4.0;

/// The loudness every sound aims for, against the default Reverb: as loud
/// as it, within half of [`REVERB_LOUDNESS_SPREAD_DB`] either side.
const LOUDNESS_DB: f64 = 0.0;

/// [`LOUDNESS_DB`], give or take half the spread.
const LOUDNESS: RangeInclusive<f64> = (LOUDNESS_DB - REVERB_LOUDNESS_SPREAD_DB / 2.0)
    ..=(LOUDNESS_DB + REVERB_LOUDNESS_SPREAD_DB / 2.0);

impl ReverbFactorySound {
    /// Every factory sound, in the menu's order: the short spaces, then the
    /// halls, then the long ambient ones.
    pub fn all() -> [ReverbFactorySound; 11] {
        [
            drum_room(),
            gated_drums(),
            dark_room(),
            vocal_plate(),
            echo_chamber(),
            concert_hall(),
            distant(),
            cathedral(),
            bloom(),
            ambient_wash(),
            endless(),
        ]
    }

    /// The factory sound called `key`, if there is one.
    pub fn find(key: &str) -> Option<ReverbFactorySound> {
        Self::all().into_iter().find(|sound| sound.key == key)
    }

    /// The factory sound whose key or name is `name`, ignoring case, as a
    /// song file names it.
    pub fn named(name: &str) -> Option<ReverbFactorySound> {
        Self::all().into_iter().find(|sound| {
            sound.key.eq_ignore_ascii_case(name) || sound.name.eq_ignore_ascii_case(name)
        })
    }
}

fn drum_room() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "drum_room",
        name: "Drum room",
        brief: "A kit in the live room it was played in: thick and real, barely any tail. \
                The early reflections off close walls do the work, so the drums sound \
                bigger and glued together without washing out; the burst is over in about \
                20 ms and what rings after is short, about half a second in the middle.",
        targets: ReverbTargets {
            mid_decay_seconds: Some(0.4..=0.65),
            onset_ms: Some(0.0..=5.0),
            early_to_late_db: Some(3.0..=20.0),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.2,
        settings: ReverbSettings {
            mode: ReverbMode::Room,
            predelay_seconds: 0.002,
            decay_seconds: 0.5,
            depth: 0.3,
            early_size_seconds: 0.02,
            diffusion: 0.7,
            late_size: 0.2,
            bass_mult: 0.8,
            high_mult: 0.6,
            ..ReverbSettings::default()
        },
    }
}

fn gated_drums() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "gated_drums",
        name: "Gated drums",
        brief: "The eighties gated snare: a big, dense burst of room that stops dead, with \
                no tail, as a noise gate on a loud room mic did. All of it is the early \
                burst, about a quarter of a second long, then nothing 20 dB down by 300 ms.",
        targets: ReverbTargets {
            gone_by_ms: Some(180.0..=300.0),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.3,
        settings: ReverbSettings {
            mode: ReverbMode::Chamber,
            predelay_seconds: 0.0,
            decay_seconds: 0.5,
            depth: 0.0,
            early_size_seconds: 0.25,
            diffusion: 1.0,
            ..ReverbSettings::default()
        },
    }
}

fn dark_room() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "dark_room",
        name: "Dark room",
        brief: "A cheap, dark, slightly wobbly space, for lo-fi keys and snares, like a \
                worn spring or an old budget digital unit: a small room's tail of about \
                1.3 s with nothing much above 3.5 kHz, its pitch audibly drifting.",
        targets: ReverbTargets {
            mid_decay_seconds: Some(1.15..=1.45),
            most_above: Some((3_500.0, 5.0)),
            drift_cents: Some(15.0..=40.0),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.25,
        settings: ReverbSettings {
            mode: ReverbMode::Room,
            predelay_seconds: 0.01,
            decay_seconds: 1.3,
            depth: 0.6,
            high_cut_hz: 3_500.0,
            diffusion: 0.6,
            early_mod_depth: 0.5,
            late_size: 0.3,
            late_mod_rate_hz: 1.2,
            late_mod_depth: 1.0,
            ..ReverbSettings::default()
        },
    }
}

fn vocal_plate() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "vocal_plate",
        name: "Vocal plate",
        brief: "A steel plate, the studio's vocal reverb, like an EMT 140: no walls, so no \
                early reflections, dense at once, bright, and thin in the bass, about 1.8 s \
                in the middle with the highs lasting nearly as long. An approximation on \
                the Chamber mode: a real plate's own ring needs a Plate mode.",
        targets: ReverbTargets {
            mid_decay_seconds: Some(1.6..=2.0),
            low_decay_ratio: Some(0.6..=0.8),
            high_decay_ratio: Some(0.8..=1.0),
            dense_after_ms: Some(0.0..=10.0),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.2,
        settings: ReverbSettings {
            mode: ReverbMode::Chamber,
            predelay_seconds: 0.0,
            decay_seconds: 1.8,
            depth: 0.85,
            low_cut_hz: 150.0,
            early_size_seconds: 0.01,
            diffusion: 1.0,
            late_size: 0.35,
            late_mod_rate_hz: 0.8,
            late_mod_depth: 0.4,
            bass_mult: 0.7,
            bass_xover_hz: 400.0,
            high_mult: 0.9,
            high_xover_hz: 3_000.0,
            ..ReverbSettings::default()
        },
    }
}

fn echo_chamber() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "echo_chamber",
        name: "Echo chamber",
        brief: "A hard, tiled room built for reverb, fed from a speaker and heard by a mic, \
                with the lows and the very top filtered out of what's sent to it (the \"Abbey \
                Road trick\", a rule of thumb). A dense, even burst and about 1.9 s in the \
                middle, with little energy below 500 Hz or above 10 kHz.",
        targets: ReverbTargets {
            mid_decay_seconds: Some(1.7..=2.1),
            dense_after_ms: Some(0.0..=20.0),
            most_above: Some((10_000.0, 3.0)),
            most_below: Some((500.0, 15.0)),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.2,
        settings: ReverbSettings {
            mode: ReverbMode::Chamber,
            predelay_seconds: 0.01,
            decay_seconds: 1.9,
            depth: 0.7,
            high_cut_hz: 10_000.0,
            low_cut_hz: 500.0,
            diffusion: 0.9,
            late_size: 0.45,
            high_mult: 0.7,
            ..ReverbSettings::default()
        },
    }
}

fn concert_hall() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "concert_hall",
        name: "Concert hall",
        brief: "A big, lush digital hall, like a Lexicon's, with slow movement in the tail: \
                a 30 ms gap before the hall answers, about 2.8 s in the middle, the lows \
                lasting longer (1.3×) and the highs darkening as it rings (0.55×).",
        targets: ReverbTargets {
            mid_decay_seconds: Some(2.5..=3.1),
            low_decay_ratio: Some(1.15..=1.45),
            high_decay_ratio: Some(0.45..=0.65),
            onset_ms: Some(28.0..=33.0),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.25,
        settings: ReverbSettings {
            mode: ReverbMode::Room,
            predelay_seconds: 0.03,
            decay_seconds: 2.8,
            depth: 0.75,
            early_size_seconds: 0.06,
            late_size: 0.8,
            late_mod_rate_hz: 0.4,
            late_mod_depth: 0.6,
            bass_mult: 1.3,
            high_mult: 0.55,
            ..ReverbSettings::default()
        },
    }
}

fn distant() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "distant",
        name: "Distant",
        brief: "A part placed far behind the mix: late, dull and mostly space, as a sound \
                at the far end of a hall reaches you after its own reflections. The first \
                sound after about 110 ms, mostly tail, with little above 3.2 kHz or below \
                300 Hz.",
        targets: ReverbTargets {
            onset_ms: Some(105.0..=120.0),
            early_to_late_db: Some(-20.0..=-3.0),
            most_above: Some((3_200.0, 5.0)),
            most_below: Some((300.0, 15.0)),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.35,
        settings: ReverbSettings {
            mode: ReverbMode::Room,
            predelay_seconds: 0.11,
            decay_seconds: 2.5,
            depth: 0.95,
            high_cut_hz: 3_200.0,
            low_cut_hz: 300.0,
            late_size: 0.7,
            ..ReverbSettings::default()
        },
    }
}

fn cathedral() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "cathedral",
        name: "Cathedral",
        brief: "Stone, enormous: the walls so far away the first sound comes after about \
                60 ms, a tail of about 7 s in the middle, darkening as it rings, the highs \
                lasting under half as long (0.4×).",
        targets: ReverbTargets {
            mid_decay_seconds: Some(6.3..=7.7),
            high_decay_ratio: Some(0.3..=0.5),
            onset_ms: Some(55.0..=70.0),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.3,
        settings: ReverbSettings {
            mode: ReverbMode::Space,
            predelay_seconds: 0.06,
            decay_seconds: 7.0,
            depth: 0.8,
            early_size_seconds: 0.1,
            late_size: 0.9,
            bass_mult: 1.2,
            high_mult: 0.4,
            high_xover_hz: 3_000.0,
            ..ReverbSettings::default()
        },
    }
}

fn bloom() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "bloom",
        name: "Bloom",
        brief: "Each note swells in rather than starting at once, so chords grow out of \
                nothing: the space at its loudest 0.3 to 0.5 s after the note, then a long \
                tail of about 6 s in the middle.",
        targets: ReverbTargets {
            mid_decay_seconds: Some(5.4..=6.6),
            loudest_at_ms: Some(300.0..=500.0),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.35,
        settings: ReverbSettings {
            mode: ReverbMode::Space,
            predelay_seconds: 0.0,
            decay_seconds: 6.0,
            depth: 0.85,
            early_size_seconds: 0.6,
            diffusion: 1.0,
            early_send: 1.0,
            late_size: 0.7,
            ..ReverbSettings::default()
        },
    }
}

fn ambient_wash() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "ambient_wash",
        name: "Ambient wash",
        brief: "Long, dark and drifting, every note melting into the next: the ambient \
                song's space. About 18 s in the middle, the highs lasting about a third as \
                long, and little above 6 kHz.",
        targets: ReverbTargets {
            mid_decay_seconds: Some(16.0..=20.0),
            high_decay_ratio: Some(0.25..=0.45),
            most_above: Some((6_000.0, 5.0)),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.4,
        settings: ReverbSettings {
            mode: ReverbMode::Space,
            predelay_seconds: 0.02,
            decay_seconds: 18.0,
            depth: 0.9,
            high_cut_hz: 6_000.0,
            late_size: 1.0,
            late_mod_depth: 0.6,
            high_mult: 0.35,
            high_xover_hz: 3_000.0,
            ..ReverbSettings::default()
        },
    }
}

fn endless() -> ReverbFactorySound {
    ReverbFactorySound {
        key: "endless",
        name: "Endless",
        brief: "The longest, darkest tail there is, as near a freeze as this design gets: a \
                pad chord still hanging under the next. About 30 s in the middle, the highs \
                fading at under a third of that, and little above 5 kHz.",
        targets: ReverbTargets {
            mid_decay_seconds: Some(26.0..=33.0),
            high_decay_ratio: Some(0.2..=0.4),
            most_above: Some((5_000.0, 5.0)),
            ..ReverbTargets::LOUD_AS_DEFAULT
        },
        suggested_mix: 0.4,
        settings: ReverbSettings {
            mode: ReverbMode::Space,
            predelay_seconds: 0.02,
            decay_seconds: ReverbSettings::MAX_DECAY_SECONDS,
            depth: 0.95,
            high_cut_hz: 5_000.0,
            late_size: 1.0,
            late_mod_rate_hz: 0.2,
            late_mod_depth: 0.7,
            high_mult: 0.3,
            high_xover_hz: 3_000.0,
            ..ReverbSettings::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_reverb_factory_sound_is_in_range_and_named_once() {
        let sounds = ReverbFactorySound::all();
        let keys: HashSet<_> = sounds.iter().map(|sound| sound.key).collect();
        let names: HashSet<_> = sounds.iter().map(|sound| sound.name).collect();
        assert_eq!(keys.len(), sounds.len());
        assert_eq!(names.len(), sounds.len());
        for sound in &sounds {
            assert_eq!(sound.settings.validate(), Ok(()), "{}", sound.key);
            assert!(!sound.brief.is_empty(), "{}", sound.key);
            assert!((0.0..=1.0).contains(&sound.suggested_mix), "{}", sound.key);
            assert_eq!(ReverbFactorySound::find(sound.key).as_ref(), Some(sound));
            assert_eq!(
                ReverbFactorySound::named(&sound.name.to_uppercase()).as_ref(),
                Some(sound)
            );
        }
        assert_eq!(ReverbFactorySound::find("spring"), None);
    }

    /// RFC-010's eleven, by name: the eight it recommended and the three
    /// ambient ones Will added (open question 6).
    #[test]
    fn they_are_rfc_010s_eleven() {
        let mut names: Vec<_> = ReverbFactorySound::all()
            .iter()
            .map(|sound| sound.name)
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                "Ambient wash",
                "Bloom",
                "Cathedral",
                "Concert hall",
                "Dark room",
                "Distant",
                "Drum room",
                "Echo chamber",
                "Endless",
                "Gated drums",
                "Vocal plate",
            ]
        );
    }
}
