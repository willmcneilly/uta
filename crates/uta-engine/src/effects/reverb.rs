//! The Reverb: a predelay, a diffuser for the early burst, and a feedback
//! delay network for the late tail, balanced by Depth. See RFC-010, "The
//! building blocks".
//!
//! In the order the sound passes through them:
//! - **Predelay**: a plain delay line, so the first sound arrives exactly
//!   Predelay after the input.
//! - **The diffuser**: the sound split into 8 lanes, four a side, then in
//!   each step every lane delayed a different short time, shuffled (some
//!   flipped) within its side, and mixed with the others, so one click
//!   becomes thousands of echoes. It has no allpasses, so it doesn't ring
//!   (Geraint Luff, "Let's Write a Reverb", ADC 2021). The mix is three
//!   butterflies: the first two mix each side's lanes, as far as Diffusion
//!   says, from no mixing at all to an even mix; the third mixes the left's
//!   lanes with the right's, as far as Early Cross says. Each keeps the
//!   energy the same. The early output is taken from the last step's lanes
//!   before their mixing: summing a mix's outputs would keep only part of
//!   the echoes, since its signs cancel the rest (RFC-010's note).
//! - **The network**: 16 delay lines, 8 a side, whose outputs are mixed and
//!   go round again. Each side's lines are mixed by a Householder
//!   reflection, then each left line with its right partner, as far as
//!   Late Cross says: at 0 they're two separate reverbs, at full each lap
//!   spreads every line's sound evenly over both sides. Each line loses
//!   exactly as much per lap as its length takes at the Decay, so every
//!   resonance fades at the same rate (Jot, 1991) and no single one sings
//!   on; and its two shelves take as much more or less from the lows and
//!   the highs as Bass Mult and High Mult say, so every resonance in a band
//!   fades at that band's rate. It's fed by the predelayed sound and by the
//!   diffuser's last mix, in the balance Early Send sets, each side into
//!   its own lines.
//! - **Depth** blends the early burst and the tail, equal-power, exactly
//!   one or the other at its ends. The tail is scaled with the Decay so its
//!   energy is the same at any Decay, so turning Decay doesn't change the
//!   blend (RFC-010, "Putting them together").
//! - **High Cut and Low Cut** shape the whole reverb.
//!
//! The lines' lengths drift slowly and at random, each at its own rate,
//! as much as Late Mod Depth says, and some of the diffuser's lanes as
//! much as Early Mod Depth says: that smears the resonances. They're read
//! between samples with the Delay's cubic interpolation, and the drift
//! starts again from the same point on every Play.
//!
//! A mode is an [`Arrangement`]: its lines' lengths, its diffuser's steps,
//! where the outputs are taken, which lanes and lines drift, and how dark
//! it's built. Room is the first.
//!
//! Every delay length is worked out in seconds, then rounded to samples at
//! the sample rate, so the Reverb sounds the same at every rate, though its
//! samples aren't identical. Every setting glides while it rings: the
//! sizes and Predelay as the Delay's time does, each line's length moving
//! to its new one, bending the pitch of what's ringing.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4, LN_10};

use uta_core::{ReverbMode, ReverbSettings};

use super::Effect;
use super::glide::{Glide, cubic};
use crate::VOLUME_SMOOTHING_SECONDS;
use crate::drums::filter::{Svf, prewarp};
use crate::noise::Noise;
use crate::ramp::Ramp;

/// The diffuser's lanes: 8, four a side, mixed by three butterflies.
const LANES: usize = 8;
/// The most lines a mode's network has.
const MAX_LINES: usize = 16;
/// Below this, −120 dB, nothing going in, coming out or written counts as
/// sound.
const SILENCE: f32 = 1e-6;
/// The Late Size scales every line's length by 2 to the power of
/// `2 × size − 1`: from half its arrangement's length at 0 to twice it at
/// 1, with the arrangement's own at 50%.
const LATE_SIZE_OCTAVES: f64 = 2.0;
/// The output filters' and the lines' shelves' quality: Butterworth, with
/// no bump at the cutoff, and a shelf that goes straight from one level to
/// the other without overshooting either.
const TONE_Q: f64 = FRAC_1_SQRT_2;
/// Below this, a filter's memory is cleared before it decays into slow
/// "denormal" numbers.
const TONE_REST: f64 = 1e-30;
/// How far the High shelf in each line sits above the Bass shelf at the
/// least, as a ratio: an octave. Two shelves that both let a band last
/// longer, on top of each other, could add up to a band that never fades;
/// an octave apart they can't, at any Mults (see the tests).
const SHELVES_APART: f64 = 2.0;
/// How far Low Cut turns the lows down, in dB: a low shelf, off at its
/// lowest setting and at this depth from an octave above it. A first guess,
/// tuned by ear in UTA-101.
const LOW_CUT_DB: f64 = -24.0;
/// How far the lines' lengths drift at full Late Mod Depth, either way, in
/// seconds; and the diffuser's lanes' at full Early Mod Depth, never more
/// than half a lane's own length. First guesses, tuned by ear in UTA-101.
const LATE_MOD_SECONDS: f64 = 0.003;
const EARLY_MOD_SECONDS: f64 = 0.001;
/// How long a change of a mod depth takes, and how long the drift takes to
/// glide back to its start when Play restarts it while the tail rings: a
/// change of drift over a few milliseconds would bend the pitch by far
/// more than the drift itself does.
const MOD_GLIDE_SECONDS: f64 = 0.5;
/// Samples set aside past each line's longest length, for cubic
/// interpolation's reach past the sample it reads.
const READ_MARGIN: usize = 3;
/// How often the drift's curve is worked out, in samples: see [`Drift`].
const DRIFT_STEP_SAMPLES: u32 = 16;
/// Where the drift's random numbers start, for the first modulator; every
/// other one starts at this times its number, one up.
const MOD_SEED: u32 = 0x9E37_79B9;

/// One step of a diffuser: its share of Early Size, and where in its eighth
/// of that each lane's delay falls, from 0 to 1. Lane `j` is delayed
/// `share × Early Size × (j + offset) / 8`, so the lanes spread evenly over
/// the step, each at its own irregular place in its slot.
#[derive(Debug)]
pub(crate) struct Step {
    pub share: f64,
    pub offsets: [f64; LANES],
}

