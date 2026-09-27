//! Uta's audio engine: a control side and an audio-thread processor. See
//! RFC-001, "The audio thread follows strict rules".
//!
//! [`engine`] returns the two halves, joined by fixed-size lock-free
//! queues:
//! - [`Controller`] stays on the control side. It sends commands and new
//!   snapshots, reads status, and frees the snapshots the processor is done
//!   with.
//! - [`Processor`] goes to the audio thread, or to [`offline::Renderer`] for
//!   rendering without a device.

mod control;
pub mod offline;
mod processor;
mod ramp;
mod snapshot;

pub use control::{Controller, QueueFull};
pub use processor::{FADE_SECONDS, Processor, VOLUME_SMOOTHING_SECONDS};
pub use snapshot::{Snapshot, db_to_gain};

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How many commands can wait for the audio thread at once.
pub const COMMAND_CAPACITY: usize = 256;
/// How many status messages can wait for the control side. At 48 kHz and
/// 32-sample blocks that's about 170 ms of status.
pub const STATUS_CAPACITY: usize = 256;
/// How many used snapshots can wait to be freed. When it's full the processor
/// leaves further swaps queued until the control side catches up.
pub const USED_SNAPSHOT_CAPACITY: usize = 64;

/// A message from the control side to the audio thread.
#[derive(Debug)]
pub enum Command {
    /// Start the tone, fading in.
    Play,
    /// Stop the tone, fading out.
    Stop,
    /// Swap in a new "what to play" snapshot at the start of the next block.
    SetSnapshot(Box<Snapshot>),
}

/// What the audio thread reports after each block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Status {
    /// Frames played so far (it doesn't advance while stopped).
    pub position: u64,
    /// The loudest sample since the last status message, as a linear level.
    pub peak: f32,
    /// Dropouts the device driver has reported so far.
    pub dropouts: u64,
    /// Whether the transport is playing.
    pub playing: bool,
}

/// Engine setup.
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    pub sample_rate: u32,
    /// Output channels. Every channel carries the same signal for now.
    pub channels: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 1,
        }
    }
}

/// Builds the engine's two halves: a controller and a processor joined by
/// fresh queues, starting from `snapshot`, stopped.
pub fn engine(config: EngineConfig, snapshot: Snapshot) -> (Controller, Processor) {
    let (command_tx, command_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
    let (status_tx, status_rx) = rtrb::RingBuffer::new(STATUS_CAPACITY);
    let (used_tx, used_rx) = rtrb::RingBuffer::new(USED_SNAPSHOT_CAPACITY);
    let processor = Processor::new(
        config.sample_rate,
        config.channels,
        Box::new(snapshot.clone()),
        command_rx,
        status_tx,
        used_tx,
    );
    let controller = Controller::new(snapshot, command_tx, status_rx, used_rx);
    (controller, processor)
}
