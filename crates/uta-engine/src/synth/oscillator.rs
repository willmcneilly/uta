//! A band-limited oscillator using PolyBLEP.
//!
//! A naive saw or square jumps once or twice a cycle, and those jumps carry
//! harmonics far above half the sample rate, which fold back down as
//! "aliasing": tones that aren't in tune with the note. PolyBLEP smooths each
//! jump over the two samples around it with a small polynomial, which removes
//! most of the aliasing cheaply. The triangle has corners rather than jumps,
//! and gets the integrated version (PolyBLAMP) at each corner.
//!
//! The pulse is one saw minus another shifted by its width, so its two jumps
//! are each a saw's, corrected the same way, and it never drifts off centre
//! whatever the width.
//!
//! The sub-oscillator is a square that flips each time oscillator 1 wraps
//! (every second wrap for two octaves down), like the Juno's divider. Its
//! jumps fall where oscillator 1 wraps, so it gets its PolyBLEP correction
//! from oscillator 1's own phase. See RFC-008, "In the engine".
//!
//! A [`Synced`] oscillator restarts its cycle each time its leader,
//! oscillator 1, starts one (hard sync). The restart is a jump of any size,
//! and for the triangle and sine a corner too, at a point between two
//! samples that the leader's phase gives. So it finds each jump and corner in
//! the step to the next sample, natural or from a restart, and corrects the
//! samples either side by its size: PolyBLEP for the jump, PolyBLAMP for the
//! corner.
//!
//! References: Välimäki and Huovilainen, "Antialiasing oscillators in
//! subtractive synthesis" (2007), and Esqueda, Välimäki and Bilbao,
//! "Rounding corners with BLAMP" (DAFx 2016).

use std::f64::consts::TAU;

use super::Waveform;

/// The width of a square: half of each cycle high.
pub(crate) const SQUARE: f64 = 0.5;

/// An oscillator's phase and speed. Its output for any waveform is worked out
/// from the same phase, so the voice can crossfade between two waveforms.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Oscillator {
    /// Where it is in the cycle, 0..1.
    phase: f64,
    /// How far it moves per sample, in cycles.
    increment: f64,
}

impl Oscillator {
    /// Starts at `phase` in the cycle, from 0 to 1: 0 is the start.
    pub(crate) fn start_at(&mut self, phase: f64) {
        self.phase = phase;
    }

    /// Sets how far it moves each sample, in cycles: its frequency over the
    /// sample rate.
    #[inline]
    pub(crate) fn set_increment(&mut self, increment: f64) {
        self.increment = increment;
    }

    /// The output for this sample, from -1 to 1, or for a pulse narrower or
    /// wider than a square, up to 2 - 2 × its width: a pulse's `width` is
    /// how much of each cycle it spends high.
    #[inline]
    pub(crate) fn value(&self, waveform: Waveform, width: f64) -> f32 {
        let t = self.phase;
        let dt = self.increment;
        let value = match waveform {
            Waveform::Sine => (t * TAU).sin(),
            Waveform::Saw => saw(t, dt),
            Waveform::Pulse => pulse(t, dt, width),
            Waveform::Triangle => {
                // Rises from -1 to 1 over the first half, falls back over the
                // second. Its slope changes by 8 per cycle at each corner.
                let naive = 1.0 - 4.0 * (t - 0.5).abs();
                naive + 8.0 * dt * (poly_blamp(t, dt) - poly_blamp((t + 0.5).fract(), dt))
            }
        };
        value as f32
    }

    /// The sub-oscillator under this one: a square `divide` times lower (2
    /// or 4), from -1 to 1. `cycle` counts this oscillator's wraps; the sub
    /// is the same at `cycle` and `cycle + divide`.
    #[inline]
    pub(crate) fn sub_value(&self, cycle: u32, divide: u32) -> f32 {
        let divide = f64::from(divide);
        let t = (f64::from(cycle) % divide + self.phase) / divide;
        square(t, self.increment / divide) as f32
    }

    /// When it next wraps, if it does before the next sample: how far
    /// between this sample and the next, from just after 0 to 1.
    #[inline]
    fn wraps_at(&self) -> Option<f64> {
        (self.phase + self.increment >= 1.0).then(|| (1.0 - self.phase) / self.increment)
    }

    /// Moves on by one sample, and says whether it wrapped round to the
    /// start of a cycle.
    #[inline]
    pub(crate) fn advance(&mut self) -> bool {
        let next = self.phase + self.increment;
        self.phase = next.fract();
        next >= 1.0
    }
}

