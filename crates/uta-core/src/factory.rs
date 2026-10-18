//! The synth's factory sounds: starting points for the main jobs a synth
//! does in a loop, each made from a written brief with measurable targets
//! (RFC-008, "Factory sounds, and turning an idea into settings").
//!
//! A sound is made with a fixed loop: brief → settings → render → measure
//! against the targets → Will listens → adjust. The targets are checked by
//! `uta-engine`'s `tests/factory.rs`, which renders each sound's audition
//! and measures it as `uta analyse` does. What making them taught is in
//! `docs/sound-design/synth.md`, the voicing guide.

use std::ops::RangeInclusive;

use crate::{FilterMode, LfoShape, ModDestination, ModSource, SynthSettings, VoiceMode, Waveform};

/// One factory sound: its brief, the targets that say it meets it, and the
/// settings that make it.
#[derive(Debug, Clone, PartialEq)]
pub struct FactorySound {
    /// Its name as the app's call carries it: `"pluck"`.
    pub key: &'static str,
    /// Its name in the Sound menu: "Pluck".
    pub name: &'static str,
    /// What it should sound like, with a reference: what the settings were
    /// made from.
    pub brief: &'static str,
    /// What it plays to be measured.
    pub audition: Audition,
    /// What the audition should measure.
    pub targets: Targets,
    /// Every synth setting.
    pub settings: SynthSettings,
}

/// What a sound plays to be measured: the notes of one chord, or one note,
/// held for a time at velocity 100, then let go. A pad is measured on a
/// chord, as it's played, so its loudness is the loudness you hear.
#[derive(Debug, Clone, PartialEq)]
pub struct Audition {
    /// The notes, as MIDI note numbers (middle C is 60).
    pub pitches: &'static [u8],
    /// How long they're held, in seconds.
    pub held_seconds: f64,
}

/// What a sound's audition should measure, each as a range. Each is one of
/// `uta analyse`'s measurements, taken on the audition's render from the
/// note's start until it has died away.
#[derive(Debug, Clone, PartialEq)]
pub struct Targets {
    /// How bright it is: the spectral centroid of the whole render, in Hz.
    pub brightness_hz: RangeInclusive<f64>,
    /// How fast it starts: the rise from 10% to 90% of its peak, in ms.
    pub attack_ms: RangeInclusive<f64>,
    /// How long it lasts: from its first sound until it has faded for good
    /// to 40 dB under its loudest, in seconds. A pluck's is its own decay,
    /// held or not; a held sound's is the time it's held and its release,
    /// so its range starts far enough past the hold to need the release.
    pub fade_seconds: RangeInclusive<f64>,
    /// How loud it is while held, against the default sound (a new
    /// track's saw) playing middle C for a second, in dB: the K-weighted RMS
    /// (the filter of ITU-R BS.1770) from its first sound for as long as
    /// it's held. Against the default, so choosing a factory sound on a new
    /// track doesn't change how loud it is.
    pub loudness_db: RangeInclusive<f64>,
}

/// How far apart the factory sounds' loudness may be, in dB: from the
/// quietest to the loudest, each played as its audition. Switching sounds
/// in a loop shouldn't make you reach for the volume.
pub const LOUDNESS_SPREAD_DB: f64 = 3.0;

/// The loudness every sound aims for while held, against the default
/// sound, as [`Targets::loudness_db`] measures it: as loud as it, within
/// half of [`LOUDNESS_SPREAD_DB`] either side.
const LOUDNESS_DB: f64 = 0.0;

/// [`LOUDNESS_DB`], give or take half the spread.
const LOUDNESS: RangeInclusive<f64> =
    (LOUDNESS_DB - LOUDNESS_SPREAD_DB / 2.0)..=(LOUDNESS_DB + LOUDNESS_SPREAD_DB / 2.0);

impl FactorySound {
    /// Every factory sound, in the Sound menu's order: basses, then the
    /// short sounds, the long ones and the leads, then the bell.
    pub fn all() -> [FactorySound; 8] {
        [
            bass(),
            glide_bass(),
            pluck(),
            pad(),
            strings(),
            lead(),
            sync_lead(),
            bell(),
        ]
    }

