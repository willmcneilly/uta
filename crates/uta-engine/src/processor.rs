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
/// How long a change to the master volume, or to a track's volume, pan or
/// mute, takes to glide to its new level.
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
    /// Where the loop is playing, in samples from the start of the song,
    /// timed at the snapshot's rate.
    playhead: u64,
    /// The next event in the snapshot's sequence: every event before it is
    /// earlier than the playhead.
    ///
    /// Like the voices, it's a plain value, never a reference into the
    /// snapshot, so nothing on the audio thread shares ownership of snapshot
    /// data. See RFC-002, "The shared model", point 7.
    bookmark: usize,
    /// The track's gain for the left and right, from its mixer strip.
    track_gains: [Ramp; 2],
    /// The master volume.
    volume: Ramp,
    /// Fades everything out before a stream is replaced, and back in on the
    /// next one.
    output_gain: Ramp,

    position: u64,
    dropouts: u64,
    dropped_note_events: u64,
    /// Samples the hard clip has cut off so far.
    clips: u64,
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
            track_gains: snapshot
                .mixer
                .gains()
                .map(|gain| Ramp::new(gain, samples(VOLUME_SMOOTHING_SECONDS))),
            volume: Ramp::new(snapshot.gain, samples(VOLUME_SMOOTHING_SECONDS)),
            output_gain: Ramp::new(1.0, samples(FADE_SECONDS)),
            snapshot,
            position: 0,
            dropouts: 0,
            dropped_note_events: 0,
            clips: 0,
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
    /// place in the music, as it does when a new snapshot changes the tempo.
    /// The sound fades in, so the switch doesn't click. Every note falls
    /// silent.
    pub(crate) fn prepare(&mut self, sample_rate: u32, channels: usize) {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(channels > 0, "need at least one channel");
        let retimed = self.snapshot.at_sample_rate(sample_rate);
        self.playhead = retimed
            .sequence
            .playhead_from(&self.snapshot.sequence, self.playhead);
        *self.snapshot = retimed;
        self.find_bookmark();

        let sample_rate = f64::from(sample_rate);
        self.position = (self.position as f64 * sample_rate / self.sample_rate).round() as u64;
        self.sample_rate = sample_rate;
        self.channels = channels;

        let samples = |seconds: f64| (seconds * sample_rate).round() as u32;
        self.track_gains = self
            .snapshot
            .mixer
            .gains()
            .map(|gain| Ramp::new(gain, samples(VOLUME_SMOOTHING_SECONDS)));
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
                        sequenced: true,
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

    /// After a new snapshot, releases every note the sequencer started that
    /// the snapshot no longer plays at the playhead: deleted, re-pitched,
    /// moved or shortened away from it, or left behind when the playhead
    /// went back to the loop's start. Otherwise its end event would never
    /// come, and it would stick. Bounded by the number of voices, each a
    /// binary search.
    fn release_changed_notes(&mut self) {
        let sequence = &self.snapshot.sequence;
        let playhead = self.playhead;
        self.synth.release_sequenced_unless(|key, pitch| {
            sequence
                .note(key)
                .is_some_and(|note| note.pitch == pitch && note.contains(playhead))
        });
    }

    /// Renders the synth into `output`, which holds whole frames: the
    /// track's volume, pan and mute make it stereo, then the master volume,
    /// then a hard clip at full scale. The first two channels get the left
    /// and right, and any more are silent. A single channel gets their
    /// average, so a centred track sounds the same in mono. See RFC-003, "The
    /// master and headroom".
    fn render(&mut self, output: &mut [f32]) {
        let mut peak = self.peak;
        let mut clips = self.clips;
        let [left_gain, right_gain] = &mut self.track_gains;
        for frame in output.chunks_exact_mut(self.channels) {
            let sample = self.synth.next_sample();
            let master = self.volume.next_value() * self.output_gain.next_value();
            let left = hard_clip(sample * left_gain.next_value() * master, &mut clips);
            let right = hard_clip(sample * right_gain.next_value() * master, &mut clips);
            peak = peak.max(left.abs()).max(right.abs());
            match frame {
                [mono] => *mono = (left + right) * 0.5,
                [first, second, rest @ ..] => {
                    *first = left;
                    *second = right;
                    rest.fill(0.0);
                }
                [] => {}
            }
        }
        self.peak = peak;
        self.clips = clips;
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
                    for (ramp, gain) in self.track_gains.iter_mut().zip(new.mixer.gains()) {
                        ramp.set_target(gain);
                    }
                    self.volume.set_target(new.gain);
                    self.synth.set_settings(new.synth);
                    let old = std::mem::replace(&mut self.snapshot, new);
                    self.playhead = self
                        .snapshot
                        .sequence
                        .playhead_from(&old.sequence, self.playhead);
                    self.find_bookmark();
                    self.release_changed_notes();
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
                    sequenced: false,
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
            clips: self.clips,
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

/// Cuts `sample` off at full scale, counting it in `clips` if it was past it.
/// No delay and no state, so a sample within full scale passes through
/// exactly.
#[inline]
fn hard_clip(sample: f32, clips: &mut u64) -> f32 {
    if sample.abs() > 1.0 {
        *clips += 1;
        sample.clamp(-1.0, 1.0)
    } else {
        sample
    }
}

#[cfg(test)]
mod tests {
    use uta_core::time::{TempoMap, Ticks};
    use uta_core::{Command, Note, NoteId, Project, ProjectId, SynthParam};
    use uuid::Uuid;

    use super::*;
    use crate::offline::Renderer;
    use crate::{EngineConfig, VOICE_LEVEL, db_to_gain, velocity_to_gain};

    const BEAT: Ticks = 960;

    /// A 1-bar loop at 120 BPM: A4 on every beat, an eighth note long, as a
    /// sine at full sustain with a 5 ms release.
    fn beats() -> Project {
        let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
        let track = project.tracks()[0].id();
        let clip = project.tracks()[0].clips()[0].id();
        let notes = (0..4)
            .map(|i| Note {
                id: NoteId::from_uuid(Uuid::from_u128(100 + i)),
                pitch: 69,
                velocity: 100,
                start: i as Ticks * BEAT,
                length: BEAT / 2,
            })
            .collect();
        let commands = [
            Command::SetLoopLength { bars: 1 },
            Command::SetSynthParam {
                track,
                param: SynthParam::Waveform(uta_core::Waveform::Sine),
            },
            Command::SetSynthParam {
                track,
                param: SynthParam::Sustain(1.0),
            },
            Command::SetSynthParam {
                track,
                param: SynthParam::ReleaseSeconds(0.005),
            },
            Command::AddNotes { clip, notes },
        ];
        for command in &commands {
            project.apply(command).unwrap();
        }
        project
    }

    /// Where each note starts: the sample before each sound that follows at
    /// least 100 samples of silence.
    fn onsets(samples: &[f32]) -> Vec<usize> {
        let mut onsets = Vec::new();
        let mut silent = usize::MAX;
        for (i, &sample) in samples.iter().enumerate() {
            if sample == 0.0 {
                silent = silent.saturating_add(1);
            } else {
                if silent >= 100 {
                    onsets.push(i.saturating_sub(1));
                }
                silent = 0;
            }
        }
        onsets
    }

    /// A sample within full scale comes out of the hard clip bit for bit
    /// and isn't counted; one past it is cut off at full scale and counted.
    #[test]
    fn the_hard_clip_passes_quiet_samples_through_exactly() {
        let mut clips = 0;
        // Every 1,000th value from 0 to 1, both signs, and the ends.
        for bits in (0..=1.0f32.to_bits())
            .step_by(1_000)
            .chain([1.0f32.to_bits()])
        {
            for sample in [f32::from_bits(bits), -f32::from_bits(bits)] {
                assert_eq!(hard_clip(sample, &mut clips).to_bits(), sample.to_bits());
            }
        }
        assert_eq!(clips, 0);

        let past = [1.000_000_1, -1.000_000_1, 4.0, -250.0, f32::INFINITY];
        let cut: Vec<f32> = past
            .iter()
            .map(|&sample| hard_clip(sample, &mut clips))
            .collect();
        assert_eq!(cut, [1.0, -1.0, 1.0, -1.0, 1.0]);
        assert_eq!(clips, past.len() as u64);
    }

    /// A device switch mid-note, from 48 kHz to 44.1 kHz and back, the way
    /// the supervisor does it: fade out, move the processor to the new rate,
    /// carry on. The loop keeps its bar and beat, and every note after the
    /// switch lands on its exact sample at the new rate, for ten passes.
    #[test]
    fn a_new_sample_rate_keeps_the_loop_in_time() {
        for (from, to) in [(48_000, 44_100), (44_100, 96_000)] {
            let config = EngineConfig {
                sample_rate: from,
                channels: 1,
            };
            let mut renderer = Renderer::new(config, Snapshot::from(&beats()), 128);
            renderer.controller.play().unwrap();
            // Mid-note on beat 3.
            renderer.render(renderer.frames_for(1.23) / 128 * 128);
            renderer.processor().fade_out_for_handover();
            renderer.render((FADE_SECONDS * f64::from(from)) as usize);
            let switch_at = renderer.samples().len();
            renderer.processor().prepare(to, 1);
            assert_eq!(renderer.processor().sample_rate(), to);
            let loop_at_new_rate = (2 * to) as usize;
            renderer.render(10 * loop_at_new_rate);
            // The controller has sent the snapshot again, retimed.
            assert_eq!(renderer.controller.snapshot().sequence.sample_rate(), to);

            // The first tick the old rate hadn't reached, at the new rate.
            let tempo = TempoMap::new(120.0);
            let tick = (0..)
                .find(|&tick| tempo.ticks_to_samples(tick, from) >= switch_at as u64)
                .unwrap();
            let playhead = tempo.ticks_to_samples(tick, to) as usize;
            let pass_start = switch_at as i64 - playhead as i64;
            let mut expected: Vec<usize> = (0..4)
                .map(|beat| tempo.ticks_to_samples(beat * BEAT, from) as usize)
                .filter(|&at| at < switch_at)
                .collect();
            for pass in 0..11 {
                for beat in 0..4 {
                    let at = pass_start
                        + (pass * loop_at_new_rate) as i64
                        + tempo.ticks_to_samples(beat * BEAT, to) as i64;
                    if at >= switch_at as i64 && (at as usize) < renderer.samples().len() {
                        expected.push(at as usize);
                    }
                }
            }
            let samples = renderer.samples();
            assert_eq!(onsets(samples), expected, "{from} Hz to {to} Hz");

            // No clicks: one A4 sine voice moving at most as fast as its
            // tone, its 5 ms attack and its release allow, plus 10%.
            let level =
                VOICE_LEVEL * velocity_to_gain(100) * db_to_gain(Snapshot::DEFAULT_VOLUME_DB);
            let rate = f64::from(from.min(to));
            let tone = (2.0 * (std::f64::consts::PI * 440.0 / rate).sin()) as f32;
            let release = 7.0 / (0.005 * rate) as f32;
            let limit = level * (tone + release) * 1.1;
            let jump = samples
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).abs())
                .fold(0.0f32, f32::max);
            assert!(
                jump <= limit,
                "{from} Hz to {to} Hz: jump {jump}, limit {limit}"
            );
        }
    }
}
