//! A track's chain on the audio thread: its effects' states in a fixed
//! table, in chain order, and what every effect gets from the chain rather
//! than doing itself: on/off and Mix. See RFC-007, "The chain" and
//! "Patterns for every effect after this one", points 4 and 6.

use rtrb::Producer;
use uta_core::{EffectId, MAX_EFFECTS};

use super::{EffectSnapshot, EffectState};
use crate::ramp::Ramp;
use crate::{TRACK_BUFFER_FRAMES, VOLUME_SMOOTHING_SECONDS};

/// A new effect's state on its way to the audio thread, with the snapshot
/// that first has the effect. The audio thread takes the state, and the
/// empty `Arriving` goes back with the snapshot.
#[derive(Debug)]
pub(crate) struct Arriving {
    /// The slot whose track's chain has the effect.
    pub slot: usize,
    pub id: EffectId,
    pub state: Option<Box<EffectState>>,
}

/// The effects of the track in one slot. Everything here is set aside when
/// the processor is made, apart from the states, which arrive made.
pub(crate) struct Chain {
    /// The effects' states, in chain order, each found by its effect's ID.
    /// Places past the end of the chain are empty.
    effects: [Option<Held>; MAX_EFFECTS],
    /// What went into the effect being processed, to blend back in by its
    /// Mix.
    dry: [[f32; TRACK_BUFFER_FRAMES]; 2],
    sample_rate: f64,
}

/// One effect's state, which the audio thread owns while the effect is in
/// the chain, and what the chain does for it.
struct Held {
    id: EffectId,
    state: Box<EffectState>,
    enabled: bool,
    mix: Mix,
}

impl Chain {
    /// A chain of `effects`, with a new state made for each. Allocates:
    /// control side only.
    pub(crate) fn new(effects: &[EffectSnapshot], sample_rate: f64) -> Self {
        let mut chain = Self {
            effects: std::array::from_fn(|_| None),
            dry: [[0.0; TRACK_BUFFER_FRAMES]; 2],
            sample_rate,
        };
        for (place, effect) in chain.effects.iter_mut().zip(effects) {
            *place = Some(Held::new(
                effect,
                EffectState::new(&effect.settings, sample_rate),
                sample_rate,
            ));
        }
        chain
    }

    /// Takes on the chain `effects` from a new snapshot, for the track in
    /// `slot`. Real-time safe:
    /// - each state whose effect is gone is sent back on `used`, never
    ///   freed here. The processor checks there's room before the swap;
    /// - each state is moved to its effect's place in the chain, and glides
    ///   to its new settings and Mix;
    /// - a new effect's state is taken from `arriving`. If it didn't arrive
    ///   (it always does), the effect plays dry.
    ///
    /// Bounded by [`MAX_EFFECTS`] squared, plus the states arriving.
    pub(crate) fn load(
        &mut self,
        slot: usize,
        effects: &[EffectSnapshot],
        arriving: &mut [Arriving],
        used: &mut Producer<Box<EffectState>>,
    ) {
        for place in &mut self.effects {
            if place
                .as_ref()
                .is_some_and(|held| !effects.iter().any(|effect| effect.id == held.id))
                && let Some(held) = place.take()
            {
                // Checked by the processor: there is room, so this never
                // drops a state.
                let _ = used.push(held.state);
            }
        }
        for (index, effect) in effects.iter().enumerate().take(MAX_EFFECTS) {
            let found = (index..MAX_EFFECTS).find(|&at| {
                self.effects[at]
                    .as_ref()
                    .is_some_and(|held| held.id == effect.id)
            });
            if let Some(at) = found {
                self.effects.swap(index, at);
                if let Some(held) = &mut self.effects[index] {
                    held.state.set_settings(&effect.settings);
                    held.enabled = effect.enabled;
                    held.mix.set(effect.mix);
                }
                continue;
            }
            // Every state still held belongs to an effect after this one,
            // so there's an empty place at or after it.
            let Some(empty) = (index..MAX_EFFECTS).find(|&at| self.effects[at].is_none()) else {
                continue;
            };
            self.effects.swap(index, empty);
            if let Some(mut state) = arriving
                .iter_mut()
                .find(|arriving| arriving.slot == slot && arriving.id == effect.id)
                .and_then(|arriving| arriving.state.take())
            {
                // Made for the rate the controller last heard of, which a
                // device switch may have changed since.
                state.prepare(self.sample_rate);
                // The place is empty, so nothing is dropped.
                self.effects[index] = Some(Held::new(effect, state, self.sample_rate));
            }
        }
    }