/// A mode: how the Reverb's blocks are arranged (RFC-010, "Modes").
#[derive(Debug)]
pub(crate) struct Arrangement {
    /// The network's lines' lengths at Late Size 50%, in seconds, chosen so
    /// no two share a rhythm: an even number of them, at most
    /// [`MAX_LINES`].
    pub lines: &'static [f64],
    /// Each line's rate of drift, as a multiple of Late Mod Rate, so no two
    /// drift together: one for each line.
    pub line_rates: &'static [f64],
    /// The diffuser's steps, first to last. Lane 0's offset is 0 in every
    /// step and the shuffle keeps it in place, so one path through the
    /// diffuser has no delay at all, and the first sound arrives exactly at
    /// Predelay.
    pub steps: &'static [Step],
    /// Which step's lanes drift, and each lane's rate as a multiple of
    /// Early Mod Rate, 0 for one that doesn't. Lane 0 doesn't, so the first
    /// sound still arrives exactly at Predelay.
    pub drifting_step: usize,
    pub lane_rates: [f64; LANES],
    /// After each step's delays: which lane each lane's sound moves to, and
    /// whether it's flipped. Lanes 0 to 3 are the left's and 4 to 7 the
    /// right's, and each moves within its side: only the cross mixes them.
    pub shuffle: [(usize, f32); LANES],
    /// Where the outputs are taken: the diffuser's lanes that make the
    /// early left and right, from the last step before its mix; the
    /// network's lines whose outputs make the late left and right, which
    /// the predelayed left and right and their side's lanes go into. The
    /// `n`th line of one side is crossed with the `n`th of the other.
    pub early_out: [[usize; LANES / 2]; 2],
    pub late_out: [&'static [usize]; 2],
    /// How dark it's built: the low-pass on everything it puts out, in Hz,
    /// or `None` for as bright as its lines make it.
    pub top_hz: Option<f64>,
}

/// Room: a natural, medium to large room. Lines from 42 to 156 ms at its
/// middle size, each about 9% longer than the last, nudged off that so no
/// two line up; four diffuser steps, each half as long as the one before,
/// the longest drifting.
pub(crate) const ROOM: Arrangement = Arrangement {
    lines: &[
        0.0423, 0.0467, 0.0491, 0.0553, 0.0589, 0.0661, 0.0702, 0.0785, 0.0833, 0.0931, 0.0997,
        0.1109, 0.1181, 0.1317, 0.1403, 0.1559,
    ],
    // From 0.6 to 1.4, in an order that puts neighbours far apart.
    line_rates: &[
        1.00, 0.71, 1.27, 0.86, 1.13, 0.62, 1.38, 0.93, 1.21, 0.67, 1.06, 0.80, 1.33, 0.75, 1.17,
        0.89,
    ],
    steps: &[
        Step {
            share: 8.0 / 15.0,
            offsets: [0.0, 0.37, 0.81, 0.14, 0.66, 0.29, 0.93, 0.52],
        },
        Step {
            share: 4.0 / 15.0,
            offsets: [0.0, 0.58, 0.21, 0.89, 0.43, 0.07, 0.74, 0.35],
        },
        Step {
            share: 2.0 / 15.0,
            offsets: [0.0, 0.19, 0.63, 0.41, 0.97, 0.55, 0.12, 0.78],
        },
        Step {
            share: 1.0 / 15.0,
            offsets: [0.0, 0.84, 0.47, 0.26, 0.09, 0.71, 0.38, 0.61],
        },
    ],
    drifting_step: 0,
    lane_rates: [0.0, 1.0, 0.77, 1.31, 0.88, 1.19, 0.69, 1.42],
    shuffle: [
        (0, 1.0),
        (3, -1.0),
        (1, 1.0),
        (2, -1.0),
        (6, -1.0),
        (4, 1.0),
        (7, -1.0),
        (5, 1.0),
    ],
    early_out: [[0, 1, 2, 3], [4, 5, 6, 7]],
    late_out: [&[0, 2, 4, 6, 8, 10, 12, 14], &[1, 3, 5, 7, 9, 11, 13, 15]],
    // A natural room: its brightness is High Mult's and High Cut's to set.
    top_hz: None,
};

impl Arrangement {
    /// The arrangement a mode plays.
    pub(crate) fn of(mode: ReverbMode) -> &'static Arrangement {
        match mode {
            ReverbMode::Room => &ROOM,
        }
    }
}

/// The settings with every value the engine plays inside its range. A value
/// that isn't a number takes its default, so a bad value never reaches the
/// network.
fn clamped(settings: ReverbSettings) -> ReverbSettings {
    use ReverbSettings as S;
    let default = S::default();
    let clamp = |value: f32, min: f32, max: f32, fallback: f32| {
        if value.is_nan() {
            fallback
        } else {
            value.clamp(min, max)
        }
    };
    let amount = |value, fallback| clamp(value, S::MIN_AMOUNT, S::MAX_AMOUNT, fallback);
    let rate = |value, fallback| clamp(value, S::MIN_MOD_RATE_HZ, S::MAX_MOD_RATE_HZ, fallback);
    ReverbSettings {
        mode: settings.mode,
        predelay_seconds: clamp(
            settings.predelay_seconds,
            S::MIN_PREDELAY_SECONDS,
            S::MAX_PREDELAY_SECONDS,
            default.predelay_seconds,
        ),
        decay_seconds: clamp(
            settings.decay_seconds,
            S::MIN_DECAY_SECONDS,
            S::MAX_DECAY_SECONDS,
            default.decay_seconds,
        ),
        depth: amount(settings.depth, default.depth),
        high_cut_hz: clamp(
            settings.high_cut_hz,
            S::MIN_HIGH_CUT_HZ,
            S::MAX_HIGH_CUT_HZ,
            default.high_cut_hz,
        ),
        low_cut_hz: clamp(
            settings.low_cut_hz,
            S::MIN_LOW_CUT_HZ,
            S::MAX_LOW_CUT_HZ,
            default.low_cut_hz,
        ),
        early_size_seconds: clamp(
            settings.early_size_seconds,
            S::MIN_EARLY_SIZE_SECONDS,
            S::MAX_EARLY_SIZE_SECONDS,
            default.early_size_seconds,
        ),
        early_cross: amount(settings.early_cross, default.early_cross),
        diffusion: amount(settings.diffusion, default.diffusion),
        early_send: amount(settings.early_send, default.early_send),
        early_mod_rate_hz: rate(settings.early_mod_rate_hz, default.early_mod_rate_hz),
        early_mod_depth: amount(settings.early_mod_depth, default.early_mod_depth),
        late_size: amount(settings.late_size, default.late_size),
        late_cross: amount(settings.late_cross, default.late_cross),
        late_mod_rate_hz: rate(settings.late_mod_rate_hz, default.late_mod_rate_hz),
        late_mod_depth: amount(settings.late_mod_depth, default.late_mod_depth),
        bass_mult: clamp(
            settings.bass_mult,
            S::MIN_BASS_MULT,
            S::MAX_BASS_MULT,
            default.bass_mult,
        ),
        bass_xover_hz: clamp(
            settings.bass_xover_hz,
            S::MIN_BASS_XOVER_HZ,
            S::MAX_BASS_XOVER_HZ,
            default.bass_xover_hz,
        ),
        high_mult: clamp(
            settings.high_mult,
            S::MIN_HIGH_MULT,
            S::MAX_HIGH_MULT,
            default.high_mult,
        ),
        high_xover_hz: clamp(
            settings.high_xover_hz,
            S::MIN_HIGH_XOVER_HZ,
            S::MAX_HIGH_XOVER_HZ,
            default.high_xover_hz,
        ),
    }
}

/// A delay line that remembers how much of it holds sound written since it
/// was last cleared, so clearing it is setting that to 0, not writing over
/// all of it on the audio thread.
struct Line {
    buffer: Box<[f32]>,
    /// Where the next sample is written.
    write: usize,
    /// How many of the samples just written hold sound since it was last
    /// cleared. Anything further back reads as silence.
    filled: usize,
}

impl Line {
    /// A line that delays by up to `longest` samples, between samples too.
    /// Allocates: control side only.
    fn new(longest: usize) -> Self {
        Self {
            buffer: vec![0.0; longest.max(1) + READ_MARGIN].into_boxed_slice(),
            write: 0,
            filled: 0,
        }
    }

