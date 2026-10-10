//! The 808 cymbal: the hats' metal, split into three bands, each with its
//! own envelope and clipping amplifier, so the low band rings longest and
//! the high band is a short sizzle. See RFC-006, "How each sound is made"
//! (Cymbal).
//!
//! The structure is Werner, Abel and Smith's analysis of the 808 cymbal
//! (ICMC/SMC 2014, figure 2), and the numbers are theirs where the paper
//! gives them:
//! - **The metal** (see [`super::metal`]), the same six oscillators the
//!   hats use, goes through two band-passes, at 3.44 kHz and 7.1 kHz
//!   (section 4).
//! - **Three bands.** The low band is the 3.44 kHz band-pass on the Decay
//!   envelope. The mid band is the 7.1 kHz band-pass on two envelopes, a
//!   long one that follows Decay and a fast one. The high band is the same
//!   7.1 kHz band-pass on a short envelope (sections 7 and 8).
//! - **A swing VCA on each band:** the 808's clipping amplifier, ported
//!   from Emilie Gillet's `SwingVCA` in Plaits (MIT licence), as the hats'
//!   is (section 8).
//! - **A high-pass after each,** of second, second and third order, the
//!   third slightly resonant near 10.5 kHz (section 9). They take away the
//!   envelope's own thump and the low tones the clipping makes.
//! - **Tone** mainly turns the high band down, and the mid band a little,
//!   as the 808's tone stage does (section 10).
//!
//! Where it differs from the 808, and why:
//! - **The envelopes are single exponentials, charged by a hit,** as every
//!   sound's here are: a hit raises each band's envelope to its strength if
//!   that's higher, and leaves a louder ring alone. Each edge is smoothed
//!   over 0.1 ms, as the 808's attack smoother does (section 6). The 808's
//!   envelope circuits shape their rise and fall in more detail (figure 7);
//!   their lengths here are read from that figure, a judgement, not a fit.
//! - **The mid band's envelopes are added, not multiplied,** as the
//!   research's recipe has it: a fast burst on top of a long ring.
//! - **The filters are the kit's prewarped state-variable filters,** not
//!   Werner's exact Sallen-Key responses, and the tone stage is two gains,
//!   not his fifth-order network. The research found the structure matters
//!   more than these details. The high band has a gentle low-pass at
//!   14 kHz too, against the false tones.
//! - **Velocity drives the amplifiers and moves the band-passes up,** as on
//!   the hats: a harder hit is brighter and rougher, not only louder. Werner
//!   says the 808's accent works on the cymbal in a way that is "complex"
//!   (section 5); this is a judgement, not a measurement of it.
//!
//! Sonic Pi's SC-808 cymbal (whose licence is informal) was read for ideas
//! only; nothing here is taken from it.
//!
//! It runs at the kit's sample rate, as the hats do: Will couldn't hear 2×
//! oversampling on them (UTA-49), and the cymbal's false tones are measured
//! to stay as far down as theirs.

use crate::drums::filter::{OnePole, Svf, prewarp};
use crate::drums::metal::swing_vca;
use crate::drums::{CymbalSettings, EDGE_SECONDS, decay_per_sample, log_tau, smoothing};
use crate::ramp::Ramp;

