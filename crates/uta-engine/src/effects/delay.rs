//! The Delay: a tape loop with a recording head, a playback head behind it,
//! and some of what plays back fed round again. See RFC-007, "The Delay".
//!
//! Round the loop, each echo goes through the Low cut and the High cut,
//! then the tape's saturation, then back onto the tape, so each repeat is
//! thinner and darker than the last, and loud ones are squashed rather than
//! growing without end. The playback head glides to a new time, bending the
//! echoes on their way in pitch as tape does (repitch), and Wobble sways it
//! with wow and flutter. Ping-pong records both sides onto the left tape and
//! crosses each echo to the other side as it comes round.

use std::f64::consts::{FRAC_1_SQRT_2, TAU};

use uta_core::DelayMode;

use super::Effect;
use crate::VOLUME_SMOOTHING_SECONDS;
use crate::drums::filter::{Svf, prewarp};
use crate::ramp::Ramp;

/// The longest echo the tape holds, in seconds: the core's longest Delay
/// time.
pub const DELAY_TAPE_SECONDS: f64 = uta_core::DelaySettings::MAX_TIME_SECONDS as f64;
/// Tape set aside past the longest time, for Wobble to sway the playback
/// head into: far more than its reach.
const WOBBLE_ROOM_SECONDS: f64 = 0.05;
/// How quickly the playback head glides to a new time. Its place is
/// smoothed twice in a row with this time constant, so it eases away and
/// eases in, like a tape machine's head, and lands within a second.
const REPITCH_SECONDS: f64 = 0.05;
/// The fastest the playback head moves against the tape while it glides,
/// in samples a sample: the echoes play between half speed (an octave
/// down) and one and a half (a fifth up), never backwards.
const MAX_REPITCH: f64 = 0.5;
/// Closer than this to its new time, in samples, the glide lands on it.
const GLIDE_LANDS: f64 = 1e-3;
/// Below this, −120 dB, nothing going in, coming out or going round counts
/// as sound.
const SILENCE: f32 = 1e-6;
/// The closest the playback head gets to the recording head, in samples:
/// cubic interpolation reads two samples newer than the one it's on, and
/// both must already be on the tape.
const MIN_DELAY_SAMPLES: f64 = 3.0;
/// The samples cubic interpolation reads on each side of the playback head,
/// kept clear of the recording head at the longest time.
const READ_MARGIN: usize = 4;
/// The cuts' quality: Butterworth, 12 dB an octave with no bump at the
/// cutoff. Each repeat goes through them once more, so the slope steepens
/// with every echo.
const CUT_Q: f64 = FRAC_1_SQRT_2;
/// Below this, a cut's memory is cleared before it decays into slow
/// "denormal" numbers.
const CUT_REST: f64 = 1e-30;
/// The saturation passes anything up to this level unchanged, so with no
/// Drive the echoes of an ordinary sound are clean.
const KNEE: f32 = 0.5;
/// Above the knee, the saturation curves smoothly towards this level and
/// never passes it: what holds Feedback past 100% at a steady howl.
const CEILING: f32 = 1.0;
/// Full Drive hits the saturation this much harder, in dB. The echoes come
/// out as much quieter again, so Drive changes how squashed they are, not
/// how loud quiet ones are, and the loop's gain stays the Feedback.
const MAX_DRIVE_DB: f32 = 18.0;
/// Wow: two slow sways of the playback head, in Hz, and their shares of
/// it. Two rates that never line up make a drift rather than a wobble.
const WOW: [(f64, f64); 2] = [(0.8, 0.75), (0.31, 0.25)];
/// How far wow sways the playback head at full Wobble, either way.
const WOW_SECONDS: f64 = 0.0025;
/// Flutter: a faster, smaller waver.
const FLUTTER_HZ: f64 = 6.5;
const FLUTTER_SECONDS: f64 = 0.0002;
/// How far Wobble sways the playback head at full Wobble, either way: wow
/// and flutter together.
pub const WOBBLE_REACH_SECONDS: f64 = WOW_SECONDS + FLUTTER_SECONDS;
/// When Play restarts the wobble while echoes ring, how long the playback
/// head takes to glide from where the old sway had it.
const RESTART_GLIDE_SECONDS: f64 = 0.05;

/// The Delay's settings, as the engine plays them: the core's, with the
/// time worked out at the tempo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelaySettings {
    /// The echoes' spacing, from 1 ms to 5 s: a synced time at the tempo,
    /// or the time in seconds.
    pub time_seconds: f64,
    pub mode: DelayMode,
    /// How much of each echo comes round again, from 0 to 1.1.
    pub feedback: f32,
    /// The high-pass and low-pass filters in the loop, in Hz.
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    /// How hard the echoes hit the saturation, from 0 to 1.
    pub drive: f32,
    /// How much the playback head sways, from 0 to 1.
    pub wobble: f32,
}

