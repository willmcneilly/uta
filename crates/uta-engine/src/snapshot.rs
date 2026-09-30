//! The "what to play" snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use uta_core::time::{TempoMap, Ticks};
use uta_core::{ClipId, Note, Project, Source, TrackId, Transport};

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
    /// The tempo and the loop, timed in samples.
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

    /// The slot it plays in, from 0 to [`TRACK_SLOTS`]. A track keeps its
    /// slot for as long as it's in the project.
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
    ///   its ID wherever it was, shares its notes with `previous`;
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
        let sequence = Sequence::new(project.transport(), sample_rate);
        let same_timing = previous.is_some_and(|previous| previous.sequence == sequence);
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
                                && shared.notes.iter().eq(clip.notes()) =>
                        {
                            Arc::clone(shared)
                        }
                        _ => Arc::new(ClipNotes {
                            id: clip.id(),
                            start: clip.start(),
                            length: clip.length(),
                            notes: clip.notes().copied().collect(),
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

/// The tempo and the loop, as the audio thread plays them: in ticks, and
/// worked out at one sample rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    tempo_map: TempoMap,
    loop_start: Ticks,
    loop_length: Ticks,

    sample_rate: u32,
    loop_start_sample: u64,
    loop_end_sample: u64,
}

/// One track's notes, as the audio thread plays them.
///
/// It keeps the musical version (each clip's notes, in ticks, in an `Arc`)
/// and, worked out from it at the snapshot's timing, every note start and end
/// in samples as one list sorted by time. Notes are trimmed to the loop: only
/// notes that start inside it play, and any that run past its end stop
/// there. See RFC-002, "The shared model", points 5 and 7, and RFC-003, "The
/// shared model, extended", point 5.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackNotes {
    clips: Vec<Arc<ClipNotes>>,
    events: Vec<NoteEvent>,
    /// The same notes, one entry each, sorted by key, so a sounding note can
    /// be looked up when a new snapshot arrives.
    notes: Vec<NoteSpan>,
}

/// One clip's notes, in ticks.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNotes {
    pub id: ClipId,
    /// Where the clip starts, in ticks from the start of the song.
    pub start: Ticks,
    /// Notes starting at or after this, from the clip's start, don't play.
    pub length: Ticks,
    pub notes: Vec<Note>,
}

/// A note starting or ending, on an exact sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEvent {
    /// The sample it happens on, counted from the start of the song.
    pub sample: u64,
    pub kind: NoteEventKind,
}