/// The two band-passes' centres (Werner, section 4), and their quality:
/// Plaits' 1, a broad band, as the hats'.
const LOW_BAND_HZ: f64 = 3440.0;
const HIGH_BAND_HZ: f64 = 7100.0;
const BAND_Q: f64 = 1.0;
/// The high-passes after the amplifiers. The low and mid bands' sit under
/// their band-passes, the mid band's where the hats' does (0.85 of 7.1 kHz,
/// the research's recipe), with no resonance. The high band's is near
/// 10.5 kHz and slightly resonant, and a one-pole at the same frequency
/// makes it third order (Werner, section 9).
const LOW_HIGH_PASS_HZ: f64 = 2000.0;
const MID_HIGH_PASS_HZ: f64 = 6000.0;
const HIGH_HIGH_PASS_HZ: f64 = 10_500.0;
const FLAT_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
const RESONANT_Q: f64 = 1.2;
/// A gentle low-pass on the high band, which isn't the 808's: the false
/// tones (aliasing) the squares leave gather in the high band, and this
/// takes the edge off those nearest half the sample rate. With it, and the
/// high band's weight, the cymbal's false tones stay as far down as the
/// hats' at their defaults (see the `the_false_tones_stay_down` test).
const HIGH_LOW_PASS_HZ: f64 = 14_000.0;
/// The mid band's fast envelope and the high band's: the seconds each takes
/// to die away by 40 dB, read from Werner's figure 7 (EG #2 and EG #3).
const MID_FAST_DECAY_SECONDS: f64 = 0.3;
const HIGH_DECAY_SECONDS: f64 = 0.15;
/// How much of the mid band's envelope is the long one and how much the
/// fast one.
const MID_LONG: f64 = 0.35;
const MID_FAST: f64 = 0.65;
/// Each band's weight in the mix, at full Tone. The 808's output stage
/// rises 6 dB an octave (Werner, section 11), and its third high-pass
/// resonates, so its upper bands come out far stronger than their
/// band-passes alone would make them; these stand in for both. A judgement:
/// over the first 50 ms of an accent at full Tone, the mid band measures
/// 2 dB under the low band and the high band 7 dB under it. The high band
/// is held there by the false tones (see [`HIGH_LOW_PASS_HZ`]).
const LOW_WEIGHT: f64 = 1.0;
const MID_WEIGHT: f64 = 1.6;
const HIGH_WEIGHT: f64 = 2.5;
/// How far Tone turns the high and mid bands down at its lowest, in dB. At
/// its highest, they're at full level. The 808's turns the high band down
/// by about 14 dB (Werner, figure 9); this goes a little further, so the
/// sizzle can be taken right off.
const HIGH_TONE_DB: f32 = -18.0;
const MID_TONE_DB: f32 = -6.0;
/// How hard the band-passed metal goes into the amplifiers' clipping at
/// full accent, and how much less at no strength at all, as the hats'.
const DRIVE: f64 = 1.0;
const SOFTEST_DRIVE: f64 = 0.6;
/// How far the band-passes move with the hit's strength, in octaves a unit
/// of strength, from where they are at an unaccented hit (strength 0.3), as
/// the hats'.
const BRIGHTER_OCTAVES: f64 = 0.35;
const UNACCENTED: f64 = 0.3;
/// How fast the drive and band-passes follow a new hit's strength, so they
/// never jump.
const STRENGTH_SECONDS: f64 = 0.002;
/// The output level that puts a default cymbal at full accent at the kit's
/// reference peak (see [`super::REFERENCE_PEAK`]). Measured, not derived:
/// see the `the_cymbal_at_full_accent_peaks_at_the_reference` test.
const OUTPUT_GAIN: f32 = 1.2168;
/// Below this, summed over its envelopes and the high-passes after the
/// amplifiers, the cymbal has died away and stops doing work: far under
/// anything audible.
const SILENT: f64 = 1.0e-6;

/// Tone's gain on a band that it turns down by `lowest_db` at 0.
fn tone_gain(tone: f32, lowest_db: f32) -> f32 {
    super::db_to_gain(lowest_db * (1.0 - tone))
}

/// The cymbal's controls, as they glide.
#[derive(Debug, Clone, Copy)]
struct Controls {
    /// Tone's gains on the high and mid bands.
    high_gain: Ramp,
    mid_gain: Ramp,
    /// Decay's time constant, as its log, so it glides evenly.
    log_tau: Ramp,
    level: Ramp,
}

impl Controls {
    fn new(settings: CymbalSettings, smoothing: u32) -> Self {
        Self {
            high_gain: Ramp::new(tone_gain(settings.tone, HIGH_TONE_DB), smoothing),
            mid_gain: Ramp::new(tone_gain(settings.tone, MID_TONE_DB), smoothing),
            log_tau: Ramp::new(log_tau(settings.decay_seconds), smoothing),
            level: Ramp::new(super::db_to_gain(settings.level_db), smoothing),
        }
    }
}

/// Per-sample constants for a sample rate.
#[derive(Debug, Clone, Copy)]
struct Rates {
    sample_rate: f64,
    strength: f64,
    edge: f64,
    mid_fast: f64,
    high: f64,
    low_high_pass: f64,
    mid_high_pass: f64,
    high_high_pass: f64,
    high_low_pass: f64,
}

impl Rates {
    fn new(sample_rate: f64) -> Self {
        let tau = |seconds: f64| seconds / super::LN_100;
        Self {
            sample_rate,
            strength: smoothing(STRENGTH_SECONDS, sample_rate),
            edge: smoothing(EDGE_SECONDS, sample_rate),
            mid_fast: decay_per_sample(tau(MID_FAST_DECAY_SECONDS), sample_rate),
            high: decay_per_sample(tau(HIGH_DECAY_SECONDS), sample_rate),
            low_high_pass: prewarp(LOW_HIGH_PASS_HZ, sample_rate),
            mid_high_pass: prewarp(MID_HIGH_PASS_HZ, sample_rate),
            high_high_pass: prewarp(HIGH_HIGH_PASS_HZ, sample_rate),
            high_low_pass: prewarp(HIGH_LOW_PASS_HZ, sample_rate),
        }
    }
}

