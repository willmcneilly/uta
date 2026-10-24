//! The Reverb: a predelay, a diffuser for the early burst, and a feedback
//! delay network for the late tail, balanced by Depth. See RFC-010, "The
//! building blocks".
//!
//! In the order the sound passes through them:
//! - **Predelay**: a plain delay line, so the first sound arrives exactly
//!   Predelay after the input.
//! - **The diffuser**: the sound split into 8 lanes, then in each step every
//!   lane delayed a different short time, shuffled (some flipped), and mixed
//!   with the others, so one click becomes thousands of echoes. It has no
//!   allpasses, so it doesn't ring (Geraint Luff, "Let's Write a Reverb",
//!   ADC 2021). The mix is a Hadamard matrix made of three butterflies;
//!   Diffusion turns each butterfly from no mixing at all to an even mix,
//!   keeping the energy the same all the way. The early output is taken
//!   from the last step's lanes before their mixing: summing a mix's
//!   outputs would keep only part of the echoes, since its signs cancel the
//!   rest (RFC-010's note).
//! - **The network**: 16 delay lines whose outputs are mixed into all of
//!   them (a Householder reflection) and go round again. A Hadamard mix,
//!   which sends every line into every other at the same strength, was
//!   measured against it: it was no denser, and its tail's spectrum was
//!   bumpier. Each line loses exactly as much per lap as its length takes at
//!   the Decay, so every resonance fades at the same rate (Jot, 1991) and no
//!   single one sings on. It's fed by the predelayed sound and by the diffuser's last mix, in
//!   the balance Early Send sets.
//! - **Depth** blends the early burst and the tail, equal-power, exactly
//!   one or the other at its ends.
//!
//! A mode is an [`Arrangement`]: its lines' lengths, its diffuser's steps,
//! where the outputs are taken, and how dark it's built. Room is the first.
//!
//! Every delay length is worked out in seconds, then rounded to samples at
//! the sample rate, so the Reverb sounds the same at every rate, though its
//! samples aren't identical. The tone, the crosses and the modulation are
//! UTA-100's: until then the network is fully crossed and still. Decay,
//! Depth, Diffusion and Early Send glide; the sizes and Predelay go
//! straight to their new lengths, which UTA-100 glides.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4, LN_10};

use uta_core::{ReverbMode, ReverbSettings};

use super::Effect;
use crate::VOLUME_SMOOTHING_SECONDS;
use crate::drums::filter::{Svf, prewarp};
use crate::ramp::Ramp;

/// The diffuser's lanes: 8, mixed by a Hadamard matrix of three
/// butterflies.
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
/// The output filter's quality: Butterworth, with no bump at its cutoff.
const TONE_Q: f64 = FRAC_1_SQRT_2;
/// Below this, the output filter's memory is cleared before it decays into
/// slow "denormal" numbers.
const TONE_REST: f64 = 1e-30;

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
    /// The diffuser's steps, first to last. Lane 0's offset is 0 in every
    /// step and the shuffle keeps it in place, so one path through the
    /// diffuser has no delay at all, and the first sound arrives exactly at
    /// Predelay.
    pub steps: &'static [Step],
    /// After each step's delays: which lane each lane's sound moves to, and
    /// whether it's flipped.
    pub shuffle: [(usize, f32); LANES],
    /// Where the outputs are taken: the diffuser's lanes that make the
    /// early left and right, from the last step before its mix; the
    /// network's lines whose outputs make the late left and right, which
    /// the predelayed left and right go into.
    pub early_out: [[usize; LANES / 2]; 2],
    pub late_out: [&'static [usize]; 2],
    /// How dark it's built: the low-pass on everything it puts out, in Hz,
    /// or `None` for as bright as its lines make it.
    pub top_hz: Option<f64>,
}

