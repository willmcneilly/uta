//! A chain on the audio thread, a track's or the master's: its effects'
//! states in a fixed table, in the order the sound goes through them, and
//! what every effect gets from the chain rather than doing itself: on/off,
//! Mix, the fades when one is added, removed, moved or switched, and
//! sleeping. See RFC-007, "The chain", "Stopping, switching off, and tails"
//! and "Patterns for every effect after this one", points 4 to 7.
//!
//! **Presence.** Each effect has a presence, from 1 (it's in the sound) to
//! 0 (the sound passes it by), which fades over [`EFFECT_FADE_SECONDS`]:
//! - added, it fades in; removed, it fades out where it was, then its state
//!   goes back to the control side;
//! - switched off, it fades out, is cleared, and does no work; switched on,
//!   it fades in from silence;
//! - moved, it fades out where it was and back in where it lands, keeping
//!   its state, so a delay keeps its echoes.
//!
//! While an effect fades, what goes into it fades too, so it never hears
//! the sound start or stop with a jump: a delay added to a playing track
//! echoes a fade-in, not a click. The chain splits each block at the sample
//! where a fade ends, so moving, clearing and handing back happen on that
//! sample whatever the block size.

use rtrb::Producer;
use uta_core::{EffectId, MAX_EFFECTS};

use super::{EffectSettings, EffectSnapshot, EffectState};
use crate::ramp::Ramp;
use crate::{TRACK_BUFFER_FRAMES, VOLUME_SMOOTHING_SECONDS};

/// How long an effect takes to fade in when it's added or switched on, and
/// out when it's removed or switched off. A move is a fade out and a fade
/// in. Short enough not to notice, long enough not to click (RFC-007, open
/// question 3).
pub const EFFECT_FADE_SECONDS: f64 = 0.01;

/// The most effect states a chain holds at once: its effects, and as many
/// again fading out after being removed. If edits remove effects faster
/// than they fade, the oldest removed one is handed back at once to make
/// room.
pub const CHAIN_CAPACITY: usize = 2 * MAX_EFFECTS;

/// Which chain on the audio thread: the one in a track slot, or the
/// master's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChainAt {
    Slot(usize),
    Master,
}

/// A new effect's state on its way to the audio thread, with the snapshot
/// that first has the effect. The audio thread takes the state, and the
/// empty `Arriving` goes back with the snapshot.
#[derive(Debug)]
pub(crate) struct Arriving {
    /// The chain that has the effect.
    pub at: ChainAt,
    pub id: EffectId,
    pub state: Option<Box<EffectState>>,
}

/// The effects of one chain. Everything here is set aside when the
/// processor is made, apart from the states, which arrive made.
pub(crate) struct Chain {
    /// The effects' states in the order the sound goes through them: the
    /// chain's effects, with any fading out where they were. The first
    /// `count` places are full and the rest empty.
    held: [Option<Held>; CHAIN_CAPACITY],
    count: usize,
    /// What went into the effect being processed, to blend back in.
    dry: [[f32; TRACK_BUFFER_FRAMES]; 2],
    sample_rate: f64,
}

/// One effect's state, which the audio thread owns while it's in the
/// chain, and what the chain does for it.
struct Held {
    id: EffectId,
    state: Box<EffectState>,
    /// Its settings, kept to make its state again at a new sample rate.
    settings: EffectSettings,
    /// Its place in the chain the last snapshot brought, or `None` once it
    /// has been removed: it fades out where it is, then goes back.
    place: Option<usize>,
    enabled: bool,
    /// Fading out where it is, to fade back in at its place.
    moving: bool,
    /// Cleared since it was last switched off.
    cleared: bool,
    mix: Mix,
    /// How much it's in the sound, from 0 to 1.
    presence: Ramp,
}

impl Chain {
    /// A chain of `effects`, with a new state made for each, already in the
    /// sound: nothing fades in. Allocates: control side only.
    pub(crate) fn new(effects: &[EffectSnapshot], sample_rate: f64) -> Self {
        let mut chain = Self {
            held: std::array::from_fn(|_| None),
            count: 0,
            dry: [[0.0; TRACK_BUFFER_FRAMES]; 2],
            sample_rate,
        };
        for (place, effect) in effects.iter().take(MAX_EFFECTS).enumerate() {
            let state = EffectState::new(&effect.settings, sample_rate);
            let mut held = Held::new(effect, place, state, sample_rate);
            held.presence.jump_to(held.target());
            chain.held[place] = Some(held);
            chain.count += 1;
        }
        chain
    }

