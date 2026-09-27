//! The two callbacks a device stream calls. Both can run on the audio thread,
//! so both follow the audio thread rules in `CLAUDE.md`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::Processor;

/// How many device errors can wait for the supervisor. Xruns are the only
/// ones that come in bursts; if the queue is full, the extra ones are lost.
pub const ERROR_CAPACITY: usize = 64;

/// A device error, as a code small enough to pass through a queue without
/// allocating. See [`ErrorCallback::report`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceError {
    /// The system's default output changed. cpal has already rerouted the
    /// stream, but the new device may want a different rate or buffer size.
    Changed,
    /// The stream's configuration is no longer valid and it must be rebuilt.
    Invalidated,
    /// There's no output device to play to.
    Gone,
    /// A buffer underrun or overrun: a dropout.
    Xrun,
    /// Anything else cpal reports. Nothing is done about it.
    Other,
}

impl From<cpal::ErrorKind> for DeviceError {
    fn from(kind: cpal::ErrorKind) -> Self {
        match kind {
            cpal::ErrorKind::DeviceChanged => Self::Changed,
            cpal::ErrorKind::StreamInvalidated => Self::Invalidated,
            cpal::ErrorKind::DeviceNotAvailable => Self::Gone,
            cpal::ErrorKind::Xrun => Self::Xrun,
            _ => Self::Other,
        }
    }
}

/// The data callback: owns the [`Processor`] while a stream plays it.
///
/// When the stream is dropped, cpal drops this callback, and it sends the
/// processor back through `home` so the supervisor can move it to the next
/// stream. That works on any thread, because pushing into the preallocated
/// queue neither waits nor allocates.
pub struct AudioCallback {
    processor: Option<Processor>,
    home: Producer<Processor>,
    handover: Arc<AtomicBool>,
}

/// Asks a playing [`AudioCallback`] to fade out, ahead of closing its stream.
pub struct Handover(Arc<AtomicBool>);

impl Handover {
    pub fn start(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl AudioCallback {
    /// A callback that plays `processor`, the queue it comes back through
    /// when the callback is dropped, and the switch that fades it out.
    pub fn new(processor: Processor) -> (Self, Consumer<Processor>, Handover) {
        let (home, returned) = RingBuffer::new(1);
        let handover = Arc::new(AtomicBool::new(false));
        let callback = Self {
            processor: Some(processor),
            home,
            handover: handover.clone(),
        };
        (callback, returned, Handover(handover))
    }

    /// Fills `output` with the next block. What the stream calls.
    #[rtsan_standalone::nonblocking]
    pub fn render(&mut self, output: &mut [f32]) {
        if let Some(processor) = &mut self.processor {
            if self.handover.load(Ordering::Relaxed) {
                processor.fade_out_for_handover();
            }
            processor.process(output);
        }
    }

    #[cfg(test)]
    pub(crate) fn processor(&self) -> Option<&Processor> {
        self.processor.as_ref()
    }

    #[rtsan_standalone::nonblocking]
    fn send_home(&mut self) {
        if let Some(processor) = self.processor.take() {
            // The queue holds one and this is the only push, so there's
            // always room and the processor is never dropped here.
            let _ = self.home.push(processor);
        }
    }
}

impl Drop for AudioCallback {
    fn drop(&mut self) {
        self.send_home();
    }
}

/// The error callback: turns each cpal error into a [`DeviceError`] code and
/// pushes it into a queue for the supervisor. Nothing else.
///
/// cpal calls this from its own listener threads, and for xruns from the
/// audio thread. cpal builds the error before calling it; on the paths that
/// can reach the audio thread its message is a static string, so dropping it
/// here frees nothing.
pub struct ErrorCallback {
    codes: Producer<DeviceError>,
}

impl ErrorCallback {
    /// A callback and the queue its codes arrive on.
    pub fn new() -> (Self, Consumer<DeviceError>) {
        let (codes, received) = RingBuffer::new(ERROR_CAPACITY);
        (Self { codes }, received)
    }

    /// Pushes the error's code. If the queue is full the code is lost.
    #[rtsan_standalone::nonblocking]
    pub fn report(&mut self, error: cpal::Error) {
        let _ = self.codes.push(DeviceError::from(error.kind()));
    }
}
