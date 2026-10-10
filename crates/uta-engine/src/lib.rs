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
mod drums;
pub mod live;
mod mixer;
pub mod offline;
mod processor;
mod ramp;
mod snapshot;
mod synth;

pub use control::{Controller, NoteError, QueueFull, VolumeError};
pub use drums::{
    ClapSettings, DRUM_SMOOTHING_SECONDS, KickSettings, KitSettings, REFERENCE_PEAK, SnareSettings,
    velocity_to_strength,
};
pub use mixer::MixerStrip;
pub use processor::{
    FADE_SECONDS, MAX_NOTE_EVENTS_PER_BLOCK, Processor, TRACK_BUFFER_FRAMES,
    VOLUME_SMOOTHING_SECONDS,
};
pub use snapshot::{
    ClipNotes, NoteEvent, NoteEventKind, NoteSpan, Sequence, Snapshot, SoundingNotes, TrackNotes,
    TrackSnapshot, TrackSound, db_to_gain,
};
pub use synth::{
    NoteKey, SYNTH_SMOOTHING_SECONDS, SynthSettings, TAKE_OVER_SECONDS, VOICE_LEVEL, VOICES,
    Waveform, pitch_to_hz, velocity_to_gain,
};

/// The crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The sample rate a snapshot is timed at until the engine reports its own.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// How many tracks can play at once: each has its own slot on the audio
/// thread, with its own voices and its own kit, set aside when the stream
/// starts, so a slot can play either kind of track. See RFC-003, "Tracks",
/// and RFC-006, "In the engine".
pub const TRACK_SLOTS: usize = uta_core::Project::MAX_TRACKS;
// Sets of slots are sent as a `u32` bitmask.
const _: () = assert!(TRACK_SLOTS <= 32);

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
    /// Play from the play start. Does nothing if already playing.
    Play,
    /// Stop, release every note, and go back to the play start.
    Stop,
    /// Stop and release every note, but leave the playhead where it is, for
    /// [`Command::Continue`] to carry on from. The play start stays where it
    /// was. Does nothing if stopped.
    Pause,
    /// Play from where [`Command::Pause`] left the playhead, or from the play
    /// start if it wasn't paused there. Does nothing if already playing.
    Continue,
    /// While stopped, move the play start here. While playing, jump here
    /// instead, and leave the play start where it was. In ticks from the
    /// start of the song. See RFC-003, "Playing a song".
    Locate(uta_core::time::Ticks),
    /// Swap in a new "what to play" snapshot at the start of the next block.
    SetSnapshot(Box<Snapshot>),
    /// Start a note on the track in `slot`, whether or not the transport is
    /// playing: on its synth, or a hit on its kit.
    NoteOn {
        slot: usize,
        key: NoteKey,
        /// MIDI note number, 0 to 127.
        pitch: u8,
        /// 1 to 127.
        velocity: u8,
    },
    /// Release the note started with this key in `slot`.
    NoteOff { slot: usize, key: NoteKey },
}

/// What the audio thread reports after each block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Status {
    /// Frames played so far (it doesn't advance while stopped).
    pub position: u64,
    /// The playhead's musical position, in ticks from the start of the song.
    /// While stopped, it's the play start, or where it was paused.
    pub playhead: uta_core::time::Ticks,
    /// Where Play starts and Stop goes back to, in ticks from the start of
    /// the song.
    pub play_start: uta_core::time::Ticks,
    /// The master's loudest sample since the last status message, as a
    /// linear level.
    pub peak: f32,
    /// Each slot's loudest sample since the last status message, after its
    /// track's volume, pan, mute and solo, before the master volume.
    pub track_peaks: [f32; TRACK_SLOTS],
    /// The slowest block since the last status message, as a share of its
    /// deadline: the time it took over the time the device takes to play
    /// it (its frames at the sample rate). 1.0 or more is late.
    pub slowest_block: f32,
    /// Samples the master has clipped so far, left and right counted
    /// separately: each one was past full scale and was cut off there.
    pub clips: u64,
    /// Dropouts the device driver has reported so far.
    pub dropouts: u64,
    /// Whether the transport is playing.
    pub playing: bool,
    /// Notes that couldn't be played so far because a block had more note
    /// events on a track than [`MAX_NOTE_EVENTS_PER_BLOCK`]: starts its
    /// voices couldn't hold, and chased notes past the limit.
    pub dropped_note_events: u64,
    /// The rate the engine is running at.
    pub sample_rate: u32,
    /// How many snapshots have been swapped in so far.
    pub snapshots: u64,
    /// The slots with a voice or a drum still sounding, as a bitmask (bit
    /// `n` is slot `n`).
    pub sounding_slots: u32,
}

/// Engine setup.
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    pub sample_rate: u32,
    /// Output channels. The first two get the left and right of the mix,
    /// and any more are silent. With one, it gets their average.
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
/// stopped at the start of the song.
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