    /// The longest delay it holds, in samples, with room for cubic
    /// interpolation's reach past it.
    fn longest(&self) -> usize {
        self.buffer.len() - READ_MARGIN
    }

    /// The sample written `delay` samples ago, from 1 to the buffer's
    /// length.
    #[inline(always)]
    fn tap(&self, delay: usize) -> f32 {
        if delay > self.filled {
            return 0.0;
        }
        let length = self.buffer.len();
        let at = self.write + length - delay;
        self.buffer[if at >= length { at - length } else { at }]
    }

    /// The sound `delay` samples ago, at least 1: the sample itself for a
    /// whole number, and between samples a smooth curve through the four
    /// around it (the Delay's cubic interpolation), or a straight line
    /// between the two around it for a delay under 2, which has no sample
    /// newer than both.
    #[inline(always)]
    fn read(&self, delay: f64) -> f32 {
        let whole = delay.ceil();
        let fraction = (whole - delay) as f32;
        let whole = whole as usize;
        if fraction == 0.0 {
            self.tap(whole)
        } else if whole >= 3 {
            // The oldest of the four, whole + 1 samples ago.
            let oldest = match self.write.checked_sub(whole + 1) {
                Some(at) => at,
                None => self.write + self.buffer.len() - whole - 1,
            };
            if whole < self.filled && oldest + 3 < self.buffer.len() {
                // All four are sound, side by side in the buffer: the usual
                // case, read straight.
                let four = &self.buffer[oldest..oldest + 4];
                return cubic(four[0], four[1], four[2], four[3], fraction);
            }
            cubic(
                self.tap(whole + 1),
                self.tap(whole),
                self.tap(whole - 1),
                self.tap(whole - 2),
                fraction,
            )
        } else {
            self.tap(whole) + fraction * (self.tap(whole - 1) - self.tap(whole))
        }
    }

    /// Writes the next sample: values so small they'd be slow "denormal"
    /// numbers are written as 0.
    #[inline]
    fn write(&mut self, sample: f32) {
        self.buffer[self.write] = if sample.abs() < f32::MIN_POSITIVE {
            0.0
        } else {
            sample
        };
        self.write += 1;
        if self.write == self.buffer.len() {
            self.write = 0;
        }
        self.filled = (self.filled + 1).min(self.buffer.len());
    }

    /// Writes `sample` and returns the sound `delay` samples before it,
    /// which is `sample` itself for no delay, and between it and the last
    /// sample for a delay under 1.
    #[inline]
    fn delay(&mut self, sample: f32, delay: f64) -> f32 {
        let out = if delay >= 1.0 {
            self.read(delay)
        } else if delay > 0.0 {
            sample + delay as f32 * (self.tap(1) - sample)
        } else {
            sample
        };
        self.write(sample);
        out
    }

    fn clear(&mut self) {
        self.filled = 0;
    }
}

/// A shelf: a state-variable filter whose band-pass and low-pass are
/// added to what goes in (Andrew Simper's shelves, Cytomic's "SVF linear
/// trap optimised 2"), as its prewarped gain and the shares of each.
#[derive(Debug, Clone, Copy, Default)]
struct Shelf {
    g: f64,
    input: f64,
    band: f64,
    low: f64,
}

impl Shelf {
    /// A low shelf at the prewarped cutoff `t` whose lows are `a²` times
    /// the rest: `a` is the square root of the gain, and `root` its square
    /// root.
    fn low(t: f64, a: f64, root: f64) -> Self {
        Self {
            g: t / root,
            input: 1.0,
            band: (a - 1.0) / TONE_Q,
            low: a * a - 1.0,
        }
    }

    /// A high shelf at the prewarped cutoff `t` whose highs are `a²` times
    /// the rest.
    fn high(t: f64, a: f64, root: f64) -> Self {
        Self {
            g: t * root,
            input: a * a,
            band: (1.0 - a) * a / TONE_Q,
            low: 1.0 - a * a,
        }
    }

    /// `input` through `filter` as this shelf.
    #[inline]
    fn process(&self, filter: &mut Svf, input: f64) -> f64 {
        let out = filter.process(input, self.g, TONE_Q);
        if filter.state() < TONE_REST {
            *filter = Svf::default();
        }
        self.input * input + self.band * out.band_pass + self.low * out.low_pass
    }
}

/// One shelf in each of the network's lines, all worked at once: each
/// array holds one number for every line, so the compiler can work on
/// several lines in one instruction. The same filter as [`Shelf`], in
/// Simper's own form, with its coefficients worked out when it changes.
#[derive(Debug, Clone, Copy, Default)]
struct Shelves {
    a1: [f32; MAX_LINES],
    a2: [f32; MAX_LINES],
    a3: [f32; MAX_LINES],
    input: [f32; MAX_LINES],
    band: [f32; MAX_LINES],
    low: [f32; MAX_LINES],
    /// Each filter's memory.
    ic1: [f32; MAX_LINES],
    ic2: [f32; MAX_LINES],
}

impl Shelves {
    /// Line `line`'s shelf becomes `shelf`.
    fn set(&mut self, line: usize, shelf: Shelf) {
        let g = shelf.g;
        let a1 = 1.0 / (1.0 + g * (g + 1.0 / TONE_Q));
        self.a1[line] = a1 as f32;
        self.a2[line] = (g * a1) as f32;
        self.a3[line] = (g * g * a1) as f32;
        self.input[line] = shelf.input as f32;
        self.band[line] = shelf.band as f32;
        self.low[line] = shelf.low as f32;
    }

    /// Each line's sample through its shelf. Memory too small to matter is
    /// cleared before it decays into slow "denormal" numbers. One index
    /// walks every array together, which the compiler turns into work on
    /// several lines at once.
    #[inline]
    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, samples: &mut [f32; MAX_LINES]) {
        let rest = TONE_REST as f32;
        for i in 0..MAX_LINES {
            let (ic1, ic2) = (self.ic1[i], self.ic2[i]);
            let v3 = samples[i] - ic2;
            let v1 = self.a1[i] * ic1 + self.a2[i] * v3;
            let v2 = ic2 + self.a2[i] * ic1 + self.a3[i] * v3;
            let ic1 = 2.0 * v1 - ic1;
            let ic2 = 2.0 * v2 - ic2;
            self.ic1[i] = if ic1.abs() < rest { 0.0 } else { ic1 };
            self.ic2[i] = if ic2.abs() < rest { 0.0 } else { ic2 };
            samples[i] = self.input[i] * samples[i] + self.band[i] * v1 + self.low[i] * v2;
        }
    }

    fn clear(&mut self) {
        self.ic1 = [0.0; MAX_LINES];
        self.ic2 = [0.0; MAX_LINES];
    }
}

/// A slow random drift from −1 to 1: each cycle it moves along a
/// smoothstep curve, which eases away and eases in, from where it is to a
/// new place taken from its own seeded noise. Its speed never jumps, so the
/// pitch it bends never jumps either.
///
/// The curve is worked out every [`DRIFT_STEP_SAMPLES`], and the drift
/// moves in a straight line to it in between, a step a sample: never
/// steeper than the curve, and too slow for the corners to be heard.
struct Drift {
    noise: Noise,
    /// How far through the cycle, from 0 to 1, and how far it moves a
    /// sample.
    phase: f64,
    step: f64,
    /// Where it was at the cycle's start and where it'll be at its end.
    from: f32,
    to: f32,
    /// After Play restarts it while the tail rings: how far it was from
    /// its start, gliding to nothing, a step each time the curve is worked
    /// out.
    restart: Ramp,
    /// Where it is now, and how far it moves each sample until the curve
    /// is worked out again.
    now: f32,
    slope: f32,
}

