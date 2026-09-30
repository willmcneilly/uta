//! The audio-thread side of the engine.
//!
//! Everything reachable from [`Processor::process`] follows the audio thread
//! rules in `CLAUDE.md`: no waiting, no allocating or freeing, no I/O and no
//! unbounded loops.

use rtrb::{Consumer, Producer};

use uta_core::TrackId;
use uta_core::time::Ticks;

use crate::ramp::Ramp;
use crate::synth::{NoteOn, Synth};
use crate::{
    COMMAND_CAPACITY, Command, NoteEvent, NoteEventKind, Snapshot, Status, SynthSettings,
    TRACK_SLOTS, TrackNotes, TrackSnapshot,
};

/// How long the output takes to fade out before a stream is replaced, and
/// back in on the next one.
pub const FADE_SECONDS: f64 = 0.005;
/// How long a change to the master volume, or to a track's volume, pan, mute
/// or solo, takes to glide to its new level.
pub const VOLUME_SMOOTHING_SECONDS: f64 = 0.02;
/// The most note starts and ends each track handles in one block, so the work
/// per block stays bounded however dense the notes are. Chased notes count
/// too. Any more on the same track in the same block are skipped and counted
/// in [`Status::dropped_note_events`]. It's far more than music needs: 256
/// notes starting and ending within one block.
pub const MAX_NOTE_EVENTS_PER_BLOCK: usize = 512;
/// The most frames each track renders at a time: the size of its buffer, set
/// aside when the processor is created. Longer blocks are rendered in parts.
pub const TRACK_BUFFER_FRAMES: usize = 256;

/// Makes the sound. Owned by the audio thread (or the offline renderer).
pub struct Processor {
    commands: Consumer<Command>,
    status: Producer<Status>,
    used_snapshots: Producer<Box<Snapshot>>,

    snapshot: Box<Snapshot>,
    sample_rate: f64,
    channels: usize,
    /// One per track slot, [`TRACK_SLOTS`] of them, all created with the
    /// processor, on the control side.
    slots: Box<[Slot]>,
    /// The left and right of the mix, before the master volume. Boxed, so
    /// the processor stays small to move between streams.
    mix: Box<[[f32; TRACK_BUFFER_FRAMES]; 2]>,

    playing: bool,
    /// Where it's playing, in samples from the start of the song, timed at
    /// the snapshot's rate. While stopped, the play start.
    playhead: u64,
    /// Where Play starts and Stop goes back to, in ticks from the start of
    /// the song. See RFC-003, "Playing a song".
    play_start: Ticks,
    /// Whether playback is bound for the loop's end, to go round the loop,
    /// rather than for the song's end. It is when the loop is on and
    /// playback started before the loop's end.
    looping: bool,
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
    /// The master's loudest sample since status was last delivered.
    peak: f32,
    /// Snapshots swapped in so far.
    swaps: u64,
}

/// One track's place on the audio thread: its voices, its place in its
/// events, its gains and its buffer. See RFC-003, "The shared model,
/// extended", point 4.
///
/// Like the voices, everything here is a plain value, never a reference into
/// the snapshot, so nothing on the audio thread shares ownership of snapshot
/// data. See RFC-002, "The shared model", point 7.
struct Slot {
    /// The track playing in this slot in the current snapshot, if any. A
    /// slot whose track is gone keeps playing its last notes' releases.
    track: Option<TrackId>,
    synth: Synth,
    /// The next event in its track's events: every event before it is
    /// earlier than the playhead.
    bookmark: usize,
    /// Note starts and ends it may still handle in this block.
    budget: usize,
    /// The track's gain for the left and right, from its mixer strip.
    gains: [Ramp; 2],
    /// The synth's output for the part of the block being rendered.
    buffer: [f32; TRACK_BUFFER_FRAMES],
    /// The loudest sample since status was last delivered, after the gains.
    peak: f32,
}

impl Slot {
    fn new(track: Option<&TrackSnapshot>, soloing: bool, sample_rate: f64) -> Self {
        let synth = track.map_or_else(SynthSettings::default, |track| track.synth);
        Self {
            track: track.map(TrackSnapshot::id),
            synth: Synth::new(synth, sample_rate),
            bookmark: 0,
            budget: MAX_NOTE_EVENTS_PER_BLOCK,
            gains: gain_ramps(track, soloing, sample_rate),
            buffer: [0.0; TRACK_BUFFER_FRAMES],
            peak: 0.0,
        }
    }

