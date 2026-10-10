//! The 808 closed and open hats: the metal, filtered down to its top end,
//! through a clipping amplifier. See RFC-006, "How each sound is made"
//! (Hats, 808).
//!
//! Ported from Emilie Gillet's `HiHat` in Plaits (MIT licence), with its
//! `SwingVCA`, and the 808's own numbers from the research:
//! - **The metal** (see [`super::metal`]) goes through a band-pass at Tone,
//!   7.1 kHz by default, the band the research's recipe gives the hats.
//! - **The swing VCA.** On the 808 the amplifier is a transistor biased to
//!   clip, with the envelope as its supply: it lets the top of the wave
//!   through four times over and squashes the bottom, then saturates. That's
//!   the "drive" the hats have. Ported from Plaits: ×4 above zero, ×0.1
//!   below, then `s / (1 + |s|)`, plus a little of the envelope itself.
//! - **A high-pass** after it, at 0.85 of Tone (6 kHz by default), takes
//!   away the envelope's own thump and the low tones the clipping makes.
//!
//! The two hats are one circuit, as on the 808: one metal, one band-pass,
//! one amplifier and one high-pass, each hat with its own envelope into the
//! amplifier. That's how a closed hit cuts off a ringing open hat (the
//! **choke**): it drains the open hat's envelope in a few milliseconds.
//!
//! Where it differs from Plaits, and why:
//! - **A hit charges its envelope; it doesn't set it,** as every sound
//!   here does: a hit raises the envelope to its level if that's higher,
//!   and leaves a louder ring alone.
//! - **Velocity drives the amplifier,** not only its envelope. Plaits'
//!   accent sets the envelope's peak, which the VCA applies after it clips,
//!   so a soft hit is the same sound, quieter. Here a hit's strength also
//!   sets how hard the band-passed metal goes into the clipping, and moves
//!   the filters up a little, so a harder hit is brighter and rougher. The
//!   808's accent works through its attack smoother, envelopes and swing
//!   VCAs in a way Werner calls "complex"; this is a judgement, not a
//!   measurement of it.
//! - **One envelope stage, not two.** Plaits cuts its envelope faster
//!   below half, for its own Decay; the 808's are RC discharges, a single
//!   exponential, so these are too.
//! - **No clocked noise.** Plaits mixes some in for variety, and says it is
//!   "not at all part of the 808 circuit".
//! - **Every edge is smoothed** over 0.1 ms, as the 808 cymbal's attack is
//!   (Werner).
//!
//! It runs at the kit's sample rate. Running it at twice that (2×
//! oversampling) takes the false tones (aliasing) from 35 dB under the sound
//! to 54 dB, but in a blind A/B Will couldn't hear the difference, so it
//! isn't worth its cost (RFC-006, resolved open question 2; UTA-49).

use crate::drums::filter::{Svf, prewarp};
use crate::drums::{
    ClosedHatSettings, EDGE_SECONDS, OpenHatSettings, decay_per_sample, log_tau, smoothing,
};
use crate::ramp::Ramp;

/// The band-pass's quality: Plaits' 1, a broad band.
const BAND_Q: f64 = 1.0;
/// The high-pass, as a share of Tone, and its quality (no resonance). From
/// the research's recipe: 6 kHz under a 7.1 kHz band-pass.
const HIGH_PASS_RATIO: f64 = 0.85;
const HIGH_PASS_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// How hard the band-passed metal goes into the amplifier's clipping at
/// full accent, and how much less at no strength at all.
const DRIVE: f64 = 1.0;
const SOFTEST_DRIVE: f64 = 0.6;
/// How far the filters move with the hit's strength, in octaves a unit of
/// strength, from where they are at an unaccented hit (strength 0.3).
const BRIGHTER_OCTAVES: f64 = 0.35;
const UNACCENTED: f64 = 0.3;
/// How fast the drive and filters follow a new hit's strength, so they
/// never jump.
const STRENGTH_SECONDS: f64 = 0.002;
/// How fast a closed hit drains a ringing open hat: its time constant.
/// 60 ms later the open hat is 260 dB down.
const CHOKE_SECONDS: f64 = 0.002;
/// The output level that puts a default closed hat at full accent at the
/// kit's reference peak (see [`super::REFERENCE_PEAK`]). Measured, not
/// derived: see the `the_closed_hat_at_full_accent_peaks_at_the_reference`
/// test.
const OUTPUT_GAIN: f32 = 2.4431;
/// Below this, summed over its envelopes and the high-pass after the
/// amplifier, the hats have died away and stop doing work: far under
/// anything audible.
const SILENT: f64 = 1.0e-6;