impl DelaySettings {
    /// The core's settings, played at `bpm`: a synced time is the length of
    /// its division at that tempo, so a tempo change glides the playback
    /// head to the new time as any other change of time does.
    pub fn new(settings: &uta_core::DelaySettings, bpm: f32) -> Self {
        Self {
            time_seconds: settings.time_seconds_at(bpm),
            mode: settings.mode,
            feedback: settings.feedback,
            low_cut_hz: settings.low_cut_hz,
            high_cut_hz: settings.high_cut_hz,
            drive: settings.drive,
            wobble: settings.wobble,
        }
    }

    /// The plain Delay: `time_seconds` apart, with its cuts wide open and
    /// no Drive or Wobble. See [`uta_core::DelaySettings::plain`].
    pub fn plain(time_seconds: f32, feedback: f32) -> Self {
        Self::new(
            &uta_core::DelaySettings::plain(time_seconds, feedback),
            120.0,
        )
    }

    /// These settings with every value inside its range. A value that isn't
    /// a number takes its default, so a bad value never reaches the tape.
    pub fn clamped(self) -> Self {
        use uta_core::DelaySettings as Core;
        let default = Core::default();
        let clamp = |value: f32, min: f32, max: f32, fallback: f32| {
            if value.is_nan() {
                fallback
            } else {
                value.clamp(min, max)
            }
        };
        let time_seconds = if self.time_seconds.is_nan() {
            f64::from(default.time_seconds)
        } else {
            self.time_seconds.clamp(
                f64::from(Core::MIN_TIME_SECONDS),
                f64::from(Core::MAX_TIME_SECONDS),
            )
        };
        Self {
            time_seconds,
            mode: self.mode,
            feedback: clamp(
                self.feedback,
                Core::MIN_FEEDBACK,
                Core::MAX_FEEDBACK,
                default.feedback,
            ),
            low_cut_hz: clamp(
                self.low_cut_hz,
                Core::MIN_LOW_CUT_HZ,
                Core::MAX_LOW_CUT_HZ,
                default.low_cut_hz,
            ),
            high_cut_hz: clamp(
                self.high_cut_hz,
                Core::MIN_HIGH_CUT_HZ,
                Core::MAX_HIGH_CUT_HZ,
                default.high_cut_hz,
            ),
            drive: clamp(
                self.drive,
                Core::MIN_AMOUNT,
                Core::MAX_AMOUNT,
                default.drive,
            ),
            wobble: clamp(
                self.wobble,
                Core::MIN_AMOUNT,
                Core::MAX_AMOUNT,
                default.wobble,
            ),
        }
    }

    /// The gain into the saturation for this Drive.
    fn drive_gain(&self) -> f32 {
        10f32.powf(self.drive * MAX_DRIVE_DB / 20.0)
    }

    /// How far into ping-pong: 0 for stereo, 1 for ping-pong.
    fn ping_pong(&self) -> f32 {
        match self.mode {
            DelayMode::Stereo => 0.0,
            DelayMode::PingPong => 1.0,
        }
    }
}

/// The playback head's place behind the recording head, in samples,
/// gliding to a new time as a tape machine's head would: smoothed twice in
/// a row, so it eases away and eases in, and never faster than
/// [`MAX_REPITCH`]. It moves one sample at a time, so the result doesn't
/// depend on how the audio is split into blocks.
#[derive(Debug, Clone, Copy)]
struct Glide {
    /// The first smoothing, which `current` follows.
    first: f64,
    current: f64,
    target: f64,
    /// How much of the way each smoothing goes each sample.
    coefficient: f64,
}

impl Glide {
    fn new(value: f64, sample_rate: f64) -> Self {
        Self {
            first: value,
            current: value,
            target: value,
            coefficient: 1.0 - (-1.0 / (REPITCH_SECONDS * sample_rate)).exp(),
        }
    }

    fn set_target(&mut self, target: f64) {
        self.target = target;
    }

    fn jump_to(&mut self, value: f64) {
        self.first = value;
        self.current = value;
        self.target = value;
    }

    /// Whether it's still on its way to its target.
    fn is_moving(&self) -> bool {
        self.current != self.target || self.first != self.target
    }

    /// Returns the place for this sample and advances by one.
    #[inline]
    fn next_value(&mut self) -> f64 {
        let value = self.current;
        if self.is_moving() {
            self.first += (self.target - self.first) * self.coefficient;
            self.current +=
                ((self.first - self.current) * self.coefficient).clamp(-MAX_REPITCH, MAX_REPITCH);
            if (self.target - self.first).abs() < GLIDE_LANDS
                && (self.target - self.current).abs() < GLIDE_LANDS
            {
                self.jump_to(self.target);
            }
        }
        value
    }
}

