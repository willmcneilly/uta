//! The synth's settings, as the engine plays them.
//!
//! The project core keeps its own copy of these for `SetSynthParam` to check.
//! Both follow the "Synth settings" table in `docs/plans/make-a-loop.md` and
//! RFC-008's "The instrument": the same names, units, ranges and defaults, so
//! the engine's snapshot can be filled from the project field by field.

use std::ops::RangeInclusive;

/// An oscillator's waveform.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
    #[default]
    Saw,
    /// A pulse: oscillator 1's has a Width, and oscillator 2's is a square.
    Pulse,
}

impl Waveform {
    /// Every wave, in order.
    pub const ALL: [Waveform; 4] = [Self::Sine, Self::Triangle, Self::Saw, Self::Pulse];
}

/// Which part of the sound the filter lets through.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FilterMode {
    #[default]
    LowPass,
    BandPass,
    HighPass,
}

/// The LFO's wave shape. See `uta_core::LfoShape`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
    SampleAndHold,
}

impl LfoShape {
    /// Every shape, in order.
    pub const ALL: [LfoShape; 5] = [
        Self::Sine,
        Self::Triangle,
        Self::Saw,
        Self::Square,
        Self::SampleAndHold,
    ];
}

/// How many notes the synth plays at once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VoiceMode {
    /// One voice for each note, up to [`crate::VOICES`].
    #[default]
    Poly,
    /// One note at a time, in one voice: the latest held note.
    Mono,
}

/// What drives a mod slot. See `uta_core::ModSource`: each runs from 0 to
/// 1, or -1 to 1 for the LFO and the key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModSource {
    FilterEnv,
    AmpEnv,
    Lfo,
    #[default]
    Velocity,
    /// -1 five octaves under middle C, 1 five octaves over it.
    Key,
}

impl ModSource {
    /// Every source, in order.
    pub const ALL: [ModSource; 5] = [
        Self::FilterEnv,
        Self::AmpEnv,
        Self::Lfo,
        Self::Velocity,
        Self::Key,
    ];
}

/// What a mod slot moves. See `uta_core::ModDestination`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModDestination {
    /// An empty slot.
    #[default]
    Off,
    Osc1Pitch,
    Osc2Pitch,
    Osc1Level,
    Osc2Level,
    SubLevel,
    NoiseLevel,
    Osc1Width,
    Cutoff,
    Resonance,
    LfoRate,
    Volume,
}

impl ModDestination {
    /// Every destination, in order. A destination's place in it is its
    /// index in a voice's modulation.
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

    /// How far it moves at an Amount of 1 with the source at 1, in its own
    /// unit: semitones for a pitch (two octaves, enough for a sync sweep),
    /// octaves for the cutoff (as far as the filter envelope's Env amount)
    /// and for the LFO's rate, and the whole range of a level, the width
    /// and the resonance. Volume's is a share of the voice's level: 1
    /// doubles it (+6 dB), -1 silences it. Agreed with Will for UTA-79.
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
            Self::Osc1Width => 0.9,
            Self::Cutoff => 10.0,
            Self::LfoRate => 4.0,
        }
    }
}

/// One mod slot: its source moves its destination by its amount, from -1
/// to 1, [`SynthSettings::MOD_AMOUNT`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModSlot {
    pub source: ModSource,
    pub destination: ModDestination,
    /// It glides. A new source or destination glides the old one's amount
    /// to nothing first, then the new one's in.
    pub amount: f32,
}

impl ModSlot {
    /// An empty slot, with `source` ready.
    pub const fn off(source: ModSource) -> Self {
        Self {
            source,
            destination: ModDestination::Off,
            amount: 0.0,
        }
    }
}

/// How many mod slots the synth has.
pub const MOD_SLOTS: usize = 3;