/// Plaits' `SwingVCA`: the 808 hat's amplifier, which lets the top of the
/// wave through four times over and squashes the bottom, saturates, and
/// lets a little of its supply, the envelope, through.
#[inline]
fn swing_vca(s: f64, gain: f64) -> f64 {
    let s = s * if s > 0.0 { 4.0 } else { 0.1 };
    let s = s / (1.0 + s.abs());
    (s + 0.1) * gain
}

/// The hats' controls, as they glide.
#[derive(Debug, Clone, Copy)]
struct Controls {
    /// Tone, in octaves (log2 of Hz), so it glides on a log scale.
    tone_octaves: Ramp,
    /// Each hat's decay's time constant, as its log, so it glides evenly.
    closed_log_tau: Ramp,
    open_log_tau: Ramp,
    closed_level: Ramp,
    open_level: Ramp,
}

impl Controls {
    fn new(closed: ClosedHatSettings, open: OpenHatSettings, smoothing: u32) -> Self {
        Self {
            tone_octaves: Ramp::new(closed.tone_hz.log2(), smoothing),
            closed_log_tau: Ramp::new(log_tau(closed.decay_seconds), smoothing),
            open_log_tau: Ramp::new(log_tau(open.decay_seconds), smoothing),
            closed_level: Ramp::new(super::db_to_gain(closed.level_db), smoothing),
            open_level: Ramp::new(super::db_to_gain(open.level_db), smoothing),
        }
    }
}

/// Per-sample constants for a sample rate.
#[derive(Debug, Clone, Copy)]
struct Rates {
    sample_rate: f64,
    strength: f64,
    edge: f64,
    choke: f64,
}

impl Rates {
    fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            strength: smoothing(STRENGTH_SECONDS, sample_rate),
            edge: smoothing(EDGE_SECONDS, sample_rate),
            choke: decay_per_sample(CHOKE_SECONDS, sample_rate),
        }
    }
}

/// What the controls' values work out to, kept until they move.
#[derive(Debug, Clone, Copy)]
struct Derived {
    closed_log_tau: f32,
    closed_decay: f64,
    open_log_tau: f32,
    open_decay: f64,
    /// Where the filters sit, in octaves, and their prewarped gains.
    centre: f64,
    band: f64,
    high_pass: f64,
}

/// What's ringing: all zero when it's silent.
#[derive(Debug, Clone, Copy)]
struct State {
    /// Each hat's envelope: what it was charged to, dying away.
    closed: f64,
    open: f64,
    /// Whether a closed hit has cut the open hat off.
    choked: bool,
    /// The envelopes into the amplifier, with their edges smoothed.
    envelope: f64,
    /// How hard the metal goes into the amplifier, and where it's heading.
    drive: f64,
    drive_target: f64,
    /// How far the filters sit from Tone, in octaves, and where they're
    /// heading.
    brightness: f64,
    brightness_target: f64,
    band: Svf,
    high_pass: Svf,
    /// Whether it's ringing, or about to.
    active: bool,
}

impl State {
    fn silent() -> Self {
        Self {
            closed: 0.0,
            open: 0.0,
            choked: false,
            envelope: 0.0,
            drive: 0.0,
            drive_target: 0.0,
            brightness: 0.0,
            brightness_target: 0.0,
            band: Svf::default(),
            high_pass: Svf::default(),
            active: false,
        }
    }
}

/// The 808's hat circuit: both hats. Everything in it is a plain number.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hats {
    closed: ClosedHatSettings,
    open: OpenHatSettings,
    controls: Controls,
    rates: Rates,
    derived: Derived,
    state: State,
}