impl Drift {
    fn new(seed: u32) -> Self {
        let mut drift = Self {
            noise: Noise::seeded(seed),
            phase: 0.0,
            step: 0.0,
            from: 0.0,
            to: 0.0,
            restart: Ramp::new(0.0, 1),
            now: 0.0,
            slope: 0.0,
        };
        drift.start_again();
        drift
    }

    /// Starts from 0 and the start of its noise.
    fn start_again(&mut self) {
        self.noise.restart();
        self.phase = 0.0;
        self.from = 0.0;
        self.to = self.noise.next_sample();
    }

    /// Where the curve is, with what's left of the glide back from a
    /// restart: never further than 2 either way, however often Play
    /// restarts it.
    #[inline]
    fn curve(&self) -> f32 {
        let t = self.phase as f32;
        let along = self.from + (self.to - self.from) * t * t * (3.0 - 2.0 * t);
        (along + self.restart.value()).clamp(-2.0, 2.0)
    }

    /// Where it is on this sample.
    #[inline]
    fn value(&self) -> f32 {
        self.now
    }

    /// Works out where the curve will be [`DRIFT_STEP_SAMPLES`] on, and
    /// heads there. The fastest rate moves its cycle on by under a
    /// hundredth in that time, so it passes at most one cycle's end.
    #[inline]
    fn aim(&mut self) {
        self.restart.next_value();
        self.phase += DRIFT_STEP_SAMPLES as f64 * self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.from = self.to;
            self.to = self.noise.next_sample();
        }
        self.slope = (self.curve() - self.now) / DRIFT_STEP_SAMPLES as f32;
    }

    /// Moves on by a sample.
    #[inline]
    fn advance(&mut self) {
        self.now += self.slope;
    }

    /// Starts again from the same point, gliding there over
    /// [`MOD_GLIDE_SECONDS`] from where it is if `gliding`.
    fn restart(&mut self, gliding: bool) {
        let before = self.now;
        self.start_again();
        self.restart.jump_to(0.0);
        if gliding && before != 0.0 {
            self.restart.jump_to(before);
            self.restart.set_target(0.0);
        }
        self.now = self.curve();
        self.slope = 0.0;
    }
}

/// The Reverb's state: its delay lines, set aside on the control side for
/// the longest sizes at the sample rate, and what glides.
pub(crate) struct Reverb {
    arrangement: &'static Arrangement,
    sample_rate: f64,
    settings: ReverbSettings,
    /// Each side's predelay.
    predelay: [Line; 2],
    /// Each diffuser step's lanes.
    diffuser: Box<[[Line; LANES]]>,
    /// The network's lines.
    lines: Box<[Line]>,
    /// Where each is read from, in samples, gliding to its new length: the
    /// predelay's, each step's lanes', and each line's.
    predelay_head: Glide,
    lane_heads: Box<[[Glide; LANES]]>,
    line_heads: Box<[Glide]>,
    /// Where each is read from now, in samples: the heads' places, kept
    /// here so nothing need ask them while none is gliding.
    predelay_length: f64,
    lane_lengths: Box<[[f64; LANES]]>,
    line_lengths: [f64; MAX_LINES],
    /// Whether any head is gliding.
    gliding: bool,
    /// Each line's tone in the loop: how much it keeps of what passes
    /// through it each lap in the mids, and its low and high shelves, which
    /// keep more or less of the lows and the highs. Worked out again only
    /// while what they come from glides.
    gains: [f32; MAX_LINES],
    low_shelves: Shelves,
    high_shelves: Shelves,
    /// One over Decay, in 1/s, gliding as heard.
    decay_rate: Ramp,
    /// The Mults, and the crossovers in octaves above 1 Hz, gliding as
    /// heard.
    bass_mult: Ramp,
    high_mult: Ramp,
    bass_xover: Ramp,
    high_xover: Ramp,
    /// Depth, Diffusion, Early Send and the crosses, gliding as heard.
    depth: Ramp,
    diffusion: Ramp,
    early_send: Ramp,
    early_cross: Ramp,
    late_cross: Ramp,
    /// The mod depths, gliding over [`MOD_GLIDE_SECONDS`].
    early_mod_depth: Ramp,
    late_mod_depth: Ramp,
    /// Each line's drift, and each lane's of the drifting step, and how
    /// many samples until their curves are worked out again.
    line_drifts: Box<[Drift]>,
    lane_drifts: [Drift; LANES],
    drift_countdown: u32,
    /// The output's High Cut and Low Cut, in octaves above 1 Hz, gliding as
    /// heard.
    high_cut: Ramp,
    low_cut: Ramp,
    /// The gains and the butterflies' and crosses' cosines and sines they
    /// come to, worked out again only while they glide.
    early_gain: f32,
    late_gain: f32,
    /// The tail's energy for every unit that goes into it, from the lines'
    /// gains: see [`Reverb::set_tones`].
    tail_energy: f32,
    dry_send: f32,
    early_send_gain: f32,
    butterfly: (f32, f32),
    early_crossing: (f32, f32),
    late_crossing: (f32, f32),
    /// The High Cut's prewarped gain and the Low Cut's shelf.
    high_cut_gain: f64,
    low_cut_shelf: Shelf,
    /// The arrangement's output filter's prewarped gain, if it has one;
    /// each side's filter, High Cut and Low Cut.
    top_gain: Option<f64>,
    top: [Svf; 2],
    high_cuts: [Svf; 2],
    low_cuts: [Svf; 2],
    /// Samples in a row with nothing above [`SILENCE`] going in, coming out
    /// or written, or `u32::MAX` once it's asleep: the tail has died away and
    /// every line is clear.
    quiet: u32,
    #[cfg(test)]
    processed: u64,
    /// The fastest any line or lane has been read moving through it since
    /// its length last glided, in samples a sample, and where each was read
    /// last.
    #[cfg(test)]
    fastest: f64,
    #[cfg(test)]
    last_reads: [f64; MAX_LINES + LANES],
}

impl Reverb {
    /// A Reverb with `settings`, with lines long enough for the longest sizes
    /// at `sample_rate`. It starts asleep, with every line clear. Allocates:
    /// control side only.
    pub(crate) fn new(settings: ReverbSettings, sample_rate: f64) -> Self {
        Self::arranged(Arrangement::of(settings.mode), settings, sample_rate)
    }

