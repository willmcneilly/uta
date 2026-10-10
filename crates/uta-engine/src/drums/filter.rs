//! The filters the noise sounds use, as plain numbers: a one-pole and a
//! state-variable filter, both "topology-preserving" (Vadim Zavalishin's
//! "The Art of VA Filter Design"), as Plaits' are. Their cutoffs are
//! prewarped, so a filter set to 9 kHz is at 9 kHz, not somewhere under it,
//! and they stay stable when their cutoff moves every sample. See the
//! research's "Techniques every voice needs", point 3.

use std::f64::consts::PI;

/// The prewarped gain for a cutoff, `tan(π f / rate)`. The cutoff is kept
/// under 0.45 of the sample rate, where `tan` stays finite.
pub(crate) fn prewarp(cutoff_hz: f64, sample_rate: f64) -> f64 {
    (PI * cutoff_hz.clamp(1.0, 0.45 * sample_rate) / sample_rate).tan()
}

/// A one-pole filter: a gentle 6 dB an octave.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OnePole {
    state: f64,
}

impl OnePole {
    /// The low-pass output for `input`, with `g` from [`prewarp`].
    #[inline]
    pub(crate) fn low_pass(&mut self, input: f64, g: f64) -> f64 {
        let v = (input - self.state) * g / (1.0 + g);
        let out = v + self.state;
        self.state = out + v;
        out
    }

    /// The high-pass output for `input`, with `g` from [`prewarp`].
    #[inline]
    pub(crate) fn high_pass(&mut self, input: f64, g: f64) -> f64 {
        input - self.low_pass(input, g)
    }

    pub(crate) fn state(&self) -> f64 {
        self.state
    }
}

/// A state-variable filter: 12 dB an octave, with a resonance.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Svf {
    s1: f64,
    s2: f64,
}

/// A state-variable filter's outputs for one sample.
pub(crate) struct SvfOut {
    pub(crate) low_pass: f64,
    pub(crate) band_pass: f64,
    pub(crate) high_pass: f64,
}

impl Svf {
    /// Filters `input`, with `g` from [`prewarp`] and quality `q` (0.5 is
    /// no resonance, higher rings more).
    #[inline]
    pub(crate) fn process(&mut self, input: f64, g: f64, q: f64) -> SvfOut {
        let r = 1.0 / q;
        let h = 1.0 / (1.0 + r * g + g * g);
        let hp = (input - r * self.s1 - g * self.s1 - self.s2) * h;
        let band_pass = g * hp + self.s1;
        self.s1 = g * hp + band_pass;
        let low_pass = g * band_pass + self.s2;
        self.s2 = g * band_pass + low_pass;
        SvfOut {
            low_pass,
            band_pass,
            high_pass: hp,
        }
    }

    /// How much is still ringing in it: zero when it's at rest.
    pub(crate) fn state(&self) -> f64 {
        self.s1.abs() + self.s2.abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    /// The gain of a filter at `hz`, from a steady sine through it.
    fn gain(mut filter: impl FnMut(f64) -> f64, hz: f64) -> f64 {
        let samples = (RATE * 0.5) as usize;
        let mut peak: f64 = 0.0;
        for n in 0..samples {
            let out = filter((2.0 * PI * hz * n as f64 / RATE).sin());
            if n > samples / 2 {
                peak = peak.max(out.abs());
            }
        }
        peak
    }

    /// Prewarped, each filter is exactly at its cutoff, even at 9 kHz, where
    /// one that wasn't would be about 10% low.
    #[test]
    fn the_cutoffs_are_where_they_are_set() {
        for cutoff in [200.0, 1000.0, 9000.0] {
            let g = prewarp(cutoff, RATE);
            let mut one_pole = OnePole::default();
            let at_cutoff = gain(|x| one_pole.low_pass(x, g), cutoff);
            assert!(
                (at_cutoff - 0.5f64.sqrt()).abs() < 0.01,
                "{cutoff}: {at_cutoff}"
            );
            let mut one_pole = OnePole::default();
            let at_cutoff = gain(|x| one_pole.high_pass(x, g), cutoff);
            assert!(
                (at_cutoff - 0.5f64.sqrt()).abs() < 0.01,
                "{cutoff}: {at_cutoff}"
            );
            // The band-pass peaks at its centre, at a gain of 1.
            let mut svf = Svf::default();
            let centre = gain(|x| svf.process(x, g, 2.0).band_pass / 2.0, cutoff);
            assert!((centre - 1.0).abs() < 0.01, "{cutoff}: {centre}");
            let mut svf = Svf::default();
            let octave_up = gain(|x| svf.process(x, g, 2.0).band_pass / 2.0, cutoff * 2.0);
            assert!(octave_up < 0.5, "{cutoff}: {octave_up}");
            // The high-pass is at -3 dB at its cutoff with no resonance
            // (quality 0.5^0.5), and passes an octave up.
            let q = 0.5f64.sqrt();
            let mut svf = Svf::default();
            let at_cutoff = gain(|x| svf.process(x, g, q).high_pass, cutoff);
            assert!(
                (at_cutoff - 0.5f64.sqrt()).abs() < 0.01,
                "{cutoff}: {at_cutoff}"
            );
            let mut svf = Svf::default();
            let octave_down = gain(|x| svf.process(x, g, q).high_pass, cutoff / 2.0);
            assert!(octave_down < 0.3, "{cutoff}: {octave_down}");
        }
    }
}
