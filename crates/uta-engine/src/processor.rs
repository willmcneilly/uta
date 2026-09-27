//! The audio-thread side of the engine.
//!
//! Everything reachable from [`Processor::process`] follows the audio thread
//! rules in `CLAUDE.md`: no waiting, no allocating or freeing, no I/O and no
//! unbounded loops.

use rtrb::{Consumer, Producer};

use crate::ramp::Ramp;
use crate::synth::{NoteOn, Synth};
use crate::{COMMAND_CAPACITY, Command, NoteEventKind, Snapshot, Status};

/// How long the output takes to fade out before a stream is replaced, and
/// back in on the next one.
pub const FADE_SECONDS: f64 = 0.005;
/// How long a volume change takes to glide to its new level.
pub const VOLUME_SMOOTHING_SECONDS: f64 = 0.02;
/// The most note starts and ends handled in one block, so the work per block
/// stays bounded however dense the notes are. Any more in the same block are
/// skipped and counted in [`Status::dropped_note_events`]. It's far more
/// than music needs: 256 notes starting and ending within one block.
pub const MAX_NOTE_EVENTS_PER_BLOCK: usize = 512;

/// Makes the sound. Owned by the audio thread (or the offline renderer).
pub struct Processor {
    commands: Consumer<Command>,
    status: Producer<Status>,
    used_snapshots: Producer<Box<Snapshot>>,

    snapshot: Box<Snapshot>,
    sample_rate: f64,
    channels: usize,
    /// Boxed, so the processor stays small to move between streams. It's
    /// created with the processor, on the control side.
    synth: Box<Synth>,

    playing: bool,
    /// Where the loop is playing, in samples from the start of the song.
    playhead: u64,
    /// The next event in the snapshot's sequence: every event before it is
    /// earlier than the playhead.
    bookmark: usize,
    volume: Ramp,
    /// Fades everything out before a stream is replaced, and back in on the
    /// next one.
    output_gain: Ramp,