    /// The factory sound called `key`, if there is one.
    pub fn find(key: &str) -> Option<FactorySound> {
        Self::all().into_iter().find(|sound| sound.key == key)
    }
}

fn bass() -> FactorySound {
    FactorySound {
        key: "bass",
        name: "Bass",
        brief: "A round, solid bass for lines around C1 to C2, like a Juno-106 or Moog \
                bass: a saw with a square-wave sub under it, warm rather than buzzy, with a \
                little bite at the start of each note that settles while it's held. It \
                should be felt under a kick, and still be heard as a note on small speakers.",
        audition: Audition {
            pitches: &[36],
            held_seconds: 0.5,
        },
        targets: Targets {
            brightness_hz: 150.0..=450.0,
            attack_ms: 0.0..=25.0,
            fade_seconds: 0.4..=0.8,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Saw,
            sub_octave: -1,
            osc1_level: 0.8,
            sub_level: 0.7,
            cutoff_hz: 320.0,
            resonance: 0.15,
            filter_env_octaves: 2.5,
            filter_key: 0.5,
            filter_attack_seconds: 0.001,
            filter_decay_seconds: 0.25,
            filter_sustain: 0.25,
            filter_release_seconds: 0.08,
            attack_seconds: 0.003,
            decay_seconds: 0.3,
            sustain: 0.8,
            release_seconds: 0.08,
            ..SynthSettings::default()
        },
    }
}

fn glide_bass() -> FactorySound {
    FactorySound {
        key: "glide_bass",
        name: "Glide bass",
        brief: "A mono bass that slides between overlapping notes, like an SH-101 or a \
                TB-303 line: a saw and a pulse an octave under it through a resonant filter \
                that snaps open on each fresh note. Overlapping notes glide in about 80 ms \
                without re-attacking (Legato), so a line sounds like one rubbery voice.",
        audition: Audition {
            pitches: &[36],
            held_seconds: 0.5,
        },
        targets: Targets {
            brightness_hz: 250.0..=700.0,
            attack_ms: 0.0..=25.0,
            fade_seconds: 0.3..=0.8,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Saw,
            osc2_wave: Waveform::Pulse,
            osc2_octave: -1,
            osc1_level: 0.8,
            osc2_level: 0.4,
            cutoff_hz: 260.0,
            resonance: 0.55,
            filter_env_octaves: 3.5,
            filter_key: 0.3,
            filter_attack_seconds: 0.001,
            filter_decay_seconds: 0.22,
            filter_sustain: 0.1,
            filter_release_seconds: 0.06,
            attack_seconds: 0.002,
            decay_seconds: 0.4,
            sustain: 0.75,
            release_seconds: 0.05,
            voice_mode: VoiceMode::Mono,
            legato: true,
            glide_seconds: 0.08,
            ..SynthSettings::default()
        },
    }
}

fn pluck() -> FactorySound {
    FactorySound {
        key: "pluck",
        name: "Pluck",
        brief: "A round, short Juno pluck that sits under a chord: two saws a few cents \
                apart, with the filter snapping open and closing over about 150 ms, and the \
                note gone within about half a second whether you hold it or not. Bright at the \
                start, never harsh; it should work as 8th-note arpeggios around middle C.",
        audition: Audition {
            pitches: &[60],
            held_seconds: 0.25,
        },
        targets: Targets {
            brightness_hz: 400.0..=1100.0,
            attack_ms: 0.0..=10.0,
            fade_seconds: 0.2..=0.6,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Saw,
            osc2_wave: Waveform::Saw,
            osc2_pitch_semitones: 0.1,
            osc1_level: 1.0,
            osc2_level: 1.0,
            sub_level: 0.6,
            cutoff_hz: 250.0,
            resonance: 0.2,
            filter_env_octaves: 3.5,
            filter_key: 0.6,
            filter_attack_seconds: 0.001,
            filter_decay_seconds: 0.4,
            filter_sustain: 0.0,
            filter_release_seconds: 0.15,
            attack_seconds: 0.002,
            decay_seconds: 1.2,
            sustain: 0.0,
            release_seconds: 0.35,
            ..SynthSettings::default()
        },
    }
}