/// The Delay's state: a tape for each side, where the heads are, and the
/// loop's filters.
pub(crate) struct Delay {
    /// The left and right tapes, each a power of two long so the heads wrap
    /// round with a mask. Set aside when the Delay is made, on the control
    /// side, and never resized.
    tapes: [Box<[f32]>; 2],
    mask: usize,
    /// Where the recording head writes next.
    write: usize,
    /// How many of the samples just behind the recording head hold sound
    /// written since the tape was last cleared. The playback head hears
    /// anything further back as silence, so clearing the tape is setting
    /// this to 0, not writing over 5 seconds of it on the audio thread.
    filled: usize,
    sample_rate: f64,
    settings: DelaySettings,
    /// How far behind the recording head the playback head is, in samples,
    /// before Wobble sways it.
    time: Glide,
    feedback: Ramp,
    /// The gain into the saturation.
    drive: Ramp,
    /// How much of its reach Wobble sways the playback head, from 0 to 1.
    wobble: Ramp,
    /// From 0 (stereo) to 1 (ping-pong), so switching Mode crossfades how
    /// the tapes are recorded rather than jumping.
    ping_pong: Ramp,
    /// The cutoffs, in octaves above 1 Hz, gliding as heard.
    low_cut: Ramp,
    high_cut: Ramp,
    /// The cuts' prewarped gains, worked out again only while they glide.
    low_gain: f64,
    high_gain: f64,
    /// Each side's Low cut and High cut.
    cuts: [[Svf; 2]; 2],
    /// Where wow's two sways and flutter are in their cycles, from 0 to 1,
    /// and how far each moves a sample.
    phases: [f64; 3],
    phase_steps: [f64; 3],
    /// After Play restarts the sways while echoes ring: how far the old
    /// sway had the playback head, in samples, gliding to nothing.
    restart: Ramp,
    /// Samples in a row with nothing above [`SILENCE`] going in, coming out
    /// or going round, or `u32::MAX` once it's asleep: the echoes have died
    /// away and the tape is clear.
    quiet: u32,
    #[cfg(test)]
    processed: u64,
}

impl Delay {
    /// A Delay with `settings`, with a tape long enough for the longest time
    /// at `sample_rate`. It starts asleep, with a clear tape. Allocates:
    /// control side only.
    pub(crate) fn new(settings: DelaySettings, sample_rate: f64) -> Self {
        let samples = ((DELAY_TAPE_SECONDS + WOBBLE_ROOM_SECONDS) * sample_rate).ceil() as usize;
        let length = (samples + 2 * READ_MARGIN).next_power_of_two();
        let tape = || vec![0.0; length].into_boxed_slice();
        let mut delay = Self {
            tapes: [tape(), tape()],
            mask: length - 1,
            write: 0,
            filled: 0,
            sample_rate,
            settings: settings.clamped(),
            time: Glide::new(0.0, sample_rate),
            feedback: Ramp::new(0.0, 1),
            drive: Ramp::new(1.0, 1),
            wobble: Ramp::new(0.0, 1),
            ping_pong: Ramp::new(0.0, 1),
            low_cut: Ramp::new(0.0, 1),
            high_cut: Ramp::new(0.0, 1),
            low_gain: 0.0,
            high_gain: 0.0,
            cuts: Default::default(),
            phases: [0.0; 3],
            phase_steps: [0.0; 3],
            restart: Ramp::new(0.0, 1),
            quiet: u32::MAX,
            #[cfg(test)]
            processed: 0,
        };
        delay.prepare(sample_rate);
        delay
    }

    /// The furthest the playback head can be behind the recording head, in
    /// samples, with Wobble.
    fn longest(&self) -> f64 {
        (self.tapes[0].len() - 2 * READ_MARGIN) as f64
    }

    /// `settings`' time in samples at the current rate, kept to what the
    /// tape holds.
    fn time_samples(&self, settings: DelaySettings) -> f64 {
        // Rounded to an `f32`, as precise as a time in seconds set as one,
        // so a time of 0.02 s is 960 samples exactly, not 959.99997.
        let samples = (settings.time_seconds * self.sample_rate) as f32;
        f64::from(samples).clamp(MIN_DELAY_SAMPLES, self.longest())
    }

    /// The samples a glide of `seconds` takes at the current rate.
    fn glide(&self, seconds: f64) -> u32 {
        (seconds * self.sample_rate).round() as u32
    }

    /// How far the sways have the playback head now, in samples, at full
    /// Wobble.
    #[inline]
    fn sway(&self) -> f64 {
        let [first, second, flutter] = self.phases.map(|phase| (TAU * phase).sin());
        let wow = WOW[0].1 * first + WOW[1].1 * second;
        (WOW_SECONDS * wow + FLUTTER_SECONDS * flutter) * self.sample_rate
    }

    /// How many samples it has processed: the recording head has moved on
    /// one for each.
    #[cfg(test)]
    pub(crate) fn processed(&self) -> u64 {
        self.processed
    }

    /// Everything that glides goes straight to its settings: there's
    /// nothing on the tape for it to bend. Real-time safe.
    fn jump_to_settings(&mut self) {
        let settings = self.settings;
        self.time.jump_to(self.time_samples(settings));
        self.feedback.jump_to(settings.feedback);
        self.drive.jump_to(settings.drive_gain());
        self.wobble.jump_to(settings.wobble);
        self.ping_pong.jump_to(settings.ping_pong());
        self.low_cut.jump_to(settings.low_cut_hz.log2());
        self.high_cut.jump_to(settings.high_cut_hz.log2());
        self.restart.jump_to(0.0);
        self.low_gain = prewarp(f64::from(settings.low_cut_hz), self.sample_rate);
        self.high_gain = prewarp(f64::from(settings.high_cut_hz), self.sample_rate);
    }