/// What the controls' values work out to, kept until they move.
#[derive(Debug, Clone, Copy)]
struct Derived {
    log_tau: f32,
    decay: f64,
    /// How far the band-passes sit from where they rest, in octaves, and
    /// their prewarped gains.
    brightness: f64,
    low_band: f64,
    high_band: f64,
}

/// What's ringing: all zero when it's silent.
#[derive(Debug, Clone, Copy)]
struct State {
    /// Each envelope: what it was charged to, dying away.
    low: f64,
    mid_long: f64,
    mid_fast: f64,
    high: f64,
    /// Each band's envelope into its amplifier, with its edges smoothed.
    low_envelope: f64,
    mid_envelope: f64,
    high_envelope: f64,
    /// How hard the metal goes into the amplifiers, and where it's heading.
    drive: f64,
    drive_target: f64,
    /// How far the band-passes sit from where they rest, in octaves, and
    /// where they're heading.
    brightness: f64,
    brightness_target: f64,
    low_band: Svf,
    high_band: Svf,
    low_high_pass: Svf,
    mid_high_pass: Svf,
    high_high_pass: Svf,
    high_high_pass_pole: OnePole,
    high_low_pass: OnePole,
    /// Whether it's ringing, or about to.
    active: bool,
}

impl State {
    fn silent() -> Self {
        Self {
            low: 0.0,
            mid_long: 0.0,
            mid_fast: 0.0,
            high: 0.0,
            low_envelope: 0.0,
            mid_envelope: 0.0,
            high_envelope: 0.0,
            drive: 0.0,
            drive_target: 0.0,
            brightness: 0.0,
            brightness_target: 0.0,
            low_band: Svf::default(),
            high_band: Svf::default(),
            low_high_pass: Svf::default(),
            mid_high_pass: Svf::default(),
            high_high_pass: Svf::default(),
            high_high_pass_pole: OnePole::default(),
            high_low_pass: OnePole::default(),
            active: false,
        }
    }

    /// How much is still ringing after the amplifiers, where the sound is.
    /// The band-passes aren't counted: they're before the amplifiers, and
    /// follow the metal, which never stops.
    fn ringing(&self) -> f64 {
        self.low
            + self.mid_long
            + self.mid_fast
            + self.high
            + self.low_envelope
            + self.mid_envelope
            + self.high_envelope
            + self.low_high_pass.state()
            + self.mid_high_pass.state()
            + self.high_high_pass.state()
            + self.high_high_pass_pole.state().abs()
            + self.high_low_pass.state().abs()
    }
}

/// The three bands' outputs for one sample, before Tone and Level.
#[derive(Debug, Clone, Copy)]
struct Bands {
    low: f64,
    mid: f64,
    high: f64,
}

/// The 808's cymbal circuit. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cymbal {
    settings: CymbalSettings,
    controls: Controls,
    rates: Rates,
    derived: Derived,
    state: State,
}

impl Cymbal {
    pub(crate) fn new(settings: CymbalSettings, sample_rate: f64) -> Self {
        let settings = settings.clamped();
        Self {
            settings,
            controls: Controls::new(settings, super::smoothing_samples(sample_rate)),
            rates: Rates::new(sample_rate),
            derived: Derived {
                log_tau: f32::NAN,
                decay: 0.0,
                brightness: f64::NAN,
                low_band: 0.0,
                high_band: 0.0,
            },
            state: State::silent(),
        }
    }

