//! The built-in synth's settings. See RFC-002, "The shared model", point 6,
//! and RFC-008, "The instrument".
//!
//! The engine keeps its own copy of these, with the same names, units, ranges
//! and defaults (the "Synth settings" table in `docs/plans/make-a-loop.md`,
//! and RFC-008's), so the snapshot only has to copy values across.

use serde::{Deserialize, Serialize};

use crate::setting::{Choice, Description, Scale, Unit};

/// An oscillator's wave shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    /// A pulse wave. At a Width of 50% it's a square, which command formats
    /// before 6 called it.
    #[serde(alias = "square")]
    Pulse,
}

impl Waveform {
    /// Every wave, in the panel's order.
    pub const ALL: [Waveform; 4] = [Self::Sine, Self::Triangle, Self::Saw, Self::Pulse];

    /// The wave as `SetSynthParam` carries it: `"saw"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::Sine => "sine",
            Self::Triangle => "triangle",
            Self::Saw => "saw",
            Self::Pulse => "pulse",
        }
    }
}

/// Every wave, as the panel offers them.
const WAVEFORMS: [Choice; 4] = [
    Choice {
        value: "sine",
        label: "Sine",
    },
    Choice {
        value: "triangle",
        label: "Triangle",
    },
    Choice {
        value: "saw",
        label: "Saw",
    },
    Choice {
        value: "pulse",
        label: "Pulse",
    },
];

/// Which part of the sound the filter lets through.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    /// Everything under the cutoff: today's filter.
    #[default]
    LowPass,
    /// A band around the cutoff.
    BandPass,
    /// Everything over the cutoff.
    HighPass,
}

impl FilterMode {
    /// Every mode, in the panel's order.
    pub const ALL: [FilterMode; 3] = [Self::LowPass, Self::BandPass, Self::HighPass];

    /// The mode as `SetSynthParam` carries it: `"low_pass"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::LowPass => "low_pass",
            Self::BandPass => "band_pass",
            Self::HighPass => "high_pass",
        }
    }
}

/// Every filter mode, as the panel offers them.
const FILTER_MODES: [Choice; 3] = [
    Choice {
        value: "low_pass",
        label: "Low-pass",
    },
    Choice {
        value: "band_pass",
        label: "Band-pass",
    },
    Choice {
        value: "high_pass",
        label: "High-pass",
    },
];

/// How many notes a synth track plays at once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    /// As many as it has voices: chords and overlapping notes all sound.
    #[default]
    Poly,
    /// One at a time: the latest note wins, and letting go of it returns to
    /// one still held.
    Mono,
}

impl VoiceMode {
    /// Every mode, in the panel's order.
    pub const ALL: [VoiceMode; 2] = [Self::Poly, Self::Mono];

    /// The mode as `SetSynthParam` carries it: `"mono"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::Poly => "poly",
            Self::Mono => "mono",
        }
    }
}

/// Every voice mode, as the panel offers them.
const VOICE_MODES: [Choice; 2] = [
    Choice {
        value: "poly",
        label: "Poly",
    },
    Choice {
        value: "mono",
        label: "Mono",
    },
];

/// Everything that shapes the synth's sound, from the oscillators through
/// the mixer, filter and envelope to the voice: the order the sound flows
/// in, and the order [`Self::params`] gives them.
///
/// Serialises with the same names as [`SynthParam`], as `AddTracks` carries
/// it. A setting left out takes its default, so the settings of every
/// older command format still read, with format 4's `waveform` as
/// oscillator 1's wave.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SynthSettings {
    #[serde(alias = "waveform")]
    pub osc1_wave: Waveform,
    /// Oscillator 1's octave, from the note: -2 to 2.
    pub osc1_octave: i8,
    /// How much of each cycle oscillator 1's pulse spends high, from 0.05
    /// to 0.95: 0.5 is a square. Only the pulse has a width.
    pub osc1_width: f32,
    pub osc2_wave: Waveform,
    /// Oscillator 2's octave, from the note: -2 to 2.
    pub osc2_octave: i8,
    /// Oscillator 2's offset from its octave, in semitones: -12 to 12.
    /// Controls move it finely near zero, for detuning.
    pub osc2_pitch_semitones: f32,
    /// Whether oscillator 2 restarts its cycle each time oscillator 1
    /// starts one, so it always sounds at oscillator 1's pitch.
    pub osc2_sync: bool,
    /// How far under oscillator 1 the sub plays, in octaves: -1 or -2.
    pub sub_octave: i8,
    /// How much of each goes into the filter, from 0 to 1 (linear).
    pub osc1_level: f32,
    pub osc2_level: f32,
    pub sub_level: f32,
    pub noise_level: f32,
    pub filter_mode: FilterMode,
    /// The filter's cutoff, in Hz. Controls move it on a log scale.
    pub cutoff_hz: f32,
    /// The filter's resonance, from 0 to 1.
    pub resonance: f32,
    /// How far the filter envelope moves the cutoff at its peak, in octaves:
    /// -10 to 10. Below zero it closes the filter instead of opening it.
    pub filter_env_octaves: f32,
    /// How closely the cutoff follows the note, from 0 to 1: at 1 it moves
    /// an octave for each octave the note is above or below middle C.
    pub filter_key: f32,
    /// The filter envelope, in the same units and ranges as the amp
    /// envelope below.
    pub filter_attack_seconds: f32,
    pub filter_decay_seconds: f32,
    pub filter_sustain: f32,
    pub filter_release_seconds: f32,
    /// The amp envelope: each note's level.
    pub attack_seconds: f32,
    pub decay_seconds: f32,
    /// The level held after the decay, from 0 to 1 (linear, not dB).
    pub sustain: f32,
    pub release_seconds: f32,
    /// Whether the synth plays chords, or one note at a time.
    pub voice_mode: VoiceMode,
    /// In Mono, whether a note that overlaps the one before carries its
    /// envelopes on instead of starting them again.
    pub legato: bool,
    /// In Mono, how long the pitch takes to slide from one note to the next
    /// when they overlap, in seconds. At its shortest it's off: the pitch
    /// moves at once.
    pub glide_seconds: f32,
    /// Whether the oscillators start their wave afresh on each note. Off,
    /// they keep running between notes.
    pub osc_retrigger: bool,
}

