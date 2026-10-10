//! The 808's bridged-T resonator, which the kick and the toms share: a
//! circuit that rings at its own pitch when a pulse hits it, and dies away.
//! See RFC-006, "How each sound is made".
//!
//! Ported from Emilie Gillet's `AnalogBassDrum` in Plaits (MIT licence): a
//! trapezoidal state-variable filter (Andrew Simper's, as Plaits' is), whose
//! band-pass output is the ring. It never restarts: a hit only adds energy
//! to what's already ringing.
//!
//! Where it differs from Plaits: its Q follows its frequency, so it dies
//! away by 40 dB in the Decay time at any pitch.

use std::f64::consts::PI;

/// One resonator. Everything in it is a plain number.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Resonator {
    s1: f64,
    s2: f64,
}

/// A resonator's outputs for one sample.
pub(crate) struct ResonatorOut {
    /// The ring.
    pub(crate) band_pass: f64,
    pub(crate) low_pass: f64,
}

impl Resonator {
    /// Rings with `input` at `frequency_hz`, with the Q that makes it die
    /// away with the time constant `tau_seconds`.
    #[inline]
    pub(crate) fn process(
        &mut self,
        input: f64,
        frequency_hz: f64,
        tau_seconds: f64,
        sample_rate: f64,
    ) -> ResonatorOut {
        let g = tan(PI * frequency_hz / sample_rate);
        let r = 1.0 / (PI * frequency_hz * tau_seconds).max(0.5);
        let h = 1.0 / (1.0 + r * g + g * g);
        let hp = (input - r * self.s1 - g * self.s1 - self.s2) * h;
        let band_pass = g * hp + self.s1;
        self.s1 = g * hp + band_pass;
        let low_pass = g * band_pass + self.s2;
        self.s2 = g * band_pass + low_pass;
        ResonatorOut {
            band_pass,
            low_pass,
        }
    }

    /// How much is still ringing in it: zero when it's at rest.
    pub(crate) fn state(&self) -> f64 {
        self.s1.abs() + self.s2.abs()
    }
}

/// tan(x), by its series where it's accurate to better than one part in a
/// million: the resonator's angle stays under 0.02 at 48 kHz, for the kick's
/// attack shift and the toms' bend alike, so it never needs the library's
/// `tan`.
#[inline]
fn tan(x: f64) -> f64 {
    if x < 0.2 {
        let x2 = x * x;
        x * (1.0 + x2 * (1.0 / 3.0 + x2 * (2.0 / 15.0 + x2 * (17.0 / 315.0))))
    } else {
        x.tan()
    }
}