    /// Takes on the track this slot has in a new snapshot, or lets it go.
    fn load(&mut self, track: Option<&TrackSnapshot>, soloing: bool) {
        let Some(track) = track else {
            // The track is gone: its notes release and fade out here, and
            // the control side doesn't hand the slot on until they have.
            if self.track.take().is_some() {
                self.synth.release_all();
            }
            return;
        };
        let gains = track.mixer.gains(soloing);
        if self.track == Some(track.id()) || self.synth.is_sounding() {
            if self.track != Some(track.id()) {
                // A new track while another's notes still sound here: they
                // fade out quickly, and the sound and gains glide.
                self.synth.fade_out();
                self.track = Some(track.id());
            }
            self.synth.set_settings(track.synth);
            for (ramp, gain) in self.gains.iter_mut().zip(gains) {
                ramp.set_target(gain);
            }
        } else {
            // A new track in a silent slot starts with its own sound.
            self.track = Some(track.id());
            self.synth.load(track.synth);
            for (ramp, gain) in self.gains.iter_mut().zip(gains) {
                ramp.jump_to(gain);
            }
        }
    }

    /// Starts every note in `notes` already under way at `playhead`, with an
    /// ordinary note on, up to the slot's budget. The rest are skipped and
    /// counted in `dropped`. Each ends at its own end event. See RFC-003,
    /// "Playing a song" (note chasing).
    fn chase(&mut self, notes: &TrackNotes, playhead: u64, dropped: &mut u64) {
        let mut sounding = notes.sounding_at(playhead);
        // Bounded by the budget.
        for note in sounding.by_ref().take(self.budget) {
            self.budget -= 1;
            self.synth.note_on(NoteOn {
                key: note.key,
                pitch: note.pitch,
                velocity: note.velocity,
                sequenced: true,
            });
        }
        *dropped += sounding.len() as u64;
    }

    /// Handles every event in `events` due by `playhead`, up to the slot's
    /// budget. The rest are skipped with a search, and counted in `dropped`.
    fn handle_due_events(&mut self, events: &[NoteEvent], playhead: u64, dropped: &mut u64) {
        // Bounded by the budget, then one skip.
        while let Some(event) = events.get(self.bookmark)
            && event.sample <= playhead
        {
            if self.budget == 0 {
                let due = events.partition_point(|event| event.sample <= playhead);
                *dropped += (due - self.bookmark) as u64;
                self.bookmark = due;
                break;
            }
            self.budget -= 1;
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
    }

    /// Renders the synth into the buffer, then adds it to `left` and `right`
    /// through the track's gains. At most [`TRACK_BUFFER_FRAMES`] frames.
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let buffer = &mut self.buffer[..left.len()];
        for sample in buffer.iter_mut() {
            *sample = self.synth.next_sample();
        }
        let [left_gain, right_gain] = &mut self.gains;
        let mut peak = self.peak;
        for ((&sample, left), right) in buffer.iter().zip(left).zip(right) {
            let track_left = sample * left_gain.next_value();
            let track_right = sample * right_gain.next_value();
            *left += track_left;
            *right += track_right;
            peak = peak.max(track_left.abs()).max(track_right.abs());
        }
        self.peak = peak;
    }
}

