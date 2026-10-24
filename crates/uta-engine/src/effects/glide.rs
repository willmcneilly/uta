//! Reading a delay line between its samples, and gliding where it's read
//! from: the Delay's playback head, and the Reverb's lines. See RFC-007,
//! "Reading between samples".

/// How quickly a playback head glides to a new time. Its place is
/// smoothed twice in a row with this time constant, so it eases away and
/// eases in, like a tape machine's head. A small change lands within a
/// second; a large one is held back by [`MAX_REPITCH`], so going from 1 ms
/// to 5 s takes at least 10 s.
pub(crate) const REPITCH_SECONDS: f64 = 0.05;
/// The fastest a playback head moves against the tape while it glides,
/// in samples a sample: what it plays back plays between half speed (an octave
/// down) and one and a half (a fifth up), never backwards.
pub(crate) const MAX_REPITCH: f64 = 0.5;
/// Closer than this to its new time, in samples, the glide lands on it.
pub(crate) const GLIDE_LANDS: f64 = 1e-3;

/// A playback head's place behind the recording head, in samples,
/// gliding to a new time as a tape machine's head would: smoothed twice in
/// a row, so it eases away and eases in, and never faster than
/// [`MAX_REPITCH`]. It moves one sample at a time, so the result doesn't
/// depend on how the audio is split into blocks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Glide {
    /// The first smoothing, which `current` follows.
    first: f64,
    current: f64,
    target: f64,
    /// How much of the way each smoothing goes each sample.
    coefficient: f64,
}

impl Glide {
    pub(crate) fn new(value: f64, sample_rate: f64) -> Self {
        Self {
            first: value,
            current: value,
            target: value,
            coefficient: 1.0 - (-1.0 / (REPITCH_SECONDS * sample_rate)).exp(),
        }
    }

    pub(crate) fn set_target(&mut self, target: f64) {
        self.target = target;
    }

    pub(crate) fn jump_to(&mut self, value: f64) {
        self.first = value;
        self.current = value;
        self.target = value;
    }

    /// Where it is now: the place the next sample is read from.
    pub(crate) fn value(&self) -> f64 {
        self.current
    }

    /// Whether it's still on its way to its target.
    pub(crate) fn is_moving(&self) -> bool {
        self.current != self.target || self.first != self.target
    }

    /// Returns the place for this sample and advances by one.
    #[inline]
    pub(crate) fn next_value(&mut self) -> f64 {
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

/// A smooth curve through four samples in a row (a Catmull-Rom spline),
/// read `t` of the way from `x0` to `x1`: cubic interpolation. At `t = 0` it
/// is `x0` exactly. See RFC-007, "Reading between samples".
#[inline]
pub(crate) fn cubic(before: f32, x0: f32, x1: f32, after: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x1 - before);
    let c2 = before - 2.5 * x0 + 2.0 * x1 - 0.5 * after;
    let c3 = 0.5 * (after - before) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}
