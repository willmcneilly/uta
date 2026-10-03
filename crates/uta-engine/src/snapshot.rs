//! The "what to play" snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use uta_core::time::{TempoMap, Ticks};
use uta_core::{ClipId, Notes, Project, Source, TrackId, Transport};

use crate::{DEFAULT_SAMPLE_RATE, MixerStrip, NoteKey, SynthSettings, TRACK_SLOTS, Waveform};

/// Everything the audio thread needs to know about what to play.
///
/// The control side builds a whole new snapshot for every change, and the
/// audio thread swaps it in at the start of a block. The old one goes back to
/// the control side to be freed. It lives in a `Box` on its way through the
/// queues, so the audio thread only ever moves a pointer.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The master volume as a linear gain (1.0 is full scale).
    pub gain: f32,
    /// The tempo, the loop and the song's end, timed in samples.
    pub sequence: Sequence,
    /// Every track, in the project's order.
    tracks: Vec<TrackSnapshot>,
    /// For each slot, the index in `tracks` of the track that uses it.
    slots: [Option<u8>; TRACK_SLOTS],
}

/// One track as the engine plays it: its sound, its mixer strip and its
/// notes, and the slot on the audio thread it plays in. See RFC-003, "The
/// shared model, extended", points 4 and 5.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackSnapshot {
    id: TrackId,
    slot: usize,
    /// How the track's synth sounds.
    pub synth: SynthSettings,
    /// The track's volume, pan, mute and solo.
    pub mixer: MixerStrip,
    notes: Arc<TrackNotes>,
}

impl TrackSnapshot {
    pub fn id(&self) -> TrackId {
        self.id
    }

    /// The slot it plays in, from 0 up to but not including
    /// [`TRACK_SLOTS`]. A track keeps its slot for as long as it's in the
    /// project.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// Its clips' notes and their starts and ends, shared between snapshots
    /// until the track's clips or the timing change.
    pub fn notes(&self) -> &Arc<TrackNotes> {
        &self.notes
    }
}

impl Snapshot {
    /// The master's default volume, in dB.
    pub const DEFAULT_VOLUME_DB: f32 = -12.0;
    /// The master's loudest volume, in dB. Above it, loud chords could push
    /// samples past full scale, where the hard clip cuts them off.
    pub const MAX_VOLUME_DB: f32 = 0.0;

    /// What the engine plays for `project`, with its notes timed at
    /// `sample_rate`. The tracks take the slots from 0 up, in order.
    pub fn new(project: &Project, sample_rate: u32) -> Self {
        Self::build(project, sample_rate, None, 0)
    }

    /// What the engine plays for `project`, timed at `previous`'s rate. See
    /// RFC-003, "The shared model, extended", points 4 and 5:
    /// - every track that was in `previous` keeps its slot, and a new one
    ///   takes a slot `previous` didn't use, so it doesn't land on a removed
    ///   track's notes as they fade;
    /// - every clip whose start, length and notes haven't changed, found by
    ///   its ID wherever it was, shares its notes with `previous`. Its notes
    ///   haven't changed if it still has the same `Arc` of them as the
    ///   project `previous` was built from, so no note is compared.
    ///   `previous` holds on to that `Arc`, so its memory can't have been
    ///   freed and reused for other notes;
    /// - every track whose clips all share their notes, at the same timing,
    ///   shares its events with `previous`, so an edit rebuilds only the
    ///   tracks it touches.
    pub fn sharing(project: &Project, previous: &Snapshot) -> Self {
        Self::sharing_avoiding(project, previous, 0)
    }

    /// [`Snapshot::sharing`], with new tracks also avoiding the slots in the
    /// `avoid` bitmask, unless there's no other slot left.
    pub(crate) fn sharing_avoiding(project: &Project, previous: &Snapshot, avoid: u32) -> Self {
        Self::build(
            project,
            previous.sequence.sample_rate,
            Some(previous),
            avoid,
        )
    }

    fn build(project: &Project, sample_rate: u32, previous: Option<&Snapshot>, avoid: u32) -> Self {
        let sequence = Sequence::new(project, sample_rate);
        let same_timing = previous.is_some_and(|previous| previous.sequence.same_timing(&sequence));
        let previous_clips: HashMap<ClipId, &Arc<ClipNotes>> = previous
            .iter()
            .flat_map(|previous| &previous.tracks)
            .flat_map(|track| &track.notes.clips)
            .map(|clip| (clip.id, clip))
            .collect();
        let slots = assign_slots(project, previous, avoid);

        let tracks = project
            .tracks()
            .iter()
            .zip(slots)
            .map(|(track, slot)| {
                let clips: Vec<Arc<ClipNotes>> = track
                    .clips()
                    .iter()
                    .map(|clip| match previous_clips.get(&clip.id()) {
                        Some(shared)
                            if shared.start == clip.start()
                                && shared.length == clip.length()
                                && Arc::ptr_eq(&shared.notes, clip.shared_notes()) =>
                        {
                            Arc::clone(shared)
                        }
                        _ => Arc::new(ClipNotes {
                            id: clip.id(),
                            start: clip.start(),
                            length: clip.length(),
                            notes: Arc::clone(clip.shared_notes()),
                        }),
                    })
                    .collect();
                let notes = match previous.and_then(|previous| previous.track(track.id())) {
                    Some(old) if same_timing && old.notes.shares(&clips) => Arc::clone(&old.notes),
                    _ => Arc::new(TrackNotes::new(clips, &sequence)),
                };
                let Source::Synth(settings) = track.source();
                TrackSnapshot {
                    id: track.id(),
                    slot,
                    synth: synth_settings(settings),
                    mixer: MixerStrip::from(track.mixer()),
                    notes,
                }
            })
            .collect();
        Self::with_tracks(1.0, sequence, tracks).with_volume_db(project.master_volume_db())
    }

    fn with_tracks(gain: f32, sequence: Sequence, tracks: Vec<TrackSnapshot>) -> Self {
        let mut slots = [None; TRACK_SLOTS];
        for (index, track) in tracks.iter().enumerate() {
            assert!(slots[track.slot].is_none(), "two tracks in one slot");
            slots[track.slot] = Some(index as u8);
        }
        Self {
            gain,
            sequence,
            tracks,
            slots,
        }
    }