    /// Moves to a new sample rate, silent: the stream it was playing on has
    /// already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        *self = Self::new(self.settings, sample_rate);
    }

    /// Takes on new settings straight away, for a cymbal that isn't
    /// sounding.
    pub(crate) fn load(&mut self, settings: CymbalSettings) {
        self.settings = settings.clamped();
        self.controls = Controls::new(
            self.settings,
            super::smoothing_samples(self.rates.sample_rate),
        );
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, settings: CymbalSettings) {
        let settings = settings.clamped();
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        let controls = &mut self.controls;
        controls
            .high_gain
            .set_target(tone_gain(settings.tone, HIGH_TONE_DB));
        controls
            .mid_gain
            .set_target(tone_gain(settings.tone, MID_TONE_DB));
        controls.log_tau.set_target(log_tau(settings.decay_seconds));
        controls
            .level
            .set_target(super::db_to_gain(settings.level_db));
    }

    /// A hit with this strength (see [`super::velocity_to_strength`]): it
    /// charges every band's envelopes, and the amplifiers and band-passes
    /// head for its strength, carrying on from where they are.
    pub(crate) fn hit(&mut self, strength: f32) {
        let strength = f64::from(strength);
        let state = &mut self.state;
        state.low = state.low.max(strength);
        state.mid_long = state.mid_long.max(strength);
        state.mid_fast = state.mid_fast.max(strength);
        state.high = state.high.max(strength);
        state.drive_target = SOFTEST_DRIVE + (DRIVE - SOFTEST_DRIVE) * strength;
        state.brightness_target = BRIGHTER_OCTAVES * (strength - UNACCENTED);
        if !state.active {
            state.drive = state.drive_target;
            state.brightness = state.brightness_target;
        }
        state.active = true;
    }

    /// Whether it's ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.state.active
    }

    /// The controls' values for this sample, worked out again if they
    /// moved: Tone's gains on the high and mid bands, and Level. They glide
    /// whether or not the cymbal is ringing.
    #[inline]
    fn controls(&mut self) -> (f64, f64, f64) {
        let controls = &mut self.controls;
        let high_gain = controls.high_gain.next_value();
        let mid_gain = controls.mid_gain.next_value();
        let log_tau = controls.log_tau.next_value();
        let level = controls.level.next_value();
        let (rates, derived, state) = (&self.rates, &mut self.derived, &self.state);
        if log_tau != derived.log_tau {
            derived.log_tau = log_tau;
            derived.decay = decay_per_sample(f64::from(log_tau).exp(), rates.sample_rate);
        }
        if state.brightness != derived.brightness {
            derived.brightness = state.brightness;
            let up = state.brightness.exp2();
            derived.low_band = prewarp(LOW_BAND_HZ * up, rates.sample_rate);
            derived.high_band = prewarp(HIGH_BAND_HZ * up, rates.sample_rate);
        }
        (f64::from(high_gain), f64::from(mid_gain), f64::from(level))
    }

    /// The next sample, from this sample of the metal. A cymbal that has
    /// died away returns silence without doing the work.
    #[inline]
    pub(crate) fn next_sample(&mut self, metal: f64) -> f32 {
        let (high_gain, mid_gain, level) = self.controls();
        if !self.state.active {
            return 0.0;
        }
        let drive = self.state.drive;
        let bands = self.bands(metal, |band, envelope| swing_vca(band * drive, envelope));
        let out = (bands.low * LOW_WEIGHT
            + bands.mid * MID_WEIGHT * mid_gain
            + bands.high * HIGH_WEIGHT * high_gain)
            * level;
        if self.state.ringing() < SILENT {
            // Died away: stop, and stop doing work until the next hit.
            self.state = State::silent();
        }
        out as f32 * OUTPUT_GAIN
    }

    /// Each band for this sample of the metal: the envelopes die away and
    /// drive the amplifiers, `amplifier(band, envelope)`, between the
    /// band-passes and the high-passes.
    #[inline]
    fn bands(&mut self, metal: f64, amplifier: impl Fn(f64, f64) -> f64) -> Bands {
        let (rates, derived) = (&self.rates, &self.derived);
        let state = &mut self.state;
        state.low *= derived.decay;
        state.mid_long *= derived.decay;
        state.mid_fast *= rates.mid_fast;
        state.high *= rates.high;
        let mid = state.mid_long * MID_LONG + state.mid_fast * MID_FAST;
        state.low_envelope += rates.edge * (state.low - state.low_envelope);
        state.mid_envelope += rates.edge * (mid - state.mid_envelope);
        state.high_envelope += rates.edge * (state.high - state.high_envelope);
        state.drive += rates.strength * (state.drive_target - state.drive);
        state.brightness += rates.strength * (state.brightness_target - state.brightness);

        let low_band = state
            .low_band
            .process(metal, derived.low_band, BAND_Q)
            .band_pass
            / BAND_Q;
        let high_band = state
            .high_band
            .process(metal, derived.high_band, BAND_Q)
            .band_pass
            / BAND_Q;
        let low = amplifier(low_band, state.low_envelope);
        let mid = amplifier(high_band, state.mid_envelope);
        let high = amplifier(high_band, state.high_envelope);
        let high = state
            .high_high_pass
            .process(high, rates.high_high_pass, RESONANT_Q)
            .high_pass;
        Bands {
            low: state
                .low_high_pass
                .process(low, rates.low_high_pass, FLAT_Q)
                .high_pass,
            mid: state
                .mid_high_pass
                .process(mid, rates.mid_high_pass, FLAT_Q)
                .high_pass,
            high: state.high_low_pass.low_pass(
                state
                    .high_high_pass_pole
                    .high_pass(high, rates.high_high_pass),
                rates.high_low_pass,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::REFERENCE_PEAK;
    use crate::drums::metal::{Metal, signal_to_alias_db};

    const RATE: f64 = 48_000.0;

    fn metal() -> Metal {
        super::super::metal(crate::drums::ClosedHatSettings::default().tune_hz, RATE)
    }

    fn cymbal() -> Cymbal {
        Cymbal::new(CymbalSettings::default(), RATE)
    }

    /// Runs `cymbal` for `seconds` on `metal`, as the kit does.
    fn run(cymbal: &mut Cymbal, metal: &mut Metal, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|_| cymbal.next_sample(metal.next_sample()))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    /// The metal through the cymbal's filters and its mix at the default
    /// Tone, with the amplifiers left out (they're the one part that isn't
    /// linear): the cymbal's version of the hats' "hat path".
    fn cymbal_path(tone: f32) -> impl FnMut(f64) -> f64 {
        let mut cymbal = Cymbal::new(
            CymbalSettings {
                tone,
                ..CymbalSettings::default()
            },
            RATE,
        );
        move |metal| {
            let (high_gain, mid_gain, _) = cymbal.controls();
            let bands = cymbal.bands(metal, |band, _| band);
            bands.low * LOW_WEIGHT
                + bands.mid * MID_WEIGHT * mid_gain
                + bands.high * HIGH_WEIGHT * high_gain
        }
    }

    /// The false tones stay at or below the hats' (UTA-49), measured the
    /// same way: at its defaults, the cymbal's are at least as far under its
    /// sound as the hats' 35.2 dB at theirs. Measured: 35.4 dB. Tone brings
    /// up the high band, where they gather, so full Tone has more: measured
    /// 32.2 dB, still well clear of the hats at their brightest Tone (12 kHz,
    /// 25.6 dB). Tone 0 measures 38.4 dB. Without the high band's low-pass
    /// the default measured 34.4 dB, and with the high band 4 dB louder,
    /// 32.9 dB. Each is checked to within 0.5 dB of what was measured, so
    /// they can't quietly come back.
    #[test]
    fn the_false_tones_stay_down() {
        let default = signal_to_alias_db(RATE, cymbal_path(0.5));
        assert!(default >= 35.2, "default: {default:.1} dB");
        for (tone, measured) in [(0.0, 38.4), (1.0, 32.2)] {
            let db = signal_to_alias_db(RATE, cymbal_path(tone));
            assert!(db > measured - 0.5, "Tone {tone}: {db:.1} dB");
        }
    }

    #[test]
    fn the_cymbal_at_full_accent_peaks_at_the_reference() {
        let mut cymbal = cymbal();
        cymbal.hit(1.0);
        let peak = peak(&run(&mut cymbal, &mut metal(), 1.0));
        let db = 20.0 * (peak / REFERENCE_PEAK).log10();
        assert!(db.abs() < 0.1, "peak {peak}, {db:.2} dB");
    }

    /// Each band alone, through its amplifier, for `seconds` after an
    /// accent, as `band` picks it out.
    fn band(band: impl Fn(&Bands) -> f64, seconds: f64) -> Vec<f32> {
        let (mut cymbal, mut metal) = (cymbal(), metal());
        cymbal.hit(1.0);
        (0..(seconds * RATE) as usize)
            .map(|_| {
                cymbal.controls();
                let drive = cymbal.state.drive;
                let bands = cymbal.bands(metal.next_sample(), |band, envelope| {
                    swing_vca(band * drive, envelope)
                });
                band(&bands) as f32
            })
            .collect()
    }

    /// The seconds after its loudest that `samples` take to fall 40 dB, by
    /// the RMS in 5 ms windows.
    fn decay_40_db(samples: &[f32]) -> f64 {
        let window = (0.005 * RATE) as usize;
        let levels: Vec<f64> = samples
            .chunks(window)
            .map(|chunk| {
                (chunk.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / chunk.len() as f64)
                    .sqrt()
            })
            .collect();
        let (loudest, &peak) = levels
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        let below = levels[loudest..]
            .iter()
            .position(|&level| level < peak / 100.0)
            .unwrap();
        below as f64 * 0.005
    }

    /// The low band rings longest and the high band is a short sizzle, as
    /// on the 808: measured at the default Decay, the low band falls 40 dB
    /// in 0.79 s, the mid band in 0.62 s (its fast envelope fades first,
    /// leaving the long one, which follows Decay) and the high band in
    /// 0.155 s, about its envelope's 0.15 s.
    #[test]
    fn the_low_band_rings_longest_and_the_high_band_is_shortest() {
        let low = decay_40_db(&band(|bands| bands.low, 2.0));
        let mid = decay_40_db(&band(|bands| bands.mid, 2.0));
        let high = decay_40_db(&band(|bands| bands.high, 2.0));
        assert!((0.7..0.9).contains(&low), "low {low}");
        assert!(low > mid * 1.15 && mid > high * 3.0, "{low} {mid} {high}");
        assert!((0.12..0.2).contains(&high), "high {high}");
    }

    /// Once it has died away far below hearing it stops, and does no work
    /// until the next hit.
    #[test]
    fn a_cymbal_that_has_died_away_stops() {
        let mut cymbal = Cymbal::new(
            CymbalSettings {
                decay_seconds: 1.2,
                ..CymbalSettings::default()
            },
            RATE,
        );
        let mut metal = metal();
        assert!(!cymbal.is_sounding());
        cymbal.hit(1.0);
        let samples = run(&mut cymbal, &mut metal, 5.0);
        assert!(!cymbal.is_sounding(), "still ringing after 5 s");
        let stopped = samples.iter().rposition(|&s| s != 0.0).unwrap();
        let last = peak(&samples[stopped.saturating_sub(4800)..=stopped]);
        assert!(last < 1e-5, "last 0.1 s peaked at {last}");
        cymbal.hit(1.0);
        assert!(peak(&run(&mut cymbal, &mut metal, 0.1)) > 0.2);
    }

    /// A hit while it rings charges its envelopes rather than restarting
    /// them: a soft hit leaves a louder ring alone, and a harder one raises
    /// it.
    #[test]
    fn a_hit_charges_the_ring_rather_than_restarting_it() {
        let mut cymbal = cymbal();
        cymbal.hit(1.0);
        run(&mut cymbal, &mut metal(), 0.05);
        let before = cymbal.state;
        cymbal.hit(0.1);
        let after = cymbal.state;
        assert_eq!(
            (after.low, after.mid_long, after.mid_fast, after.high),
            (before.low, before.mid_long, before.mid_fast, before.high)
        );
        cymbal.hit(1.0);
        assert_eq!(cymbal.state.high, 1.0);
        assert_eq!(cymbal.state.low, before.low.max(1.0));
    }

    /// Every control glides to a new setting rather than jumping: a tenth
    /// of the way through the glide, it has moved about a tenth of the way.
    #[test]
    fn every_control_glides_to_a_new_setting() {
        let (mut cymbal, mut metal) = (cymbal(), metal());
        cymbal.hit(1.0);
        run(&mut cymbal, &mut metal, 0.01);
        let before = cymbal.controls;
        cymbal.set_settings(CymbalSettings {
            tone: 1.0,
            decay_seconds: 0.35,
            level_db: 6.0,
        });
        let after = Controls::new(cymbal.settings, 1);
        let glide = super::super::smoothing_samples(RATE) as usize;
        run(&mut cymbal, &mut metal, (glide / 10) as f64 / RATE);
        let share = |from: &Ramp, now: &Ramp, to: &Ramp| {
            (now.value() - from.value()) / (to.value() - from.value())
        };
        let shares = |cymbal: &Cymbal| {
            let now = &cymbal.controls;
            [
                share(&before.high_gain, &now.high_gain, &after.high_gain),
                share(&before.mid_gain, &now.mid_gain, &after.mid_gain),
                share(&before.log_tau, &now.log_tau, &after.log_tau),
                share(&before.level, &now.level, &after.level),
            ]
        };
        for share in shares(&cymbal) {
            assert!((0.05..0.2).contains(&share), "{:?}", shares(&cymbal));
        }
        run(&mut cymbal, &mut metal, glide as f64 / RATE);
        for share in shares(&cymbal) {
            assert!((share - 1.0).abs() < 1e-3, "{:?}", shares(&cymbal));
        }
    }
}