    /// Goes to sleep: the echoes have died away, so it clears the tape and
    /// the cuts, and everything that glides goes straight to its settings.
    /// Asleep, silence in gives silence out and changes nothing, so the
    /// chain can skip it. Real-time safe, and does no work proportional to
    /// the tape.
    fn sleep(&mut self) {
        self.filled = 0;
        self.quiet = u32::MAX;
        self.cuts = Default::default();
        self.jump_to_settings();
    }
}

impl Effect for Delay {
    type Settings = DelaySettings;

    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time = Glide::new(self.time_samples(self.settings), sample_rate);
        let smoothing = self.glide(VOLUME_SMOOTHING_SECONDS);
        self.feedback = Ramp::new(0.0, smoothing);
        self.drive = Ramp::new(1.0, smoothing);
        self.wobble = Ramp::new(0.0, smoothing);
        self.ping_pong = Ramp::new(0.0, smoothing);
        self.low_cut = Ramp::new(0.0, smoothing);
        self.high_cut = Ramp::new(0.0, smoothing);
        self.restart = Ramp::new(0.0, self.glide(RESTART_GLIDE_SECONDS));
        self.phase_steps = [WOW[0].0, WOW[1].0, FLUTTER_HZ].map(|hz| hz / sample_rate);
        self.jump_to_settings();
    }

    /// For each sample, on each side: reads the echo from the playback
    /// head, sends Feedback times it round the loop (the Low cut, the High
    /// cut, then the saturation), writes the input plus what came round at
    /// the recording head, and leaves the echo as the output. Ping-pong
    /// records both sides of the input on the left tape, and what comes
    /// round each side onto the other's. With Feedback at 0 and no Wobble,
    /// a click in comes back after exactly the time.
    ///
    /// Once nothing above −120 dB has gone in, come out or gone round for a
    /// little longer than the playback head reaches back, every echo on
    /// its way has passed the playback head, and it goes to sleep on that
    /// sample, clearing what's left on the tape. Anything that reaches the
    /// tape wakes it: any input except silence and values so small they're
    /// flushed to zero on the way.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let mask = self.mask;
        let length = mask + 1;
        let longest = self.longest();
        let settled_time = self.time_samples(self.settings);
        let reach = WOBBLE_REACH_SECONDS * self.sample_rate;
        let target_wobble = self.settings.wobble;
        let mut quiet = self.quiet;
        #[cfg(test)]
        {
            self.processed += left.len() as u64;
        }
        // Bounded by the block.
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            let base = self.time.next_value();
            let wobble = self.wobble.next_value();
            let restart = self.restart.next_value();
            let sway = if wobble == 0.0 {
                0.0
            } else {
                f64::from(wobble) * self.sway()
            };
            let time = (base + sway + f64::from(restart)).clamp(MIN_DELAY_SAMPLES, longest);
            // The playback head is `time` behind the recording head: on the
            // sample `whole` behind, plus `fraction` of the way to the one
            // after it. A whole number of samples reads that sample exactly.
            let whole = time.ceil();
            let fraction = (whole - time) as f32;
            let whole = whole as usize;
            let read = self.write.wrapping_sub(whole);
            // Cubic interpolation reads the samples `whole + 1` to
            // `whole - 2` behind the recording head. Usually all of them
            // were written since the tape was cleared; when they weren't,
            // the ones that weren't are silence.
            let filled = self.filled;
            let all_filled = whole < filled;
            let echoes = [0, 1].map(|side| {
                let tape = &self.tapes[side];
                let at = |offset: usize| {
                    if all_filled || whole + 1 - offset <= filled {
                        tape[read.wrapping_add(offset).wrapping_sub(1) & mask]
                    } else {
                        0.0
                    }
                };
                cubic(at(0), at(1), at(2), at(3), fraction)
            });
            let feedback = self.feedback.next_value();
            let drive = self.drive.next_value();
            let ping_pong = self.ping_pong.next_value();
            if self.low_cut.is_moving() {
                self.low_cut.next_value();
                self.low_gain = prewarp(f64::from(self.low_cut.value()).exp2(), self.sample_rate);
            }
            if self.high_cut.is_moving() {
                self.high_cut.next_value();
                self.high_gain = prewarp(f64::from(self.high_cut.value()).exp2(), self.sample_rate);
            }
            // Round the loop: thinner, darker, then onto the tape.
            let round = [0, 1].map(|side| {
                let [low, high] = &mut self.cuts[side];
                let thinned = low
                    .process(f64::from(feedback * echoes[side]), self.low_gain, CUT_Q)
                    .high_pass;
                let darkened = high.process(thinned, self.high_gain, CUT_Q).low_pass;
                for cut in [low, high] {
                    if cut.state() < CUT_REST {
                        *cut = Svf::default();
                    }
                }
                saturate(darkened as f32, drive)
            });
            let input = [*left, *right];
            let stereo = [input[0] + round[0], input[1] + round[1]];
            let written = if ping_pong == 0.0 {
                stereo
            } else {
                let crossed = [
                    (input[0] + input[1]) * FRAC_1_SQRT_2 as f32 + round[1],
                    round[0],
                ];
                [0, 1].map(|side| stereo[side] + ping_pong * (crossed[side] - stereo[side]))
            };
            let mut frame_loud = false;
            let mut frame_written = false;
            for side in 0..2 {
                let mut written = written[side];
                // Echoes fading towards silence are flushed to zero before
                // they become slow "denormal" numbers.
                if written.abs() < f32::MIN_POSITIVE {
                    written = 0.0;
                }
                self.tapes[side][self.write] = written;
                frame_loud |= input[side].abs() > SILENCE
                    || echoes[side].abs() > SILENCE
                    || written.abs() > SILENCE;
                frame_written |= written != 0.0;
            }
            *left = echoes[0];
            *right = echoes[1];
            self.write = (self.write + 1) & mask;
            self.filled = (self.filled + 1).min(length);
            if frame_loud {
                quiet = 0;
            } else if quiet == u32::MAX {
                // Asleep, the tape is clear. Anything put on it, however
                // quiet, wakes it, so it only sleeps with a clear tape, and
                // it's never skipped while it holds sound.
                if frame_written {
                    quiet = 0;
                }
            } else {
                quiet += 1;
                // As far back as the playback head can reach before the
                // sound already on the tape has gone past it.
                let furthest = base.max(settled_time)
                    + reach * f64::from(wobble.max(target_wobble))
                    + f64::from(restart.abs());
                if f64::from(quiet) > furthest + READ_MARGIN as f64 {
                    self.sleep();
                    quiet = u32::MAX;
                }
            }
            // The sways move on only while it's awake, so asleep it changes
            // nothing, as the chain expects when it skips it.
            if quiet != u32::MAX {
                for (phase, step) in self.phases.iter_mut().zip(self.phase_steps) {
                    *phase += step;
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                }
            }
        }
        self.quiet = quiet;
    }

    /// While it's awake, everything glides: the playback head to its new
    /// time, bending the echoes on their way, and the rest over a few
    /// milliseconds. Asleep, they go straight there.
    fn set_settings(&mut self, settings: DelaySettings) {
        let settings = settings.clamped();
        self.settings = settings;
        if self.is_ringing() {
            self.time.set_target(self.time_samples(settings));
            self.feedback.set_target(settings.feedback);
            self.drive.set_target(settings.drive_gain());
            self.wobble.set_target(settings.wobble);
            self.ping_pong.set_target(settings.ping_pong());
            self.low_cut.set_target(settings.low_cut_hz.log2());
            self.high_cut.set_target(settings.high_cut_hz.log2());
        } else {
            self.jump_to_settings();
        }
    }

    /// Ringing until it goes to sleep: see [`Delay::process`].
    fn is_ringing(&self) -> bool {
        self.quiet != u32::MAX
    }

    fn clear(&mut self) {
        self.sleep();
    }

    /// Wow and flutter start their cycles again, so what you hear from the
    /// top is what a render plays (RFC-007, open question 4). If echoes are
    /// ringing, the playback head glides from where the old sway had it
    /// rather than jumping.
    fn restart(&mut self) {
        let before = f64::from(self.wobble.value()) * self.sway() + f64::from(self.restart.value());
        self.phases = [0.0; 3];
        if self.is_ringing() && before != 0.0 {
            self.restart.jump_to(before as f32);
            self.restart.set_target(0.0);
        } else {
            self.restart.jump_to(0.0);
        }
    }
}