fn pad() -> FactorySound {
    FactorySound {
        key: "pad",
        name: "Pad",
        brief: "A warm, slow pad for held chords, like a Juno or OB pad: two saws detuned \
                against each other so the chord shimmers, a filter that opens gently after \
                the note starts and drifts slowly, and a little breath of noise. It swells \
                in over about half a second and fades over about a second when let go. Soft \
                enough to sit behind everything.",
        audition: Audition {
            pitches: &[48, 52, 55],
            held_seconds: 2.0,
        },
        targets: Targets {
            brightness_hz: 250.0..=800.0,
            attack_ms: 250.0..=900.0,
            fade_seconds: 2.3..=3.5,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Saw,
            osc2_wave: Waveform::Saw,
            osc2_pitch_semitones: 0.12,
            osc1_level: 0.45,
            osc2_level: 0.45,
            noise_level: 0.03,
            cutoff_hz: 650.0,
            resonance: 0.1,
            filter_env_octaves: 1.2,
            filter_key: 0.4,
            filter_lfo_octaves: 0.3,
            filter_attack_seconds: 0.8,
            filter_decay_seconds: 1.5,
            filter_sustain: 0.5,
            filter_release_seconds: 1.2,
            attack_seconds: 0.6,
            decay_seconds: 1.0,
            sustain: 0.85,
            release_seconds: 1.1,
            lfo_shape: LfoShape::Triangle,
            lfo_rate_hz: 0.25,
            ..SynthSettings::default()
        },
    }
}

fn strings() -> FactorySound {
    FactorySound {
        key: "strings",
        name: "PWM strings",
        brief: "A string-machine pad from pulse-width modulation, like a Juno-60's or a \
                Solina's strings: a pulse whose width breathes slowly, so the chord seems to \
                be played by several players, brighter and thinner than the Pad, with a \
                bowed start of about a quarter of a second. Held chords around C3 to C4.",
        audition: Audition {
            pitches: &[48, 52, 55],
            held_seconds: 2.0,
        },
        targets: Targets {
            brightness_hz: 600.0..=1600.0,
            attack_ms: 100.0..=450.0,
            fade_seconds: 2.2..=3.0,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Pulse,
            osc1_width: 0.5,
            osc2_wave: Waveform::Saw,
            osc2_pitch_semitones: -0.06,
            osc1_level: 0.33,
            osc2_level: 0.12,
            cutoff_hz: 2400.0,
            resonance: 0.05,
            filter_key: 0.5,
            attack_seconds: 0.25,
            decay_seconds: 0.5,
            sustain: 0.9,
            release_seconds: 0.7,
            lfo_shape: LfoShape::Triangle,
            lfo_rate_hz: 0.7,
            lfo_width: 0.3,
            ..SynthSettings::default()
        },
    }
}

fn lead() -> FactorySound {
    FactorySound {
        key: "lead",
        name: "Lead",
        brief: "A singing mono lead for melodies around C4 to C5, like a Minimoog or Pro-One \
                lead: two saws a few cents apart, a filter with some resonance that \
                stays fairly open, a gentle vibrato, and short glides between overlapping \
                notes. Present and clear, cutting through a mix without being shrill.",
        audition: Audition {
            pitches: &[67],
            held_seconds: 1.0,
        },
        targets: Targets {
            brightness_hz: 900.0..=2200.0,
            attack_ms: 0.0..=20.0,
            fade_seconds: 0.8..=1.6,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Saw,
            osc2_wave: Waveform::Saw,
            osc2_pitch_semitones: 0.08,
            osc1_level: 0.6,
            osc2_level: 0.5,
            cutoff_hz: 1400.0,
            resonance: 0.3,
            filter_env_octaves: 1.0,
            filter_key: 0.7,
            filter_attack_seconds: 0.005,
            filter_decay_seconds: 0.4,
            filter_sustain: 0.4,
            filter_release_seconds: 0.2,
            attack_seconds: 0.006,
            decay_seconds: 0.3,
            sustain: 0.85,
            release_seconds: 0.15,
            lfo_shape: LfoShape::Sine,
            lfo_rate_hz: 5.5,
            lfo_vibrato_semitones: 0.12,
            voice_mode: VoiceMode::Mono,
            legato: true,
            glide_seconds: 0.05,
            ..SynthSettings::default()
        },
    }
}