    position: u64,
    dropouts: u64,
    dropped_note_events: u64,
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
        let mut processor = Self {
            commands,
            status,
            used_snapshots,
            sample_rate,
            channels,
            synth: Box::new(Synth::new(snapshot.synth, sample_rate)),
            playing: false,
            playhead: snapshot.sequence.loop_samples().start,
            bookmark: 0,
            volume: Ramp::new(snapshot.gain, samples(VOLUME_SMOOTHING_SECONDS)),
            output_gain: Ramp::new(1.0, samples(FADE_SECONDS)),
            snapshot,
            position: 0,
            dropouts: 0,
            dropped_note_events: 0,
            peak: 0.0,
        };
        processor.find_bookmark();
        processor
    }

    /// Moves the processor to a new stream: a device's sample rate and
    /// channel count, which may differ from the last one's. Called on the
    /// control side between streams, never while a stream owns it.
    ///
    /// The snapshot is retimed to the new rate, and the playhead keeps its
    /// place in the music. The sound fades in, so the switch doesn't click.
    /// Every note falls silent.
    pub(crate) fn prepare(&mut self, sample_rate: u32, channels: usize) {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(channels > 0, "need at least one channel");
        let ticks = self.snapshot.sequence.ticks_at(self.playhead);
        if self.snapshot.sequence.sample_rate() != sample_rate {
            *self.snapshot = self.snapshot.at_sample_rate(sample_rate);
        }
        self.playhead = self.snapshot.sequence.sample_at(ticks);
        self.find_bookmark();

        let sample_rate = f64::from(sample_rate);
        self.position = (self.position as f64 * sample_rate / self.sample_rate).round() as u64;
        self.sample_rate = sample_rate;
        self.channels = channels;

        let samples = |seconds: f64| (seconds * sample_rate).round() as u32;
        self.volume = Ramp::new(self.snapshot.gain, samples(VOLUME_SMOOTHING_SECONDS));
        self.output_gain = Ramp::new(0.0, samples(FADE_SECONDS));
        self.output_gain.set_target(1.0);
        self.synth.prepare(sample_rate);
    }

    #[cfg(test)]
    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate as u32
    }

    /// Fades the sound out without stopping the transport, so a healthy
    /// stream can be closed without a click before it's replaced. The next
    /// [`Processor::prepare`] fades it back in. Real-time safe.
    pub(crate) fn fade_out_for_handover(&mut self) {
        self.output_gain.set_target(0.0);
    }

    /// Fills `output` with the next block of interleaved audio.
    ///
    /// Commands are applied at the start of the block, and one status message
    /// is sent at the end. While playing, the block is split at every note
    /// start and end, and at the loop's end, so each lands on its exact
    /// sample whatever the block size. `output.len()` must be a multiple of
    /// the channel count. Any block length works: nothing here depends on a
    /// maximum size.
    #[rtsan_standalone::nonblocking]
    pub fn process(&mut self, output: &mut [f32]) {
        self.apply_commands();

        let frames = output.len() / self.channels;
        let sequencing = self.playing && self.is_timed_for_this_rate();
        let mut budget = MAX_NOTE_EVENTS_PER_BLOCK;
        let mut done = 0;
        // Each pass renders at least one frame, so this ends.
        while done < frames {
            let mut run = frames - done;
            if sequencing {
                self.handle_due_events(&mut budget);
                run = run.min(self.frames_to_next_event());
            }
            let channels = self.channels;
            self.render(&mut output[done * channels..(done + run) * channels]);
            if sequencing {
                self.playhead += run as u64;
            }
            done += run;
        }

        if self.playing {
            self.position += frames as u64;
        }
        self.send_status();
    }

    /// Records a dropout reported by the device driver. Real-time safe.
    pub fn note_dropout(&mut self) {
        self.dropouts += 1;
    }

    /// Whether the snapshot's notes are timed at the rate the processor is
    /// running at. Only for a moment after a device switch, until the control
    /// side sends one retimed for the new rate, they may not be; the loop
    /// then holds its place rather than play notes at the wrong times.
    fn is_timed_for_this_rate(&self) -> bool {
        f64::from(self.snapshot.sequence.sample_rate()) == self.sample_rate
    }

    /// Handles every note event due at the playhead. At the loop's end, goes
    /// back to the loop's start and handles the events due there too, so both
    /// land on the same sample. At most `budget` events are handled; the rest
    /// are skipped and counted.
    fn handle_due_events(&mut self, budget: &mut usize) {
        // Two rounds at most: the events due now, then those at the loop's
        // start after going back to it. Handling both here means the caller
        // always has at least one frame to render next.
        for _ in 0..2 {
            let sequence = &self.snapshot.sequence;
            let events = sequence.events();
            // Bounded by the budget, then one skip.
            while let Some(event) = events.get(self.bookmark)
                && event.sample <= self.playhead
            {
                if *budget == 0 {
                    let playhead = self.playhead;
                    let due = events.partition_point(|event| event.sample <= playhead);
                    self.dropped_note_events += (due - self.bookmark) as u64;
                    self.bookmark = due;
                    break;
                }
                *budget -= 1;
                match event.kind {
                    NoteEventKind::On {
                        key,
                        pitch,
                        velocity,
                    } => self.synth.note_on(NoteOn {
                        key,
                        pitch,
                        velocity,
                    }),
                    NoteEventKind::Off { key } => self.synth.note_off(key),
                }
                self.bookmark += 1;
            }

            let loop_samples = sequence.loop_samples();
            if self.playhead < loop_samples.end {
                return;
            }
            self.playhead = loop_samples.start;
            self.find_bookmark();
        }
    }

    /// Frames until the next note event or the loop's end, whichever comes
    /// first. At least 1, once the events due now are handled.
    fn frames_to_next_event(&self) -> usize {
        let sequence = &self.snapshot.sequence;
        let next = sequence
            .events()
            .get(self.bookmark)
            .map_or(u64::MAX, |event| event.sample)
            .min(sequence.loop_samples().end);
        usize::try_from(next - self.playhead).unwrap_or(usize::MAX)
    }

    /// Points the bookmark at the first event at or after the playhead.
    fn find_bookmark(&mut self) {
        let playhead = self.playhead;
        self.bookmark = self
            .snapshot
            .sequence
            .events()
            .partition_point(|event| event.sample < playhead);
    }

    /// Renders the synth into `output`, which holds whole frames.
    fn render(&mut self, output: &mut [f32]) {
        let mut peak = self.peak;
        for frame in output.chunks_exact_mut(self.channels) {
            let master = self.volume.next_value() * self.output_gain.next_value();
            let sample = self.synth.next_sample() * master;
            frame.fill(sample);
            peak = peak.max(sample.abs());
        }
        self.peak = peak;
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
                Command::Play => self.playing = true,
                Command::Stop => {
                    self.playing = false;
                    self.synth.release_all();
                }
                Command::SetSnapshot(new) => {
                    self.volume.set_target(new.gain);
                    self.synth.set_settings(new.synth);
                    let old = std::mem::replace(&mut self.snapshot, new);
                    self.find_bookmark();
                    // Checked above: there is room, so this never drops `old`.
                    let _ = self.used_snapshots.push(old);
                }
                Command::NoteOn {
                    key,
                    pitch,
                    velocity,
                } => self.synth.note_on(NoteOn {
                    key,
                    pitch,
                    velocity,
                }),
                Command::NoteOff { key } => self.synth.note_off(key),
            }
        }
    }

    /// Sends this block's status. If the queue is full the message is skipped
    /// and the peak carries over to the next one.
    fn send_status(&mut self) {
        let sequence = &self.snapshot.sequence;
        let loop_samples = sequence.loop_samples();
        // At the loop's end, the playhead is already back at its start.
        let playhead = if self.playhead >= loop_samples.end {
            loop_samples.start
        } else {
            self.playhead
        };
        let status = Status {
            position: self.position,
            playhead: sequence.ticks_at(playhead),
            peak: self.peak,
            dropouts: self.dropouts,
            playing: self.playing,
            dropped_note_events: self.dropped_note_events,
            sample_rate: self.sample_rate as u32,
        };
        if self.status.push(status).is_ok() {
            self.peak = 0.0;
        }
    }
}