    /// A snapshot with the given master volume in dB, clamped to
    /// [`Self::MAX_VOLUME_DB`]. NaN is silence.
    pub fn with_volume_db(self, volume_db: f32) -> Self {
        let gain = if volume_db.is_nan() {
            0.0
        } else {
            db_to_gain(volume_db.min(Self::MAX_VOLUME_DB))
        };
        Self { gain, ..self }
    }

    /// This snapshot with its notes timed at `sample_rate`. The clips' notes
    /// are shared, not copied. Returns a plain clone if it's already at that
    /// rate.
    pub fn at_sample_rate(&self, sample_rate: u32) -> Self {
        if sample_rate == self.sequence.sample_rate {
            return self.clone();
        }
        let sequence = self.sequence.at_sample_rate(sample_rate);
        let tracks = self
            .tracks
            .iter()
            .map(|track| TrackSnapshot {
                notes: Arc::new(TrackNotes::new(track.notes.clips.clone(), &sequence)),
                ..track.clone()
            })
            .collect();
        Self::with_tracks(self.gain, sequence, tracks)
    }

    /// Every track, in the project's order.
    pub fn tracks(&self) -> &[TrackSnapshot] {
        &self.tracks
    }

    /// Every track, to change its sound or mixer strip.
    pub fn tracks_mut(&mut self) -> &mut [TrackSnapshot] {
        &mut self.tracks
    }

    /// The track with this ID.
    pub fn track(&self, id: TrackId) -> Option<&TrackSnapshot> {
        self.tracks.iter().find(|track| track.id == id)
    }

    /// The track playing in `slot`, if any. Real-time safe.
    pub fn track_in(&self, slot: usize) -> Option<&TrackSnapshot> {
        let index = (*self.slots.get(slot)?)?;
        self.tracks.get(usize::from(index))
    }

    /// The slot the track with this ID plays in.
    pub fn slot_of(&self, id: TrackId) -> Option<usize> {
        self.track(id).map(TrackSnapshot::slot)
    }

    /// Whether any track is soloed, so only soloed tracks play. Real-time
    /// safe.
    pub fn soloing(&self) -> bool {
        self.tracks.iter().any(|track| track.mixer.solo)
    }
}

/// The slot for each of `project`'s tracks, in order: the one it had in
/// `previous`, or else the lowest one neither `previous` nor `avoid` uses.
/// If every slot is taken that way, the lowest one no track in `project`
/// uses.
fn assign_slots(project: &Project, previous: Option<&Snapshot>, avoid: u32) -> Vec<usize> {
    let kept: Vec<Option<usize>> = project
        .tracks()
        .iter()
        .map(|track| previous.and_then(|previous| previous.slot_of(track.id())))
        .collect();
    let mask =
        |slots: &mut dyn Iterator<Item = usize>| slots.fold(0u32, |mask, slot| mask | 1 << slot);
    let mut used = mask(&mut kept.iter().flatten().copied());
    let before = previous.map_or(0, |previous| {
        mask(&mut previous.tracks.iter().map(|t| t.slot))
    });
    let lowest_free = |taken: u32| {
        let slot = (!taken).trailing_zeros() as usize;
        (slot < TRACK_SLOTS).then_some(slot)
    };
    kept.into_iter()
        .map(|slot| {
            slot.unwrap_or_else(|| {
                let slot = lowest_free(used | before | avoid)
                    .or_else(|| lowest_free(used))
                    .expect("a project has at most TRACK_SLOTS tracks");
                used |= 1 << slot;
                slot
            })
        })
        .collect()
}

/// A new project's snapshot, at [`DEFAULT_SAMPLE_RATE`]: an empty loop.
impl Default for Snapshot {
    fn default() -> Self {
        Self::from(&Project::new())
    }
}

/// What the engine plays for a project, at [`DEFAULT_SAMPLE_RATE`]. The
/// control side sends the result with [`crate::Controller::set_snapshot`]
/// after each change, and the controller retimes it to the engine's current
/// rate.
impl From<&Project> for Snapshot {
    fn from(project: &Project) -> Self {
        Self::new(project, DEFAULT_SAMPLE_RATE)
    }
}

/// The tempo, the loop and the song's end, as the audio thread plays them:
/// in ticks, and worked out at one sample rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    tempo_map: TempoMap,
    loop_start: Ticks,
    loop_length: Ticks,
    loop_enabled: bool,
    song_end: Ticks,

    sample_rate: u32,
    loop_start_sample: u64,
    loop_end_sample: u64,
    song_end_sample: u64,
}

/// One track's notes, as the audio thread plays them.
///
/// It keeps the musical version (each clip's notes, in ticks, in an `Arc`)
/// and, worked out from it at the snapshot's timing, every note start and end
/// in samples as one list sorted by time, over the whole song. The loop
/// doesn't trim them: the processor releases what's sounding when it goes
/// back to the loop's start. See RFC-002, "The shared model", points 5 and
/// 7, and RFC-003, "The shared model, extended", point 5, and "Playing a
/// song".
#[derive(Debug, Clone, PartialEq)]
pub struct TrackNotes {
    clips: Vec<Arc<ClipNotes>>,
    events: Vec<NoteEvent>,
    /// The same notes, one entry each, sorted by key, so a sounding note can
    /// be looked up when a new snapshot arrives.
    notes: Vec<NoteSpan>,
    /// The index for note chasing: indexes into `notes`, sorted by start.
    by_start: Vec<u32>,
    /// A max tree over `by_start`'s ends: leaf `i` (at `leaves + i`) is the
    /// end of note `by_start[i]`, and each node above is the latest end
    /// below it. With it, finding each note that's sounding at a sample
    /// takes one walk down the tree, however many notes there are. See
    /// RFC-003, "Risks & unknowns" (note chasing needs a fast lookup).
    latest_end: Vec<u64>,
    /// Every note's end, sorted, to count the notes sounding at a sample.
    ends: Vec<u64>,
}

/// One clip's notes, in ticks.
///
/// The notes are the project's own `Arc` of them, shared with the core
/// rather than copied (RFC-004, "How changes are spotted"). Only the control
/// side reads them, to build [`TrackNotes`]: the audio thread plays the
/// events worked out from them.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNotes {
    pub id: ClipId,
    /// Where the clip starts, in ticks from the start of the song.
    pub start: Ticks,
    /// Notes starting at or after this, from the clip's start, don't play.
    pub length: Ticks,
    pub notes: Arc<Notes>,
}