    /// Takes on the chain `effects` from a new snapshot. Real-time safe:
    /// - each effect that's gone starts fading out where it is;
    /// - each that's still there glides to its new settings and Mix, and
    ///   fades out or in if it was switched;
    /// - the fewest effects that have to move to put the chain in its new
    ///   order fade out, to fade in at their new places;
    /// - a new effect's state is taken from `arriving`, and fades in at its
    ///   place. If it didn't arrive (it always does), the effect plays dry.
    ///
    /// A state is only sent back on `used` here if the table is full of
    /// effects fading out; the processor checks there's room before the
    /// swap. Bounded by [`CHAIN_CAPACITY`] squared, plus the states
    /// arriving.
    pub(crate) fn load(
        &mut self,
        at: ChainAt,
        effects: &[EffectSnapshot],
        arriving: &mut [Arriving],
        used: &mut Producer<Box<EffectState>>,
    ) {
        let effects = &effects[..effects.len().min(MAX_EFFECTS)];
        for held in self.held[..self.count].iter_mut().flatten() {
            if held.place.is_none() {
                // Removed: an effect with its ID coming back is a new one,
                // with a new state.
                continue;
            }
            match effects.iter().position(|effect| effect.id == held.id) {
                Some(place) => held.update(&effects[place], place),
                None => {
                    held.place = None;
                    held.moving = false;
                }
            }
            held.presence.set_target(held.target());
        }
        self.find_moves();
        for (place, effect) in effects.iter().enumerate() {
            if self.present(effect.id) {
                continue;
            }
            let Some(mut state) = arriving
                .iter_mut()
                .find(|arriving| arriving.at == at && arriving.id == effect.id)
                .and_then(|arriving| arriving.state.take())
            else {
                continue;
            };
            // Made for the rate the controller last heard of, which a
            // device switch may have changed since.
            state.prepare(self.sample_rate);
            if self.count == CHAIN_CAPACITY && !self.make_room(used) {
                // Never happens: the processor checked there's room. The
                // state goes back with the swap rather than being freed.
                if let Some(arriving) = arriving
                    .iter_mut()
                    .find(|arriving| arriving.at == at && arriving.id == effect.id)
                {
                    arriving.state = Some(state);
                }
                continue;
            }
            self.insert(Held::new(effect, place, state, self.sample_rate));
        }
        self.settle(used);
    }

    /// Runs the first `frames` frames of `buffer`, the chain's left and
    /// right, through each effect in turn. Each blends its output with its
    /// input by its Mix, and with the input again by its presence while it
    /// fades. An effect that's off and faded out, or asleep while silence
    /// goes in, does no work. Sends back each removed effect's state once
    /// it has faded out.
    ///
    /// Bounded by the block, times [`CHAIN_CAPACITY`] runs at most: the
    /// block is split where a fade ends.
    pub(crate) fn process(
        &mut self,
        buffer: &mut [[f32; TRACK_BUFFER_FRAMES]; 2],
        frames: usize,
        used: &mut Producer<Box<EffectState>>,
    ) {
        self.settle(used);
        let mut start = 0;
        // Each run ends at least one fade, or the block.
        while start < frames {
            let run = self.held[..self.count]
                .iter()
                .flatten()
                .filter(|held| held.presence.is_moving())
                .map(|held| held.presence.remaining() as usize)
                .fold(frames - start, usize::min);
            self.process_run(buffer, start..start + run);
            self.settle(used);
            start += run;
        }
    }