    /// Runs the first `frames` frames of `buffer`, the track's left and
    /// right, through each effect that's on, in order. Each blends its
    /// output with its input by its Mix.
    pub(crate) fn process(&mut self, buffer: &mut [[f32; TRACK_BUFFER_FRAMES]; 2], frames: usize) {
        let [left, right] = buffer;
        let (left, right) = (&mut left[..frames], &mut right[..frames]);
        let [dry_left, dry_right] = &mut self.dry;
        let (dry_left, dry_right) = (&mut dry_left[..frames], &mut dry_right[..frames]);
        // Bounded by MAX_EFFECTS.
        for held in self.effects.iter_mut().flatten() {
            if !held.enabled {
                continue;
            }
            dry_left.copy_from_slice(left);
            dry_right.copy_from_slice(right);
            held.state.process(left, right);
            held.mix.blend([dry_left, dry_right], [left, right]);
        }
    }

    /// Whether any effect that's on is still ringing.
    pub(crate) fn is_ringing(&self) -> bool {
        self.effects
            .iter()
            .flatten()
            .any(|held| held.enabled && held.state.is_ringing())
    }

    /// Gets every effect ready to run at `sample_rate`. Called between
    /// streams, on the control side.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        for held in self.effects.iter_mut().flatten() {
            held.state.prepare(sample_rate);
            held.mix = Mix::new(held.mix.target, sample_rate);
        }
    }

    /// How many states it holds.
    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        self.effects.iter().flatten().count()
    }
}

impl Held {
    fn new(effect: &EffectSnapshot, state: Box<EffectState>, sample_rate: f64) -> Self {
        Self {
            id: effect.id,
            state,
            enabled: effect.enabled,
            mix: Mix::new(effect.mix, sample_rate),
        }
    }
}

/// An effect's Mix, gliding to each new value, with its equal-power gains:
/// the dry sound's falls as a quarter of a cosine and the wet sound's rises
/// as a quarter of a sine, so halfway both are at about −3 dB and the blend
/// doesn't dip in loudness. At 0 it's the dry sound exactly, and at 1 the
/// wet sound exactly.
struct Mix {
    ramp: Ramp,
    target: f32,
    /// The gains at the ramp's value, while it isn't moving.
    gains: [f32; 2],
}

impl Mix {
    fn new(mix: f32, sample_rate: f64) -> Self {
        let mix = clamp_mix(mix);
        Self {
            ramp: Ramp::new(mix, (VOLUME_SMOOTHING_SECONDS * sample_rate).round() as u32),
            target: mix,
            gains: equal_power(mix),
        }
    }

    fn set(&mut self, mix: f32) {
        self.target = clamp_mix(mix);
        self.ramp.set_target(self.target);
    }