impl SynthSettings {
    pub const MIN_OCTAVE: i8 = -2;
    pub const MAX_OCTAVE: i8 = 2;
    /// The narrowest pulse, and the widest: closer to 0 or 1 its two jumps
    /// fall within a sample of each other at high notes, and can't both be
    /// smoothed. See RFC-008, "In the engine".
    pub const MIN_WIDTH: f32 = 0.05;
    pub const MAX_WIDTH: f32 = 0.95;
    /// Oscillator 2's Pitch either side of its octave, in semitones.
    pub const MAX_PITCH_SEMITONES: f32 = 12.0;
    pub const MIN_SUB_OCTAVE: i8 = -2;
    pub const MAX_SUB_OCTAVE: i8 = -1;
    pub const MIN_LEVEL: f32 = 0.0;
    pub const MAX_LEVEL: f32 = 1.0;
    pub const MIN_CUTOFF_HZ: f32 = 20.0;
    pub const MAX_CUTOFF_HZ: f32 = 20_000.0;
    pub const MIN_RESONANCE: f32 = 0.0;
    pub const MAX_RESONANCE: f32 = 1.0;
    /// The filter envelope's amount either side of zero, in octaves: enough
    /// to sweep the whole range from the lowest cutoff.
    pub const MAX_FILTER_ENV_OCTAVES: f32 = 10.0;
    pub const MIN_FILTER_KEY: f32 = 0.0;
    pub const MAX_FILTER_KEY: f32 = 1.0;
    /// The shortest attack, decay or release: 1 ms.
    pub const MIN_ENVELOPE_SECONDS: f32 = 0.001;
    /// The longest attack, decay or release.
    pub const MAX_ENVELOPE_SECONDS: f32 = 10.0;
    pub const MIN_SUSTAIN: f32 = 0.0;
    pub const MAX_SUSTAIN: f32 = 1.0;
    /// The shortest Glide, which is off, and the longest.
    pub const MIN_GLIDE_SECONDS: f32 = 0.001;
    pub const MAX_GLIDE_SECONDS: f32 = 5.0;

    /// Every setting, as `SetSynthParam` carries them, in the order the
    /// sound flows through them.
    pub fn params(&self) -> [SynthParam; 29] {
        [
            SynthParam::Osc1Wave(self.osc1_wave),
            SynthParam::Osc1Octave(self.osc1_octave),
            SynthParam::Osc1Width(self.osc1_width),
            SynthParam::Osc2Wave(self.osc2_wave),
            SynthParam::Osc2Octave(self.osc2_octave),
            SynthParam::Osc2PitchSemitones(self.osc2_pitch_semitones),
            SynthParam::Osc2Sync(self.osc2_sync),
            SynthParam::SubOctave(self.sub_octave),
            SynthParam::Osc1Level(self.osc1_level),
            SynthParam::Osc2Level(self.osc2_level),
            SynthParam::SubLevel(self.sub_level),
            SynthParam::NoiseLevel(self.noise_level),
            SynthParam::FilterMode(self.filter_mode),
            SynthParam::CutoffHz(self.cutoff_hz),
            SynthParam::Resonance(self.resonance),
            SynthParam::FilterEnvOctaves(self.filter_env_octaves),
            SynthParam::FilterKey(self.filter_key),
            SynthParam::FilterAttackSeconds(self.filter_attack_seconds),
            SynthParam::FilterDecaySeconds(self.filter_decay_seconds),
            SynthParam::FilterSustain(self.filter_sustain),
            SynthParam::FilterReleaseSeconds(self.filter_release_seconds),
            SynthParam::AttackSeconds(self.attack_seconds),
            SynthParam::DecaySeconds(self.decay_seconds),
            SynthParam::Sustain(self.sustain),
            SynthParam::ReleaseSeconds(self.release_seconds),
            SynthParam::VoiceMode(self.voice_mode),
            SynthParam::Legato(self.legato),
            SynthParam::GlideSeconds(self.glide_seconds),
            SynthParam::OscRetrigger(self.osc_retrigger),
        ]
    }

    /// Checks every setting is in range, or returns the first that isn't.
    pub(crate) fn validate(&self) -> Result<(), SynthParam> {
        match self.params().into_iter().find(|param| !param.in_range()) {
            Some(param) => Err(param),
            None => Ok(()),
        }
    }