    /// [`Chain::process`] for one run, within which no fade ends.
    fn process_run(
        &mut self,
        buffer: &mut [[f32; TRACK_BUFFER_FRAMES]; 2],
        run: std::ops::Range<usize>,
    ) {
        let [left, right] = buffer;
        let (left, right) = (&mut left[run.clone()], &mut right[run.clone()]);
        let [dry_left, dry_right] = &mut self.dry;
        let (dry_left, dry_right) = (&mut dry_left[run.clone()], &mut dry_right[run]);
        // Whether what goes into the next effect is silence.
        let mut silent = is_silent(left) && is_silent(right);
        // Bounded by CHAIN_CAPACITY.
        for held in self.held[..self.count].iter_mut().flatten() {
            let fading = held.presence.is_moving();
            if !fading && held.presence.value() == 0.0 {
                // Off, or faded out: the sound passes it by.
                continue;
            }
            if silent && !fading && !held.mix.is_moving() && !held.state.is_ringing() {
                // Asleep: given silence, it would give silence.
                continue;
            }
            dry_left.copy_from_slice(left);
            dry_right.copy_from_slice(right);
            if fading {
                // What goes in fades with it, so it never hears a jump.
                let mut presence = held.presence;
                for (left, right) in left.iter_mut().zip(right.iter_mut()) {
                    let gain = presence.next_value();
                    *left *= gain;
                    *right *= gain;
                }
            }
            held.state.process(left, right);
            held.mix
                .blend([dry_left, dry_right], [left, right], &mut held.presence);
            silent = false;
        }
    }

    /// Finishes each fade that has ended: a moved effect goes to its new
    /// place and fades in, a removed one goes back on `used` (or waits, if
    /// there's no room), and one switched off is cleared. Bounded by
    /// [`CHAIN_CAPACITY`] squared.
    fn settle(&mut self, used: &mut Producer<Box<EffectState>>) {
        let mut index = 0;
        while index < self.count {
            let Some(held) = &mut self.held[index] else {
                break;
            };
            if held.presence.is_moving() || held.presence.value() != 0.0 {
                index += 1;
                continue;
            }
            if held.place.is_none() {
                if self.hand_back(index, used) {
                    continue;
                }
            } else if held.moving {
                held.moving = false;
                held.presence.set_target(held.target());
                let held = self.remove(index);
                let to = self.insert(held);
                if to > index {
                    // Something new is at `index` now.
                    continue;
                }
            } else if !held.enabled && !held.cleared {
                held.state.clear();
                held.mix.finish();
                held.cleared = true;
            }
            index += 1;
        }
    }

    /// Marks the fewest effects that have to move to put the chain in the
    /// order of their places: every one not in the longest run already in
    /// order, preferring effects that aren't already moving. An effect
    /// that's back in order stops moving and fades back in. Bounded by
    /// [`CHAIN_CAPACITY`] squared.
    fn find_moves(&mut self) {
        // The present effects, in the order the sound goes through them.
        let mut at = [0usize; CHAIN_CAPACITY];
        let mut places = [0usize; CHAIN_CAPACITY];
        let mut present = 0;
        for (index, held) in self.held[..self.count].iter().enumerate() {
            if let Some(Held {
                place: Some(place), ..
            }) = held
            {
                at[present] = index;
                places[present] = *place;
                present += 1;
            }
        }
        // The heaviest run with places in increasing order. Each effect
        // weighs more than every effect already moving put together, so the
        // run is always one of the longest.
        let weight = |n: usize| {
            let moving = self.held[at[n]].as_ref().is_some_and(|held| held.moving);
            (CHAIN_CAPACITY + 1) as u32 + u32::from(!moving)
        };
        let mut best = [0u32; CHAIN_CAPACITY];
        let mut before = [usize::MAX; CHAIN_CAPACITY];
        for n in 0..present {
            best[n] = weight(n);
            for m in 0..n {
                if places[m] < places[n] && best[m] + weight(n) > best[n] {
                    best[n] = best[m] + weight(n);
                    before[n] = m;
                }
            }
        }
        let mut in_order = [false; CHAIN_CAPACITY];
        let mut last = (0..present).max_by_key(|&n| (best[n], usize::MAX - n));
        while let Some(n) = last {
            in_order[n] = true;
            last = (before[n] != usize::MAX).then_some(before[n]);
        }
        for n in 0..present {
            if let Some(held) = &mut self.held[at[n]] {
                held.moving = !in_order[n];
                held.presence.set_target(held.target());
            }
        }
    }