/// The tape's saturation, hit `gain` times harder and coming out as much
/// quieter: anything up to [`KNEE`] (after the gain) passes unchanged, and
/// louder curves smoothly towards [`CEILING`], never past it. Its slope is
/// never more than 1, so the loop's gain is never more than the Feedback.
#[inline]
fn saturate(sample: f32, gain: f32) -> f32 {
    let driven = sample * gain;
    let level = driven.abs();
    if level <= KNEE {
        return sample;
    }
    let room = CEILING - KNEE;
    (KNEE + room * ((level - KNEE) / room).tanh()).copysign(driven) / gain
}

/// A smooth curve through four samples in a row (a Catmull-Rom spline),
/// read `t` of the way from `x0` to `x1`: cubic interpolation. At `t = 0` it
/// is `x0` exactly. See RFC-007, "Reading between samples".
#[inline]
fn cubic(before: f32, x0: f32, x1: f32, after: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x1 - before);
    let c2 = before - 2.5 * x0 + 2.0 * x1 - 0.5 * after;
    let c3 = 0.5 * (after - before) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delay(time_seconds: f32, feedback: f32, sample_rate: f64) -> Delay {
        Delay::new(DelaySettings::plain(time_seconds, feedback), sample_rate)
    }

    /// Runs `frames` of a click on both sides at sample 0 through `delay`,
    /// in blocks of `block`, and returns the left and right.
    fn click_through(delay: &mut Delay, frames: usize, block: usize) -> [Vec<f32>; 2] {
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        left[0] = 1.0;
        right[0] = -0.5;
        run(delay, &mut left, &mut right, block);
        [left, right]
    }

    fn run(delay: &mut Delay, left: &mut [f32], right: &mut [f32], block: usize) {
        for (left, right) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
            delay.process(left, right);
        }
    }

    /// A short burst of a 1 kHz tone at `level`, faded in and out (a Hann
    /// window), then silence: `frames` long.
    fn burst(level: f32, frames: usize) -> Vec<f32> {
        let length = 240;
        (0..frames)
            .map(|n| {
                if n >= length {
                    return 0.0;
                }
                let t = n as f64 / length as f64;
                let window = 0.5 - 0.5 * (TAU * t).cos();
                (f64::from(level) * window * (TAU * 1000.0 * n as f64 / 48_000.0).sin()) as f32
            })
            .collect()
    }

    /// How loud the echo of a burst sent at 0 is, `at` samples on: the
    /// root mean square over the burst and a little either side, which the
    /// cuts' small shifts in phase don't change.
    fn level_of_echo(samples: &[f32], at: usize) -> f32 {
        let window = &samples[at - 60..at + 360];
        (window.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / window.len() as f64).sqrt()
            as f32
    }

    /// Where each sample above silence is, and its value.
    fn echoes(samples: &[f32]) -> Vec<(usize, f32)> {
        samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.abs() > SILENCE)
            .map(|(at, &sample)| (at, sample))
            .collect()
    }

    #[test]
    fn a_click_comes_back_on_exactly_the_right_sample_at_each_rate() {
        for rate in [44_100.0, 48_000.0, 96_000.0] {
            let mut delay = delay(0.25, 0.0, rate);
            let at = (0.25 * rate) as usize;
            let [left, right] = click_through(&mut delay, 3 * at, 128);
            assert_eq!(echoes(&left), [(at, 1.0)], "left at {rate} Hz");
            assert_eq!(echoes(&right), [(at, -0.5)], "right at {rate} Hz");
        }
    }

    /// With the cuts wide open and no Drive, each echo of a burst is
    /// Feedback times the one before.
    #[test]
    fn each_echo_is_feedback_times_the_one_before() {
        let rate = 48_000.0;
        let at = 12_000;
        for feedback in [0.25, 0.5, 0.9, 1.0] {
            let mut delay = delay(0.25, feedback, rate);
            let mut left = burst(0.4, 6 * at);
            let mut right = left.clone();
            run(&mut delay, &mut left, &mut right, 100);
            let first = level_of_echo(&left, at);
            for n in 2..6 {
                let level = level_of_echo(&left, n * at);
                let want = first * feedback.powi(n as i32 - 1);
                assert!(
                    (level - want).abs() <= want * 2e-3,
                    "feedback {feedback}: echo {n} is {level}, not {want}"
                );
            }
        }
    }

    #[test]
    fn between_samples_the_curve_passes_through_its_points() {
        let points = [0.0, 1.0, -0.5, 0.25];
        assert_eq!(cubic(points[0], points[1], points[2], points[3], 0.0), 1.0);
        assert!((cubic(points[0], points[1], points[2], points[3], 1.0) - -0.5).abs() < 1e-6);
        // A straight line stays straight: no dulling on a ramp.
        assert!((cubic(0.0, 1.0, 2.0, 3.0, 0.25) - 1.25).abs() < 1e-6);
        // A time between samples lands between them, and the echo is spread
        // only over its neighbours.
        let mut delay = delay(0.25 + 0.5 / 48_000.0, 0.0, 48_000.0);
        let [left, _] = click_through(&mut delay, 13_000, 128);
        let heard: Vec<usize> = echoes(&left).iter().map(|&(at, _)| at).collect();
        assert_eq!(heard, [11_999, 12_000, 12_001, 12_002]);
    }

    /// Between samples, the playback head reads the right way: `time`
    /// behind the recording head, not a sample's fraction the other way.
    /// Cubic interpolation is exact on a straight line, so a ramp comes
    /// back exactly `time` later.
    #[test]
    fn a_time_between_samples_reads_the_tape_that_far_back() {
        let rate = 48_000.0;
        for quarters in [1.0, 3.0] {
            let time = 12_000.0 + quarters / 4.0;
            let mut delay = delay((time / rate) as f32, 0.0, rate);
            let slope = 1e-5;
            let mut left: Vec<f32> = (0..13_000).map(|n| n as f32 * slope).collect();
            let mut right = left.clone();
            run(&mut delay, &mut left, &mut right, 128);
            for (n, &echo) in left.iter().enumerate().skip(12_010) {
                let expected = (n as f64 - time) * f64::from(slope);
                assert!(
                    (f64::from(echo) - expected).abs() < 1e-6,
                    "{time} samples back, at {n}: {echo}, not {expected}"
                );
            }
        }
    }

    #[test]
    fn it_rings_until_the_echoes_die_away_then_is_quiet() {
        let mut delay = delay(0.01, 0.5, 48_000.0);
        assert!(!delay.is_ringing(), "a new Delay is silent");
        let mut left = vec![0.0; 48];
        let mut right = vec![0.0; 48];
        left[0] = 0.5;
        delay.process(&mut left, &mut right);
        assert!(delay.is_ringing());
        // 0.5^20 is below −120 dB: about twenty echoes 10 ms apart, then
        // one more time of silence.
        let silent = vec![0.0; 48 * 10];
        let mut ringing_blocks = 0;
        for _ in 0..100 {
            let (mut left, mut right) = (silent.clone(), silent.clone());
            delay.process(&mut left, &mut right);
            if delay.is_ringing() {
                ringing_blocks += 1;
            }
        }
        assert!(
            (19..=23).contains(&ringing_blocks),
            "rang for {ringing_blocks} blocks of 10 ms"
        );
        assert!(!delay.is_ringing());
    }

    /// Sleeping never cuts off a quiet tail: a burst at −60 dB with 90%
    /// feedback echoes over 60 times above −120 dB, over a second and a
    /// half, and every echo is heard at its level. Only once nothing above
    /// −120 dB has come out for a little longer than the time does it go
    /// to sleep, and from then on it's silent.
    #[test]
    fn a_long_quiet_tail_plays_out_before_it_sleeps() {
        let spacing = 960;
        let mut delay = delay(0.02, 0.9, 48_000.0);
        let frames = 80_000;
        let mut left = burst(1e-3, frames);
        let mut right = left.clone();
        // A sample at a time, to find the one it falls asleep on.
        let mut asleep_at = None;
        for (at, (left, right)) in left.chunks_mut(1).zip(right.chunks_mut(1)).enumerate() {
            delay.process(left, right);
            if asleep_at.is_none() && at > 0 && !delay.is_ringing() {
                asleep_at = Some(at);
            }
        }
        let first = level_of_echo(&left, spacing);
        for n in 2..=60 {
            let level = level_of_echo(&left, n * spacing);
            let want = first * 0.9f32.powi(n as i32 - 1);
            assert!(
                (level - want).abs() <= want * 0.02,
                "echo {n}: {level}, not {want}"
            );
        }
        let last = left
            .iter()
            .rposition(|sample| sample.abs() > SILENCE)
            .expect("echoes");
        assert!(last > 60 * spacing, "the last echo at {last}");
        let asleep_at = asleep_at.expect("it falls asleep");
        assert!(
            (last + spacing..=last + spacing + READ_MARGIN + 2).contains(&asleep_at),
            "asleep at {asleep_at}, the last sound at {last}"
        );
        // What's left on the tape is below −120 dB, and goes as it sleeps.
        assert!(left[asleep_at].abs() < SILENCE);
        assert!(left[asleep_at + 1..].iter().all(|&sample| sample == 0.0));
    }

    /// Asleep, its tape is clear: sound too quiet to count as sound, put
    /// on the tape, wakes it, and it sleeps again only once that has
    /// passed the playback head, clearing the tape as it goes.
    #[test]
    fn anything_put_on_the_tape_wakes_it() {
        let mut delay = delay(0.01, 0.5, 48_000.0);
        assert!(!delay.is_ringing());
        let mut out = Vec::new();
        let mut ringing = 0;
        for block in 0..20 {
            let (mut left, mut right) = (vec![0.0; 128], vec![0.0; 128]);
            if block == 0 {
                left[10] = 1e-7;
            }
            delay.process(&mut left, &mut right);
            if block == 0 {
                assert!(delay.is_ringing(), "1e-7 is on the tape");
            } else if delay.is_ringing() {
                ringing += 1;
            }
            out.extend(left);
        }
        // Nothing above −120 dB comes in or out, so it sleeps a little
        // longer than the time after it woke: on sample 10 + 485, in the
        // third block after the first.
        assert_eq!(ringing, 2, "rang for {ringing} more blocks");
        assert_eq!(out[490], 1e-7, "its echo");
        // Going to sleep cleared the tape: the next echo, at half that,
        // never comes.
        assert!(out[496..].iter().all(|&sample| sample == 0.0));
    }

    /// Asleep, a new time doesn't glide, as there's nothing on the tape to
    /// move: the next click comes back exactly on it. Awake, it glides.
    #[test]
    fn asleep_its_settings_go_straight_to_where_theyre_going() {
        let rate = 48_000.0;
        let mut asleep = delay(0.01, 0.0, rate);
        asleep.set_settings(DelaySettings::plain(0.02, 0.0));
        let [left, _] = click_through(&mut asleep, 2000, 128);
        assert_eq!(echoes(&left), [(960, 1.0)]);

        let mut awake = delay(0.01, 0.0, rate);
        let mut left = vec![1e-3; 16];
        let mut right = left.clone();
        awake.process(&mut left, &mut right);
        awake.set_settings(DelaySettings::plain(0.02, 0.0));
        assert!(awake.time.is_moving(), "awake, the time glides");
    }

    /// The playback head eases away from its old time and into its new
    /// one, never moving against the tape faster than half speed either
    /// way, and lands on it exactly.
    #[test]
    fn the_playback_head_glides_and_lands() {
        for (from, to) in [(4_800.0, 24_000.0), (24_000.0, 4_800.0), (100.0, 101.5)] {
            let mut glide = Glide::new(from, 48_000.0);
            glide.set_target(to);
            let mut last = glide.next_value();
            assert_eq!(last, from);
            let mut fastest: f64 = 0.0;
            let mut samples = 0;
            while glide.is_moving() {
                let value = glide.next_value();
                fastest = fastest.max((value - last).abs());
                last = value;
                samples += 1;
                assert!(samples < 3 * 48_000, "{from} to {to} never lands");
            }
            assert_eq!(glide.next_value(), to);
            assert!(fastest <= MAX_REPITCH, "{from} to {to}: {fastest}");
            // It eases away: the first step is tiny.
            let mut glide = Glide::new(from, 48_000.0);
            glide.set_target(to);
            glide.next_value();
            assert!((glide.next_value() - from).abs() < 0.01);
        }
    }

    /// The saturation leaves anything under its knee alone, curves smoothly
    /// above it and never passes its ceiling. Drive hits it harder and
    /// brings the result down as much, so it squashes sooner.
    #[test]
    fn the_saturation_is_clean_below_its_knee_and_bounded_above() {
        for sample in [0.0, 0.1, -0.3, 0.5, -0.5] {
            assert_eq!(saturate(sample, 1.0), sample);
        }
        for gain in [1.0, 2.0, 8.0] {
            let mut last = 0.0;
            for n in 1..2000 {
                let sample = n as f32 * 0.005;
                let out = saturate(sample, gain);
                assert!(out <= CEILING / gain, "{sample} at {gain}: {out}");
                assert!(out >= last, "{sample} at {gain}: it never turns back");
                // Its slope is never more than 1.
                assert!(out - last <= 0.005 + 1e-6, "{sample} at {gain}");
                assert_eq!(saturate(-sample, gain), -out);
                last = out;
            }
        }
        // Full Drive squashes an echo at half full scale to a fraction.
        let gain = DelaySettings {
            drive: 1.0,
            ..DelaySettings::plain(0.1, 0.5)
        }
        .drive_gain();
        assert!((gain - 7.943).abs() < 1e-3);
        assert!(saturate(0.5, gain) < 0.13);
    }

    /// Wow and flutter restart from the same point, and with Wobble at
    /// full the playback head sways by about its reach either way.
    #[test]
    fn the_sways_restart_and_reach_about_as_far_as_set() {
        let rate = 48_000.0;
        let mut delay = Delay::new(
            DelaySettings {
                wobble: 1.0,
                ..DelaySettings::plain(0.1, 0.0)
            },
            rate,
        );
        let mut furthest: f64 = 0.0;
        let mut sways = Vec::new();
        for _ in 0..(10.0 * rate) as usize {
            let sway = delay.sway();
            furthest = furthest.max(sway.abs());
            sways.push(sway);
            for (phase, step) in delay.phases.iter_mut().zip(delay.phase_steps) {
                *phase = (*phase + step).fract();
            }
        }
        let reach = WOBBLE_REACH_SECONDS * rate;
        assert!(
            (0.8 * reach..=reach).contains(&furthest),
            "{furthest} of {reach}"
        );
        delay.restart();
        assert_eq!(delay.phases, [0.0; 3]);
        assert_eq!(delay.sway(), sways[0]);
        // Asleep, nothing is ringing to glide.
        assert_eq!(delay.restart.value(), 0.0);
    }

    #[test]
    fn clearing_forgets_every_echo() {
        let mut delay = delay(0.01, 1.0, 48_000.0);
        let [left, _] = click_through(&mut delay, 1000, 128);
        assert!(!echoes(&left).is_empty() && delay.is_ringing());
        delay.clear();
        assert!(!delay.is_ringing());
        let mut silent = [vec![0.0; 2000], vec![0.0; 2000]];
        let [left, right] = &mut silent;
        delay.process(left, right);
        assert!(echoes(left).is_empty() && echoes(right).is_empty());
    }

    #[test]
    fn the_tape_holds_five_seconds_at_any_rate() {
        for rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            let mut delay = delay(5.0, 0.0, rate);
            let at = (5.0 * rate) as usize;
            let [left, _] = click_through(&mut delay, at + 10, 1024);
            assert_eq!(echoes(&left), [(at, 1.0)], "at {rate} Hz");
        }
    }

    #[test]
    fn out_of_range_settings_are_clamped_and_nan_is_the_default() {
        let wild = DelaySettings {
            time_seconds: 60.0,
            mode: DelayMode::PingPong,
            feedback: f32::NAN,
            low_cut_hz: 1.0,
            high_cut_hz: 1e6,
            drive: -1.0,
            wobble: f32::NAN,
        };
        assert_eq!(
            wild.clamped(),
            DelaySettings {
                time_seconds: 5.0,
                mode: DelayMode::PingPong,
                feedback: 0.4,
                low_cut_hz: 20.0,
                high_cut_hz: 20_000.0,
                drive: 0.0,
                wobble: 0.1,
            }
        );
    }
}
