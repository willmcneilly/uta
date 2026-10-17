//! The built-in synth's settings. See RFC-002, "The shared model", point 6,
//! and RFC-008, "The instrument".
//!
//! The engine keeps its own copy of these, with the same names, units, ranges
//! and defaults (the "Synth settings" table in `docs/plans/make-a-loop.md`,
//! and RFC-008's), so the snapshot only has to copy values across.

use serde::{Deserialize, Serialize};

use crate::setting::{Choice, Description, Scale, Unit};
use crate::time::NoteDivision;

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

/// The LFO's wave shape. Each runs from -1 to 1 over a cycle, starting
/// where a cycle starts: on the bar, when synced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LfoShape {
    /// Starts at zero, rising.
    #[default]
    Sine,
    /// Starts at zero, rising.
    Triangle,
    /// Rises from -1 to 1, then falls back at once.
    Saw,
    /// High for the first half of the cycle, low for the second.
    Square,
    /// A new random level at the start of each cycle, held until the next.
    SampleAndHold,
}

impl LfoShape {
    /// Every shape, in the panel's order.
    pub const ALL: [LfoShape; 5] = [
        Self::Sine,
        Self::Triangle,
        Self::Saw,
        Self::Square,
        Self::SampleAndHold,
    ];

    /// The shape as `SetSynthParam` carries it: `"sample_and_hold"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::Sine => "sine",
            Self::Triangle => "triangle",
            Self::Saw => "saw",
            Self::Square => "square",
            Self::SampleAndHold => "sample_and_hold",
        }
    }
}

/// Every LFO shape, as the panel offers them.
const LFO_SHAPES: [Choice; 5] = [
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
        value: "square",
        label: "Square",
    },
    Choice {
        value: "sample_and_hold",
        label: "Sample & hold",
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

/// What drives a mod slot: each runs from 0 to 1, or -1 to 1 for the LFO
/// and the key, and the slot's Amount scales it. See RFC-008, "The
/// instrument" (Mod).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModSource {
    /// The filter envelope's level, from 0 to 1, whatever its Env amount.
    FilterEnv,
    /// The amp envelope's level, from 0 to 1.
    AmpEnv,
    /// The LFO, from -1 to 1, whatever its own amounts.
    Lfo,
    /// How hard the note was played, from 0 (softest) to 1 (hardest).
    Velocity,
    /// How far the note is from middle C: -1 five octaves under it, 1 five
    /// octaves over it.
    Key,
}

impl ModSource {
    /// Every source, in the panel's order.
    pub const ALL: [ModSource; 5] = [
        Self::FilterEnv,
        Self::AmpEnv,
        Self::Lfo,
        Self::Velocity,
        Self::Key,
    ];

    /// The source as `SetSynthParam` carries it: `"velocity"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::FilterEnv => "filter_env",
            Self::AmpEnv => "amp_env",
            Self::Lfo => "lfo",
            Self::Velocity => "velocity",
            Self::Key => "key",
        }
    }
}

/// Every mod source, as the panel offers them.
const MOD_SOURCES: [Choice; 5] = [
    Choice {
        value: "filter_env",
        label: "Filter env",
    },
    Choice {
        value: "amp_env",
        label: "Amp env",
    },
    Choice {
        value: "lfo",
        label: "LFO",
    },
    Choice {
        value: "velocity",
        label: "Velocity",
    },
    Choice {
        value: "key",
        label: "Key",
    },
];

/// What a mod slot moves, or nothing. At an Amount of 1 with its source at
/// 1, each moves by its own amount: see [`ModDestination::full_scale`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModDestination {
    /// An empty slot: it moves nothing.
    #[default]
    Off,
    Osc1Pitch,
    Osc2Pitch,
    Osc1Level,
    Osc2Level,
    SubLevel,
    NoiseLevel,
    /// Oscillator 1's pulse width.
    Osc1Width,
    Cutoff,
    Resonance,
    /// The LFO's rate, when it isn't synced.
    LfoRate,
    /// The voice's level.
    Volume,
}