    /// A Reverb in `arrangement`, whatever its settings' mode says.
    fn arranged(
        arrangement: &'static Arrangement,
        settings: ReverbSettings,
        sample_rate: f64,
    ) -> Self {
        let settings = clamped(settings);
        let samples = |seconds: f64| (seconds * sample_rate).ceil() as usize;
        let longest_early = f64::from(ReverbSettings::MAX_EARLY_SIZE_SECONDS);
        let longest_late = (LATE_SIZE_OCTAVES / 2.0).exp2();
        // A drift reaches twice its depth at most, while a restart glides
        // back: so a drifting lane, whose depth is at most half its length,
        // twice its length; and a line twice the drift's depth further.
        let drift = |step: usize, lane: usize| {
            if step == arrangement.drifting_step && arrangement.lane_rates[lane] > 0.0 {
                2.0
            } else {
                1.0
            }
        };
        let mut seeds = (1..).map(|n: u32| MOD_SEED.wrapping_mul(n));
        let mut reverb = Self {
            arrangement,
            sample_rate,
            settings,
            predelay: std::array::from_fn(|_| {
                Line::new(samples(f64::from(ReverbSettings::MAX_PREDELAY_SECONDS)))
            }),
            diffuser: arrangement
                .steps
                .iter()
                .enumerate()
                .map(|(index, step)| {
                    std::array::from_fn(|lane| {
                        Line::new(samples(
                            drift(index, lane) * lane_seconds(step, lane, longest_early),
                        ))
                    })
                })
                .collect(),
            lines: arrangement
                .lines
                .iter()
                .map(|&seconds| Line::new(samples(seconds * longest_late + 2.0 * LATE_MOD_SECONDS)))
                .collect(),
            predelay_head: Glide::new(0.0, sample_rate),
            lane_heads: vec![[Glide::new(0.0, sample_rate); LANES]; arrangement.steps.len()]
                .into_boxed_slice(),
            line_heads: vec![Glide::new(1.0, sample_rate); arrangement.lines.len()]
                .into_boxed_slice(),
            predelay_length: 0.0,
            lane_lengths: vec![[0.0; LANES]; arrangement.steps.len()].into_boxed_slice(),
            line_lengths: [1.0; MAX_LINES],
            gliding: false,
            gains: [0.0; MAX_LINES],
            low_shelves: Shelves::default(),
            high_shelves: Shelves::default(),
            decay_rate: Ramp::new(0.0, 1),
            bass_mult: Ramp::new(1.0, 1),
            high_mult: Ramp::new(1.0, 1),
            bass_xover: Ramp::new(0.0, 1),
            high_xover: Ramp::new(0.0, 1),
            depth: Ramp::new(0.0, 1),
            diffusion: Ramp::new(0.0, 1),
            early_send: Ramp::new(0.0, 1),
            early_cross: Ramp::new(0.0, 1),
            late_cross: Ramp::new(0.0, 1),
            early_mod_depth: Ramp::new(0.0, 1),
            late_mod_depth: Ramp::new(0.0, 1),
            line_drifts: arrangement
                .lines
                .iter()
                .map(|_| Drift::new(seeds.next().unwrap()))
                .collect(),
            lane_drifts: std::array::from_fn(|_| Drift::new(seeds.next().unwrap())),
            drift_countdown: 0,
            high_cut: Ramp::new(0.0, 1),
            low_cut: Ramp::new(0.0, 1),
            early_gain: 0.0,
            late_gain: 0.0,
            tail_energy: 1.0,
            dry_send: 0.0,
            early_send_gain: 0.0,
            butterfly: (1.0, 0.0),
            early_crossing: (1.0, 0.0),
            late_crossing: (1.0, 0.0),
            high_cut_gain: 0.0,
            low_cut_shelf: Shelf::default(),
            top_gain: None,
            top: Default::default(),
            high_cuts: Default::default(),
            low_cuts: Default::default(),
            quiet: u32::MAX,
            #[cfg(test)]
            processed: 0,
            #[cfg(test)]
            fastest: 0.0,
            #[cfg(test)]
            last_reads: [0.0; MAX_LINES + LANES],
        };
        reverb.prepare(sample_rate);
        reverb
    }

    /// How many samples it has processed.
    #[cfg(test)]
    pub(crate) fn processed(&self) -> u64 {
        self.processed
    }

    /// Every head glides to its length at the settings, each kept to what
    /// its line holds, or, if `jump`, goes straight there: the predelay's,
    /// each step's lanes', and each line's.
    fn set_lengths(&mut self, jump: bool) {
        let rate = self.sample_rate;
        let settings = self.settings;
        let samples =
            |seconds: f64, line: &Line| (seconds * rate).round().min(line.longest() as f64);
        let move_to = |head: &mut Glide, to: f64| {
            if jump {
                head.jump_to(to);
            } else {
                head.set_target(to);
            }
        };
        move_to(
            &mut self.predelay_head,
            samples(f64::from(settings.predelay_seconds), &self.predelay[0]),
        );
        let early = f64::from(settings.early_size_seconds);
        for ((step, lines), heads) in self
            .arrangement
            .steps
            .iter()
            .zip(&self.diffuser)
            .zip(self.lane_heads.iter_mut())
        {
            for (lane, (line, head)) in lines.iter().zip(heads.iter_mut()).enumerate() {
                move_to(head, samples(lane_seconds(step, lane, early), line));
            }
        }
        let scale = (LATE_SIZE_OCTAVES * (f64::from(settings.late_size) - 0.5)).exp2();
        for ((&seconds, line), head) in self
            .arrangement
            .lines
            .iter()
            .zip(&self.lines)
            .zip(self.line_heads.iter_mut())
        {
            move_to(head, samples(seconds * scale, line).max(1.0));
        }
        if jump {
            self.predelay_length = self.predelay_head.value();
            for (lengths, heads) in self.lane_lengths.iter_mut().zip(self.lane_heads.iter()) {
                *lengths = heads.map(|head| head.value());
            }
            for (length, head) in self.line_lengths.iter_mut().zip(self.line_heads.iter()) {
                *length = head.value();
            }
            self.gliding = false;
        } else {
            self.gliding = true;
        }
    }

    /// Every head moves on by a sample, and its place for this sample is
    /// kept. Once none is moving, nothing need move them again until the
    /// lengths change.
    fn glide_lengths(&mut self) {
        let mut moving = false;
        self.predelay_length = self.predelay_head.next_value();
        moving |= self.predelay_head.is_moving();
        for (lengths, heads) in self.lane_lengths.iter_mut().zip(self.lane_heads.iter_mut()) {
            for (length, head) in lengths.iter_mut().zip(heads.iter_mut()) {
                *length = head.next_value();
                moving |= head.is_moving();
            }
        }
        for (length, head) in self.line_lengths.iter_mut().zip(self.line_heads.iter_mut()) {
            *length = head.next_value();
            moving |= head.is_moving();
        }
        self.gliding = moving;
    }

    /// Notes where line or lane `index` is read from on this sample, and
    /// how fast that's moving, unless its length is gliding.
    #[cfg(test)]
    fn watch(&mut self, index: usize, read: f64, gliding: bool) {
        let last = std::mem::replace(&mut self.last_reads[index], read);
        if !gliding {
            self.fastest = self.fastest.max((read - last).abs());
        }
    }

