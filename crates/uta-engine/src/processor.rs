//! The audio-thread side of the engine.
//!
//! Everything reachable from [`Processor::process`] follows the audio thread
//! rules in `CLAUDE.md`: no waiting, no allocating or freeing, no I/O and no
//! unbounded loops.

use std::time::Instant;

use rtrb::{Consumer, Producer};

use uta_core::TrackId;
use uta_core::time::Ticks;

use crate::drums::Kit;
use crate::ramp::Ramp;
use crate::synth::{NoteOn, Synth, VOICES};
use crate::{
    COMMAND_CAPACITY, Command, KitSettings, NoteEventKind, Snapshot, Status, SynthSettings,
    TRACK_SLOTS, TrackNotes, TrackSnapshot, TrackSound,
};

/// How long the output takes to fade out before a stream is replaced, and
/// back in on the next one.
pub const FADE_SECONDS: f64 = 0.005;
/// How long a change to the master volume, or to a track's volume, pan, mute
/// or solo, takes to glide to its new level.
pub const VOLUME_SMOOTHING_SECONDS: f64 = 0.02;
/// The most note starts and ends each track handles in one block, so the work
/// per block stays bounded however dense the notes are. Chased notes count
/// too. Past it, a track catches up on the events due instead of handling
/// each one: it releases the notes that have ended and starts the ones that
/// should be sounding, as far as its voices go. It does so at each later
/// event in that block, each time bounded by the voices, so a block's work
/// is still bounded by its length. Starts the voices can't hold,
/// and chased notes past the budget, are counted in
/// [`Status::dropped_note_events`]. It's far more than music needs: 256
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
    /// the snapshot's rate. While stopped, the play start, or where it was
    /// paused.
    playhead: u64,
    /// Whether it's stopped by a Pause, with the playhead where it paused,
    /// for Continue to carry on from.
    paused: bool,
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
    /// The slowest block since status was last delivered, as a share of its
    /// deadline. See [`Status::slowest_block`].
    slowest_block: f32,
    /// Snapshots swapped in so far.
    swaps: u64,
}

/// One track's place on the audio thread: its voices, its kit, its place in
/// its events, its gains and its buffer. See RFC-003, "The shared model,
/// extended", point 4.
///
/// Every slot has both a synth and a kit, built when the processor is, so it
/// can play either kind of track. Notes go to whichever its track has. The
/// other may still be sounding from the slot's last track, and plays out.
///
/// Like the voices, everything here is a plain value, never a reference into
/// the snapshot, so nothing on the audio thread shares ownership of snapshot
/// data. See RFC-002, "The shared model", point 7.
struct Slot {
    /// The track playing in this slot in the current snapshot, if any. A
    /// slot whose track is gone keeps playing its last notes' releases.
    track: Option<TrackId>,
    synth: Synth,
    kit: Kit,
    /// Whether its track is a drum track, so notes are hits on the kit.
    drums: bool,
    /// The next event in its track's events: every event before it is
    /// earlier than the playhead.
    bookmark: usize,
    /// Note starts and ends it may still handle in this block.
    budget: usize,
    /// The track's gain for the left and right, from its mixer strip.
    gains: [Ramp; 2],
    /// The synth's and kit's output for the part of the block being
    /// rendered.
    buffer: [f32; TRACK_BUFFER_FRAMES],
    /// The loudest sample since status was last delivered, after the gains.
    peak: f32,
}

