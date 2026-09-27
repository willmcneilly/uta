//! The ADSR envelope, worked out every sample so a note starts on its exact
//! sample whatever the block size.
//!
//! - **Attack** rises in a straight line from silence to full level.
//! - **Decay** falls on an exponential curve from full level to the sustain
//!   level.
//! - **Sustain** holds that level while the note is down.
//! - **Release** falls on an exponential curve from wherever the level is to
//!   silence.
//!
//! Each stage lands on its end level exactly when its time is up. The curves
//! aim a little past their end level and stop when they reach it, as in Nigel
//! Redmon's well-known ADSR, so they're exponential in shape yet take exactly
//! the set time rather than tailing off forever.
//!
//! Decay and release are tracked as a curve going from 1 to 0 and turned into
//! a level on each sample, so a sustain level that's moving (gliding to a new
//! setting) moves the decay with it instead of jumping when the decay ends.

/// How far past the end level decay and release aim, as a share of the
/// distance they fall. Smaller is more sharply exponential.
const OVERSHOOT: f64 = 0.001;

/// The stage times in samples, worked out once whenever the settings change
/// and shared by every voice. Times apply from the next stage a voice enters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Times {
    attack_samples: u32,
    decay: Curve,
    release: Curve,
}

impl Times {
    pub(crate) fn new(attack: f64, decay: f64, release: f64, sample_rate: f64) -> Self {
        let samples = |seconds: f64| ((seconds * sample_rate).round() as u32).max(1);
        Self {
            attack_samples: samples(attack),
            decay: Curve::new(samples(decay)),
            release: Curve::new(samples(release)),
        }
    }
}

/// A falling exponential curve from 1 to 0 over a set number of samples.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Curve {
    samples: u32,
    /// What the curve is multiplied by, around its aim point, each sample.
    coefficient: f64,
}

impl Curve {
    fn new(samples: u32) -> Self {
        let coefficient = (OVERSHOOT / (1.0 + OVERSHOOT)).powf(1.0 / f64::from(samples));
        Self {
            samples,
            coefficient,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// One voice's envelope.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Envelope {
    stage: Stage,
    /// Attack: the level, 0 to 1. Decay and release: the curve, 1 to 0.
    progress: f64,
    /// Attack: how much the level rises per sample. Decay and release: the
    /// curve's coefficient.
    rate: f64,
    /// Samples left in this stage.
    remaining: u32,
    /// The level the release started from.
    release_from: f64,
    /// The level of the last sample, where a release starts from.
    level: f64,
}

impl Envelope {
    pub(crate) fn stage(&self) -> Stage {
        self.stage
    }

    /// Starts from silence at the beginning of the attack.
    pub(crate) fn start(&mut self, times: &Times) {
        *self = Self {
            stage: Stage::Attack,
            progress: 0.0,
            rate: 1.0 / f64::from(times.attack_samples),
            remaining: times.attack_samples,
            release_from: 0.0,
            level: 0.0,
        };
    }

    /// Starts the release from the current level. Does nothing if it's
    /// already releasing or idle.
    pub(crate) fn release(&mut self, times: &Times) {
        if matches!(self.stage, Stage::Release | Stage::Idle) {
            return;
        }
        self.stage = Stage::Release;
        self.release_from = self.level;
        self.enter_curve(times.release);
    }

    /// Silences it at once. For a voice that has already faded out.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// The level for this sample, then advances by one. `sustain` is the
    /// sustain level on this sample.
    #[inline]
    pub(crate) fn next_level(&mut self, sustain: f64, times: &Times) -> f64 {
        let level = match self.stage {
            Stage::Idle => 0.0,
            Stage::Attack => self.progress,
            Stage::Decay => sustain + (1.0 - sustain) * self.progress,
            Stage::Sustain => sustain,
            Stage::Release => self.release_from * self.progress,
        };
        self.level = level;
        self.advance(times);
        level
    }

    fn advance(&mut self, times: &Times) {
        match self.stage {
            Stage::Idle | Stage::Sustain => return,
            Stage::Attack => self.progress += self.rate,
            Stage::Decay | Stage::Release => {
                self.progress = -OVERSHOOT + (self.progress + OVERSHOOT) * self.rate;
            }
        }
        self.remaining -= 1;
        if self.remaining == 0 {
            match self.stage {
                Stage::Attack => {
                    self.stage = Stage::Decay;
                    self.enter_curve(times.decay);
                }
                Stage::Decay => {
                    self.stage = Stage::Sustain;
                    self.progress = 0.0;
                }
                Stage::Release => self.reset(),
                Stage::Idle | Stage::Sustain => {}
            }
        }
    }

    fn enter_curve(&mut self, curve: Curve) {
        self.progress = 1.0;
        self.rate = curve.coefficient;
        self.remaining = curve.samples;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 1_000.0;

    fn levels(envelope: &mut Envelope, times: &Times, samples: usize) -> Vec<f64> {
        (0..samples)
            .map(|_| envelope.next_level(0.5, times))
            .collect()
    }

    #[test]
    fn stages_take_their_set_times() {
        // At 1 kHz: 10 samples of attack, 20 of decay, 40 of release.
        let times = Times::new(0.01, 0.02, 0.04, RATE);
        let mut envelope = Envelope::default();
        envelope.start(&times);

        let attack = levels(&mut envelope, &times, 10);
        assert_eq!(attack[0], 0.0);
        assert!((attack[5] - 0.5).abs() < 1e-12, "a straight line");
        // Full level on the first sample after the attack, then decaying.
        let decay = levels(&mut envelope, &times, 20);
        assert!((decay[0] - 1.0).abs() < 1e-12);
        assert!(decay.windows(2).all(|pair| pair[1] < pair[0]));
        assert_eq!(envelope.stage(), Stage::Sustain);
        assert_eq!(levels(&mut envelope, &times, 5), [0.5; 5]);

        envelope.release(&times);
        let release = levels(&mut envelope, &times, 40);
        assert_eq!(release[0], 0.5);
        assert!(release.windows(2).all(|pair| pair[1] < pair[0]));
        assert!(release[39] > 0.0);
        assert_eq!(envelope.stage(), Stage::Idle);
        assert_eq!(envelope.next_level(0.5, &times), 0.0);
    }

    #[test]
    fn decay_follows_a_moving_sustain_level() {
        let times = Times::new(0.001, 0.02, 0.04, RATE);
        let mut envelope = Envelope::default();
        envelope.start(&times);
        envelope.next_level(0.5, &times);
        let mut last = 0.0;
        for i in 0..40 {
            // Sustain slides from 0.5 to 0.2 while decaying.
            let sustain = 0.5 - 0.3 * (f64::from(i) / 40.0).min(1.0);
            let level = envelope.next_level(sustain, &times);
            if i > 0 {
                assert!((level - last).abs() < 0.2, "jumped at {i}");
            }
            last = level;
        }
        assert_eq!(envelope.stage(), Stage::Sustain);
    }

    #[test]
    fn release_during_attack_starts_from_the_current_level() {
        let times = Times::new(0.01, 0.02, 0.04, RATE);
        let mut envelope = Envelope::default();
        envelope.start(&times);
        let attack = levels(&mut envelope, &times, 4);
        envelope.release(&times);
        let first = envelope.next_level(0.5, &times);
        assert_eq!(first, attack[3]);
    }
}