/// A saw at phase `t` with `dt` cycles per sample: rising from -1 to 1, with
/// a PolyBLEP correction at its jump back down.
#[inline]
fn saw(t: f64, dt: f64) -> f64 {
    2.0 * t - 1.0 - poly_blep(t, dt)
}

/// A pulse at phase `t` with `dt` cycles per sample, high for `width` of the
/// cycle: a saw shifted by the width, minus the saw. The two saws' slopes
/// cancel, leaving a jump up at the start of the cycle and down at the
/// width, each with its saw's correction. It's 2 - 2 × `width` while high
/// and -2 × `width` while low, so it averages zero at any width.
#[inline]
fn pulse(t: f64, dt: f64, width: f64) -> f64 {
    saw((t + 1.0 - width).fract(), dt) - saw(t, dt)
}

/// A square at phase `t` with `dt` cycles per sample: up for the first half
/// of the cycle, down for the second, with a PolyBLEP correction at each
/// jump. The sub's wave.
#[inline]
fn square(t: f64, dt: f64) -> f64 {
    let naive = if t < 0.5 { 1.0 } else { -1.0 };
    naive + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt)
}

/// The most jumps and corners a synced oscillator's step can have: a
/// restart, and up to four half cycles either side of it, which is enough
/// for any pitch up to three times the sample rate's half.
const MAX_EVENTS: usize = 9;

/// The half cycles crossed either side of a restart that a step looks for:
/// more would be far above half the sample rate.
const MAX_CROSSINGS: usize = 4;

/// An oscillator that restarts its cycle where its leader starts one: hard
/// sync. Its waves are oscillator 2's: a pulse is a square.
///
/// For each sample it finds what happens in the step to the next one, from
/// both oscillators' phases: the wave's own jumps and corners (a saw's jump
/// at the end of its cycle, a square's two, a triangle's two corners) and
/// the restart, where the wave jumps from wherever it had got to back to its
/// start, and its slope changes. Each gets a correction sized by its jump
/// and its change of slope, spread over this sample and the next, at the
/// point between them where it falls.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Synced {
    /// Where it is in the cycle, 0..1.
    phase: f64,
    /// How far it moves per sample, in cycles.
    increment: f64,
    /// For each wave, the second half of the corrections for the last step,
    /// which falls on this sample. In [`Waveform`] order.
    carry: [f64; 4],
}

/// What happens to a synced oscillator between one sample and the next.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Step {
    events: [Event; MAX_EVENTS],
    count: usize,
    /// Where it is at the next sample.
    next_phase: f64,
}

/// A jump or a corner in a step.
#[derive(Debug, Clone, Copy)]
struct Event {
    /// How far between this sample and the next, from just after 0 to 1.
    at: f64,
    kind: EventKind,
}

#[derive(Debug, Clone, Copy)]
enum EventKind {
    /// The end of the cycle: a saw jumps down, a square up, and a
    /// triangle's slope turns up.
    Wrap,
    /// Half way through the cycle: a square jumps down, and a triangle's
    /// slope turns down.
    Half,
    /// The leader started a cycle, so this one restarts from `from`.
    Restart { from: f64 },
}

impl Synced {
    /// Starts where it would be had it followed `leader` since the leader's
    /// last restart, moving `increment` cycles a sample.
    pub(crate) fn start_with(&mut self, leader: &Oscillator, increment: f64) {
        self.phase = if leader.increment > 0.0 {
            (leader.phase * increment / leader.increment).fract()
        } else {
            0.0
        };
        self.increment = increment;
        self.carry = [0.0; 4];
    }

    /// Sets how far it moves each sample, in cycles.
    #[inline]
    pub(crate) fn set_increment(&mut self, increment: f64) {
        self.increment = increment;
    }

    /// What happens between this sample and the next, given its leader at
    /// this sample.
    #[inline]
    pub(crate) fn step(&self, leader: &Oscillator) -> Step {
        let dt = self.increment;
        let mut step = Step {
            events: [Event {
                at: 0.0,
                kind: EventKind::Wrap,
            }; MAX_EVENTS],
            count: 0,
            next_phase: 0.0,
        };
        match leader.wraps_at() {
            Some(at) => {
                step.crossings(self.phase, dt, 0.0, at);
                let from = (self.phase + at * dt).fract();
                step.push(at, EventKind::Restart { from });
                step.crossings(0.0, dt, at, 1.0 - at);
                step.next_phase = ((1.0 - at) * dt).fract();
            }
            None => {
                step.crossings(self.phase, dt, 0.0, 1.0);
                step.next_phase = (self.phase + dt).fract();
            }
        }
        step
    }