/// How the synth sounds. It travels in the [`crate::Snapshot`], and changes
/// glide rather than jump (see [`crate::SYNTH_SMOOTHING_SECONDS`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthSettings {
    pub osc1_wave: Waveform,
    /// Oscillator 1's octave from the note, [`Self::OCTAVE`].
    pub osc1_octave: i8,
    /// How much of each cycle oscillator 1's pulse spends high,
    /// [`Self::WIDTH`]: 0.5 is a square. It glides.
    pub osc1_width: f32,
    pub osc2_wave: Waveform,
    /// Oscillator 2's octave from the note, [`Self::OCTAVE`].
    pub osc2_octave: i8,
    /// Oscillator 2's offset from its octave in semitones,
    /// [`Self::PITCH_SEMITONES`]. It glides.
    pub osc2_pitch_semitones: f32,
    /// Whether oscillator 2 restarts its cycle where oscillator 1 starts
    /// one. Switching it crossfades.
    pub osc2_sync: bool,
    /// How far under oscillator 1 the sub plays, in octaves,
    /// [`Self::SUB_OCTAVE`].
    pub sub_octave: i8,
    /// How much of each goes into the filter, as linear levels,
    /// [`Self::LEVEL`]. Together they're balanced so that the mix is never
    /// louder than one oscillator at full level, in power (see `mix_gain`).
    pub osc1_level: f32,
    pub osc2_level: f32,
    pub sub_level: f32,
    pub noise_level: f32,
    /// Which output of the filter the voice plays. A new mode crossfades
    /// from the old one.
    pub filter_mode: FilterMode,
    /// The filter's cutoff in Hz, [`Self::CUTOFF_HZ`]. It glides on a log
    /// scale.
    pub cutoff_hz: f32,
    /// The filter's resonance, [`Self::RESONANCE`]: 0 is a plain
    /// (Butterworth) filter, 1 rings strongly at the cutoff.
    pub resonance: f32,
    /// How far the filter envelope moves the cutoff at its peak, in octaves,
    /// [`Self::FILTER_ENV_OCTAVES`]. It glides.
    pub filter_env_octaves: f32,
    /// How far the cutoff follows the note, [`Self::FILTER_KEY`]: at 1, an
    /// octave for each octave from middle C. It glides.
    pub filter_key: f32,
    /// How far the LFO moves the cutoff at its peak, in octaves,
    /// [`Self::FILTER_LFO_OCTAVES`]. It glides.
    pub filter_lfo_octaves: f32,
    /// The filter envelope, in the same units and ranges as the amp
    /// envelope's settings below.
    pub filter_attack_seconds: f32,
    pub filter_decay_seconds: f32,
    pub filter_sustain: f32,
    pub filter_release_seconds: f32,
    /// Seconds to rise from silence to full level, [`Self::TIME_SECONDS`].
    pub attack_seconds: f32,
    /// Seconds to fall from full level to the sustain level,
    /// [`Self::TIME_SECONDS`].
    pub decay_seconds: f32,
    /// The level held while the note is down, as a linear level,
    /// [`Self::SUSTAIN`].
    pub sustain: f32,
    /// Seconds to fall to silence once the note is released,
    /// [`Self::TIME_SECONDS`].
    pub release_seconds: f32,
    pub lfo_shape: LfoShape,
    /// The LFO's rate when it isn't synced, in Hz, [`Self::LFO_RATE_HZ`]. It
    /// glides on a log scale.
    pub lfo_rate_hz: f32,
    /// Whether the LFO is synced: its cycle is [`Self::lfo_synced_seconds`]
    /// long, and with Retrigger off its phase is the song's position.
    pub lfo_sync: bool,
    /// One of the LFO's cycles when synced, in seconds: the project's note
    /// division at the tempo, worked out on the control side as the
    /// Delay's synced time is, [`Self::LFO_SYNCED_SECONDS`].
    pub lfo_synced_seconds: f64,
    /// Whether each note starts the LFO's cycle afresh. Off, the voices
    /// share one.
    pub lfo_retrigger: bool,
    /// How far the LFO moves both oscillators' pitch at its peak, in
    /// semitones, [`Self::LFO_VIBRATO_SEMITONES`]. It glides.
    pub lfo_vibrato_semitones: f32,
    /// How far the LFO moves oscillator 1's pulse width at its peak,
    /// [`Self::LFO_WIDTH`]. It glides.
    pub lfo_width: f32,
    /// Poly or Mono. Switching releases every note.
    pub voice_mode: VoiceMode,
    /// In Mono, whether a note that overlaps the one before carries its
    /// envelopes on instead of attacking again.
    pub legato: bool,
    /// In Mono, how long the pitch takes to slide to an overlapping note,
    /// [`Self::GLIDE_SECONDS`]. At its shortest the pitch moves at once.
    pub glide_seconds: f32,
    /// Whether the oscillators start at the beginning of their wave on each
    /// note. Off, each note picks them up where they would be had they been
    /// running since Play.
    pub osc_retrigger: bool,
    /// The mod slots, each on top of the knobs and the fixed routings.
    pub mods: [ModSlot; MOD_SLOTS],
}

