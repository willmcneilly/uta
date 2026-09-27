//! The audio-thread side of the engine.
//!
//! Everything reachable from [`Processor::process`] follows the audio thread
//! rules in `CLAUDE.md`: no waiting, no allocating or freeing, no I/O and no
//! unbounded loops.

use std::f64::consts::TAU;

use rtrb::{Consumer, Producer};

use crate::ramp::Ramp;
use crate::{COMMAND_CAPACITY, Command, Snapshot, Status};

/// How long Play and Stop take to fade in and out.
pub const FADE_SECONDS: f64 = 0.005;
/// How long a volume change takes to glide to its new level.
pub const VOLUME_SMOOTHING_SECONDS: f64 = 0.02;

/// Makes the sound. Owned by the audio thread (or the offline renderer).
pub struct Processor {
    commands: Consumer<Command>,
    status: Producer<Status>,
    used_snapshots: Producer<Box<Snapshot>>,

    snapshot: Box<Snapshot>,
    sample_rate: f64,
    channels: usize,
    /// The oscillator's phase, in cycles (0..1).
    phase: f64,

    playing: bool,
    /// The Play/Stop fade: 0 when stopped, 1 when playing.
    transport_gain: Ramp,
    volume: Ramp,

    position: u64,
    dropouts: u64,
    /// The loudest sample since status was last delivered.
    peak: f32,
}

impl Processor {
    pub(crate) fn new(
        sample_rate: u32,
        channels: usize,
        snapshot: Box<Snapshot>,
        commands: Consumer<Command>,
        status: Producer<Status>,
        used_snapshots: Producer<Box<Snapshot>>,
    ) -> Self {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(channels > 0, "need at least one channel");
        let sample_rate = f64::from(sample_rate);
        let samples = |seconds: f64| (seconds * sample_rate).round() as u32;
        Self {
            commands,
            status,
            used_snapshots,
            sample_rate,
            channels,
            phase: 0.0,
            playing: false,
            transport_gain: Ramp::new(0.0, samples(FADE_SECONDS)),
            volume: Ramp::new(snapshot.gain, samples(VOLUME_SMOOTHING_SECONDS)),
            snapshot,
            position: 0,
            dropouts: 0,
            peak: 0.0,
        }
    }

    /// Fills `output` with the next block of interleaved audio.
    ///
    /// Commands are applied at the start of the block, and one status message
    /// is sent at the end. `output.len()` must be a multiple of the channel
    /// count. Any block length works: nothing here depends on a maximum size.
    #[rtsan_standalone::nonblocking]
    pub fn process(&mut self, output: &mut [f32]) {
        self.apply_commands();

        let phase_increment = self.snapshot.frequency_hz / self.sample_rate;
        let mut peak = self.peak;
        for frame in output.chunks_exact_mut(self.channels) {
            let gain = self.transport_gain.next_value() * self.volume.next_value();
            let sample = ((self.phase * TAU).sin() as f32) * gain;
            self.phase = (self.phase + phase_increment).fract();
            frame.fill(sample);
            peak = peak.max(sample.abs());
        }
        self.peak = peak;

        let frames = (output.len() / self.channels) as u64;
        if self.playing {
            self.position += frames;
        }
        self.send_status();
    }

    /// Records a dropout reported by the device driver. Real-time safe.
    pub fn note_dropout(&mut self) {
        self.dropouts += 1;
    }

    /// Applies queued commands. At most a queue's worth per block, so the
    /// control side can't keep this busy by pushing while it runs.
    fn apply_commands(&mut self) {
        for _ in 0..COMMAND_CAPACITY {
            let Ok(command) = self.commands.peek() else {
                break;
            };
            if matches!(command, Command::SetSnapshot(_)) && self.used_snapshots.is_full() {
                // Nowhere to send the old snapshot, and it must not be freed
                // here. Leave the swap queued until the control side has
                // collected the used ones.
                break;
            }
            let Ok(command) = self.commands.pop() else {
                break;
            };
            match command {
                Command::Play => {
                    self.playing = true;
                    self.transport_gain.set_target(1.0);
                }
                Command::Stop => {
                    self.playing = false;
                    self.transport_gain.set_target(0.0);
                }
                Command::SetSnapshot(new) => {
                    self.volume.set_target(new.gain);
                    let old = std::mem::replace(&mut self.snapshot, new);
                    // Checked above: there is room, so this never drops `old`.
                    let _ = self.used_snapshots.push(old);
                }
            }
        }
    }

    /// Sends this block's status. If the queue is full the message is skipped
    /// and the peak carries over to the next one.
    fn send_status(&mut self) {
        let status = Status {
            position: self.position,
            peak: self.peak,
            dropouts: self.dropouts,
            playing: self.playing,
        };
        if self.status.push(status).is_ok() {
            self.peak = 0.0;
        }
    }
}