/// When one note plays, in samples, after trimming to the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteSpan {
    pub key: NoteKey,
    pub pitch: u8,
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
    /// Clips may overlap: each one's notes play.
    pub fn new(clips: Vec<Arc<ClipNotes>>, sequence: &Sequence) -> Self {
        let loop_end = sequence.loop_start + sequence.loop_length;
        let loop_ticks = sequence.loop_start..loop_end;
        let mut events = Vec::new();
        let mut notes = Vec::new();
        for clip in &clips {
            for note in &clip.notes {
                if note.start >= clip.length {
                    continue;
                }
                let start = clip.start + note.start;
                if !loop_ticks.contains(&start) {
                    continue;
                }
                let end = (clip.start + (note.start + note.length).min(clip.length)).min(loop_end);
                let (start, end) = (sequence.sample_at(start), sequence.sample_at(end));
                if end <= start {
                    continue;
                }
                let key = NoteKey(note.id.as_uuid().as_u128());
                notes.push(NoteSpan {
                    key,
                    pitch: note.pitch,
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
        Self {
            clips,
            events,
            notes,
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

    /// Every note start and end in the loop, sorted by sample.
    pub fn events(&self) -> &[NoteEvent] {
        &self.events
    }

    /// The note with this key, if it plays in the loop. A binary search, so
    /// it's real-time safe.
    pub fn note(&self, key: NoteKey) -> Option<&NoteSpan> {
        self.notes
            .binary_search_by_key(&key.0, |note| note.key.0)
            .ok()
            .map(|index| &self.notes[index])
    }
}

impl Sequence {
    /// The transport's tempo and loop, at `sample_rate`.
    fn new(transport: &Transport, sample_rate: u32) -> Self {
        Self::build(
            transport.tempo_map().clone(),
            transport.loop_start(),
            transport.loop_length(),
            sample_rate,
        )
    }

    fn build(tempo_map: TempoMap, loop_start: Ticks, loop_length: Ticks, sample_rate: u32) -> Self {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(loop_length > 0, "the loop can't be empty");
        let samples = |ticks| tempo_map.ticks_to_samples(ticks, sample_rate);
        Self {
            loop_start_sample: samples(loop_start),
            loop_end_sample: samples(loop_start + loop_length),
            tempo_map,
            loop_start,
            loop_length,
            sample_rate,
        }
    }

    /// This sequence timed at `sample_rate`.
    pub fn at_sample_rate(&self, sample_rate: u32) -> Self {
        Self::build(
            self.tempo_map.clone(),
            self.loop_start,
            self.loop_length,
            sample_rate,
        )
    }

    /// The sample rate everything is timed at.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Where the loop starts and ends, in samples from the start of the song.
    pub fn loop_samples(&self) -> std::ops::Range<u64> {
        self.loop_start_sample..self.loop_end_sample
    }

    /// Where a playhead at `playhead` in `old` carries on in this sequence.
    /// Real-time safe.
    ///
    /// If the tempo or sample rate changed, it keeps its bar and beat: it
    /// moves to the first tick `old` hadn't reached yet, so every note event
    /// `old` had already played stays played and none is skipped. Otherwise
    /// it stays on the same sample. If that's outside the loop (the loop got
    /// shorter, or the playhead was waiting at the loop's end) it carries on
    /// from the loop's start.
    pub fn playhead_from(&self, old: &Sequence, playhead: u64) -> u64 {
        let playhead = if self.sample_rate == old.sample_rate && self.tempo_map == old.tempo_map {
            playhead
        } else {
            let ticks = old.ticks_at(playhead);
            let ticks = if old.sample_at(ticks) < playhead {
                ticks + 1
            } else {
                ticks
            };
            self.sample_at(ticks)
        };
        if self.loop_samples().contains(&playhead) {
            playhead
        } else {
            self.loop_start_sample
        }
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
        .map(|i| Note {
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
    use uta_core::{ClipId, Command, NoteId, ProjectId, SynthParam};
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

    #[test]
    fn notes_are_trimmed_to_the_loop() {
        let mut project = with_notes(vec![
            // Ends exactly at the loop's end (4 bars, 15,360 ticks).
            note(1, 60, 15_360 - 480, 480),
            // Crosses the loop's end: it stops there.
            note(2, 62, 15_360 - 480, 960),
            // Starts at the loop's end, and past it: neither plays.
            note(3, 64, 15_360, 480),
            note(4, 65, 20_000, 480),
        ]);
        let snapshot = Snapshot::new(&project, 48_000);
        let end = 8 * 48_000;
        assert_eq!(
            events(&snapshot),
            [
                on(end - 12_000, 1, 60),
                on(end - 12_000, 2, 62),
                off(end, 1),
                off(end, 2),
            ]
        );

        // Shortening the loop to 1 bar trims the same way at the new end.
        project.apply(&Command::SetLoopLength { bars: 1 }).unwrap();
        let snapshot = Snapshot::new(&project, 48_000);
        assert_eq!(snapshot.sequence.loop_samples(), 0..96_000);
        assert!(events(&snapshot).is_empty());
    }

    /// In project 1 the clip is always as long as the loop, so the clip's
    /// length trims notes before the loop's end can. Here the clip is longer
    /// than the loop and starts before it, so only the loop checks keep notes
    /// inside it.
    #[test]
    fn notes_are_trimmed_to_a_loop_shorter_than_the_clip() {
        // A 4-bar clip; the loop is bar 2 (ticks 3,840 to 7,680).
        let clip = Arc::new(ClipNotes {
            id: ClipId::from_uuid(Uuid::from_u128(1)),
            start: 0,
            length: 4 * 3840,
            notes: vec![
                // Starts before the loop and runs into it: doesn't play.
                note(1, 60, 3840 - 480, 960),
                // Starts on the loop's start.
                note(2, 62, 3840, 480),
                // Crosses the loop's end: released there.
                note(3, 64, 7680 - 480, 960),
                // Starts on the loop's end: doesn't play.
                note(4, 65, 7680, 480),
            ],
        });
        let sequence = Sequence::build(TempoMap::new(120.0), 3840, 3840, 48_000);
        assert_eq!(sequence.loop_samples(), 96_000..192_000);
        assert_eq!(
            TrackNotes::new(vec![clip], &sequence).events(),
            [
                on(96_000, 2, 62),
                off(108_000, 2),
                on(180_000, 3, 64),
                off(192_000, 3),
            ]
        );
    }

    #[test]
    fn events_follow_the_tempo_and_sample_rate() {
        let mut project = with_notes(vec![note(1, 60, 960, 960)]);
        project.apply(&Command::SetTempo { bpm: 90.0 }).unwrap();
        // At 90 BPM a quarter note is 2/3 s: 29,400 samples at 44.1 kHz.
        let snapshot = Snapshot::new(&project, 44_100);
        assert_eq!(events(&snapshot), [on(29_400, 1, 60), off(58_800, 1)]);
        assert_eq!(snapshot.sequence.loop_samples(), 0..16 * 29_400);
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
            // Past the loop's end: it doesn't play, so it isn't there.
            note(2, 64, 20_000, 480),
        ]);
        let snapshot = Snapshot::new(&project, 48_000);
        let sequence = snapshot.tracks()[0].notes();
        assert_eq!(
            sequence.note(key(3)),
            Some(&NoteSpan {
                key: key(3),
                pitch: 67,
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

    /// The playhead's place in the loop, as a new snapshot takes over.
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

        // Past the end of a shorter loop, or waiting at the loop's end, it
        // carries on from the loop's start.
        let mut shorter = project.clone();
        shorter.apply(&Command::SetLoopLength { bars: 1 }).unwrap();
        let one_bar = Snapshot::new(&shorter, 48_000).sequence;
        assert_eq!(one_bar.playhead_from(&at_120, 95_999), 95_999);
        assert_eq!(one_bar.playhead_from(&at_120, 96_000), 0);
        assert_eq!(one_bar.playhead_from(&at_120, 300_000), 0);
        assert_eq!(at_120.playhead_from(&at_120, 8 * 48_000), 0);
    }

    #[test]
    fn positions_convert_between_ticks_and_samples() {
        let sequence = &Snapshot::default().sequence;
        assert_eq!(sequence.sample_at(960), 24_000);
        assert_eq!(sequence.ticks_at(24_000), 960);
    }
}