    /// Each line's tone for its length now: its gain per lap in the mids
    /// is the loss its length takes at the Decay, so every line falls 60 dB
    /// in the Decay, and so does every resonance of the network (the
    /// mixing loses nothing); its shelves take the lows' and the highs'
    /// own losses at Decay times their Mults.
    fn set_tones(&mut self) {
        let per_sample = -3.0 * LN_10 * f64::from(self.decay_rate.value()) / self.sample_rate;
        let lows = 1.0 / f64::from(self.bass_mult.value()) - 1.0;
        let highs = 1.0 / f64::from(self.high_mult.value()) - 1.0;
        let bass_hz = f64::from(self.bass_xover.value()).exp2();
        let high_hz = f64::from(self.high_xover.value())
            .exp2()
            .max(SHELVES_APART * bass_hz);
        let bass_t = prewarp(bass_hz, self.sample_rate);
        let high_t = prewarp(high_hz, self.sample_rate);
        let count = self.lines.len();
        let mut energy = 0.0;
        for line in 0..count {
            let lap = per_sample * self.line_lengths[line];
            let gain = lap.exp();
            self.gains[line] = gain as f32;
            // What one sound in the line comes to over every lap, heard
            // after each lap's loss: the gain squared, then its square
            // again, and so on.
            energy += gain * gain / (1.0 - gain * gain);
            // Each shelf's gain is its band's loss over the mids', and its
            // square root and fourth root are what the shelf is made from.
            let root = (0.25 * lows * lap).exp();
            self.low_shelves
                .set(line, Shelf::low(bass_t, root * root, root));
            let root = (0.25 * highs * lap).exp();
            self.high_shelves
                .set(line, Shelf::high(high_t, root * root, root));
        }
        self.tail_energy = (energy / count as f64) as f32;
    }

    /// The tail's level: Depth's share of it, scaled so the tail's energy
    /// in the mids is the same at any Decay, and the early burst's: the
    /// longer it rings, the quieter it starts.
    fn set_depth(&mut self) {
        let [early, late] = equal_power(self.depth.value());
        self.early_gain = early;
        self.late_gain = late * LATE_LEVEL / self.tail_energy.sqrt();
    }

    fn set_early_send(&mut self) {
        [self.dry_send, self.early_send_gain] = equal_power(self.early_send.value());
    }

    /// The butterflies' angle within each side: none at no Diffusion, a
    /// quarter turn (an even mix, a Hadamard matrix) at full.
    fn set_diffusion(&mut self) {
        self.butterfly = turn(self.diffusion.value());
    }

    /// The crosses' angles: none at 0, an even mix of the two sides at
    /// full.
    fn set_crosses(&mut self) {
        self.early_crossing = turn(self.early_cross.value());
        self.late_crossing = turn(self.late_cross.value());
    }

    fn set_high_cut(&mut self) {
        self.high_cut_gain = prewarp(f64::from(self.high_cut.value()).exp2(), self.sample_rate);
    }

    /// The Low Cut's shelf: no cut at all at its lowest setting, turning
    /// down to [`LOW_CUT_DB`] over the octave above it, so it's off at its
    /// lowest and glides on from there.
    fn set_low_cut(&mut self) {
        let octaves = f64::from(self.low_cut.value());
        let lowest = f64::from(ReverbSettings::MIN_LOW_CUT_HZ).log2();
        let db = LOW_CUT_DB * (octaves - lowest).clamp(0.0, 1.0);
        let root = 10f64.powf(db / 80.0);
        self.low_cut_shelf =
            Shelf::low(prewarp(octaves.exp2(), self.sample_rate), root * root, root);
    }

    /// The drifts' speeds at the mod rates.
    fn set_mod_rates(&mut self) {
        let early = f64::from(self.settings.early_mod_rate_hz) / self.sample_rate;
        let late = f64::from(self.settings.late_mod_rate_hz) / self.sample_rate;
        for (drift, &rate) in self.line_drifts.iter_mut().zip(self.arrangement.line_rates) {
            drift.step = late * rate;
        }
        for (drift, &rate) in self
            .lane_drifts
            .iter_mut()
            .zip(&self.arrangement.lane_rates)
        {
            drift.step = early * rate;
        }
    }

    /// Everything that glides goes straight to its settings. Real-time safe.
    fn jump_to_settings(&mut self) {
        let settings = self.settings;
        self.decay_rate.jump_to(1.0 / settings.decay_seconds);
        self.bass_mult.jump_to(settings.bass_mult);
        self.high_mult.jump_to(settings.high_mult);
        self.bass_xover.jump_to(settings.bass_xover_hz.log2());
        self.high_xover.jump_to(settings.high_xover_hz.log2());
        self.depth.jump_to(settings.depth);
        self.diffusion.jump_to(settings.diffusion);
        self.early_send.jump_to(settings.early_send);
        self.early_cross.jump_to(settings.early_cross);
        self.late_cross.jump_to(settings.late_cross);
        self.early_mod_depth.jump_to(settings.early_mod_depth);
        self.late_mod_depth.jump_to(settings.late_mod_depth);
        self.high_cut.jump_to(settings.high_cut_hz.log2());
        self.low_cut.jump_to(settings.low_cut_hz.log2());
        self.set_lengths(true);
        self.set_tones();
        self.set_depth();
        self.set_early_send();
        self.set_diffusion();
        self.set_crosses();
        self.set_high_cut();
        self.set_low_cut();
        self.set_mod_rates();
    }

    /// Goes to sleep: the tail has died away, so it clears every line and
    /// filter, the drift starts again, and everything that glides goes
    /// straight to its settings. Real-time safe, and does no work
    /// proportional to the lines' lengths.
    fn sleep(&mut self) {
        for line in self
            .predelay
            .iter_mut()
            .chain(self.diffuser.iter_mut().flatten())
            .chain(self.lines.iter_mut())
        {
            line.clear();
        }
        self.low_shelves.clear();
        self.high_shelves.clear();
        self.top = Default::default();
        self.high_cuts = Default::default();
        self.low_cuts = Default::default();
        for drift in self
            .line_drifts
            .iter_mut()
            .chain(self.lane_drifts.iter_mut())
        {
            drift.restart(false);
        }
        self.drift_countdown = 0;
        self.quiet = u32::MAX;
        self.jump_to_settings();
    }

    /// How long, in samples, a sound takes to pass through its longest
    /// path at the longest sizes: once nothing above silence has been
    /// written for this long, every line holds only silence.
    fn reach(&self) -> usize {
        let longest = |lines: &[Line]| {
            lines
                .iter()
                .map(|line| line.buffer.len())
                .max()
                .unwrap_or(0)
        };
        longest(&self.predelay)
            + self
                .diffuser
                .iter()
                .map(|lanes| longest(lanes))
                .sum::<usize>()
            + longest(&self.lines)
    }
}

/// The tail's level for a network whose sound, heard after every lap,
/// comes to the energy it came in with: set so the tail's energy in the
/// mids is the early burst's, and Depth's 50% is an even blend there. The
/// Mults make the tail's lows and highs last longer or shorter on top.
const LATE_LEVEL: f32 = 1.0;

/// How long lane `lane` of `step` delays, in seconds, at an Early Size of
/// `early` seconds.
fn lane_seconds(step: &Step, lane: usize, early: f64) -> f64 {
    step.share * early * (lane as f64 + step.offsets[lane]) / LANES as f64
}

/// The gains of an equal-power blend `amount` of the way from the first to
/// the second, exactly one or the other at its ends.
fn equal_power(amount: f32) -> [f32; 2] {
    if amount <= 0.0 {
        [1.0, 0.0]
    } else if amount >= 1.0 {
        [0.0, 1.0]
    } else {
        let angle = f64::from(amount) * std::f64::consts::FRAC_PI_2;
        [angle.cos() as f32, angle.sin() as f32]
    }
}

