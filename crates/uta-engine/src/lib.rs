//! Uta's audio engine: a control side and an audio-thread processor. See
//! RFC-001, "The audio thread follows strict rules".
//!
//! [`engine`] returns the two halves, joined by fixed-size lock-free
//! queues:
//! - [`Controller`] stays on the control side. It sends commands and new
//!   snapshots, reads status, and frees the snapshots the processor is done
//!   with.
//! - [`Processor`] goes to the audio thread, through [`live::LiveOutput`], or
//!   to [`offline::Renderer`] for rendering without a device.

mod control;
pub mod live;
pub mod offline;
mod processor;
mod ramp;
mod snapshot;
mod synth;

pub use control::{Controller, NoteError, QueueFull, VolumeError};
pub use processor::{FADE_SECONDS, MAX_NOTE_EVENTS_PER_BLOCK, Processor, VOLUME_SMOOTHING_SECONDS};
pub use snapshot::{ClipNotes, NoteEvent, NoteEventKind, NoteSpan, Sequence, Snapshot, db_to_gain};
pub use synth::{
    NoteKey, SYNTH_SMOOTHING_SECONDS, SynthSettings, TAKE_OVER_SECONDS, VOICE_LEVEL, VOICES,
    Waveform, pitch_to_hz, velocity_to_gain,
};

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The sample rate a snapshot is timed at until the engine reports its own.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

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
    /// Play the loop from the playhead.
    Play,
    /// Stop the loop where it is, and release every note.
    Stop,
    /// Swap in a new "what to play" snapshot at the start of the next block.
    SetSnapshot(Box<Snapshot>),
    /// Start a note on the synth, whether or not the transport is playing.
    NoteOn {
        key: NoteKey,
        /// MIDI note number, 0 to 127.
        pitch: u8,
        /// 1 to 127.
        velocity: u8,
    },
    /// Release the note started with this key.
    NoteOff { key: NoteKey },
}

/// What the audio thread reports after each block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Status {
    /// Frames played so far (it doesn't advance while stopped).
    pub position: u64,
    /// The playhead's musical position, in ticks from the start of the song.
    /// It stays inside the loop.
    pub playhead: uta_core::time::Ticks,
    /// The loudest sample since the last status message, as a linear level.
    pub peak: f32,
    /// Dropouts the device driver has reported so far.
    pub dropouts: u64,
    /// Whether the transport is playing.
    pub playing: bool,
    /// Note starts and ends skipped so far because a block had more than
    /// [`MAX_NOTE_EVENTS_PER_BLOCK`].
    pub dropped_note_events: u64,
    /// The rate the engine is running at.
    pub sample_rate: u32,
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
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: 1,
        }
    }
}

/// Builds the engine's two halves: a controller and a processor joined by
/// fresh queues, starting from `snapshot` (retimed to the engine's rate),
/// stopped at the start of its loop.
pub fn engine(config: EngineConfig, snapshot: Snapshot) -> (Controller, Processor) {
    let snapshot = snapshot.at_sample_rate(config.sample_rate);
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
    let controller = Controller::new(snapshot, config.sample_rate, command_tx, status_rx, used_rx);
    (controller, processor)
}