/// Room: a natural, medium to large room. Lines from 42 to 156 ms at its
/// middle size, each about 9% longer than the last, nudged off that so no
/// two line up; four diffuser steps, each half as long as the one before.
pub(crate) const ROOM: Arrangement = Arrangement {
    lines: &[
        0.0423, 0.0467, 0.0491, 0.0553, 0.0589, 0.0661, 0.0702, 0.0785, 0.0833, 0.0931, 0.0997,
        0.1109, 0.1181, 0.1317, 0.1403, 0.1559,
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
    shuffle: [
        (0, 1.0),
        (6, -1.0),
        (3, 1.0),
        (5, -1.0),
        (1, -1.0),
        (7, 1.0),
        (4, -1.0),
        (2, 1.0),
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
    ReverbSettings {
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
        early_size_seconds: clamp(
            settings.early_size_seconds,
            S::MIN_EARLY_SIZE_SECONDS,
            S::MAX_EARLY_SIZE_SECONDS,
            default.early_size_seconds,
        ),
        depth: amount(settings.depth, default.depth),
        diffusion: amount(settings.diffusion, default.diffusion),
        early_send: amount(settings.early_send, default.early_send),
        late_size: amount(settings.late_size, default.late_size),
        ..settings
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
    /// A line that delays by up to `longest` samples. Allocates: control
    /// side only.
    fn new(longest: usize) -> Self {
        Self {
            buffer: vec![0.0; longest.max(1)].into_boxed_slice(),
            write: 0,
            filled: 0,
        }
    }

    /// The longest delay it holds, in samples.
    fn longest(&self) -> usize {
        self.buffer.len()
    }

    /// The sample written `delay` samples ago, from 1 to [`Self::longest`].
    #[inline]
    fn read(&self, delay: usize) -> f32 {
        if delay > self.filled {
            return 0.0;
        }
        let length = self.buffer.len();
        let at = self.write + length - delay;
        self.buffer[if at >= length { at - length } else { at }]
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

    /// Writes `sample` and returns the one `delay` samples before it, which
    /// is `sample` itself for no delay.
    #[inline]
    fn delay(&mut self, sample: f32, delay: usize) -> f32 {
        let out = if delay == 0 { sample } else { self.read(delay) };
        self.write(sample);
        out
    }

    fn clear(&mut self) {
        self.filled = 0;
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
    /// The current delays, in samples: the predelay's, each step's lanes',
    /// and each line's.
    predelay_samples: usize,
    lane_samples: Box<[[usize; LANES]]>,
    line_samples: Box<[usize]>,
    /// The lines' gains per lap, worked out again only while Decay glides
    /// or a size changes.
    gains: Box<[f32]>,
    /// One over Decay, in 1/s, gliding as heard.
    decay_rate: Ramp,
    /// Depth, Diffusion and Early Send, gliding as heard.
    depth: Ramp,
    diffusion: Ramp,
    early_send: Ramp,
    /// The gains and the butterflies' cosine and sine they come to, worked
    /// out again only while they glide.
    early_gain: f32,
    late_gain: f32,
    dry_send: f32,
    early_send_gain: f32,
    butterfly: (f32, f32),
    /// The output filter's prewarped gain, if the arrangement has one, and
    /// each side's filter.
    tone_gain: Option<f64>,
    tone: [Svf; 2],
    /// Samples in a row with nothing above [`SILENCE`] going in, coming out
    /// or written, or `u32::MAX` once it's asleep: the tail has died away and
    /// every line is clear.
    quiet: u32,
    #[cfg(test)]
    processed: u64,
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
        let samples = |seconds: f64| (seconds * sample_rate).round() as usize;
        let longest_early = f64::from(ReverbSettings::MAX_EARLY_SIZE_SECONDS);
        let longest_late = (LATE_SIZE_OCTAVES / 2.0).exp2();
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
                .map(|step| {
                    std::array::from_fn(|lane| {
                        Line::new(samples(lane_seconds(step, lane, longest_early)))
                    })
                })
                .collect(),
            lines: arrangement
                .lines
                .iter()
                .map(|&seconds| Line::new(samples(seconds * longest_late)))
                .collect(),
            predelay_samples: 0,
            lane_samples: vec![[0; LANES]; arrangement.steps.len()].into_boxed_slice(),
            line_samples: vec![1; arrangement.lines.len()].into_boxed_slice(),
            gains: vec![0.0; arrangement.lines.len()].into_boxed_slice(),
            decay_rate: Ramp::new(0.0, 1),
            depth: Ramp::new(0.0, 1),
            diffusion: Ramp::new(0.0, 1),
            early_send: Ramp::new(0.0, 1),
            early_gain: 0.0,
            late_gain: 0.0,
            dry_send: 0.0,
            early_send_gain: 0.0,
            butterfly: (1.0, 0.0),
            tone_gain: None,
            tone: Default::default(),
            quiet: u32::MAX,
            #[cfg(test)]
            processed: 0,
        };
        reverb.prepare(sample_rate);
        reverb
    }

    /// How many samples it has processed.
    #[cfg(test)]
    pub(crate) fn processed(&self) -> u64 {
        self.processed
    }

    /// Works out every delay in samples from the settings, each kept to what
    /// its line holds, and the lines' gains with them.
    fn set_lengths(&mut self) {
        let rate = self.sample_rate;
        let settings = self.settings;
        let samples =
            |seconds: f64, line: &Line| ((seconds * rate).round() as usize).min(line.longest());
        self.predelay_samples = samples(f64::from(settings.predelay_seconds), &self.predelay[0]);
        let early = f64::from(settings.early_size_seconds);
        for ((step, lanes), delays) in self
            .arrangement
            .steps
            .iter()
            .zip(&self.diffuser)
            .zip(self.lane_samples.iter_mut())
        {
            for (lane, line) in lanes.iter().enumerate() {
                delays[lane] = samples(lane_seconds(step, lane, early), line);
            }
        }
        let scale = (LATE_SIZE_OCTAVES * (f64::from(settings.late_size) - 0.5)).exp2();
        for ((&seconds, line), delay) in self
            .arrangement
            .lines
            .iter()
            .zip(&self.lines)
            .zip(self.line_samples.iter_mut())
        {
            *delay = samples(seconds * scale, line).max(1);
        }
        self.set_gains();
    }

    /// Each line's gain per lap: the loss its length takes at the Decay, so
    /// that every line falls 60 dB in the Decay, and so does every
    /// resonance of the network (the Householder mix loses nothing).
    fn set_gains(&mut self) {
        let per_sample = -3.0 * LN_10 * f64::from(self.decay_rate.value()) / self.sample_rate;
        for (gain, &length) in self.gains.iter_mut().zip(self.line_samples.iter()) {
            *gain = (per_sample * length as f64).exp() as f32;
        }
    }

    fn set_depth(&mut self) {
        [self.early_gain, self.late_gain] = equal_power(self.depth.value());
    }

    fn set_early_send(&mut self) {
        [self.dry_send, self.early_send_gain] = equal_power(self.early_send.value());
    }

    /// The butterflies' angle: none at no Diffusion, a quarter turn (an even
    /// mix, a Hadamard matrix) at full.
    fn set_diffusion(&mut self) {
        let angle = f64::from(self.diffusion.value()) * FRAC_PI_4;
        self.butterfly = (angle.cos() as f32, angle.sin() as f32);
    }

    /// Everything that glides goes straight to its settings. Real-time safe.
    fn jump_to_settings(&mut self) {
        let settings = self.settings;
        self.decay_rate.jump_to(1.0 / settings.decay_seconds);
        self.depth.jump_to(settings.depth);
        self.diffusion.jump_to(settings.diffusion);
        self.early_send.jump_to(settings.early_send);
        self.set_lengths();
        self.set_depth();
        self.set_early_send();
        self.set_diffusion();
    }

    /// Goes to sleep: the tail has died away, so it clears every line and
    /// the output filter, and everything that glides goes straight to its
    /// settings. Real-time safe, and does no work proportional to the
    /// lines' lengths.
    fn sleep(&mut self) {
        for line in self
            .predelay
            .iter_mut()
            .chain(self.diffuser.iter_mut().flatten())
            .chain(self.lines.iter_mut())
        {
            line.clear();
        }
        self.tone = Default::default();
        self.quiet = u32::MAX;
        self.jump_to_settings();
    }

    /// How long, in samples, a sound takes to pass through its longest
    /// path at the longest sizes: once nothing above silence has been
    /// written for this long, every line holds only silence.
    fn reach(&self) -> usize {
        let longest = |lines: &[Line]| lines.iter().map(Line::longest).max().unwrap_or(0);
        longest(&self.predelay)
            + self
                .diffuser
                .iter()
                .map(|lanes| longest(lanes))
                .sum::<usize>()
            + longest(&self.lines)
    }
}

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

/// Mixes a power of two of values with butterflies, one for each doubling,
/// each turning pairs of them by the angle whose cosine and sine are `c`
/// and `s`. At an eighth of a turn it's a Hadamard matrix, an even mix of
/// every value into every other. It keeps their energy the same at any
/// angle.
#[inline]
fn mix(values: &mut [f32], (c, s): (f32, f32)) {
    let count = values.len();
    let mut span = 1;
    while span < count {
        for start in (0..count).step_by(2 * span) {
            for i in start..start + span {
                let (a, b) = (values[i], values[i + span]);
                values[i] = c * a + s * b;
                values[i + span] = s * a - c * b;
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
        self.decay_rate = Ramp::new(0.0, smoothing);
        self.depth = Ramp::new(0.0, smoothing);
        self.diffusion = Ramp::new(0.0, smoothing);
        self.early_send = Ramp::new(0.0, smoothing);
        self.tone_gain = self.arrangement.top_hz.map(|hz| prewarp(hz, sample_rate));
        self.jump_to_settings();
    }

    /// For each sample: the predelay, then the diffuser's steps, whose last
    /// step's lanes before its mix are the early left and right; the
    /// network's lines read, mixed and written again with the predelayed
    /// sound and the diffuser's last mix going in; then Depth's blend of the
    /// two, through the mode's output filter.
    ///
    /// Once nothing above −120 dB has gone in, come out or been written for
    /// as long as the longest path through it, every line holds only
    /// silence, and it goes to sleep on that sample. Anything that reaches a
    /// line wakes it.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let arrangement = self.arrangement;
        let count = self.lines.len();
        // The predelayed sound goes into half the lines a side, and the
        // diffuser's 8 lanes into the lines two at a time, each keeping the
        // energy it brings.
        let dry_in = (2.0 / count as f32).sqrt();
        let early_in = (LANES as f32 / count as f32).sqrt();
        // Each side of the tail is half the lines, summed at a level that
        // keeps it near the early burst's at the default Decay.
        let late_out = 0.5;
        let reach = self.reach() as u32;
        let mut quiet = self.quiet;
        #[cfg(test)]
        {
            self.processed += left.len() as u64;
        }
        // Bounded by the block.
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            if self.decay_rate.is_moving() {
                self.decay_rate.next_value();
                self.set_gains();
            }
            if self.depth.is_moving() {
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
            let input = [*left, *right];
            let mut loud = input[0].abs() > SILENCE || input[1].abs() > SILENCE;
            let mut written = input[0] != 0.0 || input[1] != 0.0;
            let dry =
                [0, 1].map(|side| self.predelay[side].delay(input[side], self.predelay_samples));
            // Half of each side into each of its four lanes: the same energy.
            let mut lanes: [f32; LANES] = std::array::from_fn(|lane| 0.5 * dry[lane * 2 / LANES]);
            let mut taps = [0.0; LANES];
            let last = self.diffuser.len() - 1;
            for (step, (lines, delays)) in self
                .diffuser
                .iter_mut()
                .zip(self.lane_samples.iter())
                .enumerate()
            {
                for ((sample, line), &delay) in lanes.iter_mut().zip(lines.iter_mut()).zip(delays) {
                    loud |= sample.abs() > SILENCE;
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
                mix(&mut lanes, self.butterfly);
            }
            let early = arrangement
                .early_out
                .map(|side| side.iter().map(|&lane| taps[lane]).sum::<f32>());
            // Round the network: each line's output, losing its lap's share
            // of the Decay; the Householder reflection of them all, each less
            // twice their average; then each line written with what comes
            // round and what goes in.
            let mut outputs = [0.0f32; MAX_LINES];
            for ((output, line), (&delay, &gain)) in outputs
                .iter_mut()
                .zip(self.lines.iter())
                .zip(self.line_samples.iter().zip(self.gains.iter()))
            {
                *output = gain * line.read(delay);
            }
            let reflect = 2.0 / count as f32 * outputs[..count].iter().sum::<f32>();
            let mut feed = [0.0f32; MAX_LINES];
            for (side, lines) in arrangement.late_out.iter().enumerate() {
                for &line in lines.iter() {
                    feed[line] += self.dry_send * dry_in * dry[side];
                }
            }
            for (line, feed) in feed.iter_mut().enumerate().take(count) {
                *feed += self.early_send_gain * early_in * lanes[line % LANES];
            }
            for ((line, &output), &feed) in
                self.lines.iter_mut().zip(outputs.iter()).zip(feed.iter())
            {
                let sample = output - reflect + feed;
                loud |= sample.abs() > SILENCE;
                written |= sample != 0.0;
                line.write(sample);
            }
            let late = arrangement
                .late_out
                .map(|side| late_out * side.iter().map(|&line| outputs[line]).sum::<f32>());
            let out = [0, 1].map(|side| {
                let wet = self.early_gain * early[side] + self.late_gain * late[side];
                let Some(gain) = self.tone_gain else {
                    return wet;
                };
                let tone = &mut self.tone[side];
                let filtered = tone.process(f64::from(wet), gain, TONE_Q).low_pass;
                if tone.state() < TONE_REST {
                    *tone = Svf::default();
                }
                filtered as f32
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
        }
        self.quiet = quiet;
    }

    /// While it's awake, Decay, Depth, Diffusion and Early Send glide over a
    /// few milliseconds; the sizes and Predelay go straight to their new
    /// lengths. Asleep, everything goes straight there.
    fn set_settings(&mut self, settings: ReverbSettings) {
        let settings = clamped(settings);
        self.settings = settings;
        if self.is_ringing() {
            self.decay_rate.set_target(1.0 / settings.decay_seconds);
            self.depth.set_target(settings.depth);
            self.diffusion.set_target(settings.diffusion);
            self.early_send.set_target(settings.early_send);
            self.set_lengths();
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
}

#[cfg(test)]
mod tests;