    /// Sets one setting and returns the old value, or an error if `param`
    /// is out of range. On error, nothing changes.
    pub(crate) fn set(&mut self, param: SynthParam) -> Result<SynthParam, SynthParam> {
        use std::mem::replace;
        if !param.in_range() {
            return Err(param);
        }
        Ok(match param {
            SynthParam::Osc1Wave(wave) => SynthParam::Osc1Wave(replace(&mut self.osc1_wave, wave)),
            SynthParam::Osc1Octave(octave) => {
                SynthParam::Osc1Octave(replace(&mut self.osc1_octave, octave))
            }
            SynthParam::Osc1Width(value) => {
                SynthParam::Osc1Width(replace(&mut self.osc1_width, value))
            }
            SynthParam::Osc2Wave(wave) => SynthParam::Osc2Wave(replace(&mut self.osc2_wave, wave)),
            SynthParam::Osc2Octave(octave) => {
                SynthParam::Osc2Octave(replace(&mut self.osc2_octave, octave))
            }
            SynthParam::Osc2PitchSemitones(value) => {
                SynthParam::Osc2PitchSemitones(replace(&mut self.osc2_pitch_semitones, value))
            }
            SynthParam::Osc2Sync(on) => SynthParam::Osc2Sync(replace(&mut self.osc2_sync, on)),
            SynthParam::SubOctave(octave) => {
                SynthParam::SubOctave(replace(&mut self.sub_octave, octave))
            }
            SynthParam::Osc1Level(value) => {
                SynthParam::Osc1Level(replace(&mut self.osc1_level, value))
            }
            SynthParam::Osc2Level(value) => {
                SynthParam::Osc2Level(replace(&mut self.osc2_level, value))
            }
            SynthParam::SubLevel(value) => {
                SynthParam::SubLevel(replace(&mut self.sub_level, value))
            }
            SynthParam::NoiseLevel(value) => {
                SynthParam::NoiseLevel(replace(&mut self.noise_level, value))
            }
            SynthParam::FilterMode(mode) => {
                SynthParam::FilterMode(replace(&mut self.filter_mode, mode))
            }
            SynthParam::CutoffHz(value) => {
                SynthParam::CutoffHz(replace(&mut self.cutoff_hz, value))
            }
            SynthParam::Resonance(value) => {
                SynthParam::Resonance(replace(&mut self.resonance, value))
            }
            SynthParam::FilterEnvOctaves(value) => {
                SynthParam::FilterEnvOctaves(replace(&mut self.filter_env_octaves, value))
            }
            SynthParam::FilterKey(value) => {
                SynthParam::FilterKey(replace(&mut self.filter_key, value))
            }
            SynthParam::FilterAttackSeconds(value) => {
                SynthParam::FilterAttackSeconds(replace(&mut self.filter_attack_seconds, value))
            }
            SynthParam::FilterDecaySeconds(value) => {
                SynthParam::FilterDecaySeconds(replace(&mut self.filter_decay_seconds, value))
            }
            SynthParam::FilterSustain(value) => {
                SynthParam::FilterSustain(replace(&mut self.filter_sustain, value))
            }
            SynthParam::FilterReleaseSeconds(value) => {
                SynthParam::FilterReleaseSeconds(replace(&mut self.filter_release_seconds, value))
            }
            SynthParam::AttackSeconds(value) => {
                SynthParam::AttackSeconds(replace(&mut self.attack_seconds, value))
            }
            SynthParam::DecaySeconds(value) => {
                SynthParam::DecaySeconds(replace(&mut self.decay_seconds, value))
            }
            SynthParam::Sustain(value) => SynthParam::Sustain(replace(&mut self.sustain, value)),
            SynthParam::ReleaseSeconds(value) => {
                SynthParam::ReleaseSeconds(replace(&mut self.release_seconds, value))
            }
            SynthParam::VoiceMode(mode) => {
                SynthParam::VoiceMode(replace(&mut self.voice_mode, mode))
            }
            SynthParam::Legato(on) => SynthParam::Legato(replace(&mut self.legato, on)),
            SynthParam::GlideSeconds(value) => {
                SynthParam::GlideSeconds(replace(&mut self.glide_seconds, value))
            }
            SynthParam::OscRetrigger(on) => {
                SynthParam::OscRetrigger(replace(&mut self.osc_retrigger, on))
            }
        })
    }
}

impl Default for SynthSettings {
    /// Today's sound, from before RFC-008: oscillator 1 a saw at the note's
    /// pitch and full level, everything else in the mixer at zero, oscillator
    /// 2 free of oscillator 1, a low-pass filter that nothing moves, and the
    /// oscillators restarting on each note, in Poly. The filter envelope is
    /// set for a pluck, so turning up its amount alone makes one.
    fn default() -> Self {
        Self {
            osc1_wave: Waveform::Saw,
            osc1_octave: 0,
            osc1_width: 0.5,
            osc2_wave: Waveform::Saw,
            osc2_octave: 0,
            osc2_pitch_semitones: 0.0,
            osc2_sync: false,
            sub_octave: -1,
            osc1_level: 1.0,
            osc2_level: 0.0,
            sub_level: 0.0,
            noise_level: 0.0,
            filter_mode: FilterMode::LowPass,
            cutoff_hz: Self::MAX_CUTOFF_HZ,
            resonance: 0.0,
            filter_env_octaves: 0.0,
            filter_key: 0.0,
            filter_attack_seconds: 0.001,
            filter_decay_seconds: 0.3,
            filter_sustain: 0.0,
            filter_release_seconds: 0.2,
            attack_seconds: 0.005,
            decay_seconds: 0.2,
            sustain: 0.7,
            release_seconds: 0.2,
            voice_mode: VoiceMode::Poly,
            legato: false,
            glide_seconds: Self::MIN_GLIDE_SECONDS,
            osc_retrigger: true,
        }
    }
}

/// One synth setting with its new value, as `SetSynthParam` carries it.
///
/// Serialises as `{"name":"cutoff_hz","value":1000.0}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum SynthParam {
    /// Command formats before 6 called it `waveform`.
    #[serde(alias = "waveform")]
    Osc1Wave(Waveform),
    Osc1Octave(i8),
    Osc1Width(f32),
    Osc2Wave(Waveform),
    Osc2Octave(i8),
    Osc2PitchSemitones(f32),
    Osc2Sync(bool),
    SubOctave(i8),
    Osc1Level(f32),
    Osc2Level(f32),
    SubLevel(f32),
    NoiseLevel(f32),
    FilterMode(FilterMode),
    CutoffHz(f32),
    Resonance(f32),
    FilterEnvOctaves(f32),
    FilterKey(f32),
    FilterAttackSeconds(f32),
    FilterDecaySeconds(f32),
    FilterSustain(f32),
    FilterReleaseSeconds(f32),
    AttackSeconds(f32),
    DecaySeconds(f32),
    Sustain(f32),
    ReleaseSeconds(f32),
    VoiceMode(VoiceMode),
    Legato(bool),
    GlideSeconds(f32),
    OscRetrigger(bool),
}