impl SynthSettings {
    pub const OCTAVE: RangeInclusive<i8> = -2..=2;
    pub const WIDTH: RangeInclusive<f32> = 0.05..=0.95;
    pub const PITCH_SEMITONES: RangeInclusive<f32> = -12.0..=12.0;
    pub const SUB_OCTAVE: RangeInclusive<i8> = -2..=-1;
    pub const LEVEL: RangeInclusive<f32> = 0.0..=1.0;
    pub const CUTOFF_HZ: RangeInclusive<f32> = 20.0..=20_000.0;
    pub const RESONANCE: RangeInclusive<f32> = 0.0..=1.0;
    pub const FILTER_ENV_OCTAVES: RangeInclusive<f32> = -10.0..=10.0;
    pub const FILTER_KEY: RangeInclusive<f32> = 0.0..=1.0;
    pub const FILTER_LFO_OCTAVES: RangeInclusive<f32> = -5.0..=5.0;
    pub const LFO_RATE_HZ: RangeInclusive<f32> = 0.05..=20.0;
    /// Every division at every tempo fits: a triplet 1/32 at 300 BPM is
    /// 17 ms, and a dotted bar at 20 BPM 18 s.
    pub const LFO_SYNCED_SECONDS: RangeInclusive<f64> = 0.01..=60.0;
    pub const LFO_VIBRATO_SEMITONES: RangeInclusive<f32> = 0.0..=2.0;
    pub const LFO_WIDTH: RangeInclusive<f32> = 0.0..=0.45;
    pub const TIME_SECONDS: RangeInclusive<f32> = 0.001..=10.0;
    pub const SUSTAIN: RangeInclusive<f32> = 0.0..=1.0;
    pub const GLIDE_SECONDS: RangeInclusive<f32> = 0.001..=5.0;
    pub const MOD_AMOUNT: RangeInclusive<f32> = -1.0..=1.0;

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default. The synth plays what this returns, so a
    /// bad value can never reach the audio.
    pub fn clamped(self) -> Self {
        let default = Self::default();
        let clamp = |value: f32, range: RangeInclusive<f32>, fallback: f32| {
            if value.is_nan() {
                fallback
            } else {
                value.clamp(*range.start(), *range.end())
            }
        };
        let octave =
            |value: i8, range: RangeInclusive<i8>| value.clamp(*range.start(), *range.end());
        Self {
            osc1_wave: self.osc1_wave,
            osc1_octave: octave(self.osc1_octave, Self::OCTAVE),
            osc1_width: clamp(self.osc1_width, Self::WIDTH, default.osc1_width),
            osc2_wave: self.osc2_wave,
            osc2_octave: octave(self.osc2_octave, Self::OCTAVE),
            osc2_pitch_semitones: clamp(
                self.osc2_pitch_semitones,
                Self::PITCH_SEMITONES,
                default.osc2_pitch_semitones,
            ),
            osc2_sync: self.osc2_sync,
            sub_octave: octave(self.sub_octave, Self::SUB_OCTAVE),
            osc1_level: clamp(self.osc1_level, Self::LEVEL, default.osc1_level),
            osc2_level: clamp(self.osc2_level, Self::LEVEL, default.osc2_level),
            sub_level: clamp(self.sub_level, Self::LEVEL, default.sub_level),
            noise_level: clamp(self.noise_level, Self::LEVEL, default.noise_level),
            cutoff_hz: clamp(self.cutoff_hz, Self::CUTOFF_HZ, default.cutoff_hz),
            filter_mode: self.filter_mode,
            resonance: clamp(self.resonance, Self::RESONANCE, default.resonance),
            filter_env_octaves: clamp(
                self.filter_env_octaves,
                Self::FILTER_ENV_OCTAVES,
                default.filter_env_octaves,
            ),
            filter_key: clamp(self.filter_key, Self::FILTER_KEY, default.filter_key),
            filter_lfo_octaves: clamp(
                self.filter_lfo_octaves,
                Self::FILTER_LFO_OCTAVES,
                default.filter_lfo_octaves,
            ),
            filter_attack_seconds: clamp(
                self.filter_attack_seconds,
                Self::TIME_SECONDS,
                default.filter_attack_seconds,
            ),
            filter_decay_seconds: clamp(
                self.filter_decay_seconds,
                Self::TIME_SECONDS,
                default.filter_decay_seconds,
            ),
            filter_sustain: clamp(self.filter_sustain, Self::SUSTAIN, default.filter_sustain),
            filter_release_seconds: clamp(
                self.filter_release_seconds,
                Self::TIME_SECONDS,
                default.filter_release_seconds,
            ),
            attack_seconds: clamp(
                self.attack_seconds,
                Self::TIME_SECONDS,
                default.attack_seconds,
            ),
            decay_seconds: clamp(
                self.decay_seconds,
                Self::TIME_SECONDS,
                default.decay_seconds,
            ),
            sustain: clamp(self.sustain, Self::SUSTAIN, default.sustain),
            release_seconds: clamp(
                self.release_seconds,
                Self::TIME_SECONDS,
                default.release_seconds,
            ),
            lfo_shape: self.lfo_shape,
            lfo_rate_hz: clamp(self.lfo_rate_hz, Self::LFO_RATE_HZ, default.lfo_rate_hz),
            lfo_sync: self.lfo_sync,
            lfo_synced_seconds: if self.lfo_synced_seconds.is_nan() {
                default.lfo_synced_seconds
            } else {
                self.lfo_synced_seconds.clamp(
                    *Self::LFO_SYNCED_SECONDS.start(),
                    *Self::LFO_SYNCED_SECONDS.end(),
                )
            },
            lfo_retrigger: self.lfo_retrigger,
            lfo_vibrato_semitones: clamp(
                self.lfo_vibrato_semitones,
                Self::LFO_VIBRATO_SEMITONES,
                default.lfo_vibrato_semitones,
            ),
            lfo_width: clamp(self.lfo_width, Self::LFO_WIDTH, default.lfo_width),
            voice_mode: self.voice_mode,
            legato: self.legato,
            glide_seconds: clamp(
                self.glide_seconds,
                Self::GLIDE_SECONDS,
                default.glide_seconds,
            ),
            osc_retrigger: self.osc_retrigger,
            mods: self.mods.map(|slot| ModSlot {
                amount: clamp(slot.amount, Self::MOD_AMOUNT, 0.0),
                ..slot
            }),
        }
    }
}