    /// Blends `dry` into `wet` in place.
    fn blend(&mut self, dry: [&[f32]; 2], wet: [&mut [f32]; 2]) {
        if !self.ramp.is_moving() {
            let [dry_gain, wet_gain] = self.gains;
            for (dry, wet) in dry.into_iter().zip(wet) {
                if wet_gain == 0.0 {
                    wet.copy_from_slice(dry);
                } else if dry_gain != 0.0 {
                    for (dry, wet) in dry.iter().zip(wet.iter_mut()) {
                        *wet = dry * dry_gain + *wet * wet_gain;
                    }
                }
            }
            return;
        }
        let [dry_left, dry_right] = dry;
        let [wet_left, wet_right] = wet;
        for (((dry_left, dry_right), wet_left), wet_right) in dry_left
            .iter()
            .zip(dry_right.iter())
            .zip(wet_left.iter_mut())
            .zip(wet_right.iter_mut())
        {
            let [dry_gain, wet_gain] = equal_power(self.ramp.next_value());
            *wet_left = dry_left * dry_gain + *wet_left * wet_gain;
            *wet_right = dry_right * dry_gain + *wet_right * wet_gain;
        }
        self.gains = equal_power(self.ramp.value());
    }
}

/// A Mix kept from 0 to 1. NaN is the sound unchanged.
fn clamp_mix(mix: f32) -> f32 {
    if mix.is_nan() {
        0.0
    } else {
        mix.clamp(0.0, 1.0)
    }
}

/// The dry and wet gains for `mix`: exactly 1 and 0 at 0, and 0 and 1 at 1.
fn equal_power(mix: f32) -> [f32; 2] {
    if mix <= 0.0 {
        [1.0, 0.0]
    } else if mix >= 1.0 {
        [0.0, 1.0]
    } else {
        let angle = mix * std::f32::consts::FRAC_PI_2;
        [angle.cos(), angle.sin()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{DelaySettings, EffectSettings};

    fn delay(n: u128, mix: f32) -> EffectSnapshot {
        EffectSnapshot {
            id: EffectId::from_uuid(uuid::Uuid::from_u128(n)),
            enabled: true,
            mix,
            settings: EffectSettings::Delay(DelaySettings {
                time_seconds: 0.001,
                feedback: 0.0,
            }),
        }
    }

    #[test]
    fn the_blend_is_equal_power_and_exact_at_its_ends() {
        assert_eq!(equal_power(0.0), [1.0, 0.0]);
        assert_eq!(equal_power(1.0), [0.0, 1.0]);
        let [dry, wet] = equal_power(0.5);
        assert!((dry * dry + wet * wet - 1.0).abs() < 1e-6);
        assert!((dry - wet).abs() < 1e-6);
        assert_eq!(clamp_mix(f32::NAN), 0.0);
    }

    #[test]
    fn states_move_with_their_effects_and_removed_ones_go_back() {
        let rate = 48_000.0;
        let (a, b, c) = (delay(1, 0.5), delay(2, 0.5), delay(3, 0.5));
        let mut chain = Chain::new(&[a, b], rate);
        let (mut used, mut returned) = rtrb::RingBuffer::new(MAX_EFFECTS);
        let mut arriving = [Arriving {
            slot: 4,
            id: c.id,
            state: Some(EffectState::new(&c.settings, rate)),
        }];
        // B moves to the front, A goes, and C arrives at the end.
        chain.load(4, &[b, c], &mut arriving, &mut used);
        let order: Vec<_> = chain.effects.iter().flatten().map(|held| held.id).collect();
        assert_eq!(order, [b.id, c.id]);
        assert!(arriving[0].state.is_none(), "C's state was taken");
        assert!(returned.pop().is_ok(), "A's state came back");
        assert!(returned.pop().is_err());

        // A state for another slot isn't taken.
        let mut elsewhere = [Arriving {
            slot: 5,
            id: a.id,
            state: Some(EffectState::new(&a.settings, rate)),
        }];
        chain.load(4, &[b, c, a], &mut elsewhere, &mut used);
        assert!(elsewhere[0].state.is_some());
        assert_eq!(chain.held(), 2);

        // A slot whose track has gone sends back every state.
        chain.load(4, &[], &mut [], &mut used);
        assert_eq!(chain.held(), 0);
        assert_eq!(returned.slots(), 2);
    }
}