impl Slot {
    fn new(track: Option<&TrackSnapshot>, soloing: bool, sample_rate: f64) -> Self {
        let sound = track.map(|track| track.sound);
        let synth = match sound {
            Some(TrackSound::Synth(settings)) => settings,
            _ => SynthSettings::default(),
        };
        let kit = match sound {
            Some(TrackSound::Drums(kit)) => kit,
            _ => KitSettings::default(),
        };
        Self {
            track: track.map(TrackSnapshot::id),
            synth: Synth::new(synth, sample_rate),
            kit: Kit::new(kit, sample_rate),
            drums: matches!(sound, Some(TrackSound::Drums(_))),
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
            // its drums ring out. The control side doesn't hand the slot on
            // until they have.
            if self.track.take().is_some() {
                self.synth.release_all();
            }
            return;
        };
        let gains = track.mixer.gains(soloing);
        let same_track = self.track == Some(track.id());
        let sounding = self.synth.is_sounding() || self.kit.is_sounding();
        if !same_track && self.synth.is_sounding() {
            // A new track while another's notes still sound here: they fade
            // out quickly. Another track's drums just ring out.
            self.synth.fade_out();
        }
        self.track = Some(track.id());
        self.drums = matches!(track.sound, TrackSound::Drums(_));
        // The track's sound glides if it's the same track, or the slot is
        // still sounding; a new track in a silent slot starts with its own.
        let glide = same_track || sounding;
        match track.sound {
            TrackSound::Synth(settings) if glide => self.synth.set_settings(settings),
            TrackSound::Synth(settings) => self.synth.load(settings),
            TrackSound::Drums(kit) if glide => self.kit.set_settings(kit),
            TrackSound::Drums(kit) => self.kit.load(kit),
        }
        for (ramp, gain) in self.gains.iter_mut().zip(gains) {
            if glide {
                ramp.set_target(gain);
            } else {
                ramp.jump_to(gain);
            }
        }
    }

    /// Plays a note start from the sequencer or a live note: a note on the
    /// synth, or a hit on the kit.
    fn note_on(&mut self, note: NoteOn) {
        if self.drums {
            self.kit.hit(note.pitch, note.velocity);
        } else {
            self.synth.note_on(note);
        }
    }

    /// Whether anything in the slot is sounding.
    fn is_sounding(&self) -> bool {
        self.synth.is_sounding() || self.kit.is_sounding()
    }