/// Gain ramps for a track's left and right, resting at its mixer strip's
/// gains, or silent for no track.
fn gain_ramps(track: Option<&TrackSnapshot>, soloing: bool, sample_rate: f64) -> [Ramp; 2] {
    let gains = track.map_or([0.0; 2], |track| track.mixer.gains(soloing));
    let length = (VOLUME_SMOOTHING_SECONDS * sample_rate).round() as u32;
    gains.map(|gain| Ramp::new(gain, length))
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
        let soloing = snapshot.soloing();
        let mut processor = Self {
            commands,
            status,
            used_snapshots,
            sample_rate,
            channels,
            slots: (0..TRACK_SLOTS)
                .map(|slot| Slot::new(snapshot.track_in(slot), soloing, sample_rate))
                .collect(),
            mix: Box::new([[0.0; TRACK_BUFFER_FRAMES]; 2]),
            playing: false,
            playhead: 0,
            play_start: 0,
            looping: false,
            volume: Ramp::new(snapshot.gain, samples(VOLUME_SMOOTHING_SECONDS)),
            output_gain: Ramp::new(1.0, samples(FADE_SECONDS)),
            snapshot,
            position: 0,
            dropouts: 0,
            dropped_note_events: 0,
            clips: 0,
            peak: 0.0,
            swaps: 0,
        };
        processor.find_bookmarks();
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
        self.playhead = if self.playing {
            retimed
                .sequence
                .playhead_from(&self.snapshot.sequence, self.playhead)
        } else {
            retimed.sequence.sample_at(self.play_start)
        };
        *self.snapshot = retimed;
        self.find_bookmarks();

        let sample_rate = f64::from(sample_rate);
        self.position = (self.position as f64 * sample_rate / self.sample_rate).round() as u64;
        self.sample_rate = sample_rate;
        self.channels = channels;

        let samples = |seconds: f64| (seconds * sample_rate).round() as u32;
        let soloing = self.snapshot.soloing();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let track = self.snapshot.track_in(index);
            slot.gains = gain_ramps(track, soloing, sample_rate);
            slot.synth.prepare(sample_rate);
        }
        self.volume = Ramp::new(self.snapshot.gain, samples(VOLUME_SMOOTHING_SECONDS));
        self.output_gain = Ramp::new(0.0, samples(FADE_SECONDS));
        self.output_gain.set_target(1.0);
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
    /// start and end on any track, at the loop's end and at the song's end,
    /// so each lands on its exact sample whatever the block size.
    /// `output.len()` must be a multiple of the channel count. Any block
    /// length works: longer ones are rendered [`TRACK_BUFFER_FRAMES`] at a
    /// time.
    #[rtsan_standalone::nonblocking]
    pub fn process(&mut self, output: &mut [f32]) {
        // Before the commands, so notes chased on Play or a jump count.
        for slot in self.slots.iter_mut() {
            slot.budget = MAX_NOTE_EVENTS_PER_BLOCK;
        }
        self.apply_commands();

        let frames = output.len() / self.channels;
        let timed = self.is_timed_for_this_rate();
        let mut done = 0;
        // Each pass renders at least one frame, so this ends.
        while done < frames {
            let mut run = (frames - done).min(TRACK_BUFFER_FRAMES);
            if self.playing && timed {
                // This may reach the song's end and stop.
                self.handle_due_events();
            }
            let sequencing = self.playing && timed;
            if sequencing {
                run = run.min(self.frames_to_next_event());
            }
            let channels = self.channels;
            self.render(&mut output[done * channels..(done + run) * channels]);
            if self.playing {
                self.position += run as u64;
            }
            if sequencing {
                self.playhead += run as u64;
                // Stop at the song's end now, not at the next block's start,
                // so this block's status already says it has stopped and
                // shows the play start.
                if !self.looping && self.playhead >= self.snapshot.sequence.song_end_sample() {
                    self.stop();
                }
            }
            done += run;
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

    /// Handles every note event due at the playhead, on every track. At the
    /// loop's end (or past it, if a new snapshot shortened the loop), goes
    /// back to the loop's start and handles the events due there, so it lands
    /// on the same sample. At the song's end, stops. Each track handles at
    /// most its budget of events; the rest are skipped and counted.
    fn handle_due_events(&mut self) {
        // Two rounds at most: going round the loop, then the events at its
        // start. Either way the caller has at least one frame to render
        // next, or has stopped.
        for _ in 0..2 {
            let sequence = &self.snapshot.sequence;
            let loop_samples = sequence.loop_samples();
            if self.looping && self.playhead >= loop_samples.end {
                // The events at the loop's end aren't handled: `start_from`
                // releases the notes ending there, and a note starting there
                // is outside the loop, so it would only take a voice and be
                // released on the same sample.
                self.start_from(loop_samples.start);
                continue;
            }
            let playhead = self.playhead;
            // Bounded by TRACK_SLOTS.
            for track in self.snapshot.tracks() {
                self.slots[track.slot()].handle_due_events(
                    track.notes().events(),
                    playhead,
                    &mut self.dropped_note_events,
                );
            }
            if !self.looping && playhead >= self.snapshot.sequence.song_end_sample() {
                self.stop();
            }
            return;
        }
    }

    /// Frames until the next note event on any track, or the loop's or
    /// song's end, whichever comes first. At least 1, once the events due
    /// now are handled.
    fn frames_to_next_event(&self) -> usize {
        let sequence = &self.snapshot.sequence;
        let end = if self.looping {
            sequence.loop_samples().end
        } else {
            sequence.song_end_sample()
        };
        let next = self
            .snapshot
            .tracks()
            .iter()
            .filter_map(|track| {
                let events = track.notes().events();
                events.get(self.slots[track.slot()].bookmark)
            })
            .map(|event| event.sample)
            .fold(end, u64::min);
        usize::try_from(next - self.playhead).unwrap_or(usize::MAX)
    }

    /// Starts playing from `playhead`: on Play, and on a jump. Playback is
    /// bound for the loop's end if the loop is on and `playhead` is before
    /// it, and for the song's end otherwise. See RFC-003, "Playing a song",
    /// and open question 1.
    fn play_from(&mut self, playhead: u64) {
        let sequence = &self.snapshot.sequence;
        self.looping = sequence.loop_enabled() && playhead < sequence.loop_samples().end;
        self.start_from(playhead);
    }

    /// Moves the playhead to `playhead` and carries on from there: releases
    /// every note the sequencer started, finds each track's next event, and
    /// chases the notes already under way there. What Play, a jump and
    /// going round the loop do. Bounded by the number of tracks, and each
    /// track's budget.
    fn start_from(&mut self, playhead: u64) {
        self.playhead = playhead;
        for slot in self.slots.iter_mut() {
            slot.synth.release_sequenced_unless(|_, _| false);
        }
        self.find_bookmarks();
        for track in self.snapshot.tracks() {
            self.slots[track.slot()].chase(track.notes(), playhead, &mut self.dropped_note_events);
        }
    }

    /// Stops, releases every note, and goes back to the play start: on Stop,
    /// and at the song's end.
    fn stop(&mut self) {
        self.playing = false;
        self.looping = false;
        for slot in self.slots.iter_mut() {
            slot.synth.release_all();
        }
        self.playhead = self.snapshot.sequence.sample_at(self.play_start);
    }

    /// After a new snapshot while playing, decides where playback is bound:
    /// - with the loop off, for the song's end;
    /// - if it wasn't bound for the loop's end (the loop was off, or
    ///   playback started after it), for the loop's end only if the playhead
    ///   is before it;
    /// - if it was, it stays so. If the loop got shorter or moved and the
    ///   playhead is now past its end, [`Self::handle_due_events`] goes
    ///   round the loop on this same sample.
    fn follow_the_loop(&mut self) {
        let sequence = &self.snapshot.sequence;
        if !sequence.loop_enabled() {
            self.looping = false;
        } else if !self.looping {
            self.looping = self.playhead < sequence.loop_samples().end;
        }
    }

    /// Points each track's bookmark at its first event at or after the
    /// playhead.
    fn find_bookmarks(&mut self) {
        let playhead = self.playhead;
        for track in self.snapshot.tracks() {
            self.slots[track.slot()].bookmark = track
                .notes()
                .events()
                .partition_point(|event| event.sample < playhead);
        }
    }

    /// After a new snapshot, releases every note the sequencer started that
    /// its track no longer plays at the playhead: deleted, re-pitched, moved
    /// or shortened away from it. Otherwise its end event would never come,
    /// and it would stick. A note moved under the playhead isn't started: it
    /// waits for its next pass. Bounded by the number of tracks and voices,
    /// each a binary search.
    fn release_changed_notes(&mut self) {
        let playhead = self.playhead;
        for track in self.snapshot.tracks() {
            let notes = track.notes();
            self.slots[track.slot()]
                .synth
                .release_sequenced_unless(|key, pitch| {
                    notes
                        .note(key)
                        .is_some_and(|note| note.pitch == pitch && note.contains(playhead))
                });
        }
    }

    /// Renders every slot with a track or a sound into `output`, which holds
    /// whole frames: each track's volume, pan, mute and solo make it stereo,
    /// and they're added together, then the master volume, then a hard clip
    /// at full scale. The first two channels get the left and right, and any
    /// more are silent. A single channel gets their average, so a centred
    /// track sounds the same in mono. See RFC-003, "The master and headroom".
    ///
    /// Tracks are added in slot order, which reordering them doesn't change,
    /// so it doesn't change the sound either.
    fn render(&mut self, output: &mut [f32]) {
        let frames = output.len() / self.channels;
        let [left, right] = &mut *self.mix;
        let (left, right) = (&mut left[..frames], &mut right[..frames]);
        left.fill(0.0);
        right.fill(0.0);
        for slot in self.slots.iter_mut() {
            if slot.track.is_some() || slot.synth.is_sounding() {
                slot.render(left, right);
            }
        }

        let mut peak = self.peak;
        let mut clips = self.clips;
        for ((frame, &left), &right) in output
            .chunks_exact_mut(self.channels)
            .zip(left.iter())
            .zip(right.iter())
        {
            let master = self.volume.next_value() * self.output_gain.next_value();
            let left = hard_clip(left * master, &mut clips);
            let right = hard_clip(right * master, &mut clips);
            // The peak is of what the device gets.
            match frame {
                [mono] => {
                    *mono = (left + right) * 0.5;
                    peak = peak.max(mono.abs());
                }
                [first, second, rest @ ..] => {
                    *first = left;
                    *second = right;
                    rest.fill(0.0);
                    peak = peak.max(left.abs()).max(right.abs());
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
                Command::Play if !self.playing => {
                    self.playing = true;
                    self.play_from(self.snapshot.sequence.sample_at(self.play_start));
                }
                Command::Play => {}
                Command::Stop => self.stop(),
                Command::Locate(ticks) => {
                    let sample = self.snapshot.sequence.sample_at(ticks);
                    if self.playing {
                        self.play_from(sample);
                    } else {
                        self.play_start = ticks;
                        self.playhead = sample;
                    }
                }
                Command::SetSnapshot(new) => {
                    self.volume.set_target(new.gain);
                    let old = std::mem::replace(&mut self.snapshot, new);
                    let sequence = &self.snapshot.sequence;
                    self.playhead = if self.playing {
                        sequence.playhead_from(&old.sequence, self.playhead)
                    } else {
                        sequence.sample_at(self.play_start)
                    };
                    let soloing = self.snapshot.soloing();
                    for (index, slot) in self.slots.iter_mut().enumerate() {
                        slot.load(self.snapshot.track_in(index), soloing);
                    }
                    self.find_bookmarks();
                    self.release_changed_notes();
                    if self.playing {
                        self.follow_the_loop();
                    }
                    self.swaps += 1;
                    // Checked above: there is room, so this never drops `old`.
                    let _ = self.used_snapshots.push(old);
                }
                Command::NoteOn {
                    slot,
                    key,
                    pitch,
                    velocity,
                } => {
                    // Only a slot with a track plays live notes.
                    if let Some(slot) = self.slots.get_mut(slot)
                        && slot.track.is_some()
                    {
                        slot.synth.note_on(NoteOn {
                            key,
                            pitch,
                            velocity,
                            sequenced: false,
                        });
                    }
                }
                Command::NoteOff { slot, key } => {
                    if let Some(slot) = self.slots.get_mut(slot) {
                        slot.synth.note_off(key);
                    }
                }
            }
        }
    }

    /// Sends this block's status. If the queue is full the message is skipped
    /// and the peaks carry over to the next one.
    fn send_status(&mut self) {
        let sequence = &self.snapshot.sequence;
        let loop_samples = sequence.loop_samples();
        // At the loop's end, the playhead is already back at its start.
        let playhead = if self.looping && self.playhead >= loop_samples.end {
            loop_samples.start
        } else {
            self.playhead
        };
        let mut track_peaks = [0.0; TRACK_SLOTS];
        let mut sounding_slots = 0u32;
        for (index, slot) in self.slots.iter().enumerate() {
            track_peaks[index] = slot.peak;
            if slot.synth.is_sounding() {
                sounding_slots |= 1 << index;
            }
        }
        let status = Status {
            position: self.position,
            playhead: sequence.ticks_at(playhead),
            peak: self.peak,
            track_peaks,
            clips: self.clips,
            dropouts: self.dropouts,
            playing: self.playing,
            dropped_note_events: self.dropped_note_events,
            sample_rate: self.sample_rate as u32,
            snapshots: self.swaps,
            sounding_slots,
        };
        if self.status.push(status).is_ok() {
            self.peak = 0.0;
            for slot in self.slots.iter_mut() {
                slot.peak = 0.0;
            }
        }
    }
}

/// Cuts `sample` off at full scale, counting it in `clips` if it was past it.
/// No delay and no state, so a sample within full scale passes through
/// exactly. NaN isn't a sound, so it becomes silence, and counts too.
#[inline]
fn hard_clip(sample: f32, clips: &mut u64) -> f32 {
    if sample.abs() <= 1.0 {
        return sample;
    }
    *clips += 1;
    if sample.is_nan() {
        0.0
    } else {
        sample.clamp(-1.0, 1.0)
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
    /// and isn't counted; one past it is cut off at full scale and counted,
    /// and NaN is silenced and counted.
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

        let past = [
            1.000_000_1,
            -1.000_000_1,
            4.0,
            -250.0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            -f32::NAN,
        ];
        let cut: Vec<f32> = past
            .iter()
            .map(|&sample| hard_clip(sample, &mut clips))
            .collect();
        assert_eq!(cut, [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 0.0, 0.0]);
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