impl SynthParam {
    /// Whether `other` is the same setting, whatever its value.
    pub fn same_setting(&self, other: &SynthParam) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The first command format that has this setting.
    pub(crate) fn since_format(&self) -> u32 {
        match self {
            Self::Osc1Wave(_)
            | Self::CutoffHz(_)
            | Self::Resonance(_)
            | Self::AttackSeconds(_)
            | Self::DecaySeconds(_)
            | Self::Sustain(_)
            | Self::ReleaseSeconds(_) => 2,
            Self::FilterMode(_)
            | Self::FilterEnvOctaves(_)
            | Self::FilterKey(_)
            | Self::FilterAttackSeconds(_)
            | Self::FilterDecaySeconds(_)
            | Self::FilterSustain(_)
            | Self::FilterReleaseSeconds(_) => 8,
            Self::Osc1Width(_) | Self::Osc2Sync(_) => 10,
            Self::VoiceMode(_) | Self::Legato(_) | Self::GlideSeconds(_) => 11,
            _ => 6,
        }
    }

    /// The setting's value and its limits (inclusive), or `None` for a
    /// wave, a mode or a switch, which have no range.
    pub fn range(&self) -> Option<(f32, f32, f32)> {
        use SynthSettings as S;
        let octave = |octave: i8, min: i8, max: i8| {
            Some((f32::from(octave), f32::from(min), f32::from(max)))
        };
        match *self {
            Self::Osc1Wave(_)
            | Self::Osc2Wave(_)
            | Self::Osc2Sync(_)
            | Self::FilterMode(_)
            | Self::VoiceMode(_)
            | Self::Legato(_)
            | Self::OscRetrigger(_) => None,
            Self::Osc1Octave(value) | Self::Osc2Octave(value) => {
                octave(value, S::MIN_OCTAVE, S::MAX_OCTAVE)
            }
            Self::Osc1Width(value) => Some((value, S::MIN_WIDTH, S::MAX_WIDTH)),
            Self::Osc2PitchSemitones(value) => {
                Some((value, -S::MAX_PITCH_SEMITONES, S::MAX_PITCH_SEMITONES))
            }
            Self::SubOctave(value) => octave(value, S::MIN_SUB_OCTAVE, S::MAX_SUB_OCTAVE),
            Self::Osc1Level(value)
            | Self::Osc2Level(value)
            | Self::SubLevel(value)
            | Self::NoiseLevel(value) => Some((value, S::MIN_LEVEL, S::MAX_LEVEL)),
            Self::CutoffHz(hz) => Some((hz, S::MIN_CUTOFF_HZ, S::MAX_CUTOFF_HZ)),
            Self::Resonance(value) => Some((value, S::MIN_RESONANCE, S::MAX_RESONANCE)),
            Self::FilterEnvOctaves(value) => {
                Some((value, -S::MAX_FILTER_ENV_OCTAVES, S::MAX_FILTER_ENV_OCTAVES))
            }
            Self::FilterKey(value) => Some((value, S::MIN_FILTER_KEY, S::MAX_FILTER_KEY)),
            Self::AttackSeconds(seconds)
            | Self::DecaySeconds(seconds)
            | Self::ReleaseSeconds(seconds)
            | Self::FilterAttackSeconds(seconds)
            | Self::FilterDecaySeconds(seconds)
            | Self::FilterReleaseSeconds(seconds) => {
                Some((seconds, S::MIN_ENVELOPE_SECONDS, S::MAX_ENVELOPE_SECONDS))
            }
            Self::Sustain(level) | Self::FilterSustain(level) => {
                Some((level, S::MIN_SUSTAIN, S::MAX_SUSTAIN))
            }
            Self::GlideSeconds(seconds) => {
                Some((seconds, S::MIN_GLIDE_SECONDS, S::MAX_GLIDE_SECONDS))
            }
        }
    }