    /// Whether an effect with this ID is in the chain, and not on its way
    /// out.
    fn present(&self, id: EffectId) -> bool {
        self.held[..self.count]
            .iter()
            .flatten()
            .any(|held| held.id == id && held.place.is_some())
    }

    /// Puts `held` where its place says, among the effects that are where
    /// they belong: before the first with a later place. Returns where it
    /// went. There must be room.
    fn insert(&mut self, held: Held) -> usize {
        let place = held.place.unwrap_or(usize::MAX);
        let to = self.held[..self.count]
            .iter()
            .position(|other| {
                other.as_ref().is_some_and(|other| {
                    !other.moving && other.place.is_some_and(|other| other > place)
                })
            })
            .unwrap_or(self.count);
        self.held[self.count] = Some(held);
        self.held[to..=self.count].rotate_right(1);
        self.count += 1;
        to
    }

    /// Takes the effect at `index` out of the table, closing the gap.
    fn remove(&mut self, index: usize) -> Held {
        self.held[index..self.count].rotate_left(1);
        self.count -= 1;
        self.held[self.count]
            .take()
            .expect("the table's first places are full")
    }

    /// Sends the state at `index` back on `used`, if there's room. Returns
    /// whether it went.
    fn hand_back(&mut self, index: usize, used: &mut Producer<Box<EffectState>>) -> bool {
        if used.is_full() {
            return false;
        }
        let held = self.remove(index);
        // Checked above, so this never drops the state.
        let _ = used.push(held.state);
        true
    }