/// A note starting or ending, on an exact sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEvent {
    /// The sample it happens on, counted from the start of the song.
    pub sample: u64,
    pub kind: NoteEventKind,
}

/// When one note plays, in samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteSpan {
    pub key: NoteKey,
    pub pitch: u8,
    pub velocity: u8,
    /// The sample it starts on, from the start of the song.
    pub start: u64,
    /// The sample it ends on: it sounds up to, not including, this one.
    pub end: u64,
}

impl NoteSpan {
    /// Whether it's sounding at `sample`.
    pub fn contains(&self, sample: u64) -> bool {
        (self.start..self.end).contains(&sample)
    }
}

/// The notes sounding at a sample that started before it, latest start
/// first. See [`TrackNotes::sounding_at`].
#[derive(Debug, Clone)]
pub struct SoundingNotes<'a> {
    notes: &'a TrackNotes,
    sample: u64,
    /// Every note sounding at `sample` that starts at or after this position
    /// in `by_start` has been returned.
    started: usize,
    /// How many are left to return.
    remaining: usize,
}

impl<'a> Iterator for SoundingNotes<'a> {
    type Item = &'a NoteSpan;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let position = self.notes.last_sounding(self.started, self.sample)?;
        self.started = position;
        self.remaining -= 1;
        let index = self.notes.by_start[position];
        Some(&self.notes.notes[index as usize])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for SoundingNotes<'_> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteEventKind {
    /// Starts a note. The key is the note's permanent ID.
    On {
        key: NoteKey,
        pitch: u8,
        velocity: u8,
    },
    /// Releases the note started with this key.
    Off { key: NoteKey },
}

impl NoteEventKind {
    /// Ends sort before starts on the same sample, so a voice is released
    /// before the next note needs one.
    fn order(&self) -> u8 {
        match self {
            Self::Off { .. } => 0,
            Self::On { .. } => 1,
        }
    }
}

impl TrackNotes {
    /// The starts and ends of the notes in `clips`, timed by `sequence`.
    /// Clips may overlap: each one's notes play. Only the tempo and sample
    /// rate matter, not the loop.
    pub fn new(clips: Vec<Arc<ClipNotes>>, sequence: &Sequence) -> Self {
        let mut events = Vec::new();
        let mut notes = Vec::new();
        for clip in &clips {
            for note in clip.notes.values() {
                if note.start >= clip.length {
                    continue;
                }
                let start = clip.start + note.start;
                let end = clip.start + (note.start + note.length).min(clip.length);
                let (start, end) = (sequence.sample_at(start), sequence.sample_at(end));
                if end <= start {
                    continue;
                }
                let key = NoteKey(note.id.as_uuid().as_u128());
                notes.push(NoteSpan {
                    key,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    start,
                    end,
                });
                events.push(NoteEvent {
                    sample: start,
                    kind: NoteEventKind::On {
                        key,
                        pitch: note.pitch,
                        velocity: note.velocity,
                    },
                });
                events.push(NoteEvent {
                    sample: end,
                    kind: NoteEventKind::Off { key },
                });
            }
        }
        events.sort_by_key(|event| (event.sample, event.kind.order()));
        notes.sort_by_key(|note| note.key.0);

        let mut by_start: Vec<u32> = (0..notes.len() as u32).collect();
        by_start.sort_by_key(|&index| notes[index as usize].start);
        let leaves = by_start.len().next_power_of_two();
        let mut latest_end = vec![0; 2 * leaves];
        for (leaf, &index) in by_start.iter().enumerate() {
            latest_end[leaves + leaf] = notes[index as usize].end;
        }
        for node in (1..leaves).rev() {
            latest_end[node] = latest_end[2 * node].max(latest_end[2 * node + 1]);
        }
        let mut ends: Vec<u64> = notes.iter().map(|note| note.end).collect();
        ends.sort_unstable();
        Self {
            clips,
            events,
            notes,
            by_start,
            latest_end,
            ends,
        }
    }

    /// Whether these are the notes of exactly `clips`, the same `Arc`s in
    /// the same order.
    fn shares(&self, clips: &[Arc<ClipNotes>]) -> bool {
        self.clips.len() == clips.len()
            && self.clips.iter().zip(clips).all(|(a, b)| Arc::ptr_eq(a, b))
    }

    /// Each clip's notes, shared between snapshots.
    pub fn clips(&self) -> &[Arc<ClipNotes>] {
        &self.clips
    }

    /// Every note start and end in the song, sorted by sample.
    pub fn events(&self) -> &[NoteEvent] {
        &self.events
    }

    /// Every note that's sounding at `sample` but started before it: the
    /// notes to chase when playback starts, jumps or goes round the loop
    /// there. A note that starts on `sample` isn't one; its start event
    /// plays it. Latest start first.
    ///
    /// Real-time safe: it doesn't allocate, and each note costs one binary
    /// search and one walk down the index, however many notes there are.
    /// The caller bounds how many it takes.
    pub fn sounding_at(&self, sample: u64) -> SoundingNotes<'_> {
        let started = self
            .by_start
            .partition_point(|&index| self.notes[index as usize].start < sample);
        // Every note that has ended by `sample` started before it.
        let ended = self.ends.partition_point(|&end| end <= sample);
        SoundingNotes {
            notes: self,
            sample,
            started,
            remaining: started - ended,
        }
    }

    /// The position in `by_start` of the latest-starting note before
    /// position `before` that's still sounding at `sample`. One walk up the
    /// tree to find the part of `0..before` holding it, then one walk down.
    fn last_sounding(&self, before: usize, sample: u64) -> Option<usize> {
        let leaves = self.latest_end.len() / 2;
        let tree = &self.latest_end;
        // The nodes covering `0..before`, right to left: the right edge's
        // come right to left, and the left edge never leaves position 0,
        // so it adds at most the root, last.
        let (mut left, mut right) = (leaves, leaves + before);
        // Bounded by the tree's height.
        while left < right {
            if left & 1 == 1 {
                if tree[left] > sample {
                    return Some(self.rightmost_below(left, sample, leaves));
                }
                left += 1;
            }
            if right & 1 == 1 {
                right -= 1;
                if tree[right] > sample {
                    return Some(self.rightmost_below(right, sample, leaves));
                }
            }
            left /= 2;
            right /= 2;
        }
        None
    }

    /// Walks down from `node`, whose latest end is after `sample`, to the
    /// rightmost leaf below it whose end is after `sample`, and returns that
    /// leaf's position. Bounded by the tree's height.
    fn rightmost_below(&self, mut node: usize, sample: u64, leaves: usize) -> usize {
        while node < leaves {
            node = if self.latest_end[2 * node + 1] > sample {
                2 * node + 1
            } else {
                2 * node
            };
        }
        node - leaves
    }

    /// The note with this key, if it plays in the song. A binary search, so
    /// it's real-time safe.
    pub fn note(&self, key: NoteKey) -> Option<&NoteSpan> {
        self.notes
            .binary_search_by_key(&key.0, |note| note.key.0)
            .ok()
            .map(|index| &self.notes[index])
    }
}