fn sync_lead() -> FactorySound {
    FactorySound {
        key: "sync_lead",
        name: "Sync lead",
        brief: "The classic tearing sync lead, like a Prophet-5's or the Cars' \"Let's Go\": \
                oscillator 2 synced to oscillator 1, its pitch swept down by the filter \
                envelope on each note, so every note starts with a bright vocal \"yow\" that \
                settles into a steady, nasal tone. Mono, for lines around C4.",
        audition: Audition {
            pitches: &[60],
            held_seconds: 1.0,
        },
        targets: Targets {
            brightness_hz: 1000.0..=2800.0,
            attack_ms: 0.0..=20.0,
            fade_seconds: 0.8..=1.6,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Saw,
            osc2_wave: Waveform::Saw,
            osc2_pitch_semitones: 5.0,
            osc2_sync: true,
            osc1_level: 0.2,
            osc2_level: 0.8,
            cutoff_hz: 4000.0,
            resonance: 0.1,
            filter_key: 0.5,
            filter_attack_seconds: 0.001,
            filter_decay_seconds: 0.5,
            filter_sustain: 0.1,
            filter_release_seconds: 0.2,
            attack_seconds: 0.004,
            decay_seconds: 0.4,
            sustain: 0.8,
            release_seconds: 0.15,
            lfo_vibrato_semitones: 0.08,
            lfo_rate_hz: 5.0,
            voice_mode: VoiceMode::Mono,
            legato: true,
            glide_seconds: 0.03,
            mod2_source: ModSource::FilterEnv,
            mod2_destination: ModDestination::Osc2Pitch,
            mod2_amount: 0.75,
            ..SynthSettings::default()
        },
    }
}

fn bell() -> FactorySound {
    FactorySound {
        key: "bell",
        name: "Bell",
        brief: "A glassy bell for sparse melodies around C5, like an FM electric piano's \
                tine or a music box: a pure sine with a second, inharmonic partial (2.76 \
                times its pitch, as a struck bar has) that rings briefly at the strike and \
                fades fast, leaving the sine to ring for a couple of seconds. Clear, not \
                metallic-harsh.",
        audition: Audition {
            pitches: &[72],
            held_seconds: 0.5,
        },
        targets: Targets {
            brightness_hz: 550.0..=1500.0,
            attack_ms: 0.0..=10.0,
            fade_seconds: 1.0..=3.0,
            loudness_db: LOUDNESS,
        },
        settings: SynthSettings {
            osc1_wave: Waveform::Sine,
            osc2_wave: Waveform::Sine,
            osc2_octave: 1,
            osc2_pitch_semitones: 5.57,
            osc1_level: 1.0,
            osc2_level: 0.0,
            filter_mode: FilterMode::LowPass,
            cutoff_hz: SynthSettings::MAX_CUTOFF_HZ,
            filter_attack_seconds: 0.001,
            filter_decay_seconds: 0.35,
            filter_sustain: 0.0,
            filter_release_seconds: 0.3,
            attack_seconds: 0.001,
            decay_seconds: 3.0,
            sustain: 0.0,
            release_seconds: 2.5,
            mod2_source: ModSource::FilterEnv,
            mod2_destination: ModDestination::Osc2Level,
            mod2_amount: 0.6,
            ..SynthSettings::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_factory_sound_is_in_range_and_named_once() {
        let sounds = FactorySound::all();
        let keys: HashSet<_> = sounds.iter().map(|sound| sound.key).collect();
        let names: HashSet<_> = sounds.iter().map(|sound| sound.name).collect();
        assert_eq!(keys.len(), sounds.len());
        assert_eq!(names.len(), sounds.len());
        for sound in &sounds {
            assert_eq!(sound.settings.validate(), Ok(()), "{}", sound.key);
            assert!(!sound.brief.is_empty() && !sound.audition.pitches.is_empty());
            assert_eq!(FactorySound::find(sound.key).as_ref(), Some(sound));
        }
        assert_eq!(FactorySound::find("kazoo"), None);
    }
}