    /// The setting described: its label, how it sounds, its unit, range,
    /// scale and default, or its choices. The value it carries doesn't
    /// matter.
    pub fn describe(&self) -> Description {
        use Scale::{Cubic, Linear, Log};
        let defaults = SynthSettings::default();
        let default = defaults
            .params()
            .into_iter()
            .find(|param| param.same_setting(self))
            .expect("every setting has a default");
        let (value, min, max) = default.range().unwrap_or_default();
        let number = |label, sounds, unit, scale| {
            Description::number(label, sounds, unit, scale, (min, max), value)
        };
        let octave =
            |label, sounds| Description::whole(label, sounds, Unit::Octaves, (min, max), value);
        let level = |label, sounds| number(label, sounds, Unit::Fraction, Linear);
        match self {
            Self::Osc1Wave(_) => Description::choice(
                "Osc 1 wave",
                "Oscillator 1's shape, where the sound starts before the filter \
                 takes anything away. Sine is pure and round, triangle soft and \
                 hollow, saw bright and buzzy, the richest to filter, and pulse \
                 hollow and reedy.",
                &WAVEFORMS,
                defaults.osc1_wave.value(),
            ),
            Self::Osc1Octave(_) => octave(
                "Osc 1 octave",
                "Moves oscillator 1, and the sub under it, by whole octaves from \
                 the note. Down for a bass from the same notes; up for a lead \
                 that cuts through.",
            ),
            Self::Osc1Width(_) => number(
                "Osc 1 width",
                "How much of each cycle oscillator 1's pulse spends high. Half \
                 is a square, hollow and woody; narrower thins it towards a \
                 nasal, reedy buzz. Moving it slowly gives a pad the gentle, \
                 chorus-like movement of a string machine. Only the pulse has \
                 a width.",
                Unit::Fraction,
                Linear,
            ),
            Self::Osc2Wave(_) => Description::choice(
                "Osc 2 wave",
                "Oscillator 2's shape. A second saw a few cents from the first \
                 thickens it; a different wave, such as a pulse under a saw, \
                 adds a second colour.",
                &WAVEFORMS,
                defaults.osc2_wave.value(),
            ),
            Self::Osc2Octave(_) => octave(
                "Osc 2 octave",
                "Moves oscillator 2 by whole octaves from the note. An octave up \
                 adds brightness and shimmer over oscillator 1; an octave down \
                 adds weight under it.",
            ),
            Self::Osc2PitchSemitones(_) => number(
                "Osc 2 pitch",
                "Moves oscillator 2 by up to an octave either way, finely near \
                 zero. A few cents off and the two oscillators beat slowly \
                 against each other, the classic thick, detuned sound; seven \
                 semitones is a fifth, a big, organ-like interval.",
                Unit::Semitones,
                Cubic,
            ),
            Self::Osc2Sync(_) => Description::toggle(
                "Osc 2 sync",
                "Restarts oscillator 2 each time oscillator 1 starts a cycle, \
                 so it always sounds at oscillator 1's pitch. Its own Pitch \
                 then changes its tone instead of its note: higher is brighter \
                 and more nasal, and sweeping it gives the tearing, vocal sound \
                 of a classic sync lead.",
                defaults.osc2_sync,
            ),
            Self::SubOctave(_) => octave(
                "Sub octave",
                "How far under oscillator 1 the sub's square wave plays: one \
                 octave for weight under a bass or a lead, two for a deep rumble \
                 you feel more than hear.",
            ),
            Self::Osc1Level(_) => level(
                "Osc 1 level",
                "How much of oscillator 1 goes to the filter. The four levels \
                 are balanced together: once they add up to more than one \
                 oscillator, the mix is turned down to match, so bringing \
                 more in keeps it about as loud.",
            ),
            Self::Osc2Level(_) => level(
                "Osc 2 level",
                "How much of oscillator 2 goes to the filter. A little under \
                 oscillator 1 thickens it; level with it, detuned, gives a \
                 full, chorused sound.",
            ),
            Self::SubLevel(_) => level(
                "Sub level",
                "How much of the sub goes to the filter. A little adds weight \
                 without changing the tone; a lot makes a bass round and heavy.",
            ),
            Self::NoiseLevel(_) => level(
                "Noise level",
                "How much white noise goes to the filter: breath on a pad, a \
                 click on a pluck, or the hiss of a snare-like sound. A little \
                 goes a long way.",
            ),
            Self::FilterMode(_) => Description::choice(
                "Filter mode",
                "Which part of the sound the filter keeps. Low-pass keeps what's \
                 under the cutoff, warm and round, for basses and pads; band-pass \
                 keeps a band around it, thin and nasal, like a voice down a \
                 telephone; high-pass keeps what's over it, airy and light, \
                 taking the weight out from underneath.",
                &FILTER_MODES,
                defaults.filter_mode.value(),
            ),
            Self::CutoffHz(_) => number(
                "Cutoff",
                "Where the filter cuts. In low-pass it's how bright the sound is: \
                 low is dark and muffled, like a bass through a wall; high lets \
                 the whole wave through.",
                Unit::Hz,
                Log,
            ),
            Self::Resonance(_) => number(
                "Resonance",
                "How much the filter rings at the cutoff. A little adds bite and a \
                 vocal edge; a lot whistles at the cutoff's pitch and thins out \
                 the lows.",
                Unit::Amount,
                Linear,
            ),
            Self::FilterEnvOctaves(_) => number(
                "Env amount",
                "How far the filter envelope opens the filter on each note, in \
                 octaves above the cutoff. Up, with a short decay and no sustain, \
                 each note starts bright and closes down: a pluck. Below zero it \
                 closes the filter instead, so each note swells from dark.",
                Unit::Octaves,
                Linear,
            ),
            Self::FilterKey(_) => number(
                "Key",
                "How closely the cutoff follows the note. At zero every note has \
                 the same cutoff, so high notes sound duller than low ones; at \
                 full the cutoff rises an octave with each octave, and every \
                 note is as bright as the next.",
                Unit::Fraction,
                Linear,
            ),
            Self::FilterAttackSeconds(_) => number(
                "Filter attack",
                "How quickly the filter opens to its peak at the start of each \
                 note. Short is a sharp, bright edge; long is a slow brightening, \
                 like a pad coming into focus.",
                Unit::Seconds,
                Log,
            ),
            Self::FilterDecaySeconds(_) => number(
                "Filter decay",
                "How quickly the filter closes from its peak to the filter's \
                 Sustain. Short is a pluck or a tight bass; long is a slow \
                 darkening through each held note.",
                Unit::Seconds,
                Log,
            ),
            Self::FilterSustain(_) => number(
                "Filter sustain",
                "How open the filter stays while a note is held, as a share of \
                 the Env amount. None closes it back to the cutoff, as a pluck \
                 does; full keeps every held note bright.",
                Unit::Fraction,
                Linear,
            ),
            Self::FilterReleaseSeconds(_) => number(
                "Filter release",
                "How quickly the filter closes after the key is let go. Short \
                 darkens the tail at once; long lets it close slowly as it \
                 fades.",
                Unit::Seconds,
                Log,
            ),
            Self::AttackSeconds(_) => number(
                "Attack",
                "How quickly each note fades in. Short is a pluck or a stab; long \
                 is a swell, like bowed strings or a pad.",
                Unit::Seconds,
                Log,
            ),
            Self::DecaySeconds(_) => number(
                "Decay",
                "How quickly a note falls from its peak to the Sustain level. \
                 Short with a low Sustain is a pluck; long gives each note a slow \
                 fade while it's held.",
                Unit::Seconds,
                Log,
            ),
            Self::Sustain(_) => number(
                "Sustain",
                "How loud a held note stays once it has decayed. Full holds like \
                 an organ; none dies away even while the key is down, like a \
                 plucked string.",
                Unit::Fraction,
                Linear,
            ),
            Self::ReleaseSeconds(_) => number(
                "Release",
                "How long a note rings on after the key is let go. Short is tight \
                 and staccato; long leaves a tail that runs into the next notes.",
                Unit::Seconds,
                Log,
            ),
            Self::VoiceMode(_) => Description::choice(
                "Voice mode",
                "How many notes play at once. Poly plays chords and lets \
                 overlapping notes ring together; Mono plays one at a time, \
                 the latest winning, and goes back to a note still held when \
                 you let go: the way a classic bass or lead synth plays a line.",
                &VOICE_MODES,
                defaults.voice_mode.value(),
            ),
            Self::Legato(_) => Description::toggle(
                "Legato",
                "In Mono, whether a note that overlaps the last carries on \
                 from it. On, only the pitch changes, so a line of overlapping \
                 notes sounds as one smooth, connected phrase; off, each note \
                 attacks again, for a line where every note speaks. A note \
                 after a gap always attacks afresh.",
                defaults.legato,
            ),
            Self::GlideSeconds(_) => number(
                "Glide",
                "In Mono, how long the pitch takes to slide from one note to \
                 the next when they overlap. A touch is the slur of a fretless \
                 bass; long is a slow, swooping lead. At its shortest it's \
                 off, and a note after a gap never slides.",
                Unit::Seconds,
                Log,
            ),
            Self::OscRetrigger(_) => Description::toggle(
                "Osc retrigger",
                "Whether the oscillators start their wave afresh on every note. \
                 On, every note starts the same way, tight and punchy; off, they \
                 keep running between notes, as on an analog synth, so each note \
                 catches the wave somewhere different and sounds a little more \
                 alive.",
                defaults.osc_retrigger,
            ),
        }
    }