impl Sequence {
    /// The project's tempo, loop and song end, at `sample_rate`.
    fn new(project: &Project, sample_rate: u32) -> Self {
        let transport: &Transport = project.transport();
        Self::build(
            transport.tempo_map().clone(),
            transport.loop_start()..transport.loop_start() + transport.loop_length(),
            transport.loop_enabled(),
            project.song_end(),
            sample_rate,
        )
    }

    fn build(
        tempo_map: TempoMap,
        loop_ticks: std::ops::Range<Ticks>,
        loop_enabled: bool,
        song_end: Ticks,
        sample_rate: u32,
    ) -> Self {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(!loop_ticks.is_empty(), "the loop can't be empty");
        let samples = |ticks| tempo_map.ticks_to_samples(ticks, sample_rate);
        Self {
            loop_start_sample: samples(loop_ticks.start),
            loop_end_sample: samples(loop_ticks.end),
            song_end_sample: samples(song_end),
            tempo_map,
            loop_start: loop_ticks.start,
            loop_length: loop_ticks.end - loop_ticks.start,
            loop_enabled,
            song_end,
            sample_rate,
        }
    }

    /// This sequence timed at `sample_rate`.
    pub fn at_sample_rate(&self, sample_rate: u32) -> Self {
        Self::build(
            self.tempo_map.clone(),
            self.loop_start..self.loop_start + self.loop_length,
            self.loop_enabled,
            self.song_end,
            sample_rate,
        )
    }

    /// Whether notes are timed the same in both: the same tempo and sample
    /// rate. The loop and the song's end don't change when notes play.
    fn same_timing(&self, other: &Sequence) -> bool {
        self.sample_rate == other.sample_rate && self.tempo_map == other.tempo_map
    }

    /// The sample rate everything is timed at.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Where the loop starts and ends, in samples from the start of the song.
    pub fn loop_samples(&self) -> std::ops::Range<u64> {
        self.loop_start_sample..self.loop_end_sample
    }

    /// Whether playback goes round the loop, rather than on to the end of
    /// the song.
    pub fn loop_enabled(&self) -> bool {
        self.loop_enabled
    }

    /// Where the song ends, in samples from its start: one bar after the
    /// last clip ends. See RFC-003, "Playing a song".
    pub fn song_end_sample(&self) -> u64 {
        self.song_end_sample
    }

    /// Where a playhead at `playhead` in `old` carries on in this sequence.
    /// Real-time safe.
    ///
    /// If the tempo or sample rate changed, it keeps its bar and beat: it
    /// moves to the first tick `old` hadn't reached yet, so every note event
    /// `old` had already played stays played and none is skipped. Otherwise
    /// it stays on the same sample. Whether that's past the loop or the
    /// song's end is for the processor to decide.
    pub fn playhead_from(&self, old: &Sequence, playhead: u64) -> u64 {
        if self.same_timing(old) {
            return playhead;
        }
        let ticks = old.ticks_at(playhead);
        let ticks = if old.sample_at(ticks) < playhead {
            ticks + 1
        } else {
            ticks
        };
        self.sample_at(ticks)
    }

    /// The musical position of a sample, in ticks from the start of the song.
    /// Real-time safe.
    pub fn ticks_at(&self, sample: u64) -> Ticks {
        self.tempo_map.samples_to_ticks(sample, self.sample_rate)
    }

    /// The sample a tick falls on. Real-time safe.
    pub fn sample_at(&self, ticks: Ticks) -> u64 {
        self.tempo_map.ticks_to_samples(ticks, self.sample_rate)
    }
}

/// A 1-bar loop at 120 BPM filled with legato 32nd notes, so something is
/// always sounding while it plays. For tests that need sound at any moment.
#[cfg(test)]
pub(crate) fn busy_loop() -> Snapshot {
    let id = |n| uuid::Uuid::from_u128(n);
    let mut project = Project::with_id(uta_core::ProjectId::from_uuid(id(1)));
    project
        .apply(&uta_core::Command::SetLoopLength { bars: 1 })
        .unwrap();
    let clip = project.tracks()[0].clips()[0].id();
    let notes = (0..32u8)
        .map(|i| uta_core::Note {
            id: uta_core::NoteId::from_uuid(id(100 + u128::from(i))),
            pitch: 60 + i % 12,
            velocity: 100,
            start: Ticks::from(i) * 120,
            length: 120,
        })
        .collect();
    project
        .apply(&uta_core::Command::AddNotes { clip, notes })
        .unwrap();
    Snapshot::from(&project)
}

/// The core's synth settings as the engine plays them: the same fields,
/// units and ranges.
fn synth_settings(settings: &uta_core::SynthSettings) -> SynthSettings {
    SynthSettings {
        waveform: match settings.waveform {
            uta_core::Waveform::Sine => Waveform::Sine,
            uta_core::Waveform::Triangle => Waveform::Triangle,
            uta_core::Waveform::Saw => Waveform::Saw,
            uta_core::Waveform::Square => Waveform::Square,
        },
        cutoff_hz: settings.cutoff_hz,
        resonance: settings.resonance,
        attack_seconds: settings.attack_seconds,
        decay_seconds: settings.decay_seconds,
        sustain: settings.sustain,
        release_seconds: settings.release_seconds,
    }
}