/// The cosine and sine of `amount` of an eighth of a turn: a butterfly
/// that mixes nothing at 0 and evenly at 1.
fn turn(amount: f32) -> (f32, f32) {
    let angle = f64::from(amount) * FRAC_PI_4;
    (angle.cos() as f32, angle.sin() as f32)
}

/// Turns each pair of values by the angle whose cosine and sine are `c`
/// and `s`. At an eighth of a turn it's an even mix of the two. It keeps
/// their energy the same at any angle.
#[inline]
fn butterfly(a: f32, b: f32, (c, s): (f32, f32)) -> (f32, f32) {
    (c * a + s * b, s * a - c * b)
}

/// Mixes the diffuser's lanes with butterflies, one for each doubling:
/// `within` turns the pairs within each side, `across` each left lane with
/// its right partner. At an eighth of a turn each, it's a Hadamard matrix,
/// an even mix of every lane into every other. It keeps their energy the
/// same at any angles.
#[inline]
fn mix(values: &mut [f32; LANES], within: (f32, f32), across: (f32, f32)) {
    let mut span = 1;
    while span < LANES {
        let turn = if span == LANES / 2 { across } else { within };
        for start in (0..LANES).step_by(2 * span) {
            for i in start..start + span {
                (values[i], values[i + span]) = butterfly(values[i], values[i + span], turn);
            }
        }
        span *= 2;
    }
}