    /// Starts every note in `notes` already under way at `playhead`, with an
    /// ordinary note on, up to the slot's budget. The rest are skipped and
    /// counted in `dropped`. Each ends at its own end event. See RFC-003,
    /// "Playing a song" (note chasing).
    ///
    /// Drum tracks don't chase: starting in the middle of a drum note
    /// doesn't play the hit late, because that sounds wrong. See RFC-006,
    /// "In the engine".
    fn chase(&mut self, notes: &TrackNotes, playhead: u64, dropped: &mut u64) {
        if self.drums {
            return;
        }
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

    /// Handles every event in `notes` due by `playhead`, up to the slot's
    /// budget. Past it, catches up instead: see [`Self::catch_up`].
    fn handle_due_events(&mut self, notes: &TrackNotes, playhead: u64, dropped: &mut u64) {
        let events = notes.events();
        // Bounded by the budget, then one catch-up.
        while let Some(event) = events.get(self.bookmark)
            && event.sample <= playhead
        {
            if self.budget == 0 {
                self.catch_up(notes, playhead, dropped);
                break;
            }
            self.budget -= 1;
            match event.kind {
                NoteEventKind::On {
                    key,
                    pitch,
                    velocity,
                } => self.note_on(NoteOn {
                    key,
                    pitch,
                    velocity,
                    sequenced: true,
                }),
                // Drum notes are one-shots: their ends do nothing.
                NoteEventKind::Off { .. } if self.drums => {}
                NoteEventKind::Off { key } => self.synth.note_off(key),
            }
            self.bookmark += 1;
        }
    }

    /// When more events are due by `playhead` than the budget allows,
    /// catches up on them without going through them one by one:
    /// 1. releases every note the sequencer started that `notes` doesn't
    ///    play at `playhead`, so no end it didn't handle leaves a note stuck;
    /// 2. starts the notes starting at `playhead`, in the free and releasing
    ///    voices, so no held note is taken over;
    /// 3. if any of those voices are left, starts notes already under way
    ///    there that aren't held, latest start first.
    ///
    /// A start in step 2 that doesn't get a voice is counted in `dropped`:
    /// only notes the voices can't hold are. Bounded by the voices, plus a
    /// few binary searches.
    ///
    /// A drum track skips the events due instead: its hits are one-shots, so
    /// there's nothing to release, and a late hit sounds wrong. Every event
    /// skipped is counted in `dropped`, ends too. One binary search.
    fn catch_up(&mut self, notes: &TrackNotes, playhead: u64, dropped: &mut u64) {
        let events = notes.events();
        let due = self.bookmark
            + events[self.bookmark..].partition_point(|event| event.sample <= playhead);
        if self.drums {
            *dropped += (due - self.bookmark) as u64;
            self.bookmark = due;
            return;
        }
        // Ends sort before starts on the same sample, so the starts on the
        // playhead are the last of the events due. A start before it that's
        // still due was missed, and step 3 finds its note.
        let starts = self.bookmark
            + events[self.bookmark..due].partition_point(|event| {
                event.sample < playhead || matches!(event.kind, NoteEventKind::Off { .. })
            });
        self.bookmark = due;

        self.release_ended(notes, playhead);
        let mut room = VOICES - self.synth.busy_voices();
        let starting = &events[starts..due];
        let started = starting.len().min(room);
        // Bounded by the voices.
        for event in &starting[..started] {
            if let NoteEventKind::On {
                key,
                pitch,
                velocity,
            } = event.kind
            {
                self.synth.note_on(NoteOn {
                    key,
                    pitch,
                    velocity,
                    sequenced: true,
                });
            }
        }
        *dropped += (starting.len() - started) as u64;
        room -= started;

        // The notes skipped here are held, so in busy voices: the room
        // plus the skips is at most the voices.
        for note in notes.sounding_at(playhead).take(VOICES) {
            if room == 0 {
                break;
            }
            if self.synth.is_holding(note.key) {
                continue;
            }
            room -= 1;
            self.synth.note_on(NoteOn {
                key: note.key,
                pitch: note.pitch,
                velocity: note.velocity,
                sequenced: true,
            });
        }
    }

    /// Releases every note the sequencer started that `notes` doesn't play
    /// at `playhead`, and cancels any such note waiting for a voice. Bounded
    /// by the voices, each a binary search.
    fn release_ended(&mut self, notes: &TrackNotes, playhead: u64) {
        self.synth.release_sequenced_unless(|key, pitch| {
            notes
                .note(key)
                .is_some_and(|note| note.pitch == pitch && note.contains(playhead))
        });
    }

    /// Renders the synth and the kit into the buffer, then adds it to `left`
    /// and `right` through the track's gains. At most
    /// [`TRACK_BUFFER_FRAMES`] frames. Whichever of them isn't sounding does
    /// no work, beyond its settings' glide.
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let buffer = &mut self.buffer[..left.len()];
        if self.synth.is_sounding() || !self.drums {
            for sample in buffer.iter_mut() {
                *sample = self.synth.next_sample();
            }
        } else {
            buffer.fill(0.0);
        }
        if self.kit.is_sounding() || self.drums {
            for sample in buffer.iter_mut() {
                *sample += self.kit.next_sample();
            }
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
            paused: false,
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
            slowest_block: 0.0,
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
        // Paused, it stays where it paused, for Continue.
        self.playhead = if self.playing || self.paused {
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
            slot.kit.prepare(sample_rate);
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
    ///
    /// The block is timed from here to just before its status is sent, and
    /// the time is reported as a share of its deadline.
    #[rtsan_standalone::nonblocking]
    pub fn process(&mut self, output: &mut [f32]) {
        let started = Instant::now();
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
        self.time_block(started, frames);
        self.send_status();
    }

    /// Records how long a block of `frames` took since `started`, as a share
    /// of its deadline: the time the device takes to play it. Reading the
    /// clock neither waits nor allocates.
    fn time_block(&mut self, started: Instant, frames: usize) {
        if frames == 0 {
            return;
        }
        let deadline = frames as f64 / self.sample_rate;
        let load = (started.elapsed().as_secs_f64() / deadline) as f32;
        self.slowest_block = self.slowest_block.max(load);
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
    /// most its budget of events, and catches up on the rest.
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
                    track.notes(),
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
    ///
    /// Each kit's free-running parts restart from the same point, so what
    /// you hear from a place is what a render plays from there. See
    /// RFC-006, resolved open question 3.
    fn play_from(&mut self, playhead: u64) {
        let sequence = &self.snapshot.sequence;
        self.looping = sequence.loop_enabled() && playhead < sequence.loop_samples().end;
        for slot in self.slots.iter_mut() {
            slot.kit.restart();
        }
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
        self.pause();
        self.paused = false;
        self.playhead = self.snapshot.sequence.sample_at(self.play_start);
    }

    /// Stops and releases every note, leaving the playhead where it is, for
    /// Continue to carry on from. At the loop's end it's already back at the
    /// loop's start, as the status shows it.
    fn pause(&mut self) {
        let loop_samples = self.snapshot.sequence.loop_samples();
        if self.looping && self.playhead >= loop_samples.end {
            self.playhead = loop_samples.start;
        }
        self.playing = false;
        self.looping = false;
        self.paused = true;
        for slot in self.slots.iter_mut() {
            slot.synth.release_all();
        }
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
            self.slots[track.slot()].release_ended(track.notes(), playhead);
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
            if slot.track.is_some() || slot.is_sounding() {
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
                    self.paused = false;
                    self.play_from(self.snapshot.sequence.sample_at(self.play_start));
                }
                Command::Play => {}
                Command::Stop => self.stop(),
                Command::Pause if self.playing => self.pause(),
                Command::Pause => {}
                Command::Continue if !self.playing => {
                    self.playing = true;
                    // Stopped, the playhead is where it paused or the play
                    // start.
                    self.paused = false;
                    self.play_from(self.playhead);
                }
                Command::Continue => {}
                Command::Locate(ticks) => {
                    let sample = self.snapshot.sequence.sample_at(ticks);
                    if self.playing {
                        self.play_from(sample);
                    } else {
                        self.play_start = ticks;
                        self.playhead = sample;
                        self.paused = false;
                    }
                }
                Command::SetSnapshot(new) => {
                    self.volume.set_target(new.gain);
                    let old = std::mem::replace(&mut self.snapshot, new);
                    let sequence = &self.snapshot.sequence;
                    self.playhead = if self.playing || self.paused {
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
                        slot.note_on(NoteOn {
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
    /// and the peaks and the slowest block carry over to the next one.
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
            if slot.is_sounding() {
                sounding_slots |= 1 << index;
            }
        }
        let status = Status {
            position: self.position,
            playhead: sequence.ticks_at(playhead),
            play_start: self.play_start,
            peak: self.peak,
            track_peaks,
            slowest_block: self.slowest_block,
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
            self.slowest_block = 0.0;
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

    /// Notes held across a sample with more note events than a block may
    /// handle keep their voices: catching up neither takes them over nor
    /// starts them again. 600 notes start at the beginning, 4 long ones a
    /// beat later, and the 600 all end on beat 3.
    #[test]
    fn notes_held_across_a_catch_up_keep_their_voices() {
        let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
        let track = project.tracks()[0].id();
        let clip = project.tracks()[0].clips()[0].id();
        let note = |id: u128, pitch: u8, start: Ticks, length: Ticks| Note {
            id: NoteId::from_uuid(Uuid::from_u128(id)),
            pitch,
            velocity: 100,
            start,
            length,
        };
        let crowd = MAX_NOTE_EVENTS_PER_BLOCK as u128 + 88;
        let mut notes: Vec<Note> = (0..crowd)
            .map(|i| note(i, 40 + (i % 20) as u8, 0, 2 * BEAT))
            .collect();
        let long: Vec<u128> = (10_000..10_004).collect();
        notes.extend(
            long.iter()
                .map(|&id| note(id, 70 + (id % 10) as u8, BEAT, 2 * BEAT)),
        );
        project.apply(&Command::AddNotes { clip, notes }).unwrap();
        project
            .apply(&Command::SetSynthParam {
                track,
                param: SynthParam::ReleaseSeconds(0.001),
            })
            .unwrap();
        let config = EngineConfig {
            sample_rate: 48_000,
            channels: 1,
        };
        let mut renderer = Renderer::new(config, Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        let held_long = |renderer: &mut Renderer| {
            let held = renderer.processor().slots[0].synth.held_notes();
            held.into_iter()
                .filter(|(key, _)| long.contains(&key.0))
                .collect::<Vec<_>>()
        };
        // Beat 3 is at sample 48,000.
        renderer.render(47_000);
        let before = held_long(&mut renderer);
        assert_eq!(before.len(), long.len(), "the long notes are playing");
        renderer.render(2_000);
        assert_eq!(held_long(&mut renderer), before);
        assert_eq!(
            renderer.processor().slots[0].synth.held_notes().len(),
            long.len(),
            "the crowd is released"
        );
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

    /// A block that took twice its deadline reads as 2.0, a quick one after
    /// it doesn't lower it, and the slowest is reset once status gets out.
    #[test]
    fn the_slowest_block_is_kept_until_status_is_sent() {
        let config = EngineConfig {
            sample_rate: 48_000,
            channels: 1,
        };
        let mut renderer = Renderer::new(config, Snapshot::from(&beats()), 48);
        let processor = renderer.processor();
        // 48 frames at 48 kHz have a 1 ms deadline.
        let two_deadlines_ago = Instant::now() - std::time::Duration::from_millis(2);
        processor.time_block(two_deadlines_ago, 48);
        processor.time_block(Instant::now(), 48);
        // How long the test took to get here only adds to it.
        assert!(
            (2.0..3.0).contains(&processor.slowest_block),
            "{}",
            processor.slowest_block
        );
        processor.send_status();
        assert_eq!(processor.slowest_block, 0.0);
        let status = renderer.controller.poll();
        assert!((2.0..3.0).contains(&status.slowest_block), "{status:?}");
    }

    /// A device switch mid-note, from 48 kHz to 44.1 kHz and back, the way
    /// the supervisor does it: fade out, move the processor to the new rate,
    /// carry on. The loop keeps its bar and beat, and every note after the
    /// switch lands on its exact sample at the new rate, for ten passes.
    /// A device switch while paused keeps the playhead where it paused, on
    /// the same tick at the new rate, so Continue carries on from there.
    #[test]
    fn a_new_sample_rate_keeps_the_playhead_where_it_paused() {
        let config = EngineConfig {
            sample_rate: 48_000,
            channels: 1,
        };
        let mut renderer = Renderer::new(config, Snapshot::from(&beats()), 128);
        renderer.controller.play().unwrap();
        // 512 ticks at 120 BPM and 48 kHz: 12,800 samples, 100 blocks.
        renderer.render(12_800);
        renderer.controller.pause().unwrap();
        renderer.render(128);
        assert_eq!(renderer.controller.poll().playhead, 512);

        renderer.processor().prepare(44_100, 1);
        renderer.render(1280);
        let status = renderer.controller.poll();
        assert!(!status.playing);
        assert_eq!(status.playhead, 512, "still where it paused");

        renderer.controller.resume().unwrap();
        renderer.render(128);
        let status = renderer.controller.poll();
        assert!(status.playing);
        assert!(
            (512..520).contains(&status.playhead),
            "carries on from there: {}",
            status.playhead
        );
    }

    /// Pausing exactly on the loop's end pauses at its start, where playback
    /// was about to go, so Continue goes round the loop rather than on past
    /// it to the song's end.
    #[test]
    fn pausing_on_the_loops_end_carries_on_round_the_loop() {
        let config = EngineConfig {
            sample_rate: 48_000,
            channels: 1,
        };
        let mut renderer = Renderer::new(config, Snapshot::from(&beats()), 128);
        renderer.controller.play().unwrap();
        // The 1-bar loop is 96,000 samples: 750 blocks, so the pause lands on
        // its end exactly.
        renderer.render(96_000);
        renderer.controller.pause().unwrap();
        renderer.render(128);
        assert_eq!(
            renderer.controller.poll().playhead,
            0,
            "paused at the loop's start"
        );

        renderer.controller.resume().unwrap();
        // Half a bar on.
        renderer.render(48_000);
        let status = renderer.controller.poll();
        assert!(status.playing);
        assert_eq!(status.playhead, 2 * BEAT, "round the loop, half a bar in");
    }

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