    /// The output for this sample, from -1 to 1, with the first half of the
    /// corrections for `step`.
    #[inline]
    pub(crate) fn value(&self, waveform: Waveform, step: &Step) -> f32 {
        let dt = self.increment;
        let mut value = naive(waveform, self.phase) + self.carry[waveform as usize];
        for event in &step.events[..step.count] {
            let (jump, bend) = event.size(waveform, dt);
            let before = 1.0 - event.at;
            value += jump / 2.0 * before * before + bend * before * before * before / 6.0;
        }
        value as f32
    }

    /// Moves on to the next sample, carrying the second half of `step`'s
    /// corrections to it.
    #[inline]
    pub(crate) fn advance(&mut self, step: &Step) {
        let dt = self.increment;
        for waveform in Waveform::ALL {
            let mut carry = 0.0;
            for event in &step.events[..step.count] {
                let (jump, bend) = event.size(waveform, dt);
                let after = event.at;
                carry += -jump / 2.0 * after * after + bend * after * after * after / 6.0;
            }
            self.carry[waveform as usize] = carry;
        }
        self.phase = step.next_phase;
    }
}

impl Step {
    fn push(&mut self, at: f64, kind: EventKind) {
        if self.count < MAX_EVENTS {
            self.events[self.count] = Event { at, kind };
            self.count += 1;
        }
    }

    /// Adds the half cycles crossed moving `span` of a sample from `phase`,
    /// at `dt` cycles a sample, starting `start` of the way to the next
    /// sample.
    fn crossings(&mut self, phase: f64, dt: f64, start: f64, span: f64) {
        if dt <= 0.0 {
            return;
        }
        let end = phase + span * dt;
        let first = (phase * 2.0).floor() as u32 + 1;
        for half in (first..).take(MAX_CROSSINGS) {
            let at = f64::from(half) / 2.0;
            if at > end {
                break;
            }
            let kind = if half.is_multiple_of(2) {
                EventKind::Wrap
            } else {
                EventKind::Half
            };
            self.push(start + (at - phase) / dt, kind);
        }
    }
}

impl Event {
    /// The jump in `waveform`'s value here, and the change in its slope in
    /// one sample, at `dt` cycles a sample.
    #[inline]
    fn size(&self, waveform: Waveform, dt: f64) -> (f64, f64) {
        use Waveform::*;
        match (self.kind, waveform) {
            (EventKind::Wrap, Saw) => (-2.0, 0.0),
            (EventKind::Wrap, Pulse) => (2.0, 0.0),
            (EventKind::Wrap, Triangle) => (0.0, 8.0 * dt),
            (EventKind::Half, Pulse) => (-2.0, 0.0),
            (EventKind::Half, Triangle) => (0.0, -8.0 * dt),
            (EventKind::Wrap, Sine) | (EventKind::Half, Sine | Saw) => (0.0, 0.0),
            (EventKind::Restart { from }, _) => (
                naive(waveform, 0.0) - naive(waveform, from),
                (slope(waveform, 0.0) - slope(waveform, from)) * dt,
            ),
        }
    }
}

/// A wave at phase `t` with nothing smoothed: oscillator 2's waves, with a
/// square for the pulse.
#[inline]
fn naive(waveform: Waveform, t: f64) -> f64 {
    match waveform {
        Waveform::Sine => (t * TAU).sin(),
        Waveform::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
        Waveform::Saw => 2.0 * t - 1.0,
        Waveform::Pulse => {
            if t < SQUARE {
                1.0
            } else {
                -1.0
            }
        }
    }
}

/// How fast [`naive`] changes at phase `t`, per cycle.
#[inline]
fn slope(waveform: Waveform, t: f64) -> f64 {
    match waveform {
        Waveform::Sine => TAU * (t * TAU).cos(),
        Waveform::Triangle => {
            if t < 0.5 {
                4.0
            } else {
                -4.0
            }
        }
        Waveform::Saw => 2.0,
        Waveform::Pulse => 0.0,
    }
}

