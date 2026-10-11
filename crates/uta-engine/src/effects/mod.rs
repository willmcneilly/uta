//! Insert effects on the audio thread. See RFC-007, "Patterns for every
//! effect after this one".
//!
//! An effect is two things here:
//! - its **settings**, plain numbers carried in the snapshot
//!   ([`EffectSnapshot`]) and replaced whole with every change, like
//!   everything else in it;
//! - its **state** ([`EffectState`]): its working memory, such as a delay's
//!   tape. It can't live in the snapshot, or the echoes would be thrown away
//!   with every knob turn. The control side makes it, sized for the sample
//!   rate, when a snapshot first brings the effect, and sends it across with
//!   that snapshot (see [`crate::Swap`]). The audio thread keeps it in its
//!   chain's fixed table, found by the effect's ID, and when the effect is
//!   gone it sends the state back through a fixed-size queue to be freed.
//!   The audio thread owns a state it was handed, but never frees one.
//!
//! Every kind of effect does the same five things ([`Effect`]); the chain
//! ([`chain::Chain`]) does the rest for all of them: on/off, Mix, the fades
//! when one is added, removed, moved or switched, and sleeping.

pub(crate) mod chain;
mod delay;

pub use chain::{CHAIN_CAPACITY, EFFECT_FADE_SECONDS};
pub use delay::{DELAY_TAPE_SECONDS, DelaySettings};

use uta_core::EffectId;

use delay::Delay;

/// The five things every effect does on the audio thread (RFC-007, pattern
/// 6). Nothing here may allocate, free, wait or do I/O, and every loop is
/// bounded by the block or by memory set aside when the effect was made.
pub(crate) trait Effect {
    type Settings: Copy;

    /// Gets ready to run at `sample_rate`, keeping the memory it was made
    /// with.
    fn prepare(&mut self, sample_rate: f64);

    /// Processes a stereo block in place: what goes in is the effect's
    /// input, and what's left is its output alone (the wet sound). The
    /// chain blends it with the input by the effect's Mix.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Glides to new settings, so turning a knob never clicks.
    fn set_settings(&mut self, settings: Self::Settings);

    /// Whether it's still making sound of its own: a delay's echoes. While
    /// it is, the track keeps sounding after Stop, and a render runs on.
    ///
    /// Once it isn't, it's **asleep**: given silence (every sample exactly
    /// 0), it must give silence and change nothing it carries forward, so
    /// the chain can skip it without changing the sound. That means its
    /// settings mustn't be gliding while it's asleep: with nothing ringing,
    /// they can go straight to where they're going. See RFC-007,
    /// "Stopping, switching off, and tails" (Sleeping).
    fn is_ringing(&self) -> bool;

    /// Forgets everything it has heard, as if new: what switching it off
    /// does once it has faded out, so switching it on starts from silence.
    /// Like everything here, it must be quick, whatever memory it has.
    fn clear(&mut self);
}

/// An effect's own settings as the engine plays them, which also say its
/// kind: the core's, as plain numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectSettings {
    Delay(DelaySettings),
}

impl From<&uta_core::EffectSettings> for EffectSettings {
    fn from(settings: &uta_core::EffectSettings) -> Self {
        match settings {
            uta_core::EffectSettings::Delay(delay) => Self::Delay(DelaySettings::from(delay)),
        }
    }
}

/// One effect in a chain, as the snapshot carries it: plain numbers, with
/// nothing that points at its state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectSnapshot {
    /// The effect's permanent ID, which its state is found by.
    pub id: EffectId,
    /// Whether it's switched on. Off, the sound passes through it unchanged.
    pub enabled: bool,
    /// How much of it you hear, from 0 (the sound unchanged) to 1 (only
    /// the effect), blended equal-power.
    pub mix: f32,
    pub settings: EffectSettings,
}

impl From<&uta_core::Effect> for EffectSnapshot {
    fn from(effect: &uta_core::Effect) -> Self {
        Self {
            id: effect.id(),
            enabled: effect.enabled(),
            mix: effect.mix(),
            settings: EffectSettings::from(effect.settings()),
        }
    }
}

/// An effect's working memory, made on the control side and handed to the
/// audio thread. It always comes back to the control side to be freed.
pub struct EffectState(Kind);

/// The state of each kind of effect.
enum Kind {
    Delay(Delay),
}

impl std::fmt::Debug for EffectState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Kind::Delay(_) => f.write_str("EffectState::Delay"),
        }
    }
}

impl EffectState {
    /// A new state for an effect with `settings`, with its memory set aside
    /// for `sample_rate`. Allocates: control side only.
    pub(crate) fn new(settings: &EffectSettings, sample_rate: f64) -> Box<Self> {
        Box::new(Self(match *settings {
            EffectSettings::Delay(delay) => Kind::Delay(Delay::new(delay, sample_rate)),
        }))
    }

    pub(crate) fn prepare(&mut self, sample_rate: f64) {
        match &mut self.0 {
            Kind::Delay(delay) => delay.prepare(sample_rate),
        }
    }

    pub(crate) fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        match &mut self.0 {
            Kind::Delay(delay) => delay.process(left, right),
        }
    }

    /// Glides to `settings`, if they're for this kind of effect. An effect
    /// never changes kind, so they always are.
    pub(crate) fn set_settings(&mut self, settings: &EffectSettings) {
        match (&mut self.0, *settings) {
            (Kind::Delay(delay), EffectSettings::Delay(settings)) => delay.set_settings(settings),
        }
    }

    pub(crate) fn is_ringing(&self) -> bool {
        match &self.0 {
            Kind::Delay(delay) => delay.is_ringing(),
        }
    }

    pub(crate) fn clear(&mut self) {
        match &mut self.0 {
            Kind::Delay(delay) => delay.clear(),
        }
    }

    /// How many samples it has processed so far, for tests that check an
    /// effect does no work while it's switched off or asleep.
    #[cfg(test)]
    pub(crate) fn processed(&self) -> u64 {
        match &self.0 {
            Kind::Delay(delay) => delay.processed(),
        }
    }
}