impl ModDestination {
    /// Every destination, in the panel's order.
    pub const ALL: [ModDestination; 12] = [
        Self::Off,
        Self::Osc1Pitch,
        Self::Osc2Pitch,
        Self::Osc1Level,
        Self::Osc2Level,
        Self::SubLevel,
        Self::NoiseLevel,
        Self::Osc1Width,
        Self::Cutoff,
        Self::Resonance,
        Self::LfoRate,
        Self::Volume,
    ];

    /// The destination as `SetSynthParam` carries it: `"cutoff"`.
    pub fn value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Osc1Pitch => "osc1_pitch",
            Self::Osc2Pitch => "osc2_pitch",
            Self::Osc1Level => "osc1_level",
            Self::Osc2Level => "osc2_level",
            Self::SubLevel => "sub_level",
            Self::NoiseLevel => "noise_level",
            Self::Osc1Width => "osc1_width",
            Self::Cutoff => "cutoff",
            Self::Resonance => "resonance",
            Self::LfoRate => "lfo_rate",
            Self::Volume => "volume",
        }
    }

    /// How far it moves at an Amount of 1 with the source at 1, in its own
    /// unit: semitones for a pitch, octaves for the cutoff and the LFO's
    /// rate, and the setting's own scale for the rest. Volume's is a gain:
    /// 1 doubles it (+6 dB), -1 silences it. Agreed with Will for UTA-79.
    pub const fn full_scale(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Osc1Pitch | Self::Osc2Pitch => 24.0,
            Self::Osc1Level
            | Self::Osc2Level
            | Self::SubLevel
            | Self::NoiseLevel
            | Self::Resonance
            | Self::Volume => 1.0,
            Self::Osc1Width => SynthSettings::MAX_WIDTH - SynthSettings::MIN_WIDTH,
            Self::Cutoff => SynthSettings::MAX_FILTER_ENV_OCTAVES,
            Self::LfoRate => 4.0,
        }
    }
}