    /// Whether the value is within the setting's range. NaN never is.
    fn in_range(&self) -> bool {
        self.range()
            .is_none_or(|(value, min, max)| (min..=max).contains(&value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::Kind;

    #[test]
    fn defaults_match_the_plan() {
        let settings = SynthSettings::default();
        assert_eq!(settings.osc1_wave, Waveform::Saw);
        assert_eq!(settings.osc1_octave, 0);
        assert_eq!(settings.osc1_width, 0.5);
        assert_eq!(settings.osc2_wave, Waveform::Saw);
        assert_eq!(settings.osc2_octave, 0);
        assert_eq!(settings.osc2_pitch_semitones, 0.0);
        assert!(!settings.osc2_sync);
        assert_eq!(settings.sub_octave, -1);
        assert_eq!(
            [
                settings.osc1_level,
                settings.osc2_level,
                settings.sub_level,
                settings.noise_level
            ],
            [1.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(settings.filter_mode, FilterMode::LowPass);
        assert_eq!(settings.cutoff_hz, 20_000.0);
        assert_eq!(settings.resonance, 0.0);
        assert_eq!(
            (settings.filter_env_octaves, settings.filter_key),
            (0.0, 0.0)
        );
        assert_eq!(
            [
                settings.filter_attack_seconds,
                settings.filter_decay_seconds,
                settings.filter_sustain,
                settings.filter_release_seconds
            ],
            [0.001, 0.3, 0.0, 0.2]
        );
        assert_eq!(settings.attack_seconds, 0.005);
        assert_eq!(settings.decay_seconds, 0.2);
        assert_eq!(settings.sustain, 0.7);
        assert_eq!(settings.release_seconds, 0.2);
        assert_eq!(settings.voice_mode, VoiceMode::Poly);
        assert!(!settings.legato);
        assert_eq!(settings.glide_seconds, 0.001);
        assert!(settings.osc_retrigger);
    }

    #[test]
    fn set_returns_the_old_value() {
        let mut settings = SynthSettings::default();
        assert_eq!(
            settings.set(SynthParam::CutoffHz(440.0)),
            Ok(SynthParam::CutoffHz(20_000.0))
        );
        assert_eq!(settings.cutoff_hz, 440.0);
        assert_eq!(
            settings.set(SynthParam::Osc1Wave(Waveform::Pulse)),
            Ok(SynthParam::Osc1Wave(Waveform::Saw))
        );
        assert_eq!(settings.osc1_wave, Waveform::Pulse);
        assert_eq!(
            settings.set(SynthParam::SubOctave(-2)),
            Ok(SynthParam::SubOctave(-1))
        );
        assert_eq!(
            settings.set(SynthParam::OscRetrigger(false)),
            Ok(SynthParam::OscRetrigger(true))
        );
        assert_eq!(
            settings.set(SynthParam::Osc2PitchSemitones(0.07)),
            Ok(SynthParam::Osc2PitchSemitones(0.0))
        );
        assert_eq!(
            settings.set(SynthParam::Osc1Width(0.25)),
            Ok(SynthParam::Osc1Width(0.5))
        );
        assert_eq!(
            settings.set(SynthParam::Osc2Sync(true)),
            Ok(SynthParam::Osc2Sync(false))
        );
        assert_eq!((settings.osc1_width, settings.osc2_sync), (0.25, true));
        assert_eq!(
            (
                settings.sub_octave,
                settings.osc_retrigger,
                settings.osc2_pitch_semitones
            ),
            (-2, false, 0.07)
        );
    }

    /// Every setting sets its own field and nothing else.
    #[test]
    fn each_setting_sets_only_itself() {
        let changed = SynthSettings {
            osc1_wave: Waveform::Sine,
            osc1_octave: -2,
            osc1_width: 0.05,
            osc2_wave: Waveform::Triangle,
            osc2_octave: 2,
            osc2_pitch_semitones: -12.0,
            osc2_sync: true,
            sub_octave: -2,
            osc1_level: 0.5,
            osc2_level: 0.25,
            sub_level: 0.75,
            noise_level: 1.0,
            filter_mode: FilterMode::HighPass,
            cutoff_hz: 100.0,
            resonance: 1.0,
            filter_env_octaves: -10.0,
            filter_key: 0.5,
            filter_attack_seconds: 2.0,
            filter_decay_seconds: 0.01,
            filter_sustain: 1.0,
            filter_release_seconds: 4.0,
            attack_seconds: 10.0,
            decay_seconds: 0.001,
            sustain: 0.0,
            release_seconds: 3.0,
            voice_mode: VoiceMode::Mono,
            legato: true,
            glide_seconds: 0.5,
            osc_retrigger: false,
        };
        let defaults = SynthSettings::default();
        for (old, new) in defaults.params().into_iter().zip(changed.params()) {
            assert_ne!(old, new, "the test changes every setting");
            let mut settings = defaults;
            assert_eq!(settings.set(new), Ok(old));
            let differ: Vec<_> = settings
                .params()
                .into_iter()
                .zip(defaults.params())
                .filter(|(a, b)| a != b)
                .map(|(a, _)| a)
                .collect();
            assert_eq!(differ, [new]);
        }
    }

    #[test]
    fn limits_are_inclusive() {
        let mut settings = SynthSettings::default();
        for param in [
            SynthParam::Osc1Octave(-2),
            SynthParam::Osc1Octave(2),
            SynthParam::Osc1Width(0.05),
            SynthParam::Osc1Width(0.95),
            SynthParam::Osc2Octave(-2),
            SynthParam::Osc2Octave(2),
            SynthParam::Osc2PitchSemitones(-12.0),
            SynthParam::Osc2PitchSemitones(12.0),
            SynthParam::SubOctave(-2),
            SynthParam::SubOctave(-1),
            SynthParam::Osc1Level(0.0),
            SynthParam::Osc2Level(1.0),
            SynthParam::SubLevel(1.0),
            SynthParam::NoiseLevel(0.0),
            SynthParam::CutoffHz(20.0),
            SynthParam::CutoffHz(20_000.0),
            SynthParam::Resonance(0.0),
            SynthParam::Resonance(1.0),
            SynthParam::FilterEnvOctaves(-10.0),
            SynthParam::FilterEnvOctaves(10.0),
            SynthParam::FilterKey(0.0),
            SynthParam::FilterKey(1.0),
            SynthParam::FilterAttackSeconds(0.001),
            SynthParam::FilterDecaySeconds(10.0),
            SynthParam::FilterSustain(0.0),
            SynthParam::FilterSustain(1.0),
            SynthParam::FilterReleaseSeconds(10.0),
            SynthParam::AttackSeconds(0.001),
            SynthParam::DecaySeconds(10.0),
            SynthParam::Sustain(0.0),
            SynthParam::Sustain(1.0),
            SynthParam::ReleaseSeconds(0.001),
            SynthParam::GlideSeconds(0.001),
            SynthParam::GlideSeconds(5.0),
        ] {
            assert!(settings.set(param).is_ok(), "{param:?} was rejected");
        }
    }

    #[test]
    fn every_setting_is_described_with_its_range_and_default() {
        let settings = SynthSettings::default();
        for param in settings.params() {
            let description = param.describe();
            assert!(!description.label.is_empty(), "{param:?} has no label");
            assert!(
                description.sounds.ends_with('.'),
                "{param:?} doesn't say how it sounds"
            );
            match (description.kind, param.range()) {
                (
                    Kind::Number {
                        limits, default, ..
                    },
                    Some((value, min, max)),
                ) => {
                    assert_eq!(limits, (min, max), "{param:?}");
                    assert_eq!(default, value, "{param:?}");
                }
                (Kind::Choice { default, .. }, None) => {
                    let value = match param {
                        SynthParam::Osc1Wave(wave) | SynthParam::Osc2Wave(wave) => wave.value(),
                        SynthParam::FilterMode(mode) => mode.value(),
                        SynthParam::VoiceMode(mode) => mode.value(),
                        _ => panic!("{param:?} is a choice"),
                    };
                    assert_eq!(default, value);
                }
                (Kind::Toggle { default }, None) => {
                    let on = match param {
                        SynthParam::Osc2Sync(on)
                        | SynthParam::Legato(on)
                        | SynthParam::OscRetrigger(on) => on,
                        _ => panic!("{param:?} is a toggle"),
                    };
                    assert_eq!(default, on, "{param:?}");
                }
                (kind, range) => panic!("{param:?} is described as {kind:?} with {range:?}"),
            }
        }
        let labels: Vec<_> = settings
            .params()
            .iter()
            .map(|p| p.describe().label)
            .collect();
        let mut unique = labels.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), labels.len(), "two settings share a label");

        let units: Vec<_> = settings
            .params()
            .iter()
            .filter_map(|param| match param.describe().kind {
                Kind::Number {
                    unit, scale, step, ..
                } => Some((unit, scale, step)),
                Kind::Choice { .. } | Kind::Toggle { .. } => None,
            })
            .collect();
        use crate::setting::{Scale::*, Unit::*};
        assert_eq!(
            units,
            [
                (Octaves, Linear, Some(1.0)),
                (Fraction, Linear, None),
                (Octaves, Linear, Some(1.0)),
                (Semitones, Cubic, None),
                (Octaves, Linear, Some(1.0)),
                (Fraction, Linear, None),
                (Fraction, Linear, None),
                (Fraction, Linear, None),
                (Fraction, Linear, None),
                (Hz, Log, None),
                (Amount, Linear, None),
                (Octaves, Linear, None),
                (Fraction, Linear, None),
                (Seconds, Log, None),
                (Seconds, Log, None),
                (Fraction, Linear, None),
                (Seconds, Log, None),
                (Seconds, Log, None),
                (Seconds, Log, None),
                (Fraction, Linear, None),
                (Seconds, Log, None),
                (Seconds, Log, None),
            ]
        );
    }

    #[test]
    fn the_waveforms_choices_are_its_values_in_order() {
        let Kind::Choice { options, .. } = SynthParam::Osc1Wave(Waveform::Saw).describe().kind
        else {
            panic!("the waveform is a choice");
        };
        let values: Vec<_> = options.iter().map(|choice| choice.value).collect();
        let serialised: Vec<_> = Waveform::ALL
            .iter()
            .map(|wave| serde_json::to_value(wave).unwrap())
            .collect();
        assert_eq!(
            serde_json::to_value(&values).unwrap(),
            serde_json::Value::Array(serialised)
        );
        let names: Vec<_> = Waveform::ALL.iter().map(|wave| wave.value()).collect();
        assert_eq!(values, names);
    }

    #[test]
    fn the_filter_modes_choices_are_its_values_in_order() {
        let Kind::Choice { options, .. } =
            SynthParam::FilterMode(FilterMode::LowPass).describe().kind
        else {
            panic!("the filter mode is a choice");
        };
        let values: Vec<_> = options.iter().map(|choice| choice.value).collect();
        let serialised: Vec<_> = FilterMode::ALL
            .iter()
            .map(|mode| serde_json::to_value(mode).unwrap())
            .collect();
        assert_eq!(
            serde_json::to_value(&values).unwrap(),
            serde_json::Value::Array(serialised)
        );
        let names: Vec<_> = FilterMode::ALL.iter().map(|mode| mode.value()).collect();
        assert_eq!(values, names);
    }

    #[test]
    fn the_voice_modes_choices_are_its_values_in_order() {
        let Kind::Choice { options, .. } = SynthParam::VoiceMode(VoiceMode::Poly).describe().kind
        else {
            panic!("the voice mode is a choice");
        };
        let values: Vec<_> = options.iter().map(|choice| choice.value).collect();
        let serialised: Vec<_> = VoiceMode::ALL
            .iter()
            .map(|mode| serde_json::to_value(mode).unwrap())
            .collect();
        assert_eq!(
            serde_json::to_value(&values).unwrap(),
            serde_json::Value::Array(serialised)
        );
        let names: Vec<_> = VoiceMode::ALL.iter().map(|mode| mode.value()).collect();
        assert_eq!(values, names);
    }

    #[test]
    fn out_of_range_values_are_rejected_and_change_nothing() {
        let mut settings = SynthSettings::default();
        for param in [
            SynthParam::Osc1Octave(-3),
            SynthParam::Osc2Octave(3),
            SynthParam::Osc1Width(0.049),
            SynthParam::Osc1Width(0.951),
            SynthParam::Osc1Width(f32::NAN),
            SynthParam::Osc2PitchSemitones(12.01),
            SynthParam::Osc2PitchSemitones(f32::NAN),
            SynthParam::SubOctave(0),
            SynthParam::SubOctave(-3),
            SynthParam::Osc1Level(1.01),
            SynthParam::Osc2Level(-0.01),
            SynthParam::SubLevel(f32::INFINITY),
            SynthParam::NoiseLevel(2.0),
            SynthParam::CutoffHz(19.9),
            SynthParam::CutoffHz(20_001.0),
            SynthParam::CutoffHz(f32::NAN),
            SynthParam::Resonance(-0.01),
            SynthParam::Resonance(1.01),
            SynthParam::FilterEnvOctaves(10.01),
            SynthParam::FilterEnvOctaves(f32::NAN),
            SynthParam::FilterKey(-0.01),
            SynthParam::FilterKey(1.01),
            SynthParam::FilterAttackSeconds(0.0),
            SynthParam::FilterDecaySeconds(11.0),
            SynthParam::FilterSustain(1.5),
            SynthParam::FilterReleaseSeconds(f32::NAN),
            SynthParam::AttackSeconds(0.0),
            SynthParam::DecaySeconds(10.5),
            SynthParam::Sustain(f32::INFINITY),
            SynthParam::ReleaseSeconds(-1.0),
            SynthParam::GlideSeconds(0.0),
            SynthParam::GlideSeconds(5.01),
            SynthParam::GlideSeconds(f32::NAN),
        ] {
            assert!(settings.set(param).is_err(), "{param:?} was accepted");
            assert_eq!(settings, SynthSettings::default());
        }
    }

    /// Format 4's seven settings still read: `waveform` is oscillator 1's
    /// wave, `square` is a pulse, and the rest take their defaults.
    #[test]
    fn format_4_settings_read_with_the_rest_at_their_defaults() {
        let format_4: SynthSettings = serde_json::from_str(
            r#"{"waveform":"square","cutoff_hz":800.0,"resonance":0.5,"attack_seconds":0.01,
                "decay_seconds":0.3,"sustain":0.6,"release_seconds":0.4}"#,
        )
        .unwrap();
        assert_eq!(
            format_4,
            SynthSettings {
                osc1_wave: Waveform::Pulse,
                cutoff_hz: 800.0,
                resonance: 0.5,
                attack_seconds: 0.01,
                decay_seconds: 0.3,
                sustain: 0.6,
                release_seconds: 0.4,
                ..SynthSettings::default()
            }
        );
        let param: SynthParam =
            serde_json::from_str(r#"{"name":"waveform","value":"square"}"#).unwrap();
        assert_eq!(param, SynthParam::Osc1Wave(Waveform::Pulse));
        // Written in today's names.
        assert_eq!(
            serde_json::to_value(param).unwrap(),
            serde_json::json!({"name": "osc1_wave", "value": "pulse"})
        );
        let json = serde_json::to_value(SynthSettings::default()).unwrap();
        assert_eq!(json["osc1_wave"], "saw");
        assert_eq!(json["sub_octave"], -1);
        assert_eq!(json["osc_retrigger"], true);
        assert_eq!(json["osc1_width"], 0.5);
        assert_eq!(json["osc2_sync"], false);
        assert_eq!(json["voice_mode"], "poly");
        assert_eq!(json["legato"], false);
        assert!(serde_json::from_str::<SynthSettings>(r#"{"wave":"saw"}"#).is_err());
    }
}
