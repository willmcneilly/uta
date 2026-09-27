//! Linear ramps, so gain changes never jump.

/// A value that moves linearly to its target over a fixed number of samples.
///
/// It advances one sample at a time, so the result doesn't depend on how the
/// audio is split into blocks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ramp {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
    length: u32,
}

impl Ramp {
    /// A ramp resting at `value` that takes `length` samples (at least one) to
    /// reach each new target.
    pub(crate) fn new(value: f32, length: u32) -> Self {
        Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
            length: length.max(1),
        }
    }

    /// Starts moving towards `target` from wherever the ramp is now.
    pub(crate) fn set_target(&mut self, target: f32) {
        if target == self.target {
            return;
        }
        self.target = target;
        self.remaining = self.length;
        self.step = (target - self.current) / self.length as f32;
    }

    /// Returns the value for this sample and advances by one.
    #[inline]
    pub(crate) fn next_value(&mut self) -> f32 {
        let value = self.current;
        if self.remaining > 0 {
            self.remaining -= 1;
            // Land exactly on the target, so rounding never leaves it short.
            self.current = if self.remaining == 0 {
                self.target
            } else {
                self.current + self.step
            };
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_target_exactly_after_length_samples() {
        let mut ramp = Ramp::new(0.0, 4);
        ramp.set_target(1.0);
        let values: Vec<f32> = (0..6).map(|_| ramp.next_value()).collect();
        assert_eq!(values, [0.0, 0.25, 0.5, 0.75, 1.0, 1.0]);
    }

    #[test]
    fn retargeting_mid_ramp_starts_from_the_current_value() {
        let mut ramp = Ramp::new(0.0, 4);
        ramp.set_target(1.0);
        ramp.next_value();
        ramp.next_value();
        ramp.set_target(0.0);
        assert_eq!(ramp.next_value(), 0.5);
        let rest: Vec<f32> = (0..4).map(|_| ramp.next_value()).collect();
        assert_eq!(rest, [0.375, 0.25, 0.125, 0.0]);
    }

    #[test]
    fn same_target_does_not_restart() {
        let mut ramp = Ramp::new(1.0, 4);
        ramp.set_target(1.0);
        assert_eq!(ramp.next_value(), 1.0);
        assert_eq!(ramp.next_value(), 1.0);
    }
}