/// Every mod destination, as the panel offers them.
const MOD_DESTINATIONS: [Choice; 12] = [
    Choice {
        value: "off",
        label: "Off",
    },
    Choice {
        value: "osc1_pitch",
        label: "Osc 1 pitch",
    },
    Choice {
        value: "osc2_pitch",
        label: "Osc 2 pitch",
    },
    Choice {
        value: "osc1_level",
        label: "Osc 1 level",
    },
    Choice {
        value: "osc2_level",
        label: "Osc 2 level",
    },
    Choice {
        value: "sub_level",
        label: "Sub level",
    },
    Choice {
        value: "noise_level",
        label: "Noise level",
    },
    Choice {
        value: "osc1_width",
        label: "Osc 1 width",
    },
    Choice {
        value: "cutoff",
        label: "Cutoff",
    },
    Choice {
        value: "resonance",
        label: "Resonance",
    },
    Choice {
        value: "lfo_rate",
        label: "LFO rate",
    },
    Choice {
        value: "volume",
        label: "Volume",
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
    /// How far the LFO moves the cutoff at its peak, in octaves: -5 to 5.
    /// Below zero it moves it the other way.
    pub filter_lfo_octaves: f32,
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
    pub lfo_shape: LfoShape,
    /// The LFO's rate when it isn't synced, in Hz: 0.05 to 20. Controls
    /// move it on a log scale.
    pub lfo_rate_hz: f32,
    /// Whether the LFO's rate follows the tempo, as [`Self::lfo_division`],
    /// with its cycles lined up with the bars.
    pub lfo_sync: bool,
    /// The length of one of the LFO's cycles, when synced.
    pub lfo_division: NoteDivision,
    /// Whether each note starts the LFO's cycle afresh. Off, every voice
    /// shares one.
    pub lfo_retrigger: bool,
    /// How far the LFO moves both oscillators' pitch at its peak, in
    /// semitones: 0 to 2.
    pub lfo_vibrato_semitones: f32,
    /// How far the LFO moves oscillator 1's pulse width at its peak, from
    /// 0 to 0.45, either side of its Width (and never past its limits).
    pub lfo_width: f32,
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
    /// The three mod slots: each moves its destination by its source times
    /// its amount, from -1 to 1, on top of the knobs and the fixed
    /// routings. A slot whose destination is Off moves nothing.
    pub mod1_source: ModSource,
    pub mod1_destination: ModDestination,
    pub mod1_amount: f32,
    pub mod2_source: ModSource,
    pub mod2_destination: ModDestination,
    pub mod2_amount: f32,
    pub mod3_source: ModSource,
    pub mod3_destination: ModDestination,
    pub mod3_amount: f32,
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
    /// The LFO's amount on the cutoff either side of zero, in octaves:
    /// enough for a wah from a few hundred Hz to a few kHz.
    pub const MAX_FILTER_LFO_OCTAVES: f32 = 5.0;
    /// The shortest attack, decay or release: 1 ms.
    pub const MIN_ENVELOPE_SECONDS: f32 = 0.001;
    /// The longest attack, decay or release.
    pub const MAX_ENVELOPE_SECONDS: f32 = 10.0;
    pub const MIN_SUSTAIN: f32 = 0.0;
    pub const MAX_SUSTAIN: f32 = 1.0;
    /// The slowest LFO, a cycle every 20 seconds, for a pad that drifts, and
    /// the fastest, where it starts to buzz rather than move.
    pub const MIN_LFO_RATE_HZ: f32 = 0.05;
    pub const MAX_LFO_RATE_HZ: f32 = 20.0;
    /// The deepest vibrato: a tone either side, as wide as a singer's gets.
    /// Deeper pitch movement is a mod slot's job.
    pub const MAX_LFO_VIBRATO_SEMITONES: f32 = 2.0;
    /// The furthest the LFO moves the pulse width: from a square (0.5) to
    /// either limit.
    pub const MAX_LFO_WIDTH: f32 = 0.45;
    /// The shortest Glide, which is off, and the longest.
    pub const MIN_GLIDE_SECONDS: f32 = 0.001;
    pub const MAX_GLIDE_SECONDS: f32 = 5.0;
    /// A mod slot's Amount either side of zero: its destination's whole
    /// [`ModDestination::full_scale`].
    pub const MAX_MOD_AMOUNT: f32 = 1.0;

    /// Every setting, as `SetSynthParam` carries them, in the order the
    /// sound flows through them.
    pub fn params(&self) -> [SynthParam; 46] {
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
            SynthParam::FilterLfoOctaves(self.filter_lfo_octaves),
            SynthParam::FilterAttackSeconds(self.filter_attack_seconds),
            SynthParam::FilterDecaySeconds(self.filter_decay_seconds),
            SynthParam::FilterSustain(self.filter_sustain),
            SynthParam::FilterReleaseSeconds(self.filter_release_seconds),
            SynthParam::AttackSeconds(self.attack_seconds),
            SynthParam::DecaySeconds(self.decay_seconds),
            SynthParam::Sustain(self.sustain),
            SynthParam::ReleaseSeconds(self.release_seconds),
            SynthParam::LfoShape(self.lfo_shape),
            SynthParam::LfoRateHz(self.lfo_rate_hz),
            SynthParam::LfoSync(self.lfo_sync),
            SynthParam::LfoDivision(self.lfo_division),
            SynthParam::LfoRetrigger(self.lfo_retrigger),
            SynthParam::LfoVibratoSemitones(self.lfo_vibrato_semitones),
            SynthParam::LfoWidth(self.lfo_width),
            SynthParam::VoiceMode(self.voice_mode),
            SynthParam::Legato(self.legato),
            SynthParam::GlideSeconds(self.glide_seconds),
            SynthParam::OscRetrigger(self.osc_retrigger),
            SynthParam::Mod1Source(self.mod1_source),
            SynthParam::Mod1Destination(self.mod1_destination),
            SynthParam::Mod1Amount(self.mod1_amount),
            SynthParam::Mod2Source(self.mod2_source),
            SynthParam::Mod2Destination(self.mod2_destination),
            SynthParam::Mod2Amount(self.mod2_amount),
            SynthParam::Mod3Source(self.mod3_source),
            SynthParam::Mod3Destination(self.mod3_destination),
            SynthParam::Mod3Amount(self.mod3_amount),
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
            SynthParam::FilterLfoOctaves(value) => {
                SynthParam::FilterLfoOctaves(replace(&mut self.filter_lfo_octaves, value))
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
            SynthParam::LfoShape(shape) => {
                SynthParam::LfoShape(replace(&mut self.lfo_shape, shape))
            }
            SynthParam::LfoRateHz(hz) => SynthParam::LfoRateHz(replace(&mut self.lfo_rate_hz, hz)),
            SynthParam::LfoSync(on) => SynthParam::LfoSync(replace(&mut self.lfo_sync, on)),
            SynthParam::LfoDivision(division) => {
                SynthParam::LfoDivision(replace(&mut self.lfo_division, division))
            }
            SynthParam::LfoRetrigger(on) => {
                SynthParam::LfoRetrigger(replace(&mut self.lfo_retrigger, on))
            }
            SynthParam::LfoVibratoSemitones(value) => {
                SynthParam::LfoVibratoSemitones(replace(&mut self.lfo_vibrato_semitones, value))
            }
            SynthParam::LfoWidth(value) => {
                SynthParam::LfoWidth(replace(&mut self.lfo_width, value))
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
            SynthParam::Mod1Source(source) => {
                SynthParam::Mod1Source(replace(&mut self.mod1_source, source))
            }
            SynthParam::Mod1Destination(to) => {
                SynthParam::Mod1Destination(replace(&mut self.mod1_destination, to))
            }
            SynthParam::Mod1Amount(value) => {
                SynthParam::Mod1Amount(replace(&mut self.mod1_amount, value))
            }
            SynthParam::Mod2Source(source) => {
                SynthParam::Mod2Source(replace(&mut self.mod2_source, source))
            }
            SynthParam::Mod2Destination(to) => {
                SynthParam::Mod2Destination(replace(&mut self.mod2_destination, to))
            }
            SynthParam::Mod2Amount(value) => {
                SynthParam::Mod2Amount(replace(&mut self.mod2_amount, value))
            }
            SynthParam::Mod3Source(source) => {
                SynthParam::Mod3Source(replace(&mut self.mod3_source, source))
            }
            SynthParam::Mod3Destination(to) => {
                SynthParam::Mod3Destination(replace(&mut self.mod3_destination, to))
            }
            SynthParam::Mod3Amount(value) => {
                SynthParam::Mod3Amount(replace(&mut self.mod3_amount, value))
            }
        })
    }
}

impl Default for SynthSettings {
    /// Today's sound, from before RFC-008: oscillator 1 a saw at the note's
    /// pitch and full level, everything else in the mixer at zero, oscillator
    /// 2 free of oscillator 1, a low-pass filter that nothing moves, and the
    /// oscillators restarting on each note. The filter envelope is set for a
    /// pluck, so turning up its amount alone makes one, and the LFO for a
    /// vibrato, so turning up Vibrato alone makes one; it moves nothing
    /// until an amount is turned up. The mod slots are empty, with the
    /// sources of RFC-008's three examples ready: velocity, the filter
    /// envelope and the LFO.
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
            filter_lfo_octaves: 0.0,
            filter_attack_seconds: 0.001,
            filter_decay_seconds: 0.3,
            filter_sustain: 0.0,
            filter_release_seconds: 0.2,
            attack_seconds: 0.005,
            decay_seconds: 0.2,
            sustain: 0.7,
            release_seconds: 0.2,
            lfo_shape: LfoShape::Sine,
            lfo_rate_hz: 5.0,
            lfo_sync: false,
            lfo_division: NoteDivision::QUARTER,
            lfo_retrigger: false,
            lfo_vibrato_semitones: 0.0,
            lfo_width: 0.0,
            voice_mode: VoiceMode::Poly,
            legato: false,
            glide_seconds: Self::MIN_GLIDE_SECONDS,
            osc_retrigger: true,
            mod1_source: ModSource::Velocity,
            mod1_destination: ModDestination::Off,
            mod1_amount: 0.0,
            mod2_source: ModSource::FilterEnv,
            mod2_destination: ModDestination::Off,
            mod2_amount: 0.0,
            mod3_source: ModSource::Lfo,
            mod3_destination: ModDestination::Off,
            mod3_amount: 0.0,
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
    FilterLfoOctaves(f32),
    FilterAttackSeconds(f32),
    FilterDecaySeconds(f32),
    FilterSustain(f32),
    FilterReleaseSeconds(f32),
    AttackSeconds(f32),
    DecaySeconds(f32),
    Sustain(f32),
    ReleaseSeconds(f32),
    LfoShape(LfoShape),
    LfoRateHz(f32),
    LfoSync(bool),
    LfoDivision(NoteDivision),
    LfoRetrigger(bool),
    LfoVibratoSemitones(f32),
    LfoWidth(f32),
    VoiceMode(VoiceMode),
    Legato(bool),
    GlideSeconds(f32),
    OscRetrigger(bool),
    Mod1Source(ModSource),
    Mod1Destination(ModDestination),
    Mod1Amount(f32),
    Mod2Source(ModSource),
    Mod2Destination(ModDestination),
    Mod2Amount(f32),
    Mod3Source(ModSource),
    Mod3Destination(ModDestination),
    Mod3Amount(f32),
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
            Self::FilterLfoOctaves(_)
            | Self::LfoShape(_)
            | Self::LfoRateHz(_)
            | Self::LfoSync(_)
            | Self::LfoDivision(_)
            | Self::LfoRetrigger(_)
            | Self::LfoVibratoSemitones(_)
            | Self::LfoWidth(_) => 11,
            Self::VoiceMode(_) | Self::Legato(_) | Self::GlideSeconds(_) => 12,
            Self::Mod1Source(_)
            | Self::Mod1Destination(_)
            | Self::Mod1Amount(_)
            | Self::Mod2Source(_)
            | Self::Mod2Destination(_)
            | Self::Mod2Amount(_)
            | Self::Mod3Source(_)
            | Self::Mod3Destination(_)
            | Self::Mod3Amount(_) => 13,
            _ => 6,
        }
    }

    /// The setting's value and its limits (inclusive), or `None` for a
    /// wave, a mode, a division or a switch, which have no range.
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
            | Self::LfoShape(_)
            | Self::LfoSync(_)
            | Self::LfoDivision(_)
            | Self::LfoRetrigger(_)
            | Self::VoiceMode(_)
            | Self::Legato(_)
            | Self::OscRetrigger(_)
            | Self::Mod1Source(_)
            | Self::Mod1Destination(_)
            | Self::Mod2Source(_)
            | Self::Mod2Destination(_)
            | Self::Mod3Source(_)
            | Self::Mod3Destination(_) => None,
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
            Self::FilterLfoOctaves(value) => {
                Some((value, -S::MAX_FILTER_LFO_OCTAVES, S::MAX_FILTER_LFO_OCTAVES))
            }
            Self::LfoRateHz(hz) => Some((hz, S::MIN_LFO_RATE_HZ, S::MAX_LFO_RATE_HZ)),
            Self::LfoVibratoSemitones(value) => Some((value, 0.0, S::MAX_LFO_VIBRATO_SEMITONES)),
            Self::LfoWidth(value) => Some((value, 0.0, S::MAX_LFO_WIDTH)),
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
            Self::Mod1Amount(value) | Self::Mod2Amount(value) | Self::Mod3Amount(value) => {
                Some((value, -S::MAX_MOD_AMOUNT, S::MAX_MOD_AMOUNT))
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
        let source = |label, default: ModSource| {
            Description::choice(
                label,
                "What moves this slot's destination: an envelope over each note, \
                 the LFO, how hard the note was played, or how high it is.",
                &MOD_SOURCES,
                default.value(),
            )
        };
        let destination = |label| {
            Description::choice(
                label,
                "What this slot moves, on top of its knob. Off, the slot does \
                 nothing.",
                &MOD_DESTINATIONS,
                ModDestination::default().value(),
            )
        };
        let amount = |label| {
            number(
                label,
                "How far the source moves the destination, and which way: full \
                 is a pitch two octaves, the cutoff ten octaves, or a level its \
                 whole range. Below zero it moves the other way.",
                Unit::Fraction,
                Linear,
            )
        };
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
                "How much of each cycle the pulse spends high: half is a hollow \
                 square, narrower a thin, reedy buzz. Only the pulse has a width.",
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
                "Moves oscillator 2 up to an octave either way, finely near zero. A \
                 few cents off beats against oscillator 1, thick and detuned; seven \
                 is a fifth.",
                Unit::Semitones,
                Cubic,
            ),
            Self::Osc2Sync(_) => Description::toggle(
                "Osc 2 sync",
                "Restarts oscillator 2 with each cycle of oscillator 1, so its \
                 Pitch changes its tone, not its note: sweep it for a tearing sync \
                 lead.",
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
                "Low-pass keeps what's under the cutoff, warm and round; band-pass \
                 a thin, nasal band around it; high-pass what's over it, airy and \
                 light.",
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
                 octaves. Up with a short decay is a pluck; below zero each note \
                 swells from dark.",
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
            Self::FilterLfoOctaves(_) => number(
                "Filter LFO",
                "How far the LFO moves the cutoff, in octaves. Slow and small, a pad \
                 breathes; a few Hz is a wah. Below zero it moves the other way.",
                Unit::Octaves,
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
            Self::LfoShape(_) => Description::choice(
                "LFO shape",
                "How the LFO moves. Sine and triangle sway; saw rises and drops; \
                 square flips; sample-and-hold jumps somewhere random each cycle.",
                &LFO_SHAPES,
                defaults.lfo_shape.value(),
            ),
            Self::LfoRateHz(_) => number(
                "LFO rate",
                "How fast the LFO moves. Under 1 Hz is a slow sweep; about 5 Hz a \
                 singer's vibrato; faster a flutter.",
                Unit::Hz,
                Log,
            ),
            Self::LfoSync(_) => Description::toggle(
                "LFO sync",
                "Whether the LFO's rate is a note length, with its cycles on the \
                 bars, rather than in Hz.",
                defaults.lfo_sync,
            ),
            Self::LfoDivision(_) => Description::choice(
                "LFO rate (synced)",
                "One cycle as a note length: a bar is a slow sweep, a quarter \
                 pulses with the beat.",
                &NoteDivision::CHOICES,
                defaults.lfo_division.value(),
            ),
            Self::LfoRetrigger(_) => Description::toggle(
                "LFO retrigger",
                "On, each note starts the LFO afresh, so every note moves alike; \
                 off, the notes share one, and a chord moves together.",
                defaults.lfo_retrigger,
            ),
            Self::LfoVibratoSemitones(_) => number(
                "Vibrato",
                "How far the LFO bends the pitch, in semitones. A tenth or two is a \
                 warm vibrato; more is a wobble.",
                Unit::Semitones,
                Linear,
            ),
            Self::LfoWidth(_) => number(
                "LFO width",
                "How far the LFO moves oscillator 1's pulse width: the shimmer of a \
                 string machine. Only the pulse has a width.",
                Unit::Fraction,
                Linear,
            ),
            Self::VoiceMode(_) => Description::choice(
                "Voice mode",
                "Poly plays chords; Mono one note at a time, the latest winning, as \
                 a bass or lead line plays.",
                &VOICE_MODES,
                defaults.voice_mode.value(),
            ),
            Self::Legato(_) => Description::toggle(
                "Legato",
                "In Mono, an overlapping note only changes the pitch, for one \
                 smooth phrase; off, each attacks again.",
                defaults.legato,
            ),
            Self::GlideSeconds(_) => number(
                "Glide",
                "In Mono, how long the pitch slides between overlapping notes: a \
                 slur, or a slow swoop. Shortest is off.",
                Unit::Seconds,
                Log,
            ),
            Self::OscRetrigger(_) => Description::toggle(
                "Osc retrigger",
                "On, the oscillators start afresh on every note, tight and punchy; \
                 off, they run on between notes, as on an analog synth, each note a \
                 little different.",
                defaults.osc_retrigger,
            ),
            Self::Mod1Source(_) => source("Mod 1 source", defaults.mod1_source),
            Self::Mod1Destination(_) => destination("Mod 1 destination"),
            Self::Mod1Amount(_) => amount("Mod 1 amount"),
            Self::Mod2Source(_) => source("Mod 2 source", defaults.mod2_source),
            Self::Mod2Destination(_) => destination("Mod 2 destination"),
            Self::Mod2Amount(_) => amount("Mod 2 amount"),
            Self::Mod3Source(_) => source("Mod 3 source", defaults.mod3_source),
            Self::Mod3Destination(_) => destination("Mod 3 destination"),
            Self::Mod3Amount(_) => amount("Mod 3 amount"),
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
        assert_eq!(settings.filter_lfo_octaves, 0.0);
        assert_eq!(settings.lfo_shape, LfoShape::Sine);
        assert_eq!(settings.lfo_rate_hz, 5.0);
        assert!(!settings.lfo_sync);
        assert_eq!(settings.lfo_division.value(), "1/4");
        assert!(!settings.lfo_retrigger);
        assert_eq!(
            (settings.lfo_vibrato_semitones, settings.lfo_width),
            (0.0, 0.0)
        );
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
            settings.set(SynthParam::LfoShape(LfoShape::SampleAndHold)),
            Ok(SynthParam::LfoShape(LfoShape::Sine))
        );
        assert_eq!(
            settings.set(SynthParam::LfoDivision(NoteDivision::DOTTED_EIGHTH)),
            Ok(SynthParam::LfoDivision(NoteDivision::QUARTER))
        );
        assert_eq!(
            settings.set(SynthParam::LfoVibratoSemitones(0.3)),
            Ok(SynthParam::LfoVibratoSemitones(0.0))
        );
        assert_eq!(
            (
                settings.lfo_shape,
                settings.lfo_division,
                settings.lfo_vibrato_semitones
            ),
            (LfoShape::SampleAndHold, NoteDivision::DOTTED_EIGHTH, 0.3)
        );
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
            filter_lfo_octaves: -5.0,
            filter_attack_seconds: 2.0,
            filter_decay_seconds: 0.01,
            filter_sustain: 1.0,
            filter_release_seconds: 4.0,
            attack_seconds: 10.0,
            decay_seconds: 0.001,
            sustain: 0.0,
            release_seconds: 3.0,
            lfo_shape: LfoShape::Square,
            lfo_rate_hz: 0.05,
            lfo_sync: true,
            lfo_division: NoteDivision::DOTTED_EIGHTH,
            lfo_retrigger: true,
            lfo_vibrato_semitones: 2.0,
            lfo_width: 0.45,
            voice_mode: VoiceMode::Mono,
            legato: true,
            glide_seconds: 0.5,
            osc_retrigger: false,
            mod1_source: ModSource::Key,
            mod1_destination: ModDestination::Cutoff,
            mod1_amount: 1.0,
            mod2_source: ModSource::AmpEnv,
            mod2_destination: ModDestination::Osc2Pitch,
            mod2_amount: -1.0,
            mod3_source: ModSource::Velocity,
            mod3_destination: ModDestination::Volume,
            mod3_amount: 0.5,
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
            SynthParam::FilterLfoOctaves(-5.0),
            SynthParam::FilterLfoOctaves(5.0),
            SynthParam::LfoRateHz(0.05),
            SynthParam::LfoRateHz(20.0),
            SynthParam::LfoVibratoSemitones(0.0),
            SynthParam::LfoVibratoSemitones(2.0),
            SynthParam::LfoWidth(0.0),
            SynthParam::LfoWidth(0.45),
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
                        SynthParam::LfoShape(shape) => shape.value(),
                        SynthParam::LfoDivision(division) => division.value(),
                        SynthParam::VoiceMode(mode) => mode.value(),
                        SynthParam::Mod1Source(source)
                        | SynthParam::Mod2Source(source)
                        | SynthParam::Mod3Source(source) => source.value(),
                        SynthParam::Mod1Destination(to)
                        | SynthParam::Mod2Destination(to)
                        | SynthParam::Mod3Destination(to) => to.value(),
                        _ => panic!("{param:?} is a choice"),
                    };
                    assert_eq!(default, value);
                }
                (Kind::Toggle { default }, None) => {
                    let on = match param {
                        SynthParam::Osc2Sync(on)
                        | SynthParam::LfoSync(on)
                        | SynthParam::LfoRetrigger(on)
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
                (Octaves, Linear, None),
                (Seconds, Log, None),
                (Seconds, Log, None),
                (Fraction, Linear, None),
                (Seconds, Log, None),
                (Seconds, Log, None),
                (Seconds, Log, None),
                (Fraction, Linear, None),
                (Seconds, Log, None),
                (Hz, Log, None),
                (Semitones, Linear, None),
                (Fraction, Linear, None),
                (Seconds, Log, None),
                (Fraction, Linear, None),
                (Fraction, Linear, None),
                (Fraction, Linear, None),
            ]
        );
    }

    /// The mod slots offer every source and destination, in order, as
    /// their commands carry them, and an empty slot is the default.
    #[test]
    fn the_mod_slots_choices_are_their_values_in_order() {
        let options = |param: SynthParam| match param.describe().kind {
            Kind::Choice { options, default } => {
                let values: Vec<_> = options.iter().map(|choice| choice.value).collect();
                (values, default)
            }
            kind => panic!("{param:?} is {kind:?}"),
        };
        let serialised = |values: Vec<serde_json::Value>| serde_json::Value::Array(values);
        let (sources, _) = options(SynthParam::Mod1Source(ModSource::Key));
        assert_eq!(
            serde_json::to_value(&sources).unwrap(),
            serialised(
                ModSource::ALL
                    .iter()
                    .map(|s| serde_json::to_value(s).unwrap())
                    .collect()
            )
        );
        let names: Vec<_> = ModSource::ALL.iter().map(|s| s.value()).collect();
        assert_eq!(sources, names);
        for param in [
            SynthParam::Mod1Destination(ModDestination::Off),
            SynthParam::Mod2Destination(ModDestination::Off),
            SynthParam::Mod3Destination(ModDestination::Off),
        ] {
            let (destinations, default) = options(param);
            assert_eq!(default, "off");
            assert_eq!(
                serde_json::to_value(&destinations).unwrap(),
                serialised(
                    ModDestination::ALL
                        .iter()
                        .map(|d| serde_json::to_value(d).unwrap())
                        .collect()
                )
            );
            let names: Vec<_> = ModDestination::ALL.iter().map(|d| d.value()).collect();
            assert_eq!(destinations, names);
        }
        let defaults = SynthSettings::default();
        assert_eq!(
            [
                defaults.mod1_destination,
                defaults.mod2_destination,
                defaults.mod3_destination
            ],
            [ModDestination::Off; 3]
        );
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
    fn the_lfo_shapes_choices_are_its_values_in_order() {
        let Kind::Choice { options, .. } = SynthParam::LfoShape(LfoShape::Sine).describe().kind
        else {
            panic!("the LFO shape is a choice");
        };
        let values: Vec<_> = options.iter().map(|choice| choice.value).collect();
        let serialised: Vec<_> = LfoShape::ALL
            .iter()
            .map(|shape| serde_json::to_value(shape).unwrap())
            .collect();
        assert_eq!(
            serde_json::to_value(&values).unwrap(),
            serde_json::Value::Array(serialised)
        );
        let names: Vec<_> = LfoShape::ALL.iter().map(|shape| shape.value()).collect();
        assert_eq!(values, names);
    }

    /// The synced rate's menu is every division, as the Delay's synced time
    /// has, and it carries one as the Delay does.
    #[test]
    fn the_lfos_synced_rate_is_a_note_division() {
        let Kind::Choice { options, default } = SynthParam::LfoDivision(NoteDivision::QUARTER)
            .describe()
            .kind
        else {
            panic!("the synced rate is a choice");
        };
        let all: Vec<_> = NoteDivision::ALL.iter().map(|d| d.value()).collect();
        let values: Vec<_> = options.iter().map(|choice| choice.value).collect();
        assert_eq!(values, all);
        assert_eq!(default, "1/4");
        let param = SynthParam::LfoDivision(NoteDivision::DOTTED_EIGHTH);
        let json = serde_json::to_value(param).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"name": "lfo_division", "value": "1/8d"})
        );
        assert_eq!(serde_json::from_value::<SynthParam>(json).unwrap(), param);
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
            SynthParam::FilterLfoOctaves(5.01),
            SynthParam::FilterLfoOctaves(f32::NAN),
            SynthParam::LfoRateHz(0.049),
            SynthParam::LfoRateHz(20.01),
            SynthParam::LfoVibratoSemitones(-0.01),
            SynthParam::LfoVibratoSemitones(2.01),
            SynthParam::LfoWidth(-0.01),
            SynthParam::LfoWidth(0.46),
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
        assert_eq!(json["lfo_shape"], "sine");
        assert_eq!(json["lfo_division"], "1/4");
        assert!(serde_json::from_str::<SynthSettings>(r#"{"wave":"saw"}"#).is_err());
    }
}