/// Converts decibels to a linear gain. Anything at or below the project's
/// minimum volume, [`Project::MIN_VOLUME_DB`], is silence.
pub fn db_to_gain(db: f32) -> f32 {
    if db <= Project::MIN_VOLUME_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uta_core::{ClipId, Command, Note, NoteId, ProjectId, SynthParam};
    use uuid::Uuid;

    fn project() -> Project {
        Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)))
    }

    fn clip(project: &Project) -> ClipId {
        project.tracks()[0].clips()[0].id()
    }

    fn note(id: u128, pitch: u8, start: Ticks, length: Ticks) -> Note {
        Note {
            id: NoteId::from_uuid(Uuid::from_u128(id)),
            pitch,
            velocity: 100,
            start,
            length,
        }
    }

    fn with_notes(notes: Vec<Note>) -> Project {
        let mut project = project();
        let clip = clip(&project);
        project.apply(&Command::AddNotes { clip, notes }).unwrap();
        project
    }

    /// The first track's clips.
    fn clips(snapshot: &Snapshot) -> &[Arc<ClipNotes>] {
        snapshot.tracks()[0].notes().clips()
    }

    /// The first track's events.
    fn events(snapshot: &Snapshot) -> &[NoteEvent] {
        snapshot.tracks()[0].notes().events()
    }

    fn key(id: u128) -> NoteKey {
        NoteKey(Uuid::from_u128(id).as_u128())
    }

    fn on(sample: u64, id: u128, pitch: u8) -> NoteEvent {
        NoteEvent {
            sample,
            kind: NoteEventKind::On {
                key: key(id),
                pitch,
                velocity: 100,
            },
        }
    }

    fn off(sample: u64, id: u128) -> NoteEvent {
        NoteEvent {
            sample,
            kind: NoteEventKind::Off { key: key(id) },
        }
    }

    #[test]
    fn db_to_gain_known_values() {
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(-6.0) - 0.501_187).abs() < 1e-6);
        assert!((db_to_gain(-20.0) - 0.1).abs() < 1e-7);
        assert_eq!(db_to_gain(-120.0), 0.0);
        assert_eq!(db_to_gain(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn a_new_project_is_an_empty_four_bar_loop_at_the_default_volume() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.gain, db_to_gain(Snapshot::DEFAULT_VOLUME_DB));
        let [track] = snapshot.tracks() else {
            panic!("one track");
        };
        assert_eq!(track.synth, SynthSettings::default());
        assert_eq!(track.mixer, MixerStrip::default());
        assert_eq!(track.slot(), 0);
        assert!(track.notes().events().is_empty());
        let sequence = &snapshot.sequence;
        assert_eq!(sequence.sample_rate(), DEFAULT_SAMPLE_RATE);
        // 4 bars at 120 BPM is 8 s.
        assert_eq!(sequence.loop_samples(), 0..8 * 48_000);
    }

    #[test]
    fn volume_is_clamped_to_the_ceiling() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.clone().with_volume_db(40.0).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(f32::INFINITY).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(0.0).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(f32::NAN).gain, 0.0);
        assert_eq!(snapshot.with_volume_db(-6.0).gain, db_to_gain(-6.0));
    }

    #[test]
    fn the_snapshot_follows_the_master_volume_and_synth() {
        let mut project = project();
        let track = project.tracks()[0].id();
        project
            .apply(&Command::SetMasterVolume { volume_db: -6.0 })
            .unwrap();
        project
            .apply(&Command::SetSynthParam {
                track,
                param: SynthParam::Waveform(uta_core::Waveform::Square),
            })
            .unwrap();
        project
            .apply(&Command::SetSynthParam {
                track,
                param: SynthParam::CutoffHz(800.0),
            })
            .unwrap();
        let snapshot = Snapshot::from(&project);
        assert_eq!(snapshot.gain, db_to_gain(-6.0));
        assert_eq!(snapshot.tracks()[0].synth.waveform, Waveform::Square);
        assert_eq!(snapshot.tracks()[0].synth.cutoff_hz, 800.0);
    }

    #[test]
    fn the_synth_settings_copy_across_field_by_field() {
        let core = uta_core::SynthSettings::default();
        assert_eq!(synth_settings(&core), SynthSettings::default());
    }

    #[test]
    fn notes_become_starts_and_ends_in_samples_sorted_by_time() {
        // At 120 BPM and 48 kHz a quarter note (960 ticks) is 24,000 samples.
        let project = with_notes(vec![
            note(3, 67, 1920, 960),
            note(1, 60, 0, 480),
            note(2, 64, 480, 1440),
        ]);
        let snapshot = Snapshot::new(&project, 48_000);
        assert_eq!(
            events(&snapshot),
            [
                on(0, 1, 60),
                off(12_000, 1),
                on(12_000, 2, 64),
                off(48_000, 2),
                on(48_000, 3, 67),
                off(72_000, 3),
            ],
            "ends come before starts on the same sample"
        );
    }

    /// Notes play over the whole song: the loop doesn't trim them, and
    /// changing it shares every track's events. The clip still does.
    #[test]
    fn notes_play_over_the_whole_song_whatever_the_loop() {
        let mut project = with_notes(vec![
            // Crosses the loop's end (4 bars, 15,360 ticks).
            note(1, 62, 15_360 - 480, 960),
            // Starts past the clip's end: it doesn't play.
            note(2, 65, 20_000, 480),
        ]);
        project
            .apply(&Command::SetClips {
                clips: vec![uta_core::ClipPosition {
                    id: clip(&project),
                    track: project.tracks()[0].id(),
                    start: 0,
                    length: 5 * 3840,
                }],
            })
            .unwrap();
        let snapshot = Snapshot::new(&project, 48_000);
        let end = 8 * 48_000;
        assert_eq!(
            events(&snapshot),
            [on(end - 12_000, 1, 62), off(end + 12_000, 1)]
        );
        // One bar after the 5-bar clip.
        assert_eq!(snapshot.sequence.song_end_sample(), 6 * 2 * 48_000);

        project
            .apply(&Command::SetLoop {
                start_bar: 1,
                bars: 1,
            })
            .unwrap();
        project
            .apply(&Command::SetLoopEnabled { enabled: false })
            .unwrap();
        let moved = Snapshot::sharing(&project, &snapshot);
        assert_eq!(moved.sequence.loop_samples(), 96_000..192_000);
        assert!(!moved.sequence.loop_enabled());
        assert!(Arc::ptr_eq(
            snapshot.tracks()[0].notes(),
            moved.tracks()[0].notes()
        ));
    }

    /// The notes to chase at a sample are exactly those that started before
    /// it and end after it, latest start first, and the count is known up
    /// front. Checked against a plain search over many sets of notes.
    #[test]
    fn the_notes_sounding_at_a_sample_are_found_with_the_index() {
        let sequence = Snapshot::default().sequence;
        // A small linear congruential generator, so the notes are the same
        // every run.
        let mut seed = 12_345u64;
        let mut next = move |below: u64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) % below
        };
        for count in [0u128, 1, 2, 3, 7, 8, 9, 100, 333] {
            let notes = (0..count)
                .map(|i| {
                    // Mostly short, some long, some starting together.
                    let length = if next(10) == 0 {
                        1 + next(8000)
                    } else {
                        1 + next(400)
                    };
                    let note = note(i, 60, next(60) * 60, length);
                    (note.id, note)
                })
                .collect();
            let clip = Arc::new(ClipNotes {
                id: ClipId::from_uuid(Uuid::from_u128(1)),
                start: 0,
                length: 20_000,
                notes: Arc::new(notes),
            });
            let track = TrackNotes::new(vec![clip], &sequence);
            let mut spans = track.notes.clone();
            spans.sort_by_key(|note| note.start);
            for sample in (0..sequence.sample_at(12_000))
                .step_by(997)
                .chain([0, 1500])
            {
                let found = track.sounding_at(sample);
                let expected: Vec<NoteKey> = spans
                    .iter()
                    .filter(|note| note.start < sample && sample < note.end)
                    .map(|note| note.key)
                    .collect();
                assert_eq!(found.len(), expected.len(), "{count} notes at {sample}");
                let found: Vec<&NoteSpan> = found.collect();
                assert!(found.windows(2).all(|pair| pair[0].start >= pair[1].start));
                let mut found: Vec<NoteKey> = found.iter().map(|note| note.key).collect();
                let mut expected = expected;
                found.sort_by_key(|key| key.0);
                expected.sort_by_key(|key| key.0);
                assert_eq!(found, expected, "{count} notes at {sample}");
            }
        }
    }

    #[test]
    fn events_follow_the_tempo_and_sample_rate() {
        let mut project = with_notes(vec![note(1, 60, 960, 960)]);
        project.apply(&Command::SetTempo { bpm: 90.0 }).unwrap();
        // At 90 BPM a quarter note is 2/3 s: 29,400 samples at 44.1 kHz.
        let snapshot = Snapshot::new(&project, 44_100);
        assert_eq!(events(&snapshot), [on(29_400, 1, 60), off(58_800, 1)]);
        assert_eq!(snapshot.sequence.loop_samples(), 0..16 * 29_400);
        // The 4-bar clip, and a bar after it.
        assert_eq!(snapshot.sequence.song_end_sample(), 20 * 29_400);
    }

    #[test]
    fn retiming_shares_the_clips_and_matches_a_fresh_build() {
        let project = with_notes(vec![note(1, 60, 100, 700), note(2, 72, 5000, 3)]);
        let at_48k = Snapshot::new(&project, 48_000);
        let at_96k = at_48k.at_sample_rate(96_000);
        assert_eq!(at_96k, Snapshot::new(&project, 96_000));
        assert!(Arc::ptr_eq(&clips(&at_48k)[0], &clips(&at_96k)[0]));
        // Retiming to the same rate shares the events too.
        let same = at_48k.at_sample_rate(48_000);
        assert!(Arc::ptr_eq(
            at_48k.tracks()[0].notes(),
            same.tracks()[0].notes()
        ));
    }

    #[test]
    fn notes_are_looked_up_by_key_as_they_play() {
        let project = with_notes(vec![
            note(3, 67, 1920, 960),
            note(1, 60, 0, 480),
            // Past the clip's end: it doesn't play, so it isn't there.
            note(2, 64, 20_000, 480),
        ]);
        let snapshot = Snapshot::new(&project, 48_000);
        let sequence = snapshot.tracks()[0].notes();
        assert_eq!(
            sequence.note(key(3)),
            Some(&NoteSpan {
                key: key(3),
                pitch: 67,
                velocity: 100,
                start: 48_000,
                end: 72_000,
            })
        );
        assert_eq!(sequence.note(key(1)).map(|n| n.pitch), Some(60));
        assert_eq!(sequence.note(key(2)), None);
        assert_eq!(sequence.note(key(99)), None);
        let span = sequence.note(key(1)).unwrap();
        assert!(span.contains(0) && span.contains(11_999) && !span.contains(12_000));
    }

    #[test]
    fn edits_share_the_notes_of_clips_they_do_not_touch() {
        let mut project = with_notes(vec![note(1, 60, 0, 480)]);
        let first = Snapshot::new(&project, 44_100);

        project.apply(&Command::SetTempo { bpm: 90.0 }).unwrap();
        let tempo = Snapshot::sharing(&project, &first);
        assert!(Arc::ptr_eq(&clips(&first)[0], &clips(&tempo)[0]));
        assert_eq!(tempo, Snapshot::new(&project, 44_100), "timed at 44.1 kHz");

        let clip = clip(&project);
        project
            .apply(&Command::AddNotes {
                clip,
                notes: vec![note(2, 62, 960, 480)],
            })
            .unwrap();
        let added = Snapshot::sharing(&project, &tempo);
        assert!(!Arc::ptr_eq(&clips(&tempo)[0], &clips(&added)[0]));
        assert_eq!(added, Snapshot::new(&project, 44_100));
    }

    /// The playhead's place in the song, as a new snapshot takes over.
    #[test]
    fn the_playhead_keeps_its_place_in_the_music() {
        let project = project();
        let at_120 = Snapshot::new(&project, 48_000).sequence;
        let mut slower = project.clone();
        slower.apply(&Command::SetTempo { bpm: 60.0 }).unwrap();
        let at_60 = Snapshot::new(&slower, 48_000).sequence;

        // Same tempo and rate: the same sample, even between ticks.
        assert_eq!(at_120.playhead_from(&at_120, 12_345), 12_345);
        // Half the tempo: beat 2 (tick 960) moves from 24,000 to 48,000.
        assert_eq!(at_60.playhead_from(&at_120, 24_000), 48_000);
        // Between ticks (a tick is 25 samples at 120 BPM), it moves on to
        // the next tick, which hadn't been reached yet.
        assert_eq!(at_60.playhead_from(&at_120, 24_001), 48_050);
        assert_eq!(at_60.playhead_from(&at_120, 24_024), 48_050);
        // A new sample rate: the same tick, at the new rate.
        let at_96k = at_120.at_sample_rate(96_000);
        assert_eq!(at_96k.playhead_from(&at_120, 24_000), 48_000);
        assert_eq!(at_120.playhead_from(&at_96k, 48_000), 24_000);

        // Past the end of a shorter loop it stays put: the processor decides
        // what happens there.
        let mut shorter = project.clone();
        shorter.apply(&Command::SetLoopLength { bars: 1 }).unwrap();
        let one_bar = Snapshot::new(&shorter, 48_000).sequence;
        assert_eq!(one_bar.playhead_from(&at_120, 300_000), 300_000);
    }

    #[test]
    fn positions_convert_between_ticks_and_samples() {
        let sequence = &Snapshot::default().sequence;
        assert_eq!(sequence.sample_at(960), 24_000);
        assert_eq!(sequence.ticks_at(24_000), 960);
    }

    /// Track `n` with one 1-bar clip holding a note of `pitch`, with IDs
    /// worked out from `n`.
    fn track(n: u128, pitch: u8) -> uta_core::Track {
        let clip = uta_core::Clip::new(ClipId::from_uuid(Uuid::from_u128(200 + n)), 0, 3840)
            .with_notes([note(300 + n, pitch, 0, 960)]);
        uta_core::Track::new(
            TrackId::from_uuid(Uuid::from_u128(100 + n)),
            format!("Synth {n}"),
            Source::Synth(uta_core::SynthSettings::default()),
        )
        .with_clips([clip])
    }

    fn track_id(n: u128) -> TrackId {
        TrackId::from_uuid(Uuid::from_u128(100 + n))
    }

    fn add(project: &mut Project, n: u128, index: usize) {
        project
            .apply(&Command::AddTracks {
                tracks: vec![uta_core::PlacedTrack {
                    index,
                    track: track(n, 30 + (n % 64) as u8),
                }],
            })
            .unwrap();
    }

    fn remove(project: &mut Project, n: u128) {
        project
            .apply(&Command::RemoveTracks {
                tracks: vec![track_id(n)],
            })
            .unwrap();
    }

    /// The project's first track, then tracks 2 to `count`.
    fn tracks(count: u128) -> Project {
        let mut project = project();
        for n in 2..=count {
            add(&mut project, n, n as usize - 1);
        }
        project
    }

    fn slots(snapshot: &Snapshot) -> Vec<usize> {
        snapshot.tracks().iter().map(TrackSnapshot::slot).collect()
    }

    #[test]
    fn every_track_plays_in_its_own_slot_with_its_own_notes() {
        let snapshot = Snapshot::new(&tracks(3), 48_000);
        assert_eq!(slots(&snapshot), [0, 1, 2]);
        for (index, track) in snapshot.tracks().iter().enumerate() {
            assert_eq!(snapshot.track_in(track.slot()), Some(track));
            assert_eq!(snapshot.slot_of(track.id()), Some(index));
        }
        assert_eq!(snapshot.track_in(3), None);
        assert_eq!(snapshot.track_in(TRACK_SLOTS), None);
        assert!(events(&snapshot).is_empty());
        for (track, n) in snapshot.tracks()[1..].iter().zip([2, 3]) {
            let pitch = 30 + n as u8;
            assert_eq!(
                track.notes().events(),
                [on(0, 300 + n, pitch), off(24_000, 300 + n)]
            );
        }
    }

    /// A track keeps its slot for its life: reordering and removing others
    /// don't move it, and a new track takes a slot the last snapshot didn't
    /// use, so it never lands on notes still fading from a removed one.
    #[test]
    fn tracks_keep_their_slots_and_new_ones_avoid_the_last_snapshots() {
        let mut project = tracks(3);
        let first = Snapshot::new(&project, 48_000);

        project
            .apply(&Command::MoveTrack {
                track: track_id(3),
                index: 0,
            })
            .unwrap();
        let moved = Snapshot::sharing(&project, &first);
        assert_eq!(slots(&moved), [2, 0, 1]);

        // Track 2 goes and track 4 comes in the same change: track 4 doesn't
        // take slot 1, which track 2's notes may still be fading in.
        remove(&mut project, 2);
        add(&mut project, 4, 1);
        let swapped = Snapshot::sharing(&project, &moved);
        assert_eq!(slots(&swapped), [2, 3, 0]);
        // Slots the controller says are still fading are avoided too.
        add(&mut project, 5, 0);
        let avoiding = Snapshot::sharing_avoiding(&project, &swapped, 0b1_0010);
        assert_eq!(slots(&avoiding), [5, 2, 3, 0]);
        // Once the last snapshot didn't use it and nothing avoids it, slot 1
        // is free again.
        add(&mut project, 6, 0);
        let reused = Snapshot::sharing(&project, &avoiding);
        assert_eq!(slots(&reused), [1, 5, 2, 3, 0]);
    }

    /// With every slot taken or avoided, a new track still gets a slot no
    /// track in the project uses.
    #[test]
    fn with_no_slot_left_a_new_track_takes_an_avoided_one() {
        let mut project = tracks(TRACK_SLOTS as u128);
        let full = Snapshot::new(&project, 48_000);
        assert_eq!(slots(&full), (0..TRACK_SLOTS).collect::<Vec<_>>());
        remove(&mut project, 7);
        add(&mut project, 99, 0);
        let snapshot = Snapshot::sharing_avoiding(&project, &full, u32::MAX);
        assert_eq!(snapshot.slot_of(track_id(99)), Some(6));
    }

    /// Clips are found by ID wherever they are, so reordering tracks shares
    /// every clip's notes and every track's events with the last snapshot.
    #[test]
    fn a_reorder_shares_every_clip_and_every_tracks_events() {
        let mut project = tracks(3);
        let first = Snapshot::new(&project, 48_000);
        project
            .apply(&Command::MoveTrack {
                track: track_id(2),
                index: 2,
            })
            .unwrap();
        let moved = Snapshot::sharing(&project, &first);
        for track in moved.tracks() {
            let before = first.track(track.id()).unwrap();
            assert!(Arc::ptr_eq(before.notes(), track.notes()));
            assert!(Arc::ptr_eq(
                &before.notes().clips()[0],
                &track.notes().clips()[0]
            ));
        }
    }

    /// An edit to one track's notes rebuilds only that track's events, and
    /// only the clip it changed.
    #[test]
    fn editing_one_track_rebuilds_only_its_events() {
        let mut project = tracks(3);
        let clip = ClipId::from_uuid(Uuid::from_u128(203));
        project
            .apply(&Command::AddClips {
                clips: vec![uta_core::PlacedClip {
                    track: track_id(2),
                    clip: uta_core::Clip::new(ClipId::from_uuid(Uuid::from_u128(299)), 3840, 3840),
                }],
            })
            .unwrap();
        let first = Snapshot::new(&project, 48_000);
        project
            .apply(&Command::AddNotes {
                clip,
                notes: vec![note(400, 70, 960, 480)],
            })
            .unwrap();
        let edited = Snapshot::sharing(&project, &first);
        let [one, two, three] = [0, 1, 2].map(|i| (&first.tracks()[i], &edited.tracks()[i]));
        assert!(Arc::ptr_eq(one.0.notes(), one.1.notes()));
        assert!(Arc::ptr_eq(two.0.notes(), two.1.notes()));
        assert!(!Arc::ptr_eq(three.0.notes(), three.1.notes()));
        assert_eq!(three.1.notes().events().len(), 4);
        assert_eq!(edited, Snapshot::new(&project, 48_000));

        // A tempo change retimes every track, but still shares every clip.
        project.apply(&Command::SetTempo { bpm: 90.0 }).unwrap();
        let slower = Snapshot::sharing(&project, &edited);
        for (before, after) in edited.tracks().iter().zip(slower.tracks()) {
            assert!(!Arc::ptr_eq(before.notes(), after.notes()));
            for (a, b) in before.notes().clips().iter().zip(after.notes().clips()) {
                assert!(Arc::ptr_eq(a, b));
            }
        }
    }

    /// Moving a clip to another track keeps its notes and rebuilds both
    /// tracks' events, which then have the clip's notes on the new track.
    #[test]
    fn a_clip_moved_to_another_track_keeps_its_notes() {
        let mut project = tracks(3);
        let first = Snapshot::new(&project, 48_000);
        let clip = ClipId::from_uuid(Uuid::from_u128(202));
        project
            .apply(&Command::SetClips {
                clips: vec![uta_core::ClipPosition {
                    id: clip,
                    track: track_id(3),
                    start: 0,
                    length: 3840,
                }],
            })
            .unwrap();
        let moved = Snapshot::sharing(&project, &first);
        let shared = &first.tracks()[1].notes().clips()[0];
        assert!(moved.tracks()[1].notes().clips().is_empty());
        assert!(events(&moved).is_empty());
        let on_three = moved.tracks()[2].notes();
        assert!(on_three.clips().iter().any(|c| Arc::ptr_eq(c, shared)));
        assert_eq!(on_three.events().len(), 4);
        assert!(Arc::ptr_eq(
            first.tracks()[0].notes(),
            moved.tracks()[0].notes()
        ));
    }

    #[test]
    fn solo_on_any_track_means_soloing() {
        let mut project = tracks(2);
        assert!(!Snapshot::from(&project).soloing());
        project
            .apply(&Command::SetTrackMixer {
                track: track_id(2),
                mixer: uta_core::MixerStrip {
                    solo: true,
                    ..uta_core::MixerStrip::default()
                },
            })
            .unwrap();
        let snapshot = Snapshot::from(&project);
        assert!(snapshot.soloing());
        assert!(snapshot.tracks()[1].mixer.solo);
    }

    /// Each clip's notes in `snapshot`, by clip ID.
    fn clip_notes(snapshot: &Snapshot) -> HashMap<ClipId, &Arc<ClipNotes>> {
        snapshot
            .tracks()
            .iter()
            .flat_map(|track| track.notes().clips())
            .map(|clip| (clip.id, clip))
            .collect()
    }

    /// Checks every clip's notes in `snapshot` are `project`'s own `Arc` of
    /// them, not a copy.
    fn assert_shares_notes_with(snapshot: &Snapshot, project: &Project) {
        let clips = clip_notes(snapshot);
        let in_project: Vec<_> = project.tracks().iter().flat_map(|t| t.clips()).collect();
        assert_eq!(clips.len(), in_project.len());
        for clip in in_project {
            assert!(Arc::ptr_eq(&clips[&clip.id()].notes, clip.shared_notes()));
        }
    }

    /// A snapshot shares the project's notes rather than copying them, and
    /// the next one keeps every clip whose notes the project still shares.
    /// Unchanged is decided by pointer, not by comparing notes: setting a
    /// note to the values it already has gives the clip new notes in the
    /// project, so the snapshot takes them too. See RFC-004, "How changes
    /// are spotted".
    #[test]
    fn snapshots_share_the_projects_notes_and_spot_changes_by_pointer() {
        let mut project = tracks(3);
        let first = Snapshot::new(&project, 48_000);
        assert_shares_notes_with(&first, &project);

        let edited = ClipId::from_uuid(Uuid::from_u128(203));
        let same = *project.clip(edited).unwrap().notes().next().unwrap();
        project
            .apply(&Command::SetNotes {
                clip: edited,
                notes: vec![same],
            })
            .unwrap();
        let next = Snapshot::sharing(&project, &first);
        assert_shares_notes_with(&next, &project);
        let (before, after) = (clip_notes(&first), clip_notes(&next));
        for (id, clip) in &after {
            assert_eq!(Arc::ptr_eq(before[id], clip), *id != edited, "{id}");
        }
        assert_eq!(next, Snapshot::new(&project, 48_000));
    }
}