/// The PolyBLEP correction for an upward jump of 2 at phase 0, for a sample
/// at phase `t` with `dt` cycles per sample. Zero except within a sample
/// either side of the jump.
#[inline]
pub(crate) fn poly_blep(t: f64, dt: f64) -> f64 {
    let dt = dt.min(0.5);
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// The PolyBLAMP correction for a corner at phase 0 whose slope rises by one
/// per sample: the integral of the PolyBLEP correction, in samples.
#[inline]
fn poly_blamp(t: f64, dt: f64) -> f64 {
    let dt = dt.min(0.5);
    if t < dt {
        let x = 1.0 - t / dt;
        x * x * x / 6.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt + 1.0;
        x * x * x / 6.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycle(waveform: Waveform, frequency_hz: f64) -> Vec<f32> {
        pulse_cycle(waveform, frequency_hz, SQUARE)
    }

    /// A cycle of `waveform`, whose pulse is `width` wide.
    fn pulse_cycle(waveform: Waveform, frequency_hz: f64, width: f64) -> Vec<f32> {
        let mut oscillator = Oscillator::default();
        oscillator.set_increment(frequency_hz / 48_000.0);
        let samples = (48_000.0 / frequency_hz).round() as usize;
        (0..samples)
            .map(|_| {
                let value = oscillator.value(waveform, width);
                oscillator.advance();
                value
            })
            .collect()
    }

    /// At a width of a half, the pulse is the square it was before it had a
    /// width, which the sub still plays.
    #[test]
    fn a_pulse_half_wide_is_the_square() {
        for dt in [0.001, 440.0 / 48_000.0, 0.1, 0.4] {
            for n in 0..10_000 {
                let t = f64::from(n) / 10_000.0;
                let difference = (pulse(t, dt, SQUARE) - square(t, dt)).abs();
                assert!(difference < 1e-12, "{difference} at {t} with {dt}");
            }
        }
    }

    /// One saw minus another keeps the pulse centred: high for its width at
    /// 2 - 2 × the width, low for the rest at -2 × the width, so a cycle
    /// averages zero however narrow it is.
    #[test]
    fn a_pulse_has_no_offset_at_any_width() {
        for width in [0.05, 0.1, 0.25, 0.5, 0.75, 0.95] {
            let samples = pulse_cycle(Waveform::Pulse, 100.0, width);
            let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(mean.abs() < 1e-3, "{width} has an offset of {mean}");
            let high = samples.iter().filter(|&&s| s > 0.0).count() as f64;
            let share = high / samples.len() as f64;
            assert!((share - width).abs() < 0.01, "{width} is high {share}");
            let peak = samples.iter().fold(0.0f32, |p, s| p.max(*s));
            assert!(
                (f64::from(peak) - (2.0 - 2.0 * width)).abs() < 0.01,
                "{width} peaks at {peak}"
            );
        }
    }

    /// `leader` and a synced oscillator `ratio` times faster, from the
    /// start of a cycle, for `samples`.
    fn synced(waveform: Waveform, leader: f64, ratio: f64, samples: usize) -> Vec<f32> {
        let mut leader_osc = Oscillator::default();
        leader_osc.set_increment(leader);
        let mut synced = Synced::default();
        synced.start_with(&leader_osc, leader * ratio);
        (0..samples)
            .map(|_| {
                let step = synced.step(&leader_osc);
                let value = synced.value(waveform, &step);
                synced.advance(&step);
                leader_osc.advance();
                value
            })
            .collect()
    }

    /// At its leader's pitch, or an octave over it, a synced oscillator
    /// restarts just where it would have started a cycle anyway, so it
    /// plays what the free oscillator does: the same jumps and corners,
    /// corrected the same way.
    #[test]
    fn synced_at_its_leaders_pitch_or_an_octave_up_it_plays_as_if_free() {
        for leader in [1.0 / 128.0, 440.0 / 48_000.0, 2_093.0 / 48_000.0] {
            for ratio in [1.0, 2.0] {
                for waveform in Waveform::ALL {
                    let mut free = Oscillator::default();
                    free.set_increment(leader * ratio);
                    let free: Vec<f32> = (0..4_800)
                        .map(|_| {
                            let value = free.value(waveform, SQUARE);
                            free.advance();
                            value
                        })
                        .collect();
                    let synced = synced(waveform, leader, ratio, 4_800);
                    // From the second sample: the first has no step before
                    // it to carry a correction from.
                    for (n, (a, b)) in free.iter().zip(&synced).enumerate().skip(1) {
                        assert!(
                            (a - b).abs() < 1e-5,
                            "{waveform:?} at {leader} × {ratio}: {a} and {b} at {n}"
                        );
                    }
                }
            }
        }
    }

    /// Wherever the leader restarts it, the synced oscillator's jump is
    /// spread over the samples either side, sized by the jump: no step is
    /// more than three quarters of it (when the restart falls half way
    /// between two samples), where uncorrected one step would be all of it.
    #[test]
    fn a_restart_is_corrected_by_its_jump() {
        // Just under the leader's pitch, so a saw never wraps by itself, and
        // restarts from 0.95 of the way through its cycle: a jump of 1.9.
        let leader = 220.0 / 48_000.0;
        let samples = synced(Waveform::Saw, leader, 0.95, 48_000);
        let step = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0f32, f32::max);
        let slope = 2.0 * 0.95 * leader as f32;
        assert!(step < 0.75 * 1.9 + slope, "a step of {step}");
    }

    /// The triangle and the sine don't jump at a restart alone: their slope
    /// changes there too, which PolyBLAMP rounds off. Both halves of each
    /// correction are there, so each wave keeps its level and averages zero.
    #[test]
    fn synced_waves_stay_in_range() {
        for waveform in Waveform::ALL {
            for ratio in [0.6, 1.3, 2.7, 3.9] {
                let samples = synced(waveform, 330.0 / 48_000.0, ratio, 48_000);
                let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
                assert!(peak < 1.1, "{waveform:?} × {ratio} peaks at {peak}");
            }
        }
    }

    #[test]
    fn every_waveform_stays_near_full_scale() {
        for waveform in [
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Saw,
            Waveform::Pulse,
        ] {
            let peak = cycle(waveform, 100.0)
                .iter()
                .fold(0.0f32, |p, s| p.max(s.abs()));
            assert!((0.97..=1.0).contains(&peak), "{waveform:?} peaks at {peak}");
        }
    }

    #[test]
    fn waveforms_have_no_offset() {
        for waveform in [
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Saw,
            Waveform::Pulse,
        ] {
            let samples = cycle(waveform, 100.0);
            let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(mean.abs() < 1e-3, "{waveform:?} has an offset of {mean}");
        }
    }

    /// The sub flips only where the oscillator wraps: every wrap for an
    /// octave down, every second for two, starting high.
    #[test]
    fn the_sub_flips_where_the_oscillator_wraps() {
        for divide in [2, 4] {
            let mut oscillator = Oscillator::default();
            // A power of two, so the phase adds up exactly and it wraps
            // every 128 samples.
            oscillator.set_increment(1.0 / 128.0);
            let mut cycle = 0;
            let mut high = true;
            let mut flips = Vec::new();
            for sample in 0..1024 {
                let value = oscillator.sub_value(cycle, divide);
                // Exactly on a jump the correction makes it zero, half way.
                if value != 0.0 && (value > 0.0) != high {
                    high = value > 0.0;
                    flips.push(sample);
                }
                if oscillator.advance() {
                    cycle += 1;
                }
            }
            // A sample after each jump, which is at a wrap.
            let half = 128 * divide as usize / 2;
            let expected: Vec<usize> = (1..1024 / half).map(|n| n * half + 1).collect();
            assert_eq!(flips, expected, "dividing by {divide}");
        }
    }

    #[test]
    fn the_sub_is_corrected_at_its_jumps_and_has_no_offset() {
        let mut oscillator = Oscillator::default();
        let increment = 440.0 / 48_000.0;
        oscillator.set_increment(increment);
        let mut cycle = 0;
        let samples: Vec<f32> = (0..48_000)
            .map(|_| {
                let value = oscillator.sub_value(cycle, 2);
                if oscillator.advance() {
                    cycle += 1;
                }
                value
            })
            .collect();
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 1e-3, "an offset of {mean}");
        // Each jump is spread over a sample either side, so no step is the
        // whole jump of 2.
        let step = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(step < 1.9, "a step of {step}");
    }

    #[test]
    fn advance_says_when_it_wraps() {
        let mut oscillator = Oscillator::default();
        oscillator.set_increment(0.25);
        let wraps: Vec<bool> = (0..8).map(|_| oscillator.advance()).collect();
        assert_eq!(
            wraps,
            [false, false, false, true, false, false, false, true]
        );
        oscillator.start_at(0.9);
        assert!(oscillator.advance());
    }

    #[test]
    fn corrections_are_continuous_at_the_jump() {
        let dt = 0.1;
        // Either side of the jump the saw's correction meets in the middle.
        assert!((poly_blep(0.0, dt) + 1.0).abs() < 1e-12);
        assert!((poly_blep(1.0 - 1e-12, dt) - 1.0).abs() < 1e-9);
        // Both halves of the corner's correction meet at 1/6.
        assert!((poly_blamp(0.0, dt) - 1.0 / 6.0).abs() < 1e-12);
        assert!((poly_blamp(1.0 - 1e-12, dt) - 1.0 / 6.0).abs() < 1e-9);
        assert_eq!(poly_blep(0.5, dt), 0.0);
        assert_eq!(poly_blamp(0.5, dt), 0.0);
    }
}