impl Default for SynthSettings {
    /// Today's sound: oscillator 1 a saw at full level, the rest of the
    /// mixer at zero, oscillator 2 free of oscillator 1, a low-pass filter
    /// that nothing moves, the oscillators restarting on each note, an LFO
    /// that moves nothing and empty mod slots, in Poly.
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
            cutoff_hz: 20_000.0,
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
            // A quarter note at 120 BPM.
            lfo_synced_seconds: 0.5,
            lfo_retrigger: false,
            lfo_vibrato_semitones: 0.0,
            lfo_width: 0.0,
            voice_mode: VoiceMode::Poly,
            legato: false,
            glide_seconds: 0.001,
            osc_retrigger: true,
            mods: [
                ModSlot::off(ModSource::Velocity),
                ModSlot::off(ModSource::FilterEnv),
                ModSlot::off(ModSource::Lfo),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_plan() {
        let settings = SynthSettings::default();
        assert_eq!(settings.osc1_wave, Waveform::Saw);
        assert_eq!((settings.osc1_octave, settings.osc2_octave), (0, 0));
        assert_eq!(settings.osc2_wave, Waveform::Saw);
        assert_eq!(settings.osc2_pitch_semitones, 0.0);
        assert_eq!(settings.osc1_width, 0.5);
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
        assert!(!settings.lfo_sync && !settings.lfo_retrigger);
        assert_eq!(settings.lfo_synced_seconds, 0.5);
        assert_eq!(
            (settings.lfo_vibrato_semitones, settings.lfo_width),
            (0.0, 0.0)
        );
        assert_eq!(settings.voice_mode, VoiceMode::Poly);
        assert!(!settings.legato);
        assert_eq!(settings.glide_seconds, 0.001);
        assert!(settings.osc_retrigger);
        assert_eq!(settings.clamped(), settings);
    }

    #[test]
    fn clamped_keeps_values_in_range() {
        let wild = SynthSettings {
            osc1_wave: Waveform::Pulse,
            osc1_octave: -9,
            osc1_width: 1.0,
            osc2_wave: Waveform::Sine,
            osc2_octave: 9,
            osc2_pitch_semitones: f32::NAN,
            osc2_sync: true,
            sub_octave: 0,
            osc1_level: 2.0,
            osc2_level: -1.0,
            sub_level: f32::INFINITY,
            noise_level: f32::NAN,
            filter_mode: FilterMode::BandPass,
            cutoff_hz: 1.0e9,
            resonance: -3.0,
            filter_env_octaves: -40.0,
            filter_key: f32::NAN,
            filter_lfo_octaves: 9.0,
            filter_attack_seconds: 0.0,
            filter_decay_seconds: 1.0e6,
            filter_sustain: 2.0,
            filter_release_seconds: f32::NAN,
            attack_seconds: 0.0,
            decay_seconds: f32::INFINITY,
            sustain: f32::NAN,
            release_seconds: -1.0,
            lfo_shape: LfoShape::SampleAndHold,
            lfo_rate_hz: 0.0,
            lfo_sync: true,
            lfo_synced_seconds: f64::NAN,
            lfo_retrigger: true,
            lfo_vibrato_semitones: f32::INFINITY,
            lfo_width: -1.0,
            voice_mode: VoiceMode::Mono,
            legato: true,
            glide_seconds: 60.0,
            osc_retrigger: false,
            mods: [
                ModSlot {
                    source: ModSource::Key,
                    destination: ModDestination::Cutoff,
                    amount: 3.0,
                },
                ModSlot {
                    source: ModSource::Lfo,
                    destination: ModDestination::Volume,
                    amount: f32::NAN,
                },
                ModSlot {
                    source: ModSource::AmpEnv,
                    destination: ModDestination::Osc2Pitch,
                    amount: -2.0,
                },
            ],
        }
        .clamped();
        assert_eq!(
            wild.mods.map(|slot| slot.amount),
            [1.0, 0.0, -1.0],
            "an amount out of range is clamped, and one that isn't a number is none"
        );
        assert_eq!(wild.mods[2].destination, ModDestination::Osc2Pitch);
        assert_eq!(wild.osc1_wave, Waveform::Pulse);
        assert_eq!(wild.osc1_octave, -2);
        assert_eq!(wild.osc2_octave, 2);
        assert_eq!(wild.osc2_pitch_semitones, 0.0);
        assert_eq!(wild.osc1_width, 0.95);
        assert!(wild.osc2_sync);
        assert_eq!(wild.sub_octave, -1);
        assert_eq!(
            [
                wild.osc1_level,
                wild.osc2_level,
                wild.sub_level,
                wild.noise_level
            ],
            [1.0, 0.0, 1.0, 0.0]
        );
        assert_eq!(wild.filter_mode, FilterMode::BandPass);
        assert_eq!(wild.cutoff_hz, 20_000.0);
        assert_eq!(wild.resonance, 0.0);
        assert_eq!((wild.filter_env_octaves, wild.filter_key), (-10.0, 0.0));
        assert_eq!(
            [
                wild.filter_attack_seconds,
                wild.filter_decay_seconds,
                wild.filter_sustain,
                wild.filter_release_seconds
            ],
            [0.001, 10.0, 1.0, 0.2]
        );
        assert_eq!(wild.attack_seconds, 0.001);
        assert_eq!(wild.decay_seconds, 10.0);
        assert_eq!(wild.sustain, 0.7);
        assert_eq!(wild.release_seconds, 0.001);
        assert_eq!(wild.filter_lfo_octaves, 5.0);
        assert_eq!(wild.lfo_shape, LfoShape::SampleAndHold);
        assert_eq!(wild.lfo_rate_hz, 0.05);
        assert!(wild.lfo_sync && wild.lfo_retrigger);
        assert_eq!(wild.lfo_synced_seconds, 0.5);
        assert_eq!((wild.lfo_vibrato_semitones, wild.lfo_width), (2.0, 0.0));
        assert_eq!(wild.voice_mode, VoiceMode::Mono);
        assert!(wild.legato);
        assert_eq!(wild.glide_seconds, 5.0);
        assert!(!wild.osc_retrigger);
        let nan = SynthSettings {
            glide_seconds: f32::NAN,
            ..SynthSettings::default()
        }
        .clamped();
        assert_eq!(nan.glide_seconds, 0.001);
    }
}
