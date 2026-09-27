//! Live playback through the system's default output. See RFC-001, "Sound
//! devices".
//!
//! [`LiveOutput::start`] moves the [`Processor`] to a supervisor thread,
//! which plays it through a cpal stream and rebuilds the stream when the
//! device changes. The [`crate::Controller`] stays with the caller, so play,
//! stop and volume work the same as offline.

mod callbacks;
mod cpal_output;
mod supervisor;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use callbacks::{AudioCallback, DeviceError, ERROR_CAPACITY, ErrorCallback};
pub use cpal_output::CpalOutput;
pub use supervisor::{
    DEVICE_POLL_INTERVAL, DeviceInfo, DeviceState, DeviceStatus, Output, Supervisor,
};

use crate::Processor;

/// The buffer size, in frames, unless another is asked for. About 2.7 ms at
/// 48 kHz.
pub const DEFAULT_BUFFER_SIZE: u32 = 128;
/// The buffer sizes Uta offers. The device may not support the smaller ones,
/// in which case it gets the nearest it does.
pub const BUFFER_SIZES: [u32; 3] = [32, 64, 128];
/// How often the supervisor thread checks for device errors.
const TICK: Duration = Duration::from_millis(20);

/// Plays a processor through the default output until stopped.
pub struct LiveOutput {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<DeviceStatus>>,
    thread: Option<JoinHandle<()>>,
}

impl LiveOutput {
    /// Starts the supervisor thread and waits for its first attempt to open
    /// the default output. If there's no device yet, it keeps looking.
    pub fn start(processor: Processor, buffer_size: u32) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let (first_status, started) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("uta-device-supervisor".into())
            .spawn({
                let stop = stop.clone();
                move || {
                    let mut supervisor = Supervisor::start(
                        CpalOutput::new(),
                        processor,
                        buffer_size,
                        Instant::now(),
                    );
                    let status = Arc::new(Mutex::new(supervisor.status().clone()));
                    let _ = first_status.send(status.clone());
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(TICK);
                        supervisor.tick(Instant::now());
                        let mut shared = status.lock().unwrap_or_else(PoisonError::into_inner);
                        if *shared != *supervisor.status() {
                            *shared = supervisor.status().clone();
                        }
                    }
                    // Dropping the supervisor drops the stream.
                }
            })?;
        let status = started
            .recv()
            .map_err(|_| std::io::Error::other("the device supervisor stopped while starting"))?;
        Ok(Self {
            stop,
            status,
            thread: Some(thread),
        })
    }

    /// The device, buffer size and dropout count, as of the last check.
    pub fn status(&self) -> DeviceStatus {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Closes the stream and stops the supervisor thread.
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for LiveOutput {
    fn drop(&mut self) {
        self.shut_down();
    }
}
