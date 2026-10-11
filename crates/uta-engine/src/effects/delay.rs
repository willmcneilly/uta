//! The Delay: a tape loop with a recording head, a playback head behind it,
//! and some of what plays back fed round again. See RFC-007, "The Delay".
//!
//! This is the plain Delay: a time and Feedback. Sync, ping-pong, the cuts
//! and saturation in the loop, and Wobble arrive with UTA-70.

use super::Effect;
use crate::VOLUME_SMOOTHING_SECONDS;
use crate::ramp::Ramp;

/// The longest echo the tape holds, in seconds: the core's longest Delay
/// time.
pub const DELAY_TAPE_SECONDS: f64 = uta_core::DelaySettings::MAX_TIME_SECONDS as f64;
/// Tape set aside past the longest time, for Wobble to sway the playback
/// head into (UTA-70).
const WOBBLE_ROOM_SECONDS: f64 = 0.05;
/// How long a new time takes to glide to, so the playback head moves rather
/// than jumps. UTA-70 makes this the tape's repitch.
const TIME_GLIDE_SECONDS: f64 = 0.05;
/// Below this, −120 dB, nothing going in or coming out counts as sound.
const SILENCE: f32 = 1e-6;
/// The closest the playback head gets to the recording head, in samples:
/// cubic interpolation reads two samples newer than the one it's on, and
/// both must already be on the tape.
const MIN_DELAY_SAMPLES: f32 = 3.0;
/// The samples cubic interpolation reads on each side of the playback head,
/// kept clear of the recording head at the longest time.
const READ_MARGIN: usize = 4;

/// The Delay's settings, as the engine plays them: the core's, with the
/// same names, units and ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelaySettings {
    /// The echoes' spacing, from 1 ms to 5 s.
    pub time_seconds: f32,
    /// How much of each echo comes round again, from 0 to 1.
    pub feedback: f32,
}

impl DelaySettings {
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
        Self {
            time_seconds: clamp(
                self.time_seconds,
                Core::MIN_TIME_SECONDS,
                Core::MAX_TIME_SECONDS,
                default.time_seconds,
            ),
            feedback: clamp(
                self.feedback,
                Core::MIN_FEEDBACK,
                Core::MAX_FEEDBACK,
                default.feedback,
            ),
        }
    }
}

impl From<&uta_core::DelaySettings> for DelaySettings {
    fn from(settings: &uta_core::DelaySettings) -> Self {
        Self {
            time_seconds: settings.time_seconds,
            feedback: settings.feedback,
        }
    }
}

/// The Delay's state: a tape for each side, and where the heads are.
pub(crate) struct Delay {
    /// The left and right tapes, each a power of two long so the heads wrap
    /// round with a mask. Set aside when the Delay is made, on the control
    /// side, and never resized.
    tapes: [Box<[f32]>; 2],
    mask: usize,
    /// Where the recording head writes next.
    write: usize,
    sample_rate: f64,
    settings: DelaySettings,
    /// How far behind the recording head the playback head is, in samples.
    time: Ramp,
    feedback: Ramp,
    /// Samples in a row with nothing above [`SILENCE`] going in or coming
    /// out. While it's shorter than the time, echoes are still on the tape.
    quiet: u32,
}

impl Delay {
    /// A Delay with `settings`, with a tape long enough for the longest time
    /// at `sample_rate`. Allocates: control side only.
    pub(crate) fn new(settings: DelaySettings, sample_rate: f64) -> Self {
        let samples = ((DELAY_TAPE_SECONDS + WOBBLE_ROOM_SECONDS) * sample_rate).ceil() as usize;
        let length = (samples + 2 * READ_MARGIN).next_power_of_two();
        let tape = || vec![0.0; length].into_boxed_slice();
        let mut delay = Self {
            tapes: [tape(), tape()],
            mask: length - 1,
            write: 0,
            sample_rate,
            settings: settings.clamped(),
            time: Ramp::new(0.0, 1),
            feedback: Ramp::new(0.0, 1),
            quiet: u32::MAX,
        };
        delay.prepare(sample_rate);
        delay
    }

    /// `settings`' time in samples at the current rate, kept to what the
    /// tape holds.
    fn time_samples(&self, settings: DelaySettings) -> f32 {
        let longest = (self.tapes[0].len() - 2 * READ_MARGIN) as f32;
        ((f64::from(settings.time_seconds) * self.sample_rate) as f32)
            .clamp(MIN_DELAY_SAMPLES, longest)
    }

    /// The samples a glide of `seconds` takes at the current rate.
    fn glide(&self, seconds: f64) -> u32 {
        (seconds * self.sample_rate).round() as u32
    }
}

impl Effect for Delay {
    type Settings = DelaySettings;

    fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time = Ramp::new(
            self.time_samples(self.settings),
            self.glide(TIME_GLIDE_SECONDS),
        );
        self.feedback = Ramp::new(self.settings.feedback, self.glide(VOLUME_SMOOTHING_SECONDS));
    }

    /// For each sample, on each side: reads the echo from the playback head,
    /// writes the input plus Feedback times the echo at the recording head,
    /// and leaves the echo as the output. A click in comes back after
    /// exactly the time, then again Feedback times as loud, and so on.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let mask = self.mask;
        let mut quiet = self.quiet;
        // Bounded by the block.
        for (left, right) in left.iter_mut().zip(right.iter_mut()) {
            let time = self.time.next_value();
            let feedback = self.feedback.next_value();
            // The playback head is `time` behind the recording head: on the
            // sample `whole` behind, plus `fraction` of the way to the one
            // after it. A whole number of samples reads that sample exactly.
            let whole = time.ceil();
            let fraction = whole - time;
            let read = self.write.wrapping_sub(whole as usize);
            let mut frame_loud = false;
            for (sample, tape) in [left, right].into_iter().zip(&mut self.tapes) {
                let at = |offset: usize| tape[read.wrapping_add(offset).wrapping_sub(1) & mask];
                let echo = cubic(at(0), at(1), at(2), at(3), fraction);
                let input = *sample;
                let mut written = input + feedback * echo;
                // Echoes fading towards silence are flushed to zero before
                // they become slow "denormal" numbers.
                if written.abs() < f32::MIN_POSITIVE {
                    written = 0.0;
                }
                tape[self.write] = written;
                *sample = echo;
                frame_loud |= input.abs() > SILENCE || echo.abs() > SILENCE;
            }
            self.write = (self.write + 1) & mask;
            quiet = if frame_loud {
                0
            } else {
                quiet.saturating_add(1)
            };
        }
        self.quiet = quiet;
    }

    fn set_settings(&mut self, settings: DelaySettings) {
        let settings = settings.clamped();
        self.settings = settings;
        self.time.set_target(self.time_samples(settings));
        self.feedback.set_target(settings.feedback);
    }

    /// Ringing until nothing above −120 dB has gone in or come out for a
    /// little longer than the time: by then every echo on its way has
    /// passed the playback head.
    fn is_ringing(&self) -> bool {
        let time = self.time.value().max(self.time_samples(self.settings));
        (self.quiet as f32) <= time + READ_MARGIN as f32
    }

    fn clear(&mut self) {
        for tape in &mut self.tapes {
            tape.fill(0.0);
        }
        self.quiet = u32::MAX;
    }
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
        Delay::new(
            DelaySettings {
                time_seconds,
                feedback,
            },
            sample_rate,
        )
    }

    /// Runs `frames` of a click on both sides at sample 0 through `delay`,
    /// in blocks of `block`, and returns the left and right.
    fn click_through(delay: &mut Delay, frames: usize, block: usize) -> [Vec<f32>; 2] {
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        left[0] = 1.0;
        right[0] = -0.5;
        for (left, right) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
            delay.process(left, right);
        }
        [left, right]
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

    #[test]
    fn each_echo_is_feedback_times_the_one_before() {
        let rate = 48_000.0;
        let at = 12_000;
        for feedback in [0.25, 0.5, 0.9, 1.0] {
            let mut delay = delay(0.25, feedback, rate);
            let [left, _] = click_through(&mut delay, 6 * at, 100);
            let found = echoes(&left);
            let expected: Vec<(usize, f32)> = (1..6)
                .map(|n| (n * at, feedback.powi(n as i32 - 1)))
                .filter(|&(_, level)| level > SILENCE)
                .collect();
            assert_eq!(found.len(), expected.len(), "feedback {feedback}");
            for ((at, level), (want_at, want)) in found.into_iter().zip(expected) {
                assert_eq!(at, want_at, "feedback {feedback}");
                assert!(
                    (level - want).abs() <= want * 1e-6,
                    "feedback {feedback}: echo at {at} is {level}, not {want}"
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
            let slope = 1e-4;
            let mut left: Vec<f32> = (0..13_000).map(|n| n as f32 * slope).collect();
            let mut right = left.clone();
            for (left, right) in left.chunks_mut(128).zip(right.chunks_mut(128)) {
                delay.process(left, right);
            }
            for (n, &echo) in left.iter().enumerate().skip(12_010) {
                let expected = (n as f64 - time) * f64::from(slope);
                assert!(
                    (f64::from(echo) - expected).abs() < 1e-5,
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
        left[0] = 1.0;
        delay.process(&mut left, &mut right);
        assert!(delay.is_ringing());
        // 0.5^20 is below −120 dB: twenty echoes 10 ms apart, then one
        // more time of silence.
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
            (20..=23).contains(&ringing_blocks),
            "rang for {ringing_blocks} blocks of 10 ms"
        );
        assert!(!delay.is_ringing());
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
            feedback: f32::NAN,
        };
        assert_eq!(
            wild.clamped(),
            DelaySettings {
                time_seconds: 5.0,
                feedback: 0.4,
            }
        );
    }
}