impl Hats {
    pub(crate) fn new(closed: ClosedHatSettings, open: OpenHatSettings, sample_rate: f64) -> Self {
        let (closed, open) = (closed.clamped(), open.clamped());
        Self {
            closed,
            open,
            controls: Controls::new(closed, open, super::smoothing_samples(sample_rate)),
            rates: Rates::new(sample_rate),
            derived: Derived {
                closed_log_tau: f32::NAN,
                closed_decay: 0.0,
                open_log_tau: f32::NAN,
                open_decay: 0.0,
                centre: f64::NAN,
                band: 0.0,
                high_pass: 0.0,
            },
            state: State::silent(),
        }
    }

    /// Moves to a new sample rate, silent: the stream it was playing on has
    /// already faded out or gone.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        *self = Self::new(self.closed, self.open, sample_rate);
    }

    /// Takes on new settings straight away, for hats that aren't sounding.
    pub(crate) fn load(&mut self, closed: ClosedHatSettings, open: OpenHatSettings) {
        self.closed = closed.clamped();
        self.open = open.clamped();
        self.controls = Controls::new(
            self.closed,
            self.open,
            super::smoothing_samples(self.rates.sample_rate),
        );
    }

    /// Glides to new settings.
    pub(crate) fn set_settings(&mut self, closed: ClosedHatSettings, open: OpenHatSettings) {
        let (closed, open) = (closed.clamped(), open.clamped());
        if (closed, open) == (self.closed, self.open) {
            return;
        }
        (self.closed, self.open) = (closed, open);
        let controls = &mut self.controls;
        controls.tone_octaves.set_target(closed.tone_hz.log2());
        controls
            .closed_log_tau
            .set_target(log_tau(closed.decay_seconds));
        controls
            .open_log_tau
            .set_target(log_tau(open.decay_seconds));
        controls
            .closed_level
            .set_target(super::db_to_gain(closed.level_db));
        controls
            .open_level
            .set_target(super::db_to_gain(open.level_db));
    }

    /// A closed hat with this strength (see [`super::velocity_to_strength`]):
    /// it charges the closed hat's envelope and cuts the open hat off.
    pub(crate) fn hit_closed(&mut self, strength: f32) {
        let strength = f64::from(strength);
        self.state.closed = self.state.closed.max(strength);
        self.state.choked = true;
        self.struck(strength);
    }

    /// An open hat with this strength: it charges the open hat's envelope.
    pub(crate) fn hit_open(&mut self, strength: f32) {
        let strength = f64::from(strength);
        self.state.open = self.state.open.max(strength);
        self.state.choked = false;
        self.struck(strength);
    }

    /// What either hit does to the amplifier and filters: they head for the
    /// hit's strength. The filters carry on from where they are.
    fn struck(&mut self, strength: f64) {
        let state = &mut self.state;
        state.drive_target = SOFTEST_DRIVE + (DRIVE - SOFTEST_DRIVE) * strength;
        state.brightness_target = BRIGHTER_OCTAVES * (strength - UNACCENTED);
        if !state.active {
            state.drive = state.drive_target;
            state.brightness = state.brightness_target;
        }
        state.active = true;
    }

    /// The metal's Tune, which is the closed hat's.
    pub(crate) fn tune_hz(&self) -> f32 {
        self.closed.tune_hz
    }

    /// Whether either hat is ringing, or about to.
    pub(crate) fn is_sounding(&self) -> bool {
        self.state.active
    }

    /// The controls' values for this sample, worked out again if they
    /// moved. They glide whether or not the hats are ringing.
    #[inline]
    fn controls(&mut self) -> (f64, f64) {
        let controls = &mut self.controls;
        let tone_octaves = controls.tone_octaves.next_value();
        let closed_log_tau = controls.closed_log_tau.next_value();
        let open_log_tau = controls.open_log_tau.next_value();
        let closed_level = controls.closed_level.next_value();
        let open_level = controls.open_level.next_value();
        let (rates, derived, state) = (&self.rates, &mut self.derived, &self.state);
        if closed_log_tau != derived.closed_log_tau {
            derived.closed_log_tau = closed_log_tau;
            derived.closed_decay =
                decay_per_sample(f64::from(closed_log_tau).exp(), rates.sample_rate);
        }
        if open_log_tau != derived.open_log_tau {
            derived.open_log_tau = open_log_tau;
            derived.open_decay = decay_per_sample(f64::from(open_log_tau).exp(), rates.sample_rate);
        }
        let centre = f64::from(tone_octaves) + state.brightness;
        if centre != derived.centre {
            derived.centre = centre;
            let hz = centre.exp2();
            derived.band = prewarp(hz, rates.sample_rate);
            derived.high_pass = prewarp(hz * HIGH_PASS_RATIO, rates.sample_rate);
        }
        (f64::from(closed_level), f64::from(open_level))
    }

    /// The next sample, from the metal's next sample. Hats
    /// that have died away return silence without doing the work, and the
    /// kit doesn't work the metal out for them.
    #[inline]
    pub(crate) fn next_sample(&mut self, metal: f64) -> f32 {
        let (closed_level, open_level) = self.controls();
        let (rates, derived) = (&self.rates, &self.derived);
        let state = &mut self.state;
        if !state.active {
            return 0.0;
        }

        // The envelopes: each dies away at its Decay, and the open hat in
        // a few milliseconds once a closed hit has cut it off.
        state.closed *= derived.closed_decay;
        state.open *= if state.choked {
            rates.choke
        } else {
            derived.open_decay
        };
        let envelope = state.closed * closed_level + state.open * open_level;
        state.envelope += rates.edge * (envelope - state.envelope);
        state.drive += rates.strength * (state.drive_target - state.drive);
        state.brightness += rates.strength * (state.brightness_target - state.brightness);

        // The band-pass, the amplifier and the high-pass.
        let (drive, envelope) = (state.drive, state.envelope);
        let out = self.filter(metal, |band| swing_vca(band * drive, envelope));

        let state = &mut self.state;
        // The band-pass isn't counted: it's before the amplifier, and follows
        // the metal, which never stops.
        if state.closed + state.open + state.envelope + state.high_pass.state() < SILENT {
            // Died away: stop, and stop doing work until the next hit.
            *state = State::silent();
        }
        out as f32 * OUTPUT_GAIN
    }

    /// The metal through the band-pass, `amplifier` and the high-pass.
    #[inline]
    fn filter(&mut self, metal: f64, amplifier: impl Fn(f64) -> f64) -> f64 {
        let (derived, state) = (&self.derived, &mut self.state);
        let band = state.band.process(metal, derived.band, BAND_Q).band_pass / BAND_Q;
        state
            .high_pass
            .process(amplifier(band), derived.high_pass, HIGH_PASS_Q)
            .high_pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::REFERENCE_PEAK;
    use crate::drums::metal::Metal;

    const RATE: f64 = 48_000.0;

    /// The metal for the hats at `RATE`, at the 808's tuning.
    fn metal() -> Metal {
        super::super::metal(ClosedHatSettings::default().tune_hz, RATE)
    }

    /// Runs `hats` for `seconds` on `metal`, as the kit does.
    fn run(hats: &mut Hats, metal: &mut Metal, seconds: f64) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|_| hats.next_sample(metal.next_sample()))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    fn hats() -> Hats {
        Hats::new(
            ClosedHatSettings::default(),
            OpenHatSettings::default(),
            RATE,
        )
    }

    /// The metal through the hats' filters, with the amplifier left out
    /// (it's the one part that isn't linear): the "hat path" of the
    /// research's aliasing measurement, at the default Tune and Tone.
    fn hat_path(samples: usize) -> Vec<f64> {
        let (mut hats, mut metal) = (hats(), metal());
        (0..samples)
            .map(|_| {
                hats.controls();
                hats.filter(metal.next_sample(), |band| band)
            })
            .collect()
    }

    /// The power spectrum of `samples` (a power of two long) under a 4-term
    /// Blackman-Harris window, up to half the sample rate.
    fn power_spectrum(samples: &[f64]) -> Vec<f64> {
        use std::f64::consts::TAU;
        let n = samples.len();
        let mut re: Vec<f64> = samples
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                let t = TAU * i as f64 / n as f64;
                let w = 0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos()
                    - 0.01168 * (3.0 * t).cos();
                x * w
            })
            .collect();
        let mut im = vec![0.0; n];
        // An iterative radix-2 FFT: bit-reversal, then butterflies.
        let bits = n.trailing_zeros();
        for i in 0..n {
            let j = i.reverse_bits() >> (usize::BITS - bits);
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut size = 2;
        while size <= n {
            let step = -TAU / size as f64;
            for start in (0..n).step_by(size) {
                for k in 0..size / 2 {
                    let (sin, cos) = (step * k as f64).sin_cos();
                    let (a, b) = (start + k, start + k + size / 2);
                    let (tr, ti) = (re[b] * cos - im[b] * sin, re[b] * sin + im[b] * cos);
                    (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                    (re[a], im[a]) = (re[a] + tr, im[a] + ti);
                }
            }
            size *= 2;
        }
        (0..n / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect()
    }

    /// How far the false tones (aliasing) sit under the hats' own sound, in
    /// dB, the way the research measured it: the metal through the hat
    /// path for 5.5 s, its spectrum split into the bins at the squares' true
    /// harmonics (within the window's main lobe of each) and the rest, from
    /// 20 Hz to 20 kHz, the band you can hear. The harmonics folded back from
    /// above half the sample rate land in the rest. It's a steady tone, so
    /// the amplifier's envelope doesn't come into it.
    fn signal_to_alias_db() -> f64 {
        const N: usize = 1 << 18;
        let settle = (0.1 * RATE) as usize;
        let samples = hat_path(settle + N);
        let power = power_spectrum(&samples[settle..]);
        let bin = |hz: f64| (hz * N as f64 / RATE).round() as usize;
        // The window's main lobe is 4 bins either side; a bin more for the
        // oscillators' tuning, which isn't a whole number of bins.
        let lobe = 5;
        let mut harmonic = vec![false; power.len()];
        for frequency in super::super::metal::FREQUENCIES {
            for k in (1..)
                .map(f64::from)
                .take_while(|k| k * frequency < RATE / 2.0)
            {
                let at = bin(k * frequency);
                harmonic[at - lobe..=(at + lobe).min(power.len() - 1)].fill(true);
            }
        }
        let (mut signal, mut alias) = (0.0, 0.0);
        for at in bin(20.0)..bin(20_000.0) {
            if harmonic[at] {
                signal += power[at];
            } else {
                alias += power[at];
            }
        }
        10.0 * (signal / alias).log10()
    }

    /// The false tones stay at or below what was measured here, so they
    /// can't quietly come back. Measured: 35.2 dB under the sound with
    /// PolyBLEP, against 11.6 dB for naive squares, as Plaits and the 808
    /// models make them, and 53.8 dB with PolyBLEP at 2×, which Will
    /// couldn't tell apart by ear (UTA-49). (The research measured 9, 18
    /// and 30 dB for the same three, against a 64× reference rather than by
    /// harmonic; the naive figures agree, and both say each step helps.)
    #[test]
    fn the_false_tones_stay_down() {
        let measured = 35.2;
        let db = signal_to_alias_db();
        assert!(db > measured - 0.5, "{db:.1} dB, measured {measured} dB");
    }

    #[test]
    fn the_closed_hat_at_full_accent_peaks_at_the_reference() {
        let mut hats = hats();
        hats.hit_closed(1.0);
        let peak = peak(&run(&mut hats, &mut metal(), 0.5));
        let db = 20.0 * (peak / REFERENCE_PEAK).log10();
        assert!(db.abs() < 0.1, "peak {peak}, {db:.2} dB");
    }

    /// Once they have died away far below hearing they stop, and do no work
    /// until the next hit.
    #[test]
    fn hats_that_have_died_away_stop() {
        let mut hats = Hats::new(
            ClosedHatSettings::default(),
            OpenHatSettings {
                decay_seconds: 0.6,
                ..OpenHatSettings::default()
            },
            RATE,
        );
        let mut metal = metal();
        assert!(!hats.is_sounding());
        hats.hit_open(1.0);
        let samples = run(&mut hats, &mut metal, 3.0);
        assert!(!hats.is_sounding(), "still ringing after 3 s");
        let stopped = samples.iter().rposition(|&s| s != 0.0).unwrap();
        let last = peak(&samples[stopped.saturating_sub(4800)..=stopped]);
        assert!(last < 1e-5, "last 0.1 s peaked at {last}");
        hats.hit_closed(1.0);
        assert!(peak(&run(&mut hats, &mut metal, 0.1)) > 0.2);
    }
}