impl Effect for Reverb {
    type Settings = ReverbSettings;

    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let smoothing = (VOLUME_SMOOTHING_SECONDS * sample_rate).round() as u32;
        let slow = (MOD_GLIDE_SECONDS * sample_rate).round() as u32;
        for ramp in [
            &mut self.decay_rate,
            &mut self.bass_mult,
            &mut self.high_mult,
            &mut self.bass_xover,
            &mut self.high_xover,
            &mut self.depth,
            &mut self.diffusion,
            &mut self.early_send,
            &mut self.early_cross,
            &mut self.late_cross,
            &mut self.high_cut,
            &mut self.low_cut,
        ] {
            *ramp = Ramp::new(0.0, smoothing);
        }
        self.early_mod_depth = Ramp::new(0.0, slow);
        self.late_mod_depth = Ramp::new(0.0, slow);
        for drift in self
            .line_drifts
            .iter_mut()
            .chain(self.lane_drifts.iter_mut())
        {
            drift.restart = Ramp::new(0.0, slow / DRIFT_STEP_SAMPLES);
        }
        self.predelay_head = Glide::new(0.0, sample_rate);
        for head in self
            .lane_heads
            .iter_mut()
            .flatten()
            .chain(self.line_heads.iter_mut())
        {
            *head = Glide::new(0.0, sample_rate);
        }
        self.top_gain = self.arrangement.top_hz.map(|hz| prewarp(hz, sample_rate));
        self.jump_to_settings();
    }

    /// For each sample: the predelay, then the diffuser's steps, whose last
    /// step's lanes before its mix are the early left and right; the
    /// network's lines read with their drift, given their tone, mixed and
    /// written again with the predelayed sound and the diffuser's last mix
    /// going in; then Depth's blend of the two, through the mode's output
    /// filter, High Cut and Low Cut.
    ///
    /// Once nothing above −120 dB has gone in, come out or been written for
    /// as long as the longest path through it, every line holds only
    /// silence, and it goes to sleep on that sample. Anything that reaches a
    /// line wakes it.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let arrangement = self.arrangement;
        let count = self.lines.len();
        let side_count = count / 2;
        // The predelayed sound goes into half the lines a side, and each
        // side's 4 lanes into its side's lines two at a time, each keeping
        // the energy it brings.
        let dry_in = (2.0 / count as f32).sqrt();
        let early_in = (LANES as f32 / count as f32).sqrt();
        let reflect_by = 2.0 / side_count as f32;
        let late_reach = LATE_MOD_SECONDS * self.sample_rate;
        let early_reach = EARLY_MOD_SECONDS * self.sample_rate;
        let reach = self.reach() as u32;
        let mut quiet = self.quiet;
        #[cfg(test)]
        {
            self.processed += left.len() as u64;
        }
        // Bounded by the block.
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            let mut tones_move = self.gliding;
            if self.gliding {
                self.glide_lengths();
            }
            for ramp in [
                &mut self.decay_rate,
                &mut self.bass_mult,
                &mut self.high_mult,
                &mut self.bass_xover,
                &mut self.high_xover,
            ] {
                if ramp.is_moving() {
                    ramp.next_value();
                    tones_move = true;
                }
            }
            if tones_move {
                self.set_tones();
            }
            if self.depth.is_moving() || self.decay_rate.is_moving() {
                self.depth.next_value();
                self.set_depth();
            }
            if self.diffusion.is_moving() {
                self.diffusion.next_value();
                self.set_diffusion();
            }
            if self.early_send.is_moving() {
                self.early_send.next_value();
                self.set_early_send();
            }
            if self.early_cross.is_moving() || self.late_cross.is_moving() {
                self.early_cross.next_value();
                self.late_cross.next_value();
                self.set_crosses();
            }
            if self.high_cut.is_moving() {
                self.high_cut.next_value();
                self.set_high_cut();
            }
            if self.low_cut.is_moving() {
                self.low_cut.next_value();
                self.set_low_cut();
            }
            // Each drift moves on only while it's heard, so with no Mod
            // Depth it costs nothing.
            let early_drift = f64::from(self.early_mod_depth.next_value()) * early_reach;
            let late_drift = f64::from(self.late_mod_depth.next_value()) * late_reach;
            let early_drifting = early_drift != 0.0 || self.early_mod_depth.is_moving();
            let late_drifting = late_drift != 0.0 || self.late_mod_depth.is_moving();
            let aiming = self.drift_countdown == 0 && quiet != u32::MAX;
            if aiming && late_drifting {
                for drift in self.line_drifts.iter_mut() {
                    drift.aim();
                }
            }
            if aiming && early_drifting {
                for drift in self.lane_drifts.iter_mut() {
                    drift.aim();
                }
            }

            #[cfg(test)]
            let mut watched = [None; MAX_LINES + LANES];
            let input = [*left, *right];
            let mut loud = input[0].abs() > SILENCE || input[1].abs() > SILENCE;
            let mut written = input[0] != 0.0 || input[1] != 0.0;
            let predelay = self.predelay_length;
            let dry = [0, 1].map(|side| self.predelay[side].delay(input[side], predelay));
            // Half of each side into each of its four lanes: the same energy.
            let mut lanes: [f32; LANES] = std::array::from_fn(|lane| 0.5 * dry[lane * 2 / LANES]);
            let mut taps = [0.0; LANES];
            let last = self.diffuser.len() - 1;
            for (step, (lines, lengths)) in self
                .diffuser
                .iter_mut()
                .zip(self.lane_lengths.iter())
                .enumerate()
            {
                let drifting = early_drifting && step == arrangement.drifting_step;
                for (lane, ((sample, line), &length)) in lanes
                    .iter_mut()
                    .zip(lines.iter_mut())
                    .zip(lengths)
                    .enumerate()
                {
                    loud |= sample.abs() > SILENCE;
                    let delay = if drifting && arrangement.lane_rates[lane] > 0.0 {
                        // Never more than half the lane's own length.
                        let drift = early_drift.min(0.5 * length);
                        let delay = length + drift * f64::from(self.lane_drifts[lane].value());
                        #[cfg(test)]
                        {
                            let gliding = self.lane_heads[step][lane].is_moving();
                            watched[MAX_LINES + lane] = Some((delay, gliding));
                        }
                        delay
                    } else {
                        length
                    };
                    *sample = line.delay(*sample, delay);
                }
                if step == last {
                    taps = lanes;
                }
                let mut shuffled = [0.0; LANES];
                for (lane, &(to, sign)) in arrangement.shuffle.iter().enumerate() {
                    shuffled[to] = sign * lanes[lane];
                }
                lanes = shuffled;
                mix(&mut lanes, self.butterfly, self.early_crossing);
            }
            let early = arrangement
                .early_out
                .map(|side| side.iter().map(|&lane| taps[lane]).sum::<f32>());
            // Round the network: each line's output, read where its drift
            // has it, given its tone for the lap; each side's Householder
            // reflection, each less twice their average; each left line
            // crossed with its right partner; then each line written with
            // what comes round and what goes in.
            let mut outputs = [0.0f32; MAX_LINES];
            let mut reads = self.line_lengths;
            if late_drifting {
                for (read, drift) in reads.iter_mut().zip(self.line_drifts.iter()) {
                    *read += late_drift * f64::from(drift.value());
                }
            }
            for (((output, &read), line), &gain) in outputs
                .iter_mut()
                .zip(reads.iter())
                .zip(self.lines.iter())
                .zip(self.gains.iter())
            {
                *output = gain * line.read(read);
            }
            self.low_shelves.process(&mut outputs);
            self.high_shelves.process(&mut outputs);
            #[cfg(test)]
            {
                for (index, &read) in reads[..count].iter().enumerate() {
                    watched[index] = Some((read, self.line_heads[index].is_moving()));
                }
                for (index, read) in watched.into_iter().enumerate() {
                    if let Some((read, gliding)) = read {
                        self.watch(index, read, gliding);
                    }
                }
            }
            let mut mixed = outputs;
            for side in arrangement.late_out {
                let reflect = reflect_by * side.iter().map(|&line| outputs[line]).sum::<f32>();
                for &line in side {
                    mixed[line] -= reflect;
                }
            }
            let [lefts, rights] = arrangement.late_out;
            for (&l, &r) in lefts.iter().zip(rights) {
                (mixed[l], mixed[r]) = butterfly(mixed[l], mixed[r], self.late_crossing);
            }
            for (side, lines) in arrangement.late_out.iter().enumerate() {
                for (n, &line) in lines.iter().enumerate() {
                    let lane = side * LANES / 2 + n % (LANES / 2);
                    mixed[line] += self.dry_send * dry_in * dry[side]
                        + self.early_send_gain * early_in * lanes[lane];
                }
            }
            for (line, &sample) in self.lines.iter_mut().zip(mixed.iter()) {
                loud |= sample.abs() > SILENCE;
                written |= sample != 0.0;
                line.write(sample);
            }
            let late = arrangement
                .late_out
                .map(|side| side.iter().map(|&line| outputs[line]).sum::<f32>());
            let out = [0, 1].map(|side| {
                let mut wet =
                    f64::from(self.early_gain * early[side] + self.late_gain * late[side]);
                if let Some(gain) = self.top_gain {
                    wet = filtered(&mut self.top[side], wet, gain);
                }
                wet = filtered(&mut self.high_cuts[side], wet, self.high_cut_gain);
                self.low_cut_shelf.process(&mut self.low_cuts[side], wet) as f32
            });
            loud |= out[0].abs() > SILENCE || out[1].abs() > SILENCE;
            *left = out[0];
            *right = out[1];
            if loud {
                quiet = 0;
            } else if quiet == u32::MAX {
                // Asleep, every line is clear. Anything written to one,
                // however quiet, wakes it, so it only sleeps clear, and it's
                // never skipped while it holds sound.
                if written {
                    quiet = 0;
                }
            } else {
                quiet += 1;
                if quiet > reach {
                    self.sleep();
                    quiet = u32::MAX;
                }
            }
            // The drift moves on only while it's awake, so asleep it changes
            // nothing, as the chain expects when it skips it.
            if quiet != u32::MAX {
                self.drift_countdown = if self.drift_countdown == 0 {
                    DRIFT_STEP_SAMPLES - 1
                } else {
                    self.drift_countdown - 1
                };
                if late_drifting {
                    for drift in self.line_drifts.iter_mut() {
                        drift.advance();
                    }
                }
                if early_drifting {
                    for drift in self.lane_drifts.iter_mut() {
                        drift.advance();
                    }
                }
            }
        }
        self.quiet = quiet;
    }

    /// While it's awake, everything glides: the sizes and Predelay as the
    /// Delay's time does, each line moving to its new length, the mod
    /// depths over [`MOD_GLIDE_SECONDS`], and the rest over a few
    /// milliseconds. A mod rate changes how fast the drift moves on, not
    /// where it is. Asleep, everything goes straight there.
    fn set_settings(&mut self, settings: ReverbSettings) {
        let settings = clamped(settings);
        self.settings = settings;
        if self.is_ringing() {
            self.decay_rate.set_target(1.0 / settings.decay_seconds);
            self.bass_mult.set_target(settings.bass_mult);
            self.high_mult.set_target(settings.high_mult);
            self.bass_xover.set_target(settings.bass_xover_hz.log2());
            self.high_xover.set_target(settings.high_xover_hz.log2());
            self.depth.set_target(settings.depth);
            self.diffusion.set_target(settings.diffusion);
            self.early_send.set_target(settings.early_send);
            self.early_cross.set_target(settings.early_cross);
            self.late_cross.set_target(settings.late_cross);
            self.early_mod_depth.set_target(settings.early_mod_depth);
            self.late_mod_depth.set_target(settings.late_mod_depth);
            self.high_cut.set_target(settings.high_cut_hz.log2());
            self.low_cut.set_target(settings.low_cut_hz.log2());
            self.set_lengths(false);
            self.set_mod_rates();
        } else {
            self.jump_to_settings();
        }
    }

    /// Ringing until it goes to sleep: see [`Reverb::process`].
    fn is_ringing(&self) -> bool {
        self.quiet != u32::MAX
    }

    fn clear(&mut self) {
        self.sleep();
    }

    /// The drift starts again from the same point, so what you hear from
    /// the top is what a render plays. If the tail is ringing, each line
    /// glides there from where its drift had it rather than jumping.
    fn restart(&mut self) {
        let gliding = self.is_ringing();
        for drift in self
            .line_drifts
            .iter_mut()
            .chain(self.lane_drifts.iter_mut())
        {
            drift.restart(gliding);
        }
        self.drift_countdown = 0;
    }
}

/// `input` through `filter` as a Butterworth low-pass at the prewarped
/// cutoff `gain`.
#[inline]
fn filtered(filter: &mut Svf, input: f64, gain: f64) -> f64 {
    let out = filter.process(input, gain, TONE_Q).low_pass;
    if filter.state() < TONE_REST {
        *filter = Svf::default();
    }
    out
}

#[cfg(test)]
mod tests;