    /// Makes room for one more by handing back the removed effect that has
    /// faded furthest, at once. Returns whether there's room now.
    fn make_room(&mut self, used: &mut Producer<Box<EffectState>>) -> bool {
        let quietest = self.held[..self.count]
            .iter()
            .enumerate()
            .filter_map(|(index, held)| {
                let held = held.as_ref()?;
                held.place
                    .is_none()
                    .then_some((index, held.presence.value()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        quietest.is_some_and(|(index, _)| self.hand_back(index, used))
    }

    /// Whether any effect that's in the sound is still ringing.
    pub(crate) fn is_ringing(&self) -> bool {
        self.held[..self.count].iter().flatten().any(|held| {
            (held.presence.is_moving() || held.presence.value() > 0.0) && held.state.is_ringing()
        })
    }

    /// Whether it holds no states at all: none in the chain, and none
    /// fading out.
    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Gets every effect ready to run at `sample_rate`, with a new state
    /// made for it there: its echoes don't survive. Every fade jumps to its
    /// end, as the output fades back in anyway. Called between streams, on
    /// the control side, where making and freeing states is allowed.
    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        for held in self.held[..self.count].iter_mut().flatten() {
            held.state = EffectState::new(&held.settings, sample_rate);
            held.cleared = !held.enabled;
            held.mix = Mix::new(held.mix.target, sample_rate);
            held.presence = Ramp::new(held.target(), fade_length(sample_rate));
        }
    }

    /// How many states it holds.
    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        self.count
    }

    /// Each effect in the order the sound goes through them, with its
    /// presence to three places.
    #[cfg(test)]
    fn order(&self) -> Vec<(EffectId, f32)> {
        self.held[..self.count]
            .iter()
            .flatten()
            .map(|held| (held.id, (held.presence.value() * 1000.0).round() / 1000.0))
            .collect()
    }

    /// How many samples its effects have processed, all together.
    #[cfg(test)]
    pub(crate) fn processed(&self) -> u64 {
        self.held[..self.count]
            .iter()
            .flatten()
            .map(|held| held.state.processed())
            .sum()
    }
}

impl Held {
    /// A new effect at `place`, fading in if it's on.
    fn new(
        effect: &EffectSnapshot,
        place: usize,
        state: Box<EffectState>,
        sample_rate: f64,
    ) -> Self {
        let mut held = Self {
            id: effect.id,
            state,
            settings: effect.settings,
            place: Some(place),
            enabled: effect.enabled,
            moving: false,
            // A new state is clear.
            cleared: !effect.enabled,
            mix: Mix::new(effect.mix, sample_rate),
            presence: Ramp::new(0.0, fade_length(sample_rate)),
        };
        held.presence.set_target(held.target());
        held
    }

    /// Takes on its effect's settings, Mix, on/off and place from a new
    /// snapshot.
    fn update(&mut self, effect: &EffectSnapshot, place: usize) {
        self.state.set_settings(&effect.settings);
        self.settings = effect.settings;
        self.mix.set(effect.mix);
        self.place = Some(place);
        if effect.enabled {
            self.cleared = false;
        }
        self.enabled = effect.enabled;
    }

    /// The presence it's fading to: in the sound if it's in the chain,
    /// switched on and where it belongs.
    fn target(&self) -> f32 {
        if self.place.is_some() && self.enabled && !self.moving {
            1.0
        } else {
            0.0
        }
    }
}

/// The samples a fade takes at `sample_rate`.
fn fade_length(sample_rate: f64) -> u32 {
    (EFFECT_FADE_SECONDS * sample_rate).round() as u32
}

/// Whether every sample is exactly 0.
fn is_silent(samples: &[f32]) -> bool {
    samples.iter().all(|&sample| sample == 0.0)
}

/// A value too small to be anything but a slow "denormal" number, as zero.
/// Every effect's output goes through this in the chain, so a fading tail
/// never slows the processor down.
#[inline]
fn flush(sample: f32) -> f32 {
    if sample.abs() < f32::MIN_POSITIVE {
        0.0
    } else {
        sample
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

    fn is_moving(&self) -> bool {
        self.ramp.is_moving()
    }

    /// Goes straight to its target.
    fn finish(&mut self) {
        self.ramp.jump_to(self.target);
        self.gains = equal_power(self.target);
    }

    /// Blends `dry` into `wet` in place by the Mix, then, while the effect
    /// fades, with `dry` again by its `presence`: at presence `p` that's
    /// `p` of the blend and `1 − p` of the dry sound.
    fn blend(&mut self, dry: [&[f32]; 2], wet: [&mut [f32]; 2], presence: &mut Ramp) {
        if !self.ramp.is_moving() && !presence.is_moving() {
            // Not fading, so all of it is in the sound.
            let [dry_gain, wet_gain] = self.gains;
            for (dry, wet) in dry.into_iter().zip(wet) {
                if wet_gain == 0.0 {
                    wet.copy_from_slice(dry);
                } else if dry_gain == 0.0 {
                    for wet in wet.iter_mut() {
                        *wet = flush(*wet);
                    }
                } else {
                    for (dry, wet) in dry.iter().zip(wet.iter_mut()) {
                        *wet = flush(dry * dry_gain + *wet * wet_gain);
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
            let presence = presence.next_value();
            let dry_gain = presence * dry_gain + (1.0 - presence);
            let wet_gain = presence * wet_gain;
            *wet_left = flush(dry_left * dry_gain + *wet_left * wet_gain);
            *wet_right = flush(dry_right * dry_gain + *wet_right * wet_gain);
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
    use rtrb::{Consumer, RingBuffer};

    use super::*;
    use crate::effects::DelaySettings;

    const RATE: f64 = 48_000.0;
    /// A fade at 48 kHz, in samples.
    const FADE: usize = 480;

    fn id(n: u128) -> EffectId {
        EffectId::from_uuid(uuid::Uuid::from_u128(n))
    }

    fn delay(n: u128, time_seconds: f32, feedback: f32, mix: f32) -> EffectSnapshot {
        EffectSnapshot {
            id: id(n),
            enabled: true,
            mix,
            settings: EffectSettings::Delay(DelaySettings {
                time_seconds,
                feedback,
            }),
        }
    }

    fn short(n: u128) -> EffectSnapshot {
        delay(n, 0.001, 0.0, 0.5)
    }

    fn queue() -> (Producer<Box<EffectState>>, Consumer<Box<EffectState>>) {
        RingBuffer::new(2 * CHAIN_CAPACITY)
    }

    fn arriving(at: ChainAt, effect: &EffectSnapshot) -> Arriving {
        Arriving {
            at,
            id: effect.id,
            state: Some(EffectState::new(&effect.settings, RATE)),
        }
    }

    /// Runs `input` (the same on both sides) through `chain` in blocks of
    /// `block`, and returns the left.
    fn run(
        chain: &mut Chain,
        used: &mut Producer<Box<EffectState>>,
        input: &[f32],
        block: usize,
    ) -> Vec<f32> {
        let mut out = Vec::with_capacity(input.len());
        for part in input.chunks(block) {
            let mut buffer = [[0.0; TRACK_BUFFER_FRAMES]; 2];
            buffer[0][..part.len()].copy_from_slice(part);
            buffer[1][..part.len()].copy_from_slice(part);
            chain.process(&mut buffer, part.len(), used);
            out.extend_from_slice(&buffer[0][..part.len()]);
        }
        out
    }

    fn silence(frames: usize) -> Vec<f32> {
        vec![0.0; frames]
    }

    /// A click, then silence.
    fn click(frames: usize) -> Vec<f32> {
        let mut samples = silence(frames);
        samples[0] = 1.0;
        samples
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

    /// An effect's output too small to be a normal number comes out of the
    /// chain as 0, however it's blended; one at Mix 0 isn't heard at all,
    /// and the sound passes through exactly.
    #[test]
    fn denormals_are_flushed_in_the_blend() {
        let tiny = f32::MIN_POSITIVE / 4.0;
        assert!(tiny != 0.0 && !tiny.is_normal());
        for (mix, fading) in [(1.0, false), (0.5, false), (0.5, true), (1.0, true)] {
            let mut blend = Mix::new(mix, RATE);
            let mut presence = Ramp::new(1.0, 4);
            if fading {
                presence.set_target(0.0);
            }
            let dry = [0.0; 8];
            let (mut left, mut right) = ([tiny; 8], [-tiny; 8]);
            blend.blend([&dry, &dry], [&mut left, &mut right], &mut presence);
            assert_eq!(left, [0.0; 8], "Mix {mix}, fading {fading}");
            assert_eq!(right, [0.0; 8], "Mix {mix}, fading {fading}");
        }
        let mut blend = Mix::new(0.0, RATE);
        let dry = [tiny; 8];
        let (mut left, mut right) = ([1.0; 8], [1.0; 8]);
        blend.blend(
            [&dry, &dry],
            [&mut left, &mut right],
            &mut Ramp::new(1.0, 4),
        );
        assert_eq!(left, dry, "Mix 0 is the sound, exactly");
    }

    /// An added effect fades in over 10 ms; a removed one fades out over
    /// 10 ms where it was, and only then does its state go back. A state
    /// sent for another chain isn't taken.
    #[test]
    fn effects_fade_in_when_added_and_out_before_their_state_goes_back() {
        let (a, b) = (short(1), short(2));
        let mut chain = Chain::new(&[a], RATE);
        let (mut used, mut returned) = queue();
        assert_eq!(chain.order(), [(a.id, 1.0)], "there from the start");

        let mut elsewhere = [arriving(ChainAt::Slot(5), &b)];
        chain.load(ChainAt::Slot(4), &[a, b], &mut elsewhere, &mut used);
        assert!(elsewhere[0].state.is_some(), "not this chain's");
        assert_eq!(chain.held(), 1);

        let mut states = [arriving(ChainAt::Slot(4), &b)];
        chain.load(ChainAt::Slot(4), &[a, b], &mut states, &mut used);
        assert!(states[0].state.is_none(), "B's state was taken");
        assert_eq!(chain.order(), [(a.id, 1.0), (b.id, 0.0)]);
        run(&mut chain, &mut used, &silence(FADE / 2), 64);
        assert_eq!(chain.order()[1], (b.id, 0.5), "halfway in");
        run(&mut chain, &mut used, &silence(FADE / 2), 64);
        assert_eq!(chain.order()[1], (b.id, 1.0));

        chain.load(ChainAt::Slot(4), &[b], &mut [], &mut used);
        run(&mut chain, &mut used, &silence(FADE - 1), 64);
        assert_eq!(chain.held(), 2, "A is still fading out");
        assert!(returned.pop().is_err());
        run(&mut chain, &mut used, &silence(1), 64);
        assert_eq!(chain.order(), [(b.id, 1.0)]);
        assert!(returned.pop().is_ok(), "A's state came back");

        // A chain whose track has gone fades out every effect.
        chain.load(ChainAt::Slot(4), &[], &mut [], &mut used);
        run(&mut chain, &mut used, &silence(FADE), 64);
        assert!(chain.is_empty());
        assert!(returned.pop().is_ok());
    }

    /// Moving one effect fades out only that one, where it was, and fades
    /// it in where it lands; the others stay in the sound throughout. An
    /// effect coming back with the ID of one fading out is a new one.
    #[test]
    fn a_move_fades_only_the_effects_that_have_to_move() {
        let (a, b, c) = (short(1), short(2), short(3));
        let mut chain = Chain::new(&[a, b, c], RATE);
        let (mut used, _returned) = queue();
        // C moves to the front.
        chain.load(ChainAt::Master, &[c, a, b], &mut [], &mut used);
        run(&mut chain, &mut used, &silence(FADE / 2), 64);
        assert_eq!(
            chain.order(),
            [(a.id, 1.0), (b.id, 1.0), (c.id, 0.5)],
            "C fades out where it was"
        );
        run(&mut chain, &mut used, &silence(FADE / 2), 64);
        assert_eq!(chain.order(), [(c.id, 0.0), (a.id, 1.0), (b.id, 1.0)]);
        run(&mut chain, &mut used, &silence(FADE), 64);
        assert_eq!(chain.order(), [(c.id, 1.0), (a.id, 1.0), (b.id, 1.0)]);

        // Moved back before it has gone, it fades back in where it is,
        // over a fade's length from wherever it had got to.
        chain.load(ChainAt::Master, &[a, b, c], &mut [], &mut used);
        run(&mut chain, &mut used, &silence(FADE / 4), 64);
        assert_eq!(chain.order(), [(c.id, 0.75), (a.id, 1.0), (b.id, 1.0)]);
        chain.load(ChainAt::Master, &[c, a, b], &mut [], &mut used);
        run(&mut chain, &mut used, &silence(FADE / 4), 64);
        assert_eq!(chain.order(), [(c.id, 0.812), (a.id, 1.0), (b.id, 1.0)]);
        run(&mut chain, &mut used, &silence(FADE), 64);
        assert_eq!(chain.order(), [(c.id, 1.0), (a.id, 1.0), (b.id, 1.0)]);

        // Removed and back with the same ID: the old one fades out, and a
        // new one fades in with the state that came for it.
        chain.load(ChainAt::Master, &[c, b], &mut [], &mut used);
        let mut states = [arriving(ChainAt::Master, &a)];
        chain.load(ChainAt::Master, &[c, a, b], &mut states, &mut used);
        assert!(states[0].state.is_none());
        let ids: Vec<_> = chain.order().iter().map(|&(id, _)| id).collect();
        assert_eq!(ids, [c.id, a.id, a.id, b.id]);
    }

    /// Switched off, an effect fades to the sound passing through, then
    /// does no work; switched on, it starts from silence, without the
    /// echoes it had.
    #[test]
    fn switched_off_it_fades_out_does_no_work_and_starts_again_from_silence() {
        let endless = delay(1, 0.01, 1.0, 1.0);
        let mut chain = Chain::new(&[endless], RATE);
        let (mut used, _returned) = queue();
        let out = run(&mut chain, &mut used, &click(2000), 128);
        assert_eq!(out[480], 1.0, "its first echo");
        assert!(chain.is_ringing());

        let off = EffectSnapshot {
            enabled: false,
            ..endless
        };
        chain.load(ChainAt::Slot(0), &[off], &mut [], &mut used);
        run(&mut chain, &mut used, &silence(FADE), 128);
        assert!(!chain.is_ringing());
        let processed = chain.processed();
        let input: Vec<f32> = (0..4800).map(|n| (n as f32 * 0.01).sin()).collect();
        assert_eq!(run(&mut chain, &mut used, &input, 128), input, "unchanged");
        assert_eq!(chain.processed(), processed, "and no work");

        chain.load(ChainAt::Slot(0), &[endless], &mut [], &mut used);
        let out = run(&mut chain, &mut used, &silence(4800), 128);
        assert!(out.iter().all(|&sample| sample == 0.0), "no old echoes");
    }

    /// A Delay with nothing coming in and nothing ringing is asleep, and
    /// the chain doesn't run it. Once a click's echoes have died away, it
    /// sleeps again.
    #[test]
    fn asleep_it_does_no_work() {
        let mut chain = Chain::new(&[delay(1, 0.01, 0.5, 0.5)], RATE);
        let (mut used, _returned) = queue();
        run(&mut chain, &mut used, &silence(48_000), 128);
        assert_eq!(chain.processed(), 0, "a new Delay is asleep");

        run(&mut chain, &mut used, &click(128), 128);
        assert!(chain.is_ringing());
        // 0.5 to the 20th is below −120 dB: twenty echoes 10 ms apart.
        run(&mut chain, &mut used, &silence(48_000), 128);
        assert!(!chain.is_ringing());
        let processed = chain.processed();
        assert!((10_000..11_000).contains(&processed), "{processed}");
        run(&mut chain, &mut used, &silence(48_000), 128);
        assert_eq!(chain.processed(), processed, "asleep again");
    }

    /// A removed effect that has faded out waits, silent, while there's no
    /// room to send its state back, and goes once there is: it's never
    /// freed here.
    #[test]
    fn a_removed_state_waits_for_room_in_the_queue() {
        let a = short(1);
        let mut chain = Chain::new(&[a], RATE);
        let (mut used, mut returned) = RingBuffer::new(1);
        used.push(EffectState::new(&a.settings, RATE)).unwrap();
        chain.load(ChainAt::Slot(0), &[], &mut [], &mut used);
        let input: Vec<f32> = (0..2000).map(|n| (n as f32 * 0.01).sin()).collect();
        let out = run(&mut chain, &mut used, &input, 128);
        assert_eq!(chain.held(), 1, "waiting");
        assert_eq!(out[FADE..], input[FADE..], "and silent");
        drop(returned.pop().unwrap());
        run(&mut chain, &mut used, &silence(1), 128);
        assert!(chain.is_empty());
        assert!(returned.pop().is_ok());
    }

    /// With every place full of effects fading out, a new one pushes out
    /// the one that has faded furthest, sending its state back at once.
    #[test]
    fn a_full_table_makes_room_for_a_new_effect() {
        let first: Vec<_> = (0..MAX_EFFECTS as u128).map(short).collect();
        let second: Vec<_> = (100..100 + MAX_EFFECTS as u128).map(short).collect();
        let mut chain = Chain::new(&first, RATE);
        let (mut used, mut returned) = queue();
        let mut states: Vec<_> = second
            .iter()
            .map(|effect| arriving(ChainAt::Master, effect))
            .collect();
        chain.load(ChainAt::Master, &second, &mut states, &mut used);
        chain.load(ChainAt::Master, &[], &mut [], &mut used);
        assert_eq!(chain.held(), CHAIN_CAPACITY);
        run(&mut chain, &mut used, &silence(FADE / 2), 64);
        let newer = short(200);
        chain.load(
            ChainAt::Master,
            &[newer],
            &mut [arriving(ChainAt::Master, &newer)],
            &mut used,
        );
        assert_eq!(chain.held(), CHAIN_CAPACITY);
        assert!(chain.present(newer.id));
        assert!(returned.pop().is_ok(), "one went back to make room");
        assert!(returned.pop().is_err());
    }

    /// A new sample rate makes every state again, for that rate: the tape
    /// a Delay was made with at 48 kHz holds less than 3 s at 96 kHz, but
    /// after the change a 5 s echo comes back on time. Its echoes are gone.
    #[test]
    fn a_new_sample_rate_makes_every_state_again() {
        let long = delay(1, 5.0, 0.5, 1.0);
        let mut chain = Chain::new(&[long], RATE);
        let (mut used, _returned) = queue();
        run(&mut chain, &mut used, &click(128), 128);
        assert!(chain.is_ringing());
        chain.prepare(96_000.0);
        assert!(!chain.is_ringing(), "its echoes are gone");
        let out = run(&mut chain, &mut used, &click(480_001), 256);
        let heard: Vec<_> = out
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.abs() > 1e-6)
            .map(|(at, &sample)| (at, sample))
            .collect();
        assert_eq!(heard, [(480_000, 1.0)]);
    }
}
